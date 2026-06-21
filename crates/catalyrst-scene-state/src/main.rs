use anyhow::Result;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_scene_state::{api_router, build_state, Config};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5209)"),
    (
        "LOCAL_SCENE_PATH",
        "optional — path to a local scene to serve",
    ),
    (
        "WORLD_SERVER_URL",
        "optional — worlds content server to fetch scenes from",
    ),
    (
        "DEBUGGING_SECRET",
        "optional — shared secret for the debugging surface",
    ),
    (
        "CATALYRST_SCENE_STATE_ADMIN_TOKEN",
        "optional — bearer token for the admin endpoints (falls back to DEBUGGING_SECRET)",
    ),
    ("HTTP_BASE_URL", "optional — externally visible base URL"),
    (
        "AUTH_TIMEOUT_SECS",
        "websocket auth handshake timeout in seconds (default 5)",
    ),
    (
        "DISABLE_JS_RUNTIME",
        "1/true disables the JS scene runtime (default false)",
    ),
    ("REALM_NAME", "optional — realm name"),
    ("COMMIT_HASH", "reported commit hash (default empty)"),
    ("JS_HEAP_LIMIT_MB", "JS heap limit in MB (default 384)"),
    (
        "JS_TICK_BUDGET_MS",
        "per-tick JS budget in milliseconds (default 250)",
    ),
    (
        "JS_SHUTDOWN_JOIN_MS",
        "JS runtime shutdown join timeout in milliseconds (default 2000)",
    ),
    (
        "JS_UPDATE_FAILURE_CAP",
        "consecutive onUpdate throws before scene teardown (default 30)",
    ),
    (
        "CLIENT_OUTBOUND_MAX",
        "per-client outbound queue size (default 1024)",
    ),
    (
        "CLIENT_INBOUND_MAX",
        "per-client inbound queue size (default 1024)",
    ),
    ("CRDT_MAX_COMPONENTS", "CRDT component cap (default 100000)"),
    (
        "WS_MAX_FRAME_BYTES",
        "websocket max frame size in bytes (default 2097152)",
    ),
    (
        "FETCH_MAX_BODY_BYTES",
        "scene fetch download cap in bytes (default 52428800)",
    ),
    (
        "STORAGE_URL",
        "optional — world-storage origin; the ONLY origin ~system/SignedFetch may reach",
    ),
    (
        "STORAGE_ALLOW_HTTP",
        "1/true allows http STORAGE_URL for loopback hosts only (default false)",
    ),
    (
        "DELEGATION_MINTER_URL",
        "optional — catalyrst-deploy-signer --serve-delegations endpoint for minting storage delegations",
    ),
    (
        "DELEGATION_MINTER_TOKEN",
        "optional — bearer token for the delegation minter",
    ),
    (
        "STORAGE_DELEGATION",
        "optional — pre-minted base64 delegation envelope (dev/local; disables renewal)",
    ),
    (
        "SIGNED_FETCH_MAX_RESPONSE_BYTES",
        "SignedFetch response body cap in bytes (default 2097152)",
    ),
    (
        "SIGNED_FETCH_MAX_BODY_BYTES",
        "SignedFetch request body cap in bytes (default 1048576)",
    ),
    (
        "SIGNED_FETCH_MAX_IN_FLIGHT",
        "concurrent SignedFetch requests per scene (default 8)",
    ),
    (
        "SIGNED_FETCH_TIMEOUT_MS",
        "SignedFetch request timeout in milliseconds (default 10000)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_scene_state=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-scene-state", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_scene_state=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let state = build_state(&cfg).await?;

    let app = api_router()
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!(%addr, "catalyrst-scene-state listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
