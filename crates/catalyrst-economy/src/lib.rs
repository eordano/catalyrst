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

    pub eth_signer: Option<DirectSigner>,

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
        .route("/payments/config", get(handlers::payments::config))
        .route("/payments/nonce/{address}", get(handlers::payments::nonce))
        .route("/payments/verify", post(handlers::payments::verify))
        .route("/broker/buy", post(handlers::broker::buy))
        .route(
            "/admin/collections/approve-mana",
            post(handlers::broker::approve_mana_collections),
        )
        .route("/broker/names/buy", post(handlers::names::buy))
        .route("/broker/names/transfer", post(handlers::names::transfer))
        .route(
            "/admin/names/approve-mana",
            post(handlers::names::approve_mana),
        )
        .route("/escrow/reclaim", post(handlers::escrow::reclaim))
        .route("/escrow/release", post(handlers::escrow::release))
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

    sqlx::raw_sql(include_str!("../migrations/0002_broker_purchases.sql"))
        .execute(&pool)
        .await
        .context("broker_purchases table init failed")?;

    sqlx::raw_sql(include_str!("../migrations/0003_escrow_actions.sql"))
        .execute(&pool)
        .await
        .context("escrow_actions table init failed")?;

    sqlx::raw_sql(include_str!(
        "../migrations/0004_broker_forward_confirm.sql"
    ))
    .execute(&pool)
    .await
    .context("broker_purchases forward/confirm migration failed")?;

    sqlx::raw_sql(include_str!(
        "../migrations/0005_add_reservation_columns.sql"
    ))
    .execute(&pool)
    .await
    .context("transactions reservation-columns migration failed")?;

    sqlx::raw_sql(include_str!("../migrations/0006_name_transfers.sql"))
        .execute(&pool)
        .await
        .context("name_transfers table init failed")?;

    sqlx::raw_sql(include_str!("../migrations/0007_broker_trades.sql"))
        .execute(&pool)
        .await
        .context("broker_purchases trade migration failed")?;

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
    let eth_signer = DirectSigner::eth_from_config(&cfg).map_err(anyhow::Error::msg)?;
    match &eth_signer {
        Some(s) => tracing::info!(
            relayer = %s.relayer_address(),
            chain_id = cfg.names_chain_id,
            "NAMEs chain signer enabled; ensure the relayer address holds ETH for gas and MANA on Ethereum"
        ),
        None => tracing::info!(
            "NAMEs chain signer not provisioned (set ETH_RPC_URL to enable /broker/names endpoints)"
        ),
    }

    let runtime = RuntimeConfig::new();
    let reconcile_interval = Duration::from_millis(cfg.broker_reconcile_interval_ms.max(1));
    let transaction = TransactionComponent::new(pool.clone(), relayer, signer, runtime.clone());

    let state = Arc::new(AppStateInner {
        config: cfg,
        pool,
        transaction,
        contracts,
        eth_signer,
        runtime,
    });

    crate::ports::reconcile::spawn_broker_reconciler(state.clone(), reconcile_interval);

    Ok(state)
}
