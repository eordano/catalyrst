pub mod auth;
pub mod auth_chain;
pub mod catalog;
pub mod clients;
pub mod config;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod s3;
pub mod sanitize;
pub mod snapshot;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::clients::{CommsGatekeeper, Events, Presence};
use crate::config::Config;
use crate::http::errors::ApiError;
use crate::ports::lists::ListsComponent;
use crate::ports::places::PlacesComponent;

pub struct AppStateInner {
    pub places: PlacesComponent,
    pub lists: ListsComponent,
    pub admin_addresses: Vec<String>,
    pub data_team_auth_token: Option<String>,
    pub admin_auth_token: Option<String>,

    pub comms_gatekeeper: CommsGatekeeper,

    pub events: Events,

    pub presence: Presence,

    pub gossip: Arc<dyn catalyrst_fed::GossipPublisher>,

    pub domain: catalyrst_fed::Eip712Domain,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = catalyrst_db::connect_pool(
        &cfg.places_database_url,
        &catalyrst_db::PoolSettings::default(),
    )
    .await
    .context("failed to connect places_events pool")?;

    if let Err(e) = crate::catalog::ensure_schema(&pool).await {
        tracing::warn!(
            error = %e,
            "could not ensure place catalog schema; place_indexed reads 500 until it is applied \
             (role lacks CREATE, or apply the deployment's out-of-band bootstrap)"
        );
    }

    let lists = ListsComponent::new(pool.clone());
    if let Err(e) = lists.ensure_schema().await {
        // 42501 = insufficient_privilege. This role is read-only by design, so a
        // DDL denial is NOT a lists problem: nothing can self-apply, and the
        // out-of-band migrations may be missing entirely. Reported as a
        // lists-only warning it hid exactly that -- 0003 sat unapplied behind a
        // line about /pois while every read 500'd on `column "world" does not
        // exist` (2026-07-29). Say what actually failed, at a level that shows.
        let denied = matches!(
            &e,
            ApiError::Common(catalyrst_types::ApiError::Database(sqlx::Error::Database(db)))
                if db.code().as_deref() == Some("42501")
        );
        if denied {
            tracing::error!(
                error = %e,
                "cannot apply schema: this role has no CREATE here, so migrations do NOT \
                 self-apply. Run the deployment's bootstrap-places script; until then \
                 place_indexed can lag crates/catalyrst-places/migrations and reads will 500."
            );
        } else {
            tracing::warn!(error = %e, "could not ensure lists schema; /pois and /banned-names fall back to empty until deploy/sync-lists.sh seeds the tables");
        }
    }
    let mut places = PlacesComponent::new(pool.clone());

    if let Some(writer_url) = &cfg.places_writer_database_url {
        let settings = catalyrst_db::PoolSettings {
            max_connections: 5,
            ..catalyrst_db::PoolSettings::default()
        };
        match catalyrst_db::connect_pool(writer_url, &settings).await {
            Ok(writer_pool) => {
                places = places.with_writer(writer_pool);
                if let Err(e) = places.ensure_local_schema().await {
                    tracing::warn!(error = %e, "could not ensure local interaction tables; favorites/likes/report writes may degrade");
                }
            }
            Err(catalyrst_db::PoolError::InvalidUrl(e)) => {
                tracing::warn!(error = %e, "invalid writer connection string; favorites/likes/report persistence disabled (503)");
            }
            Err(catalyrst_db::PoolError::Connect(e)) => {
                tracing::warn!(error = %e, "writer pool unavailable; favorites/likes/report persistence disabled (503)");
            }
        }
    } else {
        tracing::info!(
            "no writer connection configured; favorites/likes/report persistence disabled (503)"
        );
    }

