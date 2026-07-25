use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_price::config::Config;
use catalyrst_price::handlers;
use catalyrst_price::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5156)"),
    (
        "PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — mana_price Postgres connection string",
    ),
    (
        "CATALYRST_PRICE_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "PRICE_POLL_ENABLED",
        "bool — enable the coingecko poll task (default false)",
    ),
    (
        "COINGECKO_URL",
        "coingecko API base (default https://api.coingecko.com/api/v3)",
    ),
    (
        "PRICE_POLL_INTERVAL_SECS",
        "poll interval in seconds (default 300)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_price=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-price", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_price=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-price listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
