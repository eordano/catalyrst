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
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::admin::RuntimeConfig;
use crate::config::Config;
use crate::ports::contracts::ContractsComponent;
use crate::ports::relayer::Relayer;
use crate::ports::signer::DirectSigner;
use crate::ports::transaction::TransactionComponent;

pub struct AppStateInner {
    pub config: Config,
    pub pool: PgPool,
    pub transaction: TransactionComponent,
    pub contracts: ContractsComponent,
    /// Runtime-mutable relayer controls (admin toggle + signer switch).
    pub runtime: Arc<RuntimeConfig>,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router(api_version: &str) -> Router<AppState> {
    let api_prefix = format!("/{}", api_version);
    let api = Router::new()
        .route(
            "/transactions",
            post(handlers::transactions::send_transaction),
        )
        .route(
            "/transactions/{user_address}",
            get(handlers::transactions::get_user_transactions),
        )
        .route(
            "/contracts/{address}",
            get(handlers::contracts::contracts_address),
        )
        // Bearer-gated runtime relayer controls (docs/admin-console.md §4).
        .route("/admin/relayer", get(handlers::admin::relayer_status))
        .route(
            "/admin/relayer/toggle",
            post(handlers::admin::relayer_toggle),
        )
        .route(
            "/admin/relayer/signer",
            post(handlers::admin::relayer_signer),
        );
    Router::new().nest(&api_prefix, api)
}

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let search_path = format!("{},public", cfg.dapps_schema);
    let opts = PgConnectOptions::from_str(&cfg.dapps_database_url)
        .context("invalid DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
            ("search_path", search_path.as_str()),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect marketplace_squid pool")?;

    sqlx::raw_sql(include_str!("../migrations/0001_transactions.sql"))
        .execute(&pool)
        .await
        .context("transactions table init failed")?;

    let contracts = ContractsComponent::new(
        pool.clone(),
        cfg.squid_schema.clone(),
        cfg.contract_addresses_url.clone(),
        cfg.contract_addresses_chain_key.clone(),
        Duration::from_millis(cfg.collections_fetch_interval_ms),
    );
    let relayer = Relayer::from_config(&cfg);
    let signer = DirectSigner::from_config(&cfg).map_err(anyhow::Error::msg)?;
    match (&relayer, &signer) {
        (Some(_), _) => {
            tracing::info!("OZ relayer provisioned; meta-transaction broadcast via OZ Defender")
        }
        (None, Some(s)) => tracing::info!(
            relayer = %s.relayer_address(),
            chain_id = cfg.collections_chain_id,
            "direct JSON-RPC broadcast enabled; ensure the relayer address is funded for gas"
        ),
        (None, None) => tracing::warn!(
            "no broadcast provider provisioned; POST /transactions validates + returns 503 on broadcast"
        ),
    }
    let runtime = RuntimeConfig::new();
    let transaction = TransactionComponent::new(pool.clone(), relayer, signer, runtime.clone());

    Ok(Arc::new(AppStateInner {
        config: cfg,
        pool,
        transaction,
        contracts,
        runtime,
    }))
}
