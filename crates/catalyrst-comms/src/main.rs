use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_comms::config::Config;
use catalyrst_comms::{api_router, build_state, handlers};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_comms=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    catalyrst_comms::voice_logic::spawn_expiration_job(state.clone());

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/status", get(handlers::status::status))
        .merge(api_router(state.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-comms listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
