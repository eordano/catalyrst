#![allow(clippy::result_large_err)]

pub mod auth_chain;
pub mod config;
pub mod encryption;
pub mod external;
pub mod handlers;
pub mod http;
pub mod storage;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::http::HeaderMap;
use axum::routing::{delete, get, put};
use axum::Router;
use sqlx::postgres::PgPoolOptions;

use crate::auth_chain::{verify_request, SceneAuthMetadata};
use crate::config::Config;
use crate::encryption::Encryptor;
use crate::external::{ExternalClient, GENESIS_CITY_REALM};
use crate::http::errors::ApiError;
use crate::storage::Storage;

pub struct AppStateInner {
    pub storage: Storage,
    pub encryptor: Encryptor,
    pub external: ExternalClient,
    pub cfg: Config,
    pub eip1654_validator: Option<Arc<dyn catalyrst_crypto::Eip1654Validator>>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect world_storage pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run world_storage migrations")?;

    let encryptor = Encryptor::new(&cfg.encryption_key);
    let external = ExternalClient::new(
        cfg.places_url.clone(),
        cfg.worlds_content_server_url.clone(),
        cfg.lambdas_url.clone(),
        cfg.places_cache_ttl_seconds,
    );

    let eip1654_validator: Option<Arc<dyn catalyrst_crypto::Eip1654Validator>> =
        cfg.eip1654_rpc_url.as_ref().map(|url| {
            let rpc = catalyrst_crypto::RpcEip1654Validator::new(url.clone());
            Arc::new(catalyrst_crypto::ValidationCache::new(Arc::new(rpc)))
                as Arc<dyn catalyrst_crypto::Eip1654Validator>
        });

    Ok(Arc::new(AppStateInner {
        storage: Storage::new(pool),
        encryptor,
        external,
        cfg,
        eip1654_validator,
    }))
}

pub struct SceneContext {
    pub signer: String,
    pub world_name: String,
    pub parcel: String,
    pub place_id: String,
}

#[derive(Clone, Copy)]
pub struct AuthPolicy {
    pub allow_authorized_addresses: bool,

    pub allow_owners_and_deployers: bool,
}

impl AuthPolicy {
    pub const DEFAULT: AuthPolicy = AuthPolicy {
        allow_authorized_addresses: true,
        allow_owners_and_deployers: true,
    };

    pub const OWNERS_DEPLOYERS_ONLY: AuthPolicy = AuthPolicy {
        allow_authorized_addresses: false,
        allow_owners_and_deployers: true,
    };

    pub const AUTHORIZED_ADDRESSES_ONLY: AuthPolicy = AuthPolicy {
        allow_authorized_addresses: true,
        allow_owners_and_deployers: false,
    };
}

pub fn signed_path(uri: &axum::http::Uri) -> String {
    match uri.query() {
        Some(q) if !q.is_empty() => format!("{}?{}", uri.path(), q),
        _ => uri.path().to_string(),
    }
}

