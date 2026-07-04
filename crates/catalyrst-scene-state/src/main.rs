use anyhow::Result;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_scene_state::{api_router, build_state, Config};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_scene_state=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let state = build_state(&cfg).await?;

    let app = api_router()
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!(%addr, "catalyrst-scene-state listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
