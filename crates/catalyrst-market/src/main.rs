use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_market::config::Config;
use catalyrst_market::handlers;
use catalyrst_market::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    (
        "HTTP_SERVER_HOST",
        "bind address (default 127.0.0.1; non-loopback refuses to start without CATALYRST_MARKET_ADMIN_TOKEN)",
    ),
    ("HTTP_SERVER_PORT", "listen port (default 5133)"),
    (
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — dapps Postgres connection string",
    ),
    (
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA",
        "dapps schema (default marketplace)",
    ),
    (
        "DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — dapps read-replica Postgres connection string",
    ),
    (
        "DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA",
        "dapps read-replica schema (default marketplace)",
    ),
    (
        "FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — favorites Postgres connection string",
    ),
    (
        "FAVORITES_PG_COMPONENT_PSQL_SCHEMA",
        "favorites schema (default favorites)",
    ),
    (
        "CONTENT_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "optional — catalyst content DB connection string",
    ),
    (
        "CATALYRST_MARKET_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "CATALYRST_MARKET_TRADES_PAGINATION",
        "bool — enable trades pagination (default true)",
    ),
    (
        "TRADES_SYNC_UPSTREAM_URL",
        "trades sync upstream (default https://marketplace-api.decentraland.org/v1/trades; empty disables sync)",
    ),
    (
        "TRADES_SYNC_INTERVAL_SECS",
        "trades sync interval in seconds (default 900)",
    ),
    (
        "CATALYRST_MARKET_HTTP_CACHE_TTL_SECS",
        "response cache TTL in seconds (default 30; 0 disables)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_market=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-market", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_market=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(&cfg).await?;

    let response_cache = catalyrst_market::http::response_cache::ResponseCache::from_env();
    catalyrst_market::http::response_cache::spawn_invalidation_listener(
        state.pool.clone(),
        response_cache.clone(),
    );

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(axum::middleware::from_fn_with_state(
            response_cache,
            catalyrst_market::http::response_cache::middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-market listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
