pub mod access;
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
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::bans::BansComponent;
use crate::ports::presence::PeersRegistry;
use crate::ports::worlds::WorldsComponent;
use crate::rate_limiter::RateLimiter;

pub struct AppStateInner {
    pub cfg: Config,
    pub worlds: WorldsComponent,
    pub presence: PeersRegistry,
    pub rate_limiter: RateLimiter,
    pub bans: BansComponent,
    pub http: reqwest::Client,
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

    let http = reqwest::Client::new();
    let bans = BansComponent::new(
        http.clone(),
        cfg.comms_gatekeeper_url.clone(),
        cfg.comms_gatekeeper_auth_token.clone(),
    );

    Ok(Arc::new(AppStateInner {
        worlds: WorldsComponent::new(pool),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans,
        http,
        cfg,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/world/{world_name}/about", get(handlers::about::get_about))
        .route(
            "/world/{world_name}/permissions",
            get(handlers::permissions::get_permissions),
        )
        .route(
            "/entities/active",
            post(handlers::active::active_entities),
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
}
