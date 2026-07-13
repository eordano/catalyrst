use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_comms::config::Config;
use catalyrst_comms::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5138)"),
    (
        "COMMS_PG_CONNECTION_STRING",
        "required — comms-gatekeeper Postgres connection string",
    ),
    (
        "LIVEKIT_HOST",
        "LiveKit server API base (default livekit.local)",
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
        "optional — verifies /livekit-webhook signatures when set",
    ),
    (
        "LIVEKIT_TOKEN_TTL_SECS",
        "minted token TTL in seconds (default 3600)",
    ),
    (
        "PRIVATE_MESSAGES_ROOM_ID",
        "private messages room id (default private-messages)",
    ),
    (
        "PLACES_API_URL",
        "places API base URL (default http://127.0.0.1:5134)",
    ),
    (
        "CATALYST_URL",
        "catalyst base URL (default http://127.0.0.1:5140)",
    ),
    (
        "WORLD_CONTENT_URL",
        "worlds content server base URL (default https://worlds-content-server.decentraland.org)",
    ),
    (
        "LAMBDAS_URL",
        "lambdas base URL (default https://peer.decentraland.org/lambdas)",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional — dapps Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "dapps schema (default squid_marketplace)",
    ),
    (
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional — places Postgres connection string",
    ),
    (
        "AUTHORITATIVE_SERVER_ADDRESS",
        "optional — authoritative server wallet address",
    ),
    (
        "MODERATOR_TOKEN",
        "optional — bearer token for moderator endpoints",
    ),
    (
        "PLATFORM_USER_MODERATORS",
        "comma/space-separated moderator wallet addresses",
    ),
    (
        "COMMS_GATEKEEPER_AUTH_TOKEN",
        "optional — gatekeeper auth token",
    ),
    (
        "VOICE_CHAT_CONNECTION_INTERRUPTED_TTL",
        "voice connection-interrupted TTL in ms (default 300000)",
    ),
    (
        "VOICE_CHAT_INITIAL_CONNECTION_TTL",
        "voice initial-connection TTL in ms (default 300000)",
    ),
    (
        "COMMUNITY_VOICE_CHAT_NO_MODERATOR_TTL",
        "community voice no-moderator TTL in ms (default 300000)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_comms=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-comms", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_comms=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    catalyrst_comms::voice_logic::spawn_expiration_job(state.clone());

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/status", get(handlers::status::status))
        .merge(api_router(state.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-comms listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
