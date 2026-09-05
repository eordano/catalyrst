#![allow(clippy::result_large_err)]

pub mod admin;
pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use moka::future::Cache;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::ports::badges::BadgesComponent;
use crate::ports::types::TierData;

pub struct AppStateInner {
    pub badges: BadgesComponent,
    pub categories_cache: Cache<(), Vec<String>>,
    pub tiers_cache: Cache<String, Vec<TierData>>,

    pub admin_token: Option<String>,
}

impl AppStateInner {
    pub fn new(badges: BadgesComponent, admin_token: Option<String>) -> Self {
        Self {
            badges,
            admin_token,
            categories_cache: Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(300))
                .build(),
            tiers_cache: Cache::builder()
                .max_capacity(512)
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = catalyrst_db::connect_pool(
        &cfg.badges_database_url,
        &catalyrst_db::PoolSettings::default(),
    )
    .await
    .context("failed to connect badges pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run badges migrations")?;

    Ok(Arc::new(AppStateInner::new(
        BadgesComponent::new(pool.clone(), cfg.public_asset_base_url.clone()),
        cfg.admin_token.clone(),
    )))
}

pub fn app_router(cfg: &Config) -> Router<AppState> {
    Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router(cfg))
        .layer(CorsLayer::permissive())
}

/// `cfg` only supplies `assets_dir` for the `/assets` ServeDir mount -- every
/// other route is state-only. Takes `&Config` (not `AppState`) so callers
/// that merge this into a larger router (the `catalyrst-social` bundle) can
/// build it right after `Config::from_env()`, before `AppState` exists.
pub fn api_router(cfg: &Config) -> Router<AppState> {
    Router::new()
        .route("/categories", get(handlers::badges::get_categories))
        .route(
            "/users/{address}/preview",
            get(handlers::badges::get_user_preview),
        )
        .route(
            "/users/{address}/badges",
            get(handlers::badges::get_user_badges),
        )
        .route(
            "/badges/{badge_id}/tiers",
            get(handlers::badges::get_badge_tiers),
        )
        .route(
            "/users/{address}/badges/{badge_id}",
            post(handlers::badges::grant_user_badge).delete(handlers::badges::revoke_user_badge),
        )
        .nest_service("/assets", ServeDir::new(&cfg.assets_dir))
}
