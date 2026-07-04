pub mod access;
pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod handlers;
pub mod http;
pub mod livekit;
pub mod ports;
pub mod rate_limiter;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{delete, get, post, put};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

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

    let http = reqwest::Client::new();
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

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/index", get(handlers::index::get_index))
        .route("/world/{world_name}/about", get(handlers::about::get_about))
        .route("/worlds", get(handlers::worlds_list::get_worlds))
        .route(
            "/world/{world_name}/settings",
            get(handlers::world_settings::get_world_settings)
                .put(handlers::world_settings::update_world_settings),
        )
        .route(
            "/world/{world_name}/manifest",
            get(handlers::world_manifest::get_world_manifest),
        )
        .route(
            "/world/{world_name}/permissions",
            get(handlers::permissions::get_permissions),
        )
        .route(
            "/world/{world_name}/permissions/{permission_name}",
            post(handlers::permissions::post_permissions),
        )
        .route(
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
            get(handlers::permissions::get_allowed_parcels_for_permission)
                .post(handlers::permissions::post_permission_parcels)
                .delete(handlers::permissions::delete_permission_parcels),
        )
        .route(
            "/world/{world_name}/permissions/{permission_name}/parcels",
            post(handlers::permissions::get_addresses_for_parcel_permission),
        )
        .route(
            "/world/{world_name}/permissions/access/communities/{communityId}",
            put(handlers::permissions::put_permissions_access_community)
                .delete(handlers::permissions::delete_permissions_access_community),
        )
        .route(
            "/world/{world_name}/permissions/{permission_name}/{address}",
            put(handlers::permissions::put_permissions_address)
                .delete(handlers::permissions::delete_permissions_address),
        )
        .route("/entities/active", post(handlers::active::active_entities))
        .route(
            "/entities",
            post(handlers::deploy::deploy_entity).layer(axum::extract::DefaultBodyLimit::max(
                handlers::deploy::MAX_UPLOAD_SIZE_BYTES,
            )),
        )
        .route(
            "/world/{world_name}/scenes",
            get(handlers::scenes::list_scenes),
        )
        .route(
            "/world/{world_name}/scenes/{scene_coord}",
            delete(handlers::scenes::delete_scene),
        )
        .route(
            "/worlds/{world_name}/comms",
            post(handlers::comms::world_comms),
        )
        .route(
            "/worlds/{world_name}/scenes/{scene_id}/comms",
            post(handlers::comms::world_scene_comms),
        )
        .route(
            "/contents/{hash}",
            get(handlers::contents::get_content).head(handlers::contents::head_content),
        )
        .route(
            "/wallet/{wallet}/connected-world",
            get(handlers::wallet::connected_world),
        )
        .route("/live-data", get(handlers::live_data::live_data))
        .route("/livekit-webhook", post(handlers::webhook::livekit_webhook))
        .route("/admin/worlds", get(handlers::admin::list_worlds))
        .route(
            "/admin/worlds/{world_name}",
            get(handlers::admin::world_detail),
        )
        .route(
            "/admin/worlds/{world_name}/enable",
            post(handlers::admin::enable_world),
        )
        .route(
            "/admin/worlds/{world_name}/disable",
            post(handlers::admin::disable_world),
        )
        .route(
            "/admin/worlds/{world_name}/ban-status",
            get(handlers::admin::world_ban_status),
        )
        .route("/admin/blocked", get(handlers::admin::list_blocked))
        .route(
            "/admin/blocked/{wallet}",
            post(handlers::admin::block_wallet).delete(handlers::admin::unblock_wallet),
        )
        .route("/admin/access-log", get(handlers::admin::access_log))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
}
