use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_worlds::config::Config;
use catalyrst_worlds::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5146)"),
    (
        "WORLDS_PG_CONNECTION_STRING",
        "required — worlds Postgres connection string",
    ),
    (
        "HTTP_BASE_URL",
        "public base URL of this server (default http://127.0.0.1:<port>)",
    ),
    ("NETWORK_ID", "L1 chain id advertised in /about (default 1)"),
    (
        "SQUID_PG_CONNECTION_STRING",
        "optional — squid Postgres connection string for NAME ownership checks",
    ),
    ("GLOBAL_SCENES_URN", "optional — global scenes URN"),
    (
        "CONTENT_PUBLIC_URL",
        "catalyst content public URL (default https://peer.decentraland.org/content)",
    ),
    (
        "LAMBDAS_PUBLIC_URL",
        "catalyst lambdas public URL (default https://peer.decentraland.org/lambdas)",
    ),
    (
        "LIVEKIT_HOST",
        "LiveKit server API base (default livekit.local)",
    ),
    (
        "LIVEKIT_WS_URL",
        "client-facing LiveKit signaling URL (default wss://<LIVEKIT_HOST>)",
    ),
    (
        "LIVEKIT_API_KEY",
        "required with LIVEKIT_API_SECRET unless LIVEKIT_ALLOW_DEV_CREDS=1",
    ),
    (
        "LIVEKIT_API_SECRET",
        "required with LIVEKIT_API_KEY unless LIVEKIT_ALLOW_DEV_CREDS=1",
    ),
    (
        "LIVEKIT_ALLOW_DEV_CREDS",
        "bool — allow booting with devkey/devsecret when LiveKit creds are unset (default false)",
    ),
    (
        "LIVEKIT_WEBHOOK_KEY",
        "optional — verifies LiveKit webhook signatures when set",
    ),
    (
        "MAX_USERS_PER_WORLD",
        "max users per world (default 100)",
    ),
    (
        "WORLDS_CONTENT_DIR",
        "local contents directory (default ./data/worlds/contents)",
    ),
    (
        "CONTENTS_UPSTREAM_URL",
        "upstream for /contents proxy reads (default https://worlds-content-server.decentraland.org)",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "optional — comms gatekeeper base URL",
    ),
    (
        "COMMS_GATEKEEPER_AUTH_TOKEN",
        "optional — comms gatekeeper auth token",
    ),
    ("DENYLIST_JSON_URL", "optional — denylist JSON URL"),
    ("DCL_LISTS_URL", "optional — dcl-lists base URL"),
    (
        "CATALYRST_WORLDS_ADMIN_TOKEN",
        "optional — bearer token guarding admin endpoints",
    ),
    (
        "MAX_IN_FLIGHT_UPLOAD_BYTES",
        "max in-flight upload bytes (default 4294967296)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_worlds=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-worlds", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_worlds=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let http_host = cfg.http_host.clone();
    let http_port = cfg.http_port;

    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::status::ping))
        .route("/status", get(handlers::status::status))
        .route("/health", get(handlers::status::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", http_host, http_port).parse()?;
    tracing::info!(%addr, "catalyrst-worlds listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
