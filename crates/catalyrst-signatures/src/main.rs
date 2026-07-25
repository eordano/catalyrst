use std::net::SocketAddr;

use anyhow::Result;
use axum::http::Method;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use catalyrst_signatures::config::Config;
use catalyrst_signatures::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5151)"),
    (
        "SIGNATURES_PG_CONNECTION_STRING",
        "required — signatures Postgres connection string",
    ),
    ("CHAIN_NAME", "chain name (default ETHEREUM_MAINNET)"),
    (
        "MARKETPLACE_SUBGRAPH_URL",
        "optional — marketplace subgraph URL",
    ),
    ("RENTALS_SUBGRAPH_URL", "optional — rentals subgraph URL"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional — squid Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "squid schema (default squid_marketplace)",
    ),
    (
        "AUTH_EXPIRATION_SECONDS",
        "auth chain expiration window in seconds (default 300)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_signatures=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-signatures", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_signatures=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let http_host = cfg.http_host.clone();
    let http_port = cfg.http_port;

    let state = build_state(cfg).await?;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods([Method::GET, Method::POST, Method::PATCH]);

    let app = Router::new()
        .route("/ping", get(|| async { Json("pong") }))
        .route("/health", get(|| async { Json("alive") }))
        .route("/health/live", get(|| async { Json("alive") }))
        .merge(api_router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", http_host, http_port).parse()?;
    tracing::info!(%addr, "catalyrst-signatures listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
