pub mod backend;
pub mod cache;
pub mod config;
pub mod handlers;
pub mod http;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::backend::http::HttpBackend;
use crate::backend::mock::MockBackend;
use crate::backend::TranslationBackend;
use crate::config::{BackendKind, Config};

pub struct AppStateInner {
    pub pool: PgPool,
    pub backend: Arc<dyn TranslationBackend>,
    pub backend_label: &'static str,
    pub fetch_client: reqwest::Client,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid MEDIA_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect content pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let backend: Arc<dyn TranslationBackend> = match cfg.backend_kind {
        BackendKind::Mock => Arc::new(MockBackend),
        BackendKind::Http => Arc::new(HttpBackend::new(
            cfg.backend_url
                .clone()
                .expect("backend url checked in config"),
            cfg.backend_api_key.clone(),
        )),
    };

    let fetch_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent("catalyrst-media-converter/0.1")
        // Redirects are followed MANUALLY in the /convert handler so the SSRF
        // host guard re-runs on every hop (auto-following would skip the check).
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build fetch client")?;

    Ok(Arc::new(AppStateInner {
        pool,
        backend,
        backend_label: cfg.backend_kind.label(),
        fetch_client,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/translate", post(handlers::translate::translate))
        .route("/convert", get(handlers::convert::convert))
        .route("/media/convert", get(handlers::convert::convert))
}
