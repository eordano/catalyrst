use std::sync::Arc;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tokio_util::sync::CancellationToken;

use catalyrst_comms::cluster_gateway::StateGateway;
use catalyrst_comms::cluster_subscriber::ClusterSubscriber;
use catalyrst_comms::config::{ClusterConfig, Config};
use catalyrst_comms::nats::NatsBus;
use catalyrst_comms::peer_state::ClusterPeerState;
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
        "lifetime of an island room token; short on purpose, since a displaced token the eviction could not reach stays usable this long (default 60)",
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

    let shutdown = CancellationToken::new();
    catalyrst_comms::voice_logic::spawn_expiration_job(state.clone(), shutdown.clone());

    let cluster = ClusterSubscriber::new(
        cluster_bus(&cfg.cluster),
        Arc::new(StateGateway::new(state.clone())),
        Arc::new(ClusterPeerState::new(
            cfg.cluster.peer_state_max,
            cfg.cluster.peer_state_ttl_ms,
            cfg.cluster.assignment_mirror_max,
            cfg.cluster.assignment_mirror_ttl_ms,
        )),
        cfg.cluster.clone(),
    );
    cluster.start();

    let app = catalyrst_envcfg::service_scaffold::finish_app(
        Router::new()
            .route("/ping", get(handlers::ping::ping))
            .route("/status", get(handlers::status::status))
            .route("/metrics", get(catalyrst_comms::metrics::metrics_handler))
            .merge(api_router(state.clone())),
        state,
        None,
    );

    let served =
        catalyrst_envcfg::run_service("catalyrst-comms", cfg.http_host, cfg.http_port, app).await;
    cluster.stop().await;
    served
}

#[cfg(feature = "nats")]
fn cluster_bus(cfg: &ClusterConfig) -> Arc<dyn NatsBus> {
    Arc::new(catalyrst_comms::nats::BrokerBus::new(
        cfg.nats_url.clone(),
        "catalyrst-comms",
    ))
}

/// A build without a broker client still starts, and says so, rather than reporting itself as a
/// subscriber that simply never receives anything.
#[cfg(not(feature = "nats"))]
fn cluster_bus(cfg: &ClusterConfig) -> Arc<dyn NatsBus> {
    if cfg.nats_url.is_some() {
        tracing::error!(
            "NATS_URL is set but this binary was built without the `nats` feature; the Pulse \
             cluster feed will never be consumed"
        );
    }
    Arc::new(catalyrst_comms::nats::DisabledBus)
}
