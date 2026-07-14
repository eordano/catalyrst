#![allow(clippy::result_large_err)]

pub mod auth;
pub mod config;
pub mod handlers;
pub mod poller;
pub mod ports;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::{get, put};
use axum::Router;

use crate::config::Config;
use crate::ports::overrides::OverridesComponent;
use crate::ports::prices::PricesComponent;

pub struct AppStateInner {
    pub prices: PricesComponent,
    pub overrides: OverridesComponent,

    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v3/simple/price",
            get(handlers::simple_price::simple_price),
        )
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
    let pool = catalyrst_db::connect_pool(
        &cfg.price_database_url,
        &catalyrst_db::PoolSettings {
            max_connections: 5,
            ..catalyrst_db::PoolSettings::default()
        },
    )
    .await
    .context("failed to connect mana_price pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("price-override migration failed")?;

    if cfg.price_poll_enabled {
        poller::spawn(pool.clone(), cfg);
    } else {
        tracing::warn!(
            "PRICE_POLL_ENABLED=false: NOT polling -- this crate only serves the last \
             price_snapshots row. If no external ingester keeps it fresh, the MANA/USD \
             reading will go stale and downstream credits /checkout will fail-close \
             (\"MANA/USD oracle is stale\"). Set PRICE_POLL_ENABLED=true to self-refresh."
        );
    }

    Ok(Arc::new(AppStateInner {
        prices: PricesComponent::new(pool.clone()),
        overrides: OverridesComponent::new(pool),
        admin_token: cfg.admin_token.clone(),
    }))
}
