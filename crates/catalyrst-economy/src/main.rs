use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_economy::config::Config;
use catalyrst_economy::handlers;
use catalyrst_economy::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    (
        "HTTP_SERVER_HOST",
        "bind address (default 127.0.0.1; non-loopback refuses to start without CATALYRST_ECONOMY_ADMIN_TOKEN)",
    ),
    ("HTTP_SERVER_PORT", "listen port (default 5155)"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — dapps Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "dapps schema (default marketplace)",
    ),
    (
        "SQUID_PG_COMPONENT_PSQL_SCHEMA",
        "squid schema (default squid_marketplace)",
    ),
    ("API_VERSION", "API path version segment (default v1)"),
    (
        "MIN_SALE_VALUE_IN_WEI",
        "minimum sale value in wei (default 1000000000000000000)",
    ),
    (
        "MAX_TRANSACTIONS_PER_DAY",
        "per-address daily meta-tx cap (default 10)",
    ),
    (
        "CONTRACT_ADDRESSES_URL",
        "contract addresses JSON (default https://contracts.decentraland.org/addresses.json)",
    ),
    (
        "CONTRACT_ADDRESSES_CHAIN_KEY",
        "chain key inside the addresses JSON (default matic)",
    ),
    ("COLLECTIONS_CHAIN_ID", "collections chain id (default 137)"),
    (
        "COLLECTIONS_FETCH_INTERVAL_MS",
        "collections refresh interval in ms (default 3600000)",
    ),
    ("RPC_URL", "optional — Polygon JSON-RPC endpoint"),
    (
        "MAX_GAS_PRICE_ALLOWED_IN_WEI",
        "optional — gas price ceiling in wei",
    ),
    ("MAX_GAS_LIMIT", "gas limit cap (default 1500000)"),
    (
        "OZ_RELAYER_URL",
        "optional — OpenZeppelin relayer endpoint (relayer active only with URL + ID + API key)",
    ),
    ("OZ_RELAYER_ID", "optional — OpenZeppelin relayer id"),
    ("OZ_RELAYER_API_KEY", "optional — OpenZeppelin relayer API key"),
    ("OZ_RELAYER_SPEED", "relayer speed (default fast)"),
    (
        "OZ_MAX_STATUS_CHECKS",
        "relayer status-check attempts (default 150)",
    ),
    (
        "OZ_SLEEP_TIME_BETWEEN_CHECKS_MS",
        "sleep between relayer status checks in ms (default 800)",
    ),
    (
        "META_TX_BROADCAST_ENABLED",
        "bool — enable meta-tx broadcast (default false)",
    ),
    ("RELAYER_PRIVATE_KEY", "optional — local relayer signing key"),
    (
        "CATALYRST_ECONOMY_ADMIN_TOKEN",
        "optional — bearer token guarding the admin/broker endpoints",
    ),
    (
        "LANDILER_ESCROW_ADDRESS",
        "optional — LandilerEscrow contract address",
    ),
    ("NAMES_CHAIN_ID", "names chain id (default 1)"),
    (
        "ETH_RPC_URL",
        "optional — Ethereum JSON-RPC endpoint for names",
    ),
    (
        "NAMES_MAX_PRICE_WEI",
        "optional — max name price in wei (decimal integer)",
    ),
    (
        "RECEIPT_POLL_INTERVAL_MS",
        "receipt poll interval in ms (default 3000)",
    ),
    (
        "RECEIPT_TIMEOUT_MS",
        "receipt wait timeout in ms (default 180000)",
    ),
    (
        "BROKER_RECONCILE_INTERVAL_MS",
        "broker reconcile interval in ms (default 60000)",
    ),
    (
        "MANA_USD_AGGREGATOR_ADDRESS",
        "Chainlink MANA/USD aggregator for USD-pegged (assetType 2) trades \
         (default 0xA1CbF3Fe43BC3501e3Fc4b573e822c70e76A7512, Polygon mainnet)",
    ),
    (
        "USD_PEGGED_ORACLE_MAX_AGE_SECS",
        "max age of the MANA/USD oracle round before a USD-pegged trade is refused \
         (default 60; the on-chain contract enforces its own 27s tolerance at execution)",
    ),
    (
        "USD_PEGGED_SLIPPAGE_BPS",
        "max drift in bps between the execution-time MANA charge and the caller's \
         listing-time quoteManaWei before a USD-pegged trade is refused (default 100)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_economy=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-economy", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_economy=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;
    let api_version = cfg.api_version.clone();

    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::health::health))
        .merge(api_router(&api_version))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-economy listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
