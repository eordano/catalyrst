use anyhow::Result;
use axum::routing::get;
use axum::Router;

use catalyrst_comms::config::Config;
use catalyrst_comms::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5138)"),
    (
        "COMMS_PG_CONNECTION_STRING",
        "required \u{2014} comms-gatekeeper Postgres connection string",
    ),
    (
        "LIVEKIT_HOST",
        "LiveKit host; both the client signaling URL and the API base derive from it unless overridden (default livekit.local)",
    ),
    (
        "LIVEKIT_API_HOST",
        "optional \u{2014} API base this service calls for room admin/ingress/participants when it differs from the host explorers dial (default: LIVEKIT_HOST as an http URL -- http:// for a ws/http host, https:// otherwise)",
    ),
    (
        "LIVEKIT_WS_URL",
        "optional \u{2014} client-facing LiveKit signaling URL when it differs from the API side (default: LIVEKIT_HOST as a websocket URL -- ws:// for a ws/http host, wss:// otherwise)",
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
        "optional \u{2014} verifies /livekit-webhook signatures when set",
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
        "catalyst content-core base URL (default http://127.0.0.1:5141)",
    ),
    (
        "WORLD_CONTENT_URL",
        "worlds content server base URL (default http://127.0.0.1:5142)",
    ),
    ("LAMBDAS_URL", "lambdas base URL (REQUIRED; no default)"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional \u{2014} dapps Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "dapps schema (default squid_marketplace)",
    ),
    (
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional \u{2014} places Postgres connection string",
    ),
    (
        "AUTHORITATIVE_SERVER_ADDRESS",
        "optional \u{2014} authoritative server wallet address",
    ),
    (
        "MODERATOR_TOKEN",
        "optional \u{2014} bearer token for moderator endpoints",
    ),
    (
        "PLATFORM_USER_MODERATORS",
        "comma/space-separated moderator wallet addresses",
    ),
    (
        "COMMS_GATEKEEPER_AUTH_TOKEN",
        "optional \u{2014} gatekeeper auth token",
    ),
    (
        "PULSE_ROOM_AUTHORITY_KEY",
        "optional dedicated 32-byte base64url HMAC key; enables bounded Pulse room authorization; never reuse the LiveKit secret",
    ),
    (
        "FED_PEER_ID",
        "stable federation peer id stamped as MLS epoch_author (default: a random per-instance id persisted in the comms DB)",
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
        "NATS_URL",
        "optional \u{2014} broker the Pulse cluster feed arrives on; unset leaves the subscriber inert (shared platform-wide with catalyrst-pulse)",
    ),
    (
        "CLUSTER_SUBSCRIBER_ENABLED",
        "bool string \u{2014} `true` subscribes to the Pulse cluster feed; anything else subscribes to nothing (default false)",
    ),
    (
        "NATS_QUEUE_GROUP",
        "queue group the minting and connect subscriptions share, so exactly one replica answers each event (default catalyrst-comms-cluster)",
    ),
    (
        "CLUSTER_TAKEOVER_RETRY_DELAY_MS",
        "base delay between the three displaced-session removal attempts, multiplied by the attempt; 0 is a real value meaning no sleep (default 100)",
    ),
    (
        "CLUSTER_DRAIN_TIMEOUT_MS",
        "ceiling on the shutdown drain of in-flight cluster work; 0 is a real value meaning do not wait at all (default 5000)",
    ),
    (
        "CLUSTER_ISLAND_TOKEN_TTL_SECONDS",
        "lifetime of an island room token, capped at 60 seconds so self-hosted takeover quarantine is bounded (default 60)",
    ),
    (
        "CLUSTER_SELF_HOSTED_TOKEN_QUARANTINE",
        "wait out old same-room JWTs before publishing a replacement; keep true for self-hosted LiveKit, disable only with Cloud cutoff revocation (default true)",
    ),
    (
        "CLUSTER_PEER_STATE_MAX",
        "wallets held in the last-assignment store, whose only consumer is the next event's fromIslandId (default 20000)",
    ),
    (
        "CLUSTER_PEER_STATE_TTL_MS",
        "lifetime of a last-assignment entry; the feed carries no disconnect event, so this is the only reclamation path (default 3600000)",
    ),
    (
        "CLUSTER_ASSIGNMENT_MIRROR_MAX",
        "wallets held in the replica-wide assignment mirror the reconnect path reads (default 20000)",
    ),
    (
        "CLUSTER_ASSIGNMENT_MIRROR_TTL_MS",
        "lifetime of an assignment mirror entry (default 3600000)",
    ),
    (
        "COMMS_CONTROL_PG_CONNECTION_STRING",
        "optional shared Archipelago v4 authority Postgres URL; must be set with COMMS_CONTROL_V4_AUDIENCE",
    ),
    (
        "COMMS_CONTROL_V4_AUDIENCE",
        "optional deployment audience selecting the shared Archipelago v4 authority rows; must be set with COMMS_CONTROL_PG_CONNECTION_STRING",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_comms=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-comms", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_comms=info,tower_http=info");

    catalyrst_comms::metrics::install_recorder()?;

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let mut runtime = catalyrst_comms::CommsRuntime::eager(state.clone(), &cfg.cluster).await?;
    let routes = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/status", get(handlers::status::status))
        .route("/metrics", get(catalyrst_comms::metrics::metrics_handler))
        .merge(api_router(state.clone()));
    let routes = routes.merge(catalyrst_comms::connection_router(
        state.clone(),
        runtime.assignment_reader(),
        cfg.pulse_room_authority_key,
    )?);
    let app = catalyrst_envcfg::service_scaffold::finish_app(routes, state, None);

    runtime.start();
    let served =
        catalyrst_envcfg::run_service("catalyrst-comms", cfg.http_host, cfg.http_port, app).await;
    runtime.shutdown().await;
    served
}
