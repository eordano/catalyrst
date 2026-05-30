pub mod config;
pub mod handlers;
pub mod ports;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::prices::PricesComponent;

pub struct AppStateInner {
    pub prices: PricesComponent,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new().route(
        "/api/v3/simple/price",
        get(handlers::simple_price::simple_price),
    )
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.price_database_url)
        .context("invalid PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect mana_price pool")?;

    Ok(Arc::new(AppStateInner {
        prices: PricesComponent::new(pool),
    }))
}
