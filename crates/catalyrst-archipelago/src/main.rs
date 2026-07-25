use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_archipelago::{build_state, handlers, ws, Config};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5139)"),
    (
        "ARCHIPELAGO_CONFIG_PATH",
        "optional TOML config file with cluster/server/auth/livekit/gossip sections",
    ),
    (
        "ARCHIPELAGO_REQUIRE_AUTH",
        "1/true — require a signed challenge on websocket connect (overrides config file)",
    ),
    (
        "LIVEKIT_API_KEY",
        "livekit API key (used when the config file does not set one)",
    ),
    (
        "LIVEKIT_API_SECRET",
        "livekit API secret (used when the config file does not set one)",
    ),
    ("LIVEKIT_WS_URL", "livekit websocket URL override"),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (used when the config file does not set one)",
    ),
    (
        "DENY_LIST_URL",
        "denylist JSON URL (default https://config.decentraland.org/denylist.json; empty disables)",
    ),
    ("ARCHIPELAGO_NODE_ID", "gossip node id"),
    (
        "ARCHIPELAGO_GOSSIP_PEERS",
        "comma-separated gossip peer URLs",
    ),
    ("ARCHIPELAGO_GOSSIP_HMAC_KEY", "gossip HMAC signing key"),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional — catalyst content DB connection string",
    ),
    (
        "POSTGRES_CONTENT_USER",
        "content DB user (enables the pieced-together connection when CONTENT_PG_CONNECTION_STRING is unset)",
    ),
    ("POSTGRES_CONTENT_PASSWORD", "content DB password"),
    ("POSTGRES_CONTENT_DB", "content DB name (default content)"),
    ("POSTGRES_HOST", "content DB host (default ./data/run)"),
    ("POSTGRES_PORT", "content DB port (default 6432)"),
    (
        "CONTENT_BASE_URL",
        "content server base URL (default https://peer.decentraland.org/content)",
    ),
    ("COMMIT_HASH", "build commit reported by status endpoints"),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_archipelago=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-archipelago", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_archipelago=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .merge(handlers::routes())
        .merge(ws::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-archipelago listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