    if let Some(squid_url) = &cfg.squid_database_url {
        match PgConnectOptions::from_str(squid_url) {
            Ok(squid_opts) => {
                match PgPoolOptions::new()
                    .max_connections(5)
                    .idle_timeout(Duration::from_secs(30))
                    .connect_with(squid_opts)
                    .await
                {
                    Ok(squid_pool) => {
                        places = places.with_squid(squid_pool, cfg.squid_schema.clone());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "squid pool unavailable; owner filter disabled");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid squid connection string; owner filter disabled");
            }
        }
    }

    let gossip = catalyrst_fed::build_publisher(&catalyrst_fed::GossipConfig::from_env()).await?;
    tracing::info!(
        gossip_live = gossip.is_live(),
        "places gossip publisher ready"
    );

    let state = Arc::new(AppStateInner {
        places,
        lists,
        admin_addresses: cfg.admin_addresses.clone(),
        data_team_auth_token: cfg.data_team_auth_token.clone(),
        admin_auth_token: cfg.admin_auth_token.clone(),
        comms_gatekeeper: CommsGatekeeper::new(cfg.comms_gatekeeper_url.clone()),
        events: Events::new(cfg.events_api_url.clone()),
        presence: Presence::new(cfg.presence_url.clone()),
        gossip,
        domain: catalyrst_fed::sig::domains::places(),
    });

    crate::fed::consumer::spawn(state.clone()).await;

    if cfg.mirror_upstream {
        crate::catalog::mirror::spawn(pool.clone(), cfg.upstream_url.clone());
        tracing::info!(upstream = %cfg.upstream_url, "place catalog: mirroring from upstream");
    } else if cfg.derive_places_from_content {
        match &cfg.content_database_url {
            Some(url) => {
                match catalyrst_db::connect_pool(url, &catalyrst_db::PoolSettings::default()).await
                {
                    Ok(content_pool) => {
                        crate::catalog::sync::spawn(
                            pool.clone(),
                            content_pool,
                            cfg.content_public_url.clone(),
                        );
                        tracing::info!("place catalog: deriving from content deployments");
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "content pool unavailable; place catalog will not self-populate"
                    ),
                }
            }
            None => tracing::warn!(
                "PLACES_DERIVE_FROM_CONTENT is set but CONTENT_PG_CONNECTION_STRING is missing"
            ),
        }
    }

    // Independent of the places lane: worlds live only in the upstream
    // /api/worlds listing, so a node deriving places from its own content
    // still needs this mirror for its worlds catalog.
    if cfg.worlds_mirror_upstream {
        let interval = std::time::Duration::from_secs(cfg.worlds_mirror_interval_secs);
        crate::catalog::worlds_mirror::spawn(
            pool.clone(),
            cfg.worlds_upstream_url.clone(),
            interval,
        );
        tracing::info!(
            upstream = %cfg.worlds_upstream_url,
            interval_secs = cfg.worlds_mirror_interval_secs,
            "world catalog: mirroring from upstream"
        );
    }

    Ok(state)
}

#[derive(OpenApi)]
#[openapi(info(title = "catalyrst-places"))]
struct ApiDoc;

pub fn api_router_with_spec() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let api = OpenApiRouter::new()
        .routes(routes!(handlers::categories::get_categories))
        .routes(routes!(handlers::federation::patch_place_favorites))
        .routes(routes!(handlers::federation::patch_place_likes))
        .routes(routes!(handlers::places::get_place))
        .routes(routes!(
            handlers::places::get_place_list,
            handlers::places::post_place_list_by_id
        ))
        .routes(routes!(handlers::federation::put_place_rating))
        .routes(routes!(handlers::federation::put_place_ranking))
        .routes(routes!(handlers::federation::put_place_highlight))
        .routes(routes!(handlers::categories::get_place_categories))
        .routes(routes!(
            handlers::federation::put_place_featured,
            handlers::federation::delete_place_featured
        ))
        .routes(routes!(handlers::places::post_place_status_list_by_id))
        .routes(routes!(handlers::worlds::get_world))
        .routes(routes!(handlers::worlds::get_world_list))
        .routes(routes!(handlers::worlds::get_world_names_list))
        .routes(routes!(handlers::federation::patch_world_favorites))
        .routes(routes!(handlers::federation::patch_world_likes))
        .routes(routes!(handlers::federation::put_world_highlight))
        .routes(routes!(handlers::federation::put_world_ranking))
        .routes(routes!(handlers::federation::put_world_rating))
        .routes(routes!(
            handlers::federation::put_world_featured,
            handlers::federation::delete_world_featured
        ))
        .routes(routes!(handlers::report::post_report))
        .routes(routes!(handlers::report::put_report_upload))
        .routes(routes!(handlers::map::get_map_places))
        .routes(routes!(handlers::map::get_all_places_list))
        .routes(routes!(
            handlers::destinations::get_destinations_list,
            handlers::destinations::post_destinations_list_by_id
        ))
        .routes(routes!(handlers::status::status))
        .routes(routes!(handlers::admin::get_reports))
        .routes(routes!(handlers::admin::patch_report))
        .routes(routes!(
            handlers::admin::patch_place_disable,
            handlers::federation::put_place_disable
        ))
        .routes(routes!(
            handlers::admin::get_pois,
            handlers::admin::post_poi
        ))
        .routes(routes!(
            handlers::admin::patch_poi,
            handlers::admin::delete_poi
        ));

    let social = OpenApiRouter::new()
        .routes(routes!(handlers::social::inject_place_metadata))
        .routes(routes!(handlers::social::inject_world_metadata));

    let federation = OpenApiRouter::new()
        .routes(routes!(handlers::fed_sync::snapshot))
        .routes(routes!(handlers::fed_sync::changes));

    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api", api)
        .nest("/places", social)
        .merge(federation)
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

pub fn lists_router() -> Router<AppState> {
    Router::new()
        .route("/pois", post(handlers::lists::post_pois))
        .route("/banned-names", post(handlers::lists::post_banned_names))
}

#[cfg(test)]
mod openapi_export {
    #[test]
    fn export_bindings_openapi() {
        let spec = super::api_router_with_spec().1;
        let rendered = serde_json::to_string_pretty(&spec).expect("spec serialises");
        catalyrst_contract_gate::assert_usable_spec(
            "places",
            &serde_json::from_str(&rendered).expect("spec round-trips through JSON"),
        );
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let out = std::path::Path::new(&dir).join("openapi");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("places.openapi.json"), rendered).unwrap();
    }
}
