use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_lists::config::Config;
use catalyrst_lists::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5151)"),
    (
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
        "required — places Postgres connection string",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_lists=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-lists", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_lists=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/ping", get(handlers::ping::ping))
        .route("/status", get(handlers::status::status))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-lists listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
