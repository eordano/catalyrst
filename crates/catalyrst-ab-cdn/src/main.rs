use std::net::SocketAddr;

use anyhow::Result;
use tower_http::trace::TraceLayer;

use catalyrst_ab_cdn::config::Config;
use catalyrst_ab_cdn::{build_app, build_registry_state, build_state};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_ab_cdn=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;
    // Subsume the rest of the registry (profiles, worlds, status, queues, admin)
    // when its data sources are configured, so the standalone can be retired.
    let registry = build_registry_state().await;

    let app = build_app(state, registry).layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, out_root = %cfg.abgen_out_root, "catalyrst-ab-cdn listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
