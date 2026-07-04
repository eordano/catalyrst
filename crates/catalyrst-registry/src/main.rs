use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_registry=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = catalyrst_registry::config::Config::from_env()?;
    let addr: std::net::SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let state = catalyrst_registry::build_state(&cfg).await?;
    let app = catalyrst_registry::api_router().with_state(state);

    tracing::info!(%addr, "catalyrst-registry listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
