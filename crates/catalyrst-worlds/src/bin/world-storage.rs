use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_worlds::world_storage::config::Config;
use catalyrst_worlds::world_storage::{api_router, build_state, cors, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5151)"),
    (
        "WORLD_STORAGE_PG_CONNECTION_STRING",
        "required \u{2014} world-storage Postgres connection string",
    ),
    (
        "ENCRYPTION_KEY",
        "required \u{2014} 64 hex characters (32 bytes) for AES-GCM env-value encryption",
    ),
    (
        "CORS_ALLOWED_ORIGIN_SUFFIXES",
        "comma-separated host suffixes granted CORS; matched against the Origin host on label boundaries, https-only; no-Origin (server-to-server) requests are unaffected (default decentraland.org,decentraland.zone,decentraland.today)",
    ),
    (
        "AUTHORITATIVE_SERVER_ADDRESS",
        "optional \u{2014} signer address always authorized (bypasses owner/deployer checks)",
    ),
    (
        "AUTHORIZED_ADDRESSES",
        "optional \u{2014} comma-separated signer addresses always authorized",
    ),
    (
        "RPC_ENDPOINT_ETH",
        "EIP-1654 signature-validation RPC (unset rejects contract-wallet signatures; no default)",
    ),
    (
        "WORLDS_CONTENT_SERVER_URL",
        "worlds content server (default http://127.0.0.1:5142)",
    ),
    (
        "LAMBDAS_URL",
        "catalyst lambdas (REQUIRED; no default)",
    ),
    (
        "PLACES_URL",
        "places API (default http://127.0.0.1:5134)",
    ),
    (
        "PLACES_CACHE_TTL_SECONDS",
        "places lookup cache TTL in seconds (default 300)",
    ),
    (
        "WORLD_PERMISSIONS_CACHE_TTL_SECONDS",
        "world permission lookup cache TTL in seconds (default 30)",
    ),
    (
        "STORAGE_CACHE_ENABLED",
        "single-key storage read cache (default true)",
    ),
    (
        "STORAGE_CACHE_TTL_SECONDS",
        "storage cache TTL \u{2014} bounds staleness on replicas that did not handle the write (default 60)",
    ),
    (
        "STORAGE_CACHE_MAX",
        "storage cache max entries, LRU-evicted (default 8000)",
    ),
    (
        "STORAGE_CACHE_MAX_VALUE_BYTES",
        "values larger than this are not cached (default 32768)",
    ),
    (
        "ENV_STORAGE_MAX_VALUE_SIZE_BYTES",
        "env namespace per-value cap (default 10240)",
    ),
    (
        "ENV_STORAGE_MAX_TOTAL_SIZE_BYTES",
        "env namespace total cap (default 262144)",
    ),
    (
        "WORLD_STORAGE_MAX_VALUE_SIZE_BYTES",
        "world namespace per-value cap (default 524288)",
    ),
    (
        "WORLD_STORAGE_MAX_TOTAL_SIZE_BYTES",
        "world namespace total cap (default 10485760)",
    ),
    (
        "PLAYER_STORAGE_MAX_VALUE_SIZE_BYTES",
        "player namespace per-value cap (default 102400)",
    ),
    (
        "PLAYER_STORAGE_MAX_TOTAL_SIZE_BYTES",
        "player namespace total cap (default 1048576)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_worlds=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-world-storage", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_worlds=info,tower_http=info");

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;
    let cors = cors::cors_layer(cfg.cors_allowed_origin_suffixes.clone());
    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-world-storage", host, port, app).await
}
