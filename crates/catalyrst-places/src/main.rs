use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_places::config::Config;
use catalyrst_places::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5134)"),
    (
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — places Postgres connection string",
    ),
    (
        "PLACES_PG_COMPONENT_WRITER_PSQL_CONNECTION_STRING",
        "optional — writer Postgres connection string (enables write endpoints)",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional — squid Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "squid schema (default squid_marketplace)",
    ),
    (
        "PLACES_ADMIN_ADDRESSES",
        "optional — comma-separated admin wallet addresses",
    ),
    (
        "DATA_TEAM_AUTH_TOKEN",
        "optional — bearer token for the data-team endpoints",
    ),
    (
        "PLACES_ADMIN_AUTH_TOKEN",
        "optional — bearer token for the admin endpoints",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (default https://comms-gatekeeper.decentraland.zone)",
    ),
    (
        "EVENTS_API_URL",
        "events API base URL (default https://events.decentraland.zone/api)",
    ),
    (
        "PRESENCE_URL",
        "presence service base URL (default http://127.0.0.1:5152)",
    ),
    (
        "AWS_ACCESS_KEY",
        "S3 report uploads — access key (with AWS_ACCESS_SECRET + AWS_BUCKET_NAME)",
    ),
    ("AWS_ACCESS_SECRET", "S3 report uploads — secret key"),
    ("AWS_BUCKET_NAME", "S3 report uploads — bucket name"),
    (
        "BUCKET_HOSTNAME",
        "optional — public hostname for uploaded report URLs",
    ),
    ("AWS_REGION", "S3 region (default us-east-1)"),
    ("AWS_ENDPOINT", "optional — custom S3 endpoint"),
    (
        "PLACES_REPORT_LOCAL_FALLBACK",
        "bool — allow local-dev report storage when S3 is unconfigured",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_places=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-places", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_places=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::status::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-places listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
