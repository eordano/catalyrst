pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::post;
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::lists::ListsComponent;

pub struct AppStateInner {
    pub lists: ListsComponent,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect places_events pool")?;

    Ok(Arc::new(AppStateInner {
        lists: ListsComponent::new(pool),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/pois", post(handlers::pois::post_pois))
        .route(
            "/banned-names",
            post(handlers::banned_names::post_banned_names),
        )
}
