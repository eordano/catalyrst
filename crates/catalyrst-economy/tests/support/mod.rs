#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy::hex;
use alloy::primitives::{Address, Bytes, FixedBytes};
use alloy::sol_types::SolCall;
use axum::routing::post;
use axum::Json;
use catalyrst_economy::admin::RuntimeConfig;
use catalyrst_economy::config::Config;
use catalyrst_economy::ports::abi::executeMetaTransactionCall;
use catalyrst_economy::ports::contracts::ContractsComponent;
use catalyrst_economy::ports::relayer::Relayer;
use catalyrst_economy::ports::transaction::TransactionComponent;
use catalyrst_economy::ports::upstream::UpstreamForwarder;
use catalyrst_economy::{api_router, AppStateInner};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

mod combined_sig_abi {
    alloy::sol! {
        function executeMetaTransaction(
            address userAddress,
            bytes functionData,
            bytes signature
        ) external returns (bytes);
    }
}

pub struct Scratch {
    pub pool: PgPool,
    pub schema: String,
    pub admin_url: String,
}

impl Scratch {
    pub async fn cleanup(&self) {
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP SCHEMA {} CASCADE",
                self.schema
            )))
            .execute(&admin)
            .await;
        }
    }

    pub fn scoped_url(&self) -> String {
        format!(
            "{}?options=-c%20search_path%3D{}",
            self.admin_url, self.schema
        )
    }
}

pub const PG_VAR: &str = "CATALYRST_ECONOMY_TEST_PG";

pub fn pg_url() -> Option<String> {
    catalyrst_testgate::require_pg(PG_VAR)
}

pub async fn setup_db() -> Option<Scratch> {
    let url = pg_url()?;
    let admin = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return catalyrst_testgate::pg_unusable(
                PG_VAR,
                &format!("connect to {url} failed: {e}"),
            )
        }
    };
    let schema = format!("test_economy_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));
    let scoped = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&scoped)
        .await
        .unwrap_or_else(|e| panic!("connect to scratch schema {schema} failed: {e}"));

    for sql in [
        include_str!("../../migrations/0001_transactions.sql"),
        include_str!("../../migrations/0002_broker_purchases.sql"),
        include_str!("../../migrations/0003_escrow_actions.sql"),
        include_str!("../../migrations/0004_broker_forward_confirm.sql"),
        include_str!("../../migrations/0005_add_reservation_columns.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.expect("migration");
    }

    Some(Scratch {
        pool,
        schema,
        admin_url: url,
    })
}

