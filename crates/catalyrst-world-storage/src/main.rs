use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use catalyrst_world_storage::config::Config;
use catalyrst_world_storage::{api_router, build_state, handlers};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_world_storage=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;
    let state = build_state(cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-world-storage listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
