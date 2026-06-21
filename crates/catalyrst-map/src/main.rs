use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_map::config::Config;
use catalyrst_map::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5152)"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — squid Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "squid schema (default squid_marketplace)",
    ),
    (
        "MAP_TILES_TTL_SECONDS",
        "tile refresh interval in seconds (default 60)",
    ),
    (
        "MAP_REFRESH_INTERVAL_SECS",
        "fallback name for MAP_TILES_TTL_SECONDS",
    ),
    (
        "LAND_CONTRACT_ADDRESS",
        "LAND contract (default 0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d)",
    ),
    (
        "ESTATE_CONTRACT_ADDRESS",
        "estate contract (default 0x959e104e1a4db6317fa58f8295f586e1a978c297)",
    ),
    (
        "SATELLITE_TILES_DIR",
        "satellite tiles directory (default data/satellite/0)",
    ),
    (
        "SATELLITE_SCAN_SECONDS",
        "satellite dir rescan interval in seconds (default 15)",
    ),
    (
        "SATELLITE_SOURCE_BUDGET_MB",
        "satellite source cache budget in MB (default 256)",
    ),
    (
        "SATELLITE_OUTPUT_ENTRIES",
        "satellite output cache entries (default 4096)",
    ),
    (
        "DISSOLVED_ESTATE_URL",
        "redirect target for dissolved estates (default https://ui.decentraland.org/dissolved_estate.png)",
    ),
    (
        "SIGNATURES_SERVER_URL",
        "optional — rentals signatures server base URL (enables rental listings)",
    ),
    (
        "RENTALS_SIGNATURES_SERVER_URL",
        "fallback name for SIGNATURES_SERVER_URL",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_map=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-map", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_map=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::status::ping))
        .route("/ready", get(handlers::status::ready))
        .route("/v2/ping", get(handlers::status::ping))
        .route("/v2/ready", get(handlers::status::ready))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-map listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
