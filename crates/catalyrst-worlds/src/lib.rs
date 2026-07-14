#![allow(clippy::result_large_err)]

pub mod access;
pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod handlers;
pub mod http;
pub mod livekit;
pub mod ports;
pub mod rate_limiter;
pub mod upload_limits;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use utoipa::OpenApi;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

use crate::config::Config;
use crate::ports::bans::BansComponent;
use crate::ports::denylist::DenyListComponent;
use crate::ports::name_denylist::NameDenyListChecker;
use crate::ports::presence::PeersRegistry;
use crate::ports::worlds::WorldsComponent;
use crate::rate_limiter::RateLimiter;

pub struct AppStateInner {
    pub cfg: Config,
    pub worlds: WorldsComponent,
    pub presence: PeersRegistry,
    pub rate_limiter: RateLimiter,
    pub bans: BansComponent,
    pub denylist: DenyListComponent,
    pub name_denylist: NameDenyListChecker,
    pub http: reqwest::Client,
    pub squid_pool: Option<sqlx::PgPool>,
}

pub type AppState = Arc<AppStateInner>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid WORLDS_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect worlds pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run worlds migrations")?;

    let squid_pool = match cfg.squid_database_url.as_deref() {
        Some(url) => {
            let opts = PgConnectOptions::from_str(url)
                .context("invalid SQUID_PG_CONNECTION_STRING")?
                .options([("statement_timeout", "15000")]);
            match PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .idle_timeout(Duration::from_secs(60))
                .connect_with(opts)
                .await
            {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to connect squid marketplace pool; NAME-ownership publish authz disabled (fail-closed → deny)");
                    None
                }
            }
        }
        None => {
            tracing::warn!("SQUID_PG_CONNECTION_STRING unset; NAME-ownership publish authz disabled (fail-closed → deny)");
            None
        }
    };

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("build http client")?;
    let bans = BansComponent::new(
        http.clone(),
        cfg.comms_gatekeeper_url.clone(),
        cfg.comms_gatekeeper_auth_token.clone(),
    );
    let denylist = DenyListComponent::new(http.clone(), cfg.denylist_json_url.clone());
    let name_denylist = NameDenyListChecker::new(http.clone(), cfg.dcl_lists_url.clone());

    Ok(Arc::new(AppStateInner {
        worlds: WorldsComponent::new(pool),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans,
        denylist,
        name_denylist,
        http,
        squid_pool,
        cfg,
    }))
}

#[derive(OpenApi)]
#[openapi(info(title = "catalyrst-worlds"))]
struct ApiDoc;

pub fn api_router_with_spec() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(handlers::status::status))
        .routes(routes!(handlers::index::get_index))
        .routes(routes!(handlers::about::get_about))
        .routes(routes!(handlers::worlds_list::get_worlds))
        .routes(routes!(handlers::world_manifest::get_world_manifest))
        .routes(routes!(handlers::permissions::get_permissions))
        .routes(routes!(handlers::permissions::post_permissions))
        .routes(routes!(
            handlers::permissions::get_allowed_parcels_for_permission,
            handlers::permissions::post_permission_parcels,
            handlers::permissions::delete_permission_parcels
        ))
        .routes(routes!(
            handlers::permissions::get_addresses_for_parcel_permission
        ))
        .routes(routes!(
            handlers::permissions::put_permissions_access_community,
            handlers::permissions::delete_permissions_access_community
        ))
        .routes(routes!(
            handlers::permissions::put_permissions_address,
            handlers::permissions::delete_permissions_address
        ))
        .routes(routes!(handlers::active::active_entities))
        .routes(routes!(handlers::scenes::undeploy_world))
        .routes(routes!(handlers::scenes::list_scenes))
        .routes(routes!(handlers::scenes::delete_scene))
        .routes(routes!(handlers::comms::world_comms))
        .routes(routes!(handlers::comms::world_scene_comms))
        .routes(routes!(
            handlers::contents::get_content,
            handlers::contents::head_content
        ))
        .routes(routes!(handlers::contents::available_content))
        .routes(routes!(handlers::wallet::connected_world))
        .routes(routes!(handlers::live_data::live_data))
        .routes(routes!(handlers::webhook::livekit_webhook))
        .routes(routes!(handlers::admin::list_worlds))
        .routes(routes!(handlers::admin::world_detail))
        .routes(routes!(handlers::admin::enable_world))
        .routes(routes!(handlers::admin::disable_world))
        .routes(routes!(handlers::admin::world_ban_status))
        .routes(routes!(handlers::admin::list_blocked))
        .routes(routes!(
            handlers::admin::block_wallet,
            handlers::admin::unblock_wallet
        ))
        .routes(routes!(handlers::admin::access_log))
        .routes(routes!(handlers::world_settings::get_world_settings))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .routes(
            routes!(handlers::world_settings::update_world_settings).layer(
                axum::extract::DefaultBodyLimit::max(
                    handlers::world_settings::MAX_SETTINGS_UPLOAD_WIRE_BYTES,
                ),
            ),
        )
        .routes(routes!(handlers::deploy::deploy_entity).layer(
            axum::extract::DefaultBodyLimit::max(handlers::deploy::MAX_UPLOAD_WIRE_SIZE_BYTES),
        ))
        .split_for_parts()
}

pub fn api_router() -> Router<AppState> {
    let (router, spec) = api_router_with_spec();
    router.route(
        "/openapi.json",
        get(move || {
            let spec = spec.clone();
            async move { axum::Json(spec) }
        }),
    )
}

#[cfg(test)]
mod openapi_export {
    #[test]
    fn export_bindings_openapi() {
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let spec = super::api_router_with_spec().1;
        let out = std::path::Path::new(&dir).join("openapi");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join("worlds.openapi.json"),
            serde_json::to_string_pretty(&spec).unwrap(),
        )
        .unwrap();
    }
}
