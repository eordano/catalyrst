use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_worlds::config::Config;
use catalyrst_worlds::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5146)"),
    ("PREVIEW_WEARABLE_URNS", "comma/space separated urns offered in preview"),
    (
        "WORLDS_PG_CONNECTION_STRING",
        "required \u{2014} worlds Postgres connection string",
    ),
    (
        "HTTP_BASE_URL",
        "public base URL of this server (default http://127.0.0.1:<port>)",
    ),
    ("NETWORK_ID", "L1 chain id advertised in /about (default 1)"),
    (
        "SQUID_PG_CONNECTION_STRING",
        "optional \u{2014} squid Postgres connection string for NAME ownership checks",
    ),
    ("GLOBAL_SCENES_URN", "optional \u{2014} global scenes URN"),
    (
        "CONTENT_PUBLIC_URL",
        "catalyst content public URL (REQUIRED; no default)",
    ),
    (
        "LAMBDAS_PUBLIC_URL",
        "catalyst lambdas public URL (REQUIRED; no default)",
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
        "required with LIVEKIT_API_SECRET unless LIVEKIT_ALLOW_DEV_CREDS=1 (devkey/devsecret placeholders count as unset)",
    ),
    (
        "LIVEKIT_API_SECRET",
        "required with LIVEKIT_API_KEY unless LIVEKIT_ALLOW_DEV_CREDS=1 (devkey/devsecret placeholders count as unset)",
    ),
    (
        "LIVEKIT_ALLOW_DEV_CREDS",
        "bool \u{2014} allow booting with devkey/devsecret when LiveKit creds are unset or placeholders (default false)",
    ),
    (
        "LIVEKIT_WEBHOOK_KEY",
        "optional \u{2014} verifies LiveKit webhook signatures when set",
    ),
    ("MAX_USERS_PER_WORLD", "max users per world (default 100)"),
    (
        "WORLDS_COMMS_OFFLINE_WHEN_UNREACHABLE",
        "serve offline:offline while the SFU is unreachable so entry still works (default 1)",
    ),
    (
        "WORLDS_REALM_NAME_STRIP_ENS",
        "drop the .dcl.eth suffix from the realm name of locally published worlds (default 1)",
    ),
    (
        "WORLDS_CONTENT_DIR",
        "local contents directory (default ./data/worlds/contents)",
    ),
    (
        "CONTENTS_UPSTREAM_URL",
        "upstream for /contents proxy reads (unset serves 404 on local misses; no default)",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "optional \u{2014} comms gatekeeper base URL",
    ),
    (
        "COMMS_GATEKEEPER_AUTH_TOKEN",
        "optional \u{2014} comms gatekeeper auth token",
    ),
    ("DENYLIST_JSON_URL", "optional \u{2014} denylist JSON URL"),
    ("DCL_LISTS_URL", "optional \u{2014} dcl-lists base URL"),
    (
        "CATALYRST_WORLDS_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding admin endpoints",
    ),
    (
        "MAX_IN_FLIGHT_UPLOAD_BYTES",
        "max in-flight upload bytes (default 4294967296)",
    ),
    (
        "MAX_CONCURRENT_UPLOADS",
        "max simultaneous multipart uploads (default 40)",
    ),
    (
        "MAX_IN_FLIGHT_UPLOAD_FILES",
        "max aggregate buffered upload files (default 40000)",
    ),
    (
        "MULTIPART_UPLOAD_TIMEOUT_MS",
        "deadline for receiving+parsing a multipart body (default 300000)",
    ),
    (
        "DEPLOYMENT_PROCESSING_TIMEOUT_MS",
        "deadline for post-body deployment processing (default 300000)",
    ),
    // Worlds federation. Unsetting WORLDS_FED_PEERS_FILE is how federation is turned
    // off, and it is the only way: an empty peer file means "federation is on and we
    // admit nobody", which is a different state that the mirror deliberately preserves.
    (
        "WORLDS_FED_PEERS_FILE",
        "path to the DAO-cited federation peer file; unset disables worlds federation \
         entirely (an EMPTY file instead means 'on, admitting nobody')",
    ),
    (
        "WORLDS_FED_POLL_INTERVAL_SECS",
        "how often each admitted peer's world listing is re-fetched, >= 1 (default 300)",
    ),
    (
        "WORLDS_FED_MAX_RESPONSE_BYTES",
        "cap on a peer's listing body, refused while streaming so it is never fully \
         buffered, >= 1 (default 4194304)",
    ),
    (
        "WORLDS_FED_MAX_WORLDS_PER_PEER",
        "cap on rows mirrored from one peer, >= 1 (default 10000)",
    ),
    (
        "WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS",
        "DEV ONLY: admit an http://127.0.0.1 peer with no pinned root, spoken to in \
         cleartext and authenticated by nothing (default off). Never set off a test box",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_worlds=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-worlds", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_worlds=info,tower_http=info");

    let cfg = Config::from_env()?;
    let http_host = cfg.http_host.clone();
    let http_port = cfg.http_port;

    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::status::ping))
        .route("/health", get(handlers::status::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-worlds", http_host, http_port, app).await
}