pub async fn resolve_scene_context(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<SceneContext, ApiError> {
    let verified = verify_request(headers, method, path, state.eip1654_validator.as_deref())
        .await
        .map_err(auth_chain_to_api)?;
    let (world_name, parcel) = derive_world_and_parcel(&verified.metadata)?;
    let place_id = state
        .external
        .resolve_place_id(&world_name, &parcel)
        .await?;
    Ok(SceneContext {
        signer: verified.signer,
        world_name,
        parcel,
        place_id,
    })
}

pub async fn authorize(
    state: &AppState,
    ctx: &SceneContext,
    policy: AuthPolicy,
) -> Result<(), ApiError> {
    let signer = ctx.signer.to_ascii_lowercase();

    if policy.allow_authorized_addresses {
        let mut allowed: Vec<String> = Vec::new();
        if let Some(a) = &state.cfg.authoritative_server_address {
            allowed.push(a.clone());
        }
        allowed.extend(state.cfg.authorized_addresses.iter().cloned());
        if allowed.iter().any(|a| a == &signer) {
            return Ok(());
        }
    }

    if policy.allow_owners_and_deployers {
        let has = state
            .external
            .has_world_permission(&ctx.world_name, &signer, &ctx.parcel)
            .await
            .map_err(|_| {
                ApiError::not_authorized("Unauthorized: Failed to verify world permissions")
            })?;
        if has {
            return Ok(());
        }
    }

    Err(ApiError::not_authorized(
        "Unauthorized: Signer is not authorized to perform operations on this world",
    ))
}

fn derive_world_and_parcel(meta: &SceneAuthMetadata) -> Result<(String, String), ApiError> {
    let realm = meta
        .realm
        .as_ref()
        .and_then(|r| r.server_name.clone())
        .or_else(|| meta.realm_name.clone())
        .filter(|s| !s.is_empty());
    let parcel = meta.parcel.clone().filter(|s| !s.is_empty());

    if realm.is_none() && parcel.is_none() {
        return Err(ApiError::bad_request(
            "Request must include a realm name or a parcel",
        ));
    }

    let is_world = realm
        .as_deref()
        .map(|r| r.ends_with(".dcl.eth"))
        .unwrap_or(false);
    let world_name = match (&realm, is_world) {
        (Some(r), true) => r.clone(),
        _ => GENESIS_CITY_REALM.to_string(),
    };
    let resolved_parcel = parcel.unwrap_or_else(|| "0,0".to_string());
    Ok((world_name, resolved_parcel))
}

fn auth_chain_to_api(err: auth_chain::AuthChainError) -> ApiError {
    ApiError::SignedFetch {
        status: err.status_code(),
        error: err.raw_message(),
    }
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/usage/world", get(handlers::usage::get_world_usage))
        .route(
            "/usage/players/{player_address}",
            get(handlers::usage::get_player_usage),
        )
        .route("/usage/env", get(handlers::usage::get_env_usage))
        .route("/values", get(handlers::world::list))
        .route("/values", delete(handlers::world::clear))
        .route("/values/{key}", get(handlers::world::get))
        .route("/values/{key}", put(handlers::world::upsert))
        .route("/values/{key}", delete(handlers::world::delete))
        .route("/players", get(handlers::player::list_players))
        .route("/players", delete(handlers::player::clear_all_players))
        .route(
            "/players/{player_address}/values",
            get(handlers::player::list),
        )
        .route(
            "/players/{player_address}/values",
            delete(handlers::player::clear),
        )
        .route(
            "/players/{player_address}/values/{key}",
            get(handlers::player::get),
        )
        .route(
            "/players/{player_address}/values/{key}",
            put(handlers::player::upsert),
        )
        .route(
            "/players/{player_address}/values/{key}",
            delete(handlers::player::delete),
        )
        .route("/env", get(handlers::env::list_keys))
        .route("/env", delete(handlers::env::clear))
        .route("/env/{key}", get(handlers::env::get))
        .route("/env/{key}", put(handlers::env::upsert))
        .route("/env/{key}", delete(handlers::env::delete))
}

#[cfg(test)]
mod auth_chain_to_api_tests {
    use super::auth_chain_to_api;
    use crate::auth_chain::AuthChainError;
    use crate::http::errors::ApiError;
    use axum::response::IntoResponse;

    fn status_of(err: AuthChainError) -> u16 {
        auth_chain_to_api(err).into_response().status().as_u16()
    }

    #[test]
    fn maps_each_failure_to_its_real_status() {
        assert_eq!(
            status_of(AuthChainError::MalformedChain {
                detail: "boom".into()
            }),
            400
        );
        assert_eq!(status_of(AuthChainError::InsufficientLinks), 400);
        assert_eq!(status_of(AuthChainError::InvalidTimestamp("x".into())), 400);
        assert_eq!(status_of(AuthChainError::SceneSignerRejected), 400);

        assert_eq!(status_of(AuthChainError::MissingTimestamp), 401);
        assert_eq!(
            status_of(AuthChainError::Expired {
                signed_at: 0,
                now: 1,
                window_secs: 60
            }),
            401
        );
        assert_eq!(
            status_of(AuthChainError::InvalidSignature("no".into())),
            401
        );

        assert_eq!(status_of(AuthChainError::EipNotImplemented), 503);
        assert_eq!(
            status_of(AuthChainError::CatalystUnavailable("rpc".into())),
            503
        );
    }

    #[test]
    fn body_carries_raw_error_and_fixed_adr44_message() {
        let err = auth_chain_to_api(AuthChainError::SceneSignerRejected);
        match err {
            ApiError::SignedFetch { status, error } => {
                assert_eq!(status, 400);
                assert_eq!(error, "Invalid metadata");
            }
            other => panic!("expected SignedFetch, got {other:?}"),
        }
    }
}
