pub mod handlers;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::post;
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

pub struct AppStateInner {
    pub pool: PgPool,
}

pub type AppState = Arc<AppStateInner>;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: std::env::var("HTTP_SERVER_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: std::env::var("HTTP_SERVER_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5150),
            database_url: std::env::var("TELEMETRY_PG_CONNECTION_STRING")
                .context("missing TELEMETRY_PG_CONNECTION_STRING")?,
        })
    }
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid TELEMETRY_PG_CONNECTION_STRING")?
        .options([("statement_timeout", "30000")]);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect telemetry pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run telemetry migrations")?;

    Ok(Arc::new(AppStateInner { pool }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/{project}/envelope/", post(handlers::sentry::envelope))
        .route("/api/{project}/envelope", post(handlers::sentry::envelope))
        .route("/api/{project}/store/", post(handlers::sentry::store))
        .route("/api/{project}/store", post(handlers::sentry::store))
        .route("/v1/batch", post(handlers::segment::batch))
        .route("/v1/import", post(handlers::segment::batch))
        .route("/v1/track", post(handlers::segment::single))
        .route("/v1/identify", post(handlers::segment::single))
        .route("/v1/page", post(handlers::segment::single))
        .route("/v1/screen", post(handlers::segment::single))
        .route("/v1/group", post(handlers::segment::single))
        .route("/v1/alias", post(handlers::segment::single))
}
