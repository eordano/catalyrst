#![allow(clippy::result_large_err)]

pub mod admin;
pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use moka::future::Cache;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::badges::BadgesComponent;

pub struct AppStateInner {
    pub badges: BadgesComponent,
    pub categories_cache: Cache<(), Vec<String>>,
    pub tiers_cache: Cache<String, serde_json::Value>,

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
    let opts = PgConnectOptions::from_str(&cfg.badges_database_url)
        .context("invalid BADGES_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect badges pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run badges migrations")?;

    Ok(Arc::new(AppStateInner::new(
        BadgesComponent::new(pool.clone()),
        cfg.admin_token.clone(),
    )))
}

pub fn api_router() -> Router<AppState> {
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
}