pub async fn seed_collection(pool: &PgPool, address: &str) {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS collection (id TEXT PRIMARY KEY)")
        .execute(pool)
        .await
        .expect("collection table");
    sqlx::query("INSERT INTO collection (id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(address.to_lowercase())
        .execute(pool)
        .await
        .expect("seed collection");
}

pub async fn row_count(pool: &PgPool, address: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE user_address = $1")
        .bind(address.to_lowercase())
        .fetch_one(pool)
        .await
        .expect("count")
}

pub fn split_sig_calldata(user_address: &str) -> String {
    let call = executeMetaTransactionCall {
        userAddress: user_address.parse::<Address>().expect("address"),
        functionSignature: Bytes::from_static(&[0xaa, 0xbb, 0xcc, 0xdd]),
        sigR: FixedBytes::<32>::repeat_byte(0x11),
        sigS: FixedBytes::<32>::repeat_byte(0x22),
        sigV: 27,
    };
    format!("0x{}", hex::encode(call.abi_encode()))
}

pub fn combined_sig_calldata(user_address: &str) -> String {
    let call = combined_sig_abi::executeMetaTransactionCall {
        userAddress: user_address.parse::<Address>().expect("address"),
        functionData: Bytes::from_static(&[0xaa, 0xbb, 0xcc, 0xdd]),
        signature: Bytes::from_static(&[0x33; 65]),
    };
    format!("0x{}", hex::encode(call.abi_encode()))
}

pub fn test_config(schema: &str, db_url: &str, relayer_url: String, max_per_day: i64) -> Config {
    Config {
        http_host: "127.0.0.1".to_string(),
        http_port: 0,
        dapps_database_url: db_url.to_string(),
        dapps_schema: schema.to_string(),
        squid_schema: schema.to_string(),
        api_version: "v1".to_string(),
        min_sale_value_in_wei: "1000000000000000000".to_string(),
        max_transactions_per_day: max_per_day,
        contract_addresses_url: "http://127.0.0.1:1/addresses.json".to_string(),
        contract_addresses_chain_key: "matic".to_string(),
        collections_chain_id: 137,
        collections_fetch_interval_ms: 3_600_000,
        rpc_url: None,
        max_gas_price_allowed_in_wei: None,
        max_gas_limit: 1_500_000,
        relayer_url: Some(relayer_url),
        relayer_id: Some("relayer-under-test".to_string()),
        relayer_api_key: Some("api-key-under-test".to_string()),
        relayer_speed: "fast".to_string(),
        relayer_max_status_checks: 3,
        relayer_sleep_ms: 10,
        meta_tx_broadcast_enabled: false,
        relayer_private_key: None,
        transactions_upstream_url: None,
        transactions_upstream_timeout_ms: 30_000,
        admin_token: None,
        landiler_escrow_address: None,
        names_chain_id: 1,
        eth_rpc_url: None,
        names_max_price_wei: None,
        receipt_poll_interval_ms: 100,
        receipt_timeout_ms: 1_000,
        broker_reconcile_interval_ms: 60_000,
        mana_usd_aggregator: Address::ZERO,
        usd_pegged_oracle_max_age_secs: 60,
        usd_pegged_slippage_bps: 100,
    }
}

pub async fn spawn_fake_relayer() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let app = axum::Router::new().route(
        "/api/v1/relayers/{relayer_id}/transactions",
        post(|| async {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            Json(serde_json::json!({
                "success": true,
                "data": {
                    "id": format!("oz-{n}"),
                    "hash": format!("0x{:064x}", n),
                    "status": "submitted"
                }
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake relayer");
    let addr = listener.local_addr().expect("relayer addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

pub async fn spawn_app(scratch: &Scratch, max_per_day: i64) -> String {
    let relayer_url = spawn_fake_relayer().await;
    let cfg = test_config(
        &scratch.schema,
        &scratch.admin_url,
        relayer_url,
        max_per_day,
    );
    let contracts = ContractsComponent::new(
        scratch.pool.clone(),
        cfg.squid_schema.clone(),
        cfg.contract_addresses_url.clone(),
        cfg.contract_addresses_chain_key.clone(),
        Duration::from_millis(cfg.collections_fetch_interval_ms),
    );
    let relayer = Relayer::from_config(&cfg);
    assert!(relayer.is_some(), "the fake OZ relayer must be provisioned");
    let runtime = RuntimeConfig::new();
    let transaction =
        TransactionComponent::new(scratch.pool.clone(), relayer, None, None, runtime.clone());
    let state = Arc::new(AppStateInner {
        config: cfg,
        pool: scratch.pool.clone(),
        transaction,
        contracts,
        eth_signer: None,
        runtime,
    });
    serve(state).await
}

/// An app with no local broadcast provider whose only route is the
/// upstream forward.
pub async fn spawn_app_upstream(
    scratch: &Scratch,
    max_per_day: i64,
    upstream_base: &str,
    timeout: Duration,
) -> String {
    let mut cfg = test_config(
        &scratch.schema,
        &scratch.admin_url,
        String::new(),
        max_per_day,
    );
    cfg.relayer_url = None;
    cfg.relayer_id = None;
    cfg.relayer_api_key = None;
    cfg.transactions_upstream_url = Some(upstream_base.to_string());
    let contracts = ContractsComponent::new(
        scratch.pool.clone(),
        cfg.squid_schema.clone(),
        cfg.contract_addresses_url.clone(),
        cfg.contract_addresses_chain_key.clone(),
        Duration::from_millis(cfg.collections_fetch_interval_ms),
    );
    let upstream = UpstreamForwarder::new(upstream_base, timeout);
    let runtime = RuntimeConfig::new();
    let transaction = TransactionComponent::new(
        scratch.pool.clone(),
        None,
        None,
        Some(upstream),
        runtime.clone(),
    );
    let state = Arc::new(AppStateInner {
        config: cfg,
        pool: scratch.pool.clone(),
        transaction,
        contracts,
        eth_signer: None,
        runtime,
    });
    serve(state).await
}

async fn serve(state: Arc<AppStateInner>) -> String {
    let app = api_router("v1").with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind app");
    let addr = listener.local_addr().expect("app addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

pub async fn post_transaction(
    base: &str,
    from: &str,
    contract: &str,
    calldata: &str,
) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/transactions"))
        .json(&serde_json::json!({
            "transactionData": { "from": from, "params": [contract, calldata] }
        }))
        .send()
        .await
        .expect("post /v1/transactions");
    let status = resp.status().as_u16();
    let body = resp.json::<serde_json::Value>().await.unwrap_or_default();
    (status, body)
}
