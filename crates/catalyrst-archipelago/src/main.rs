use anyhow::Result;
use tower_http::trace::TraceLayer;

use catalyrst_archipelago::{api_router, build_state, Config};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5139)"),
    (
        "ARCHIPELAGO_CONFIG_PATH",
        "optional TOML config file with cluster/server/auth/livekit/gossip sections",
    ),
    (
        "ARCHIPELAGO_REQUIRE_AUTH",
        "default 1 \u{2014} a signed challenge is required; 0/false/no accepts unsigned POST /heartbeat presence writes for any address (development only, overrides config file)",
    ),
    (
        "LIVEKIT_API_KEY",
        "livekit API key (used when the config file does not set one; the devkey placeholder counts as unset)",
    ),
    (
        "LIVEKIT_API_SECRET",
        "livekit API secret (used when the config file does not set one; the devsecret placeholder counts as unset)",
    ),
    ("LIVEKIT_WS_URL", "livekit websocket URL override"),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (used when the config file does not set one)",
    ),
    (
        "DENY_LIST_URL",
        "denylist JSON URL (unset/empty disables the denylist; no default)",
    ),
    ("ARCHIPELAGO_NODE_ID", "gossip node id"),
    (
        "ARCHIPELAGO_GOSSIP_PEERS",
        "comma-separated gossip peer URLs",
    ),
    ("ARCHIPELAGO_GOSSIP_HMAC_KEY", "gossip HMAC signing key"),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional \u{2014} catalyst content DB connection string",
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
        "content server base URL (default http://127.0.0.1:5141)",
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

    catalyrst_envcfg::init_tracing("catalyrst_archipelago=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = api_router()
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-archipelago", cfg.http_host, cfg.http_port, app).await
}
