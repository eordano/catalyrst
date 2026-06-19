pub mod auth;
pub mod config;
pub mod handlers;
pub mod ports;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, put};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::overrides::OverridesComponent;
use crate::ports::prices::PricesComponent;

pub struct AppStateInner {
    pub prices: PricesComponent,
    pub overrides: OverridesComponent,
    /// Bearer token gating the admin price-override mutations. `None` ⇒ those
    /// routes fail closed (403).
    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v3/simple/price",
            get(handlers::simple_price::simple_price),
        )
        // Admin price-override config store (docs/admin-console.md §4).
        // GET is a public read of the store; PUT/DELETE are bearer-gated.
        .route(
            "/admin/api/price/overrides",
            get(handlers::overrides::list_overrides),
        )
        .route(
            "/admin/api/price/overrides/{token}/{vs}",
            put(handlers::overrides::set_override).delete(handlers::overrides::clear_override),
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

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("price-override migration failed")?;

    Ok(Arc::new(AppStateInner {
        prices: PricesComponent::new(pool.clone()),
        overrides: OverridesComponent::new(pool),
        admin_token: cfg.admin_token.clone(),
    }))
}
