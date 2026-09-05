use anyhow::Result;

use catalyrst_map::config::Config;
use catalyrst_map::{build_state, service_router};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5152)"),
    ("MAP_TILES_CACHE_ENTRIES", "tile LRU capacity (default 512)"),
    ("MAP_PNG_CACHE_ENTRIES", "rendered-PNG LRU capacity (default 256)"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required \u{2014} squid Postgres connection string",
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
        "redirect target for dissolved estates (unset serves 404; no default)",
    ),
    (
        "SIGNATURES_SERVER_URL",
        "optional \u{2014} rentals signatures server base URL (enables rental listings)",
    ),
    (
        "RENTALS_SIGNATURES_SERVER_URL",
        "fallback name for SIGNATURES_SERVER_URL",
    ),
    (
        "MAP_IMAGE_BASE_URL",
        "base URL for generated tile/map images (default http://127.0.0.1:5162/v1)",
    ),
    (
        "MAP_EXTERNAL_BASE_URL",
        "optional \u{2014} marketplace page base URL for external_url; unset yields no link rather than a production one",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_map=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-map", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_map=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = service_router(state);

    catalyrst_envcfg::run_service("catalyrst-map", cfg.http_host, cfg.http_port, app).await
}
