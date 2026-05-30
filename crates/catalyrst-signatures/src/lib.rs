pub mod auth;
pub mod config;
pub mod db;
pub mod handlers;
pub mod http;
pub mod signature;
pub mod squid;
pub mod types;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, patch, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::db::Database;
use crate::squid::SquidMarketplace;

pub struct AppStateInner {
    pub config: Config,
    pub db: Database,
    /// Local squid marketplace reader for NFT-ownership + metadata cross-checks.
    /// `None` when `DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING` is unset (the
    /// checks then degrade to signature + DB-integrity only).
    pub squid: Option<SquidMarketplace>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid SIGNATURES_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect signatures pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run signatures migrations")?;

    let squid = match &cfg.squid_database_url {
        Some(url) => match PgConnectOptions::from_str(url) {
            Ok(opts) => {
                let opts = opts.options([("statement_timeout", "30000")]);
                match PgPoolOptions::new()
                    .max_connections(5)
                    .idle_timeout(Duration::from_secs(30))
                    .connect_with(opts)
                    .await
                {
                    Ok(squid_pool) => {
                        tracing::info!("connected to squid marketplace DB for NFT cross-checks");
                        Some(SquidMarketplace::new(squid_pool, cfg.squid_schema.clone()))
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "squid marketplace pool unavailable; NFT-ownership cross-checks disabled");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid squid connection string; NFT-ownership cross-checks disabled");
                None
            }
        },
        None => {
            tracing::info!("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING unset; NFT-ownership cross-checks disabled");
            None
        }
    };

    Ok(Arc::new(AppStateInner {
        db: Database::new(pool),
        config: cfg,
        squid,
    }))
}

pub fn api_router() -> Router<AppState> {
    let v1 = Router::new()
        .route(
            "/rentals-listings",
            post(handlers::create_rentals_listing).get(handlers::get_rentals_listings),
        )
        .route(
            "/rentals-listings/{id}",
            patch(handlers::refresh_rentals_listing),
        )
        .route(
            "/rental-listings/prices",
            get(handlers::get_rental_listings_prices),
        );

    Router::new().nest("/v1", v1)
}
