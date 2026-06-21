use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_worlds::config::Config;
use catalyrst_worlds::{api_router, build_state, handlers};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_worlds=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let http_host = cfg.http_host.clone();
    let http_port = cfg.http_port;

    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::status::ping))
        .route("/status", get(handlers::status::status))
        .route("/health", get(handlers::status::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", http_host, http_port).parse()?;
    tracing::info!(%addr, "catalyrst-worlds listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
