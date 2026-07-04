use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_places::config::Config;
use catalyrst_places::{api_router, build_state, handlers};

#[tokio::main]
async fn main() -> Result<()> {
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
