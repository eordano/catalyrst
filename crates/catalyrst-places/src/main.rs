use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_places::config::Config;
use catalyrst_places::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5134)"),
    (
        "PLACES_FED_ALLOW_REPLAY_SKIP",
        "bool -- allow federation writes without replay protection when no writer pool is configured (default false)",
    ),
    (
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required -- places Postgres connection string",
    ),
    (
        "PLACES_PG_COMPONENT_WRITER_PSQL_CONNECTION_STRING",
        "optional -- writer Postgres connection string (enables write endpoints)",
    ),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional -- content Postgres connection string (source for the place catalog)",
    ),
    (
        "PLACES_DERIVE_FROM_CONTENT",
        "bool -- build the place catalog from this node's own content deployments",
    ),
    (
        "CONTENT_PUBLIC_URL",
        "public content base for derived place images (default /content)",
    ),
    (
        "PLACES_MIRROR_UPSTREAM",
        "bool -- mirror the place catalog from an upstream places API (takes precedence over content derivation)",
    ),
    (
        "PLACES_UPSTREAM_URL",
        "upstream places API base for the mirror (default https://places.decentraland.org)",
    ),
    (
        "WORLDS_MIRROR_UPSTREAM",
        "bool -- mirror the world catalog from an upstream places API's /api/worlds",
    ),
    (
        "WORLDS_UPSTREAM_URL",
        "upstream places API base for the worlds mirror (default https://places.decentraland.org)",
    ),
    (
        "WORLDS_MIRROR_INTERVAL_SECS",
        "seconds between worlds mirror passes (default 3600)",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional -- squid Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "squid schema (default squid_marketplace)",
    ),
    (
        "PLACES_ADMIN_ADDRESSES",
        "optional -- comma-separated admin wallet addresses",
    ),
    (
        "DATA_TEAM_AUTH_TOKEN",
        "optional -- bearer token for the data-team endpoints",
    ),
    (
        "PLACES_ADMIN_AUTH_TOKEN",
        "optional -- bearer token for the admin endpoints",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (default http://127.0.0.1:5138)",
    ),
    (
        "EVENTS_API_URL",
        "events API base URL (default http://127.0.0.1:5135)",
    ),
    (
        "PRESENCE_URL",
        "presence service base URL (default http://127.0.0.1:5152)",
    ),
    (
        "AWS_ACCESS_KEY",
        "S3 report uploads -- access key (with AWS_ACCESS_SECRET + AWS_BUCKET_NAME)",
    ),
    ("AWS_ACCESS_SECRET", "S3 report uploads -- secret key"),
    ("AWS_BUCKET_NAME", "S3 report uploads -- bucket name"),
    (
        "BUCKET_HOSTNAME",
        "optional -- public hostname for uploaded report URLs",
    ),
    ("AWS_REGION", "S3 region (default us-east-1)"),
    ("AWS_ENDPOINT", "optional -- custom S3 endpoint"),
    (
        "PLACES_REPORT_LOCAL_FALLBACK",
        "bool -- allow local-dev report storage when S3 is unconfigured",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_places=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-places", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_places=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::status::health))
        .merge(api_router())
        .merge(catalyrst_places::lists_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-places", cfg.http_host, cfg.http_port, app).await
}
