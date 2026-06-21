use std::net::SocketAddr;

use anyhow::Result;
use axum::http::Method;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use catalyrst_camera_reel::config::Config;
use catalyrst_camera_reel::{api_router, build_state};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_camera_reel=info,tower_http=info".into()),
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
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE]);

    let app = Router::new()
        .route("/health", get(|| async { Json("alive") }))
        .route("/health/live", get(|| async { Json("alive") }))
        .merge(api_router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", http_host, http_port).parse()?;
    tracing::info!(%addr, "catalyrst-camera-reel listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
