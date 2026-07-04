use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_market::config::Config;
use catalyrst_market::handlers;
use catalyrst_market::{api_router, build_state};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_market=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(&cfg).await?;

    let response_cache = catalyrst_market::http::response_cache::ResponseCache::from_env();
    catalyrst_market::http::response_cache::spawn_invalidation_listener(
        state.pool.clone(),
        response_cache.clone(),
    );

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(axum::middleware::from_fn_with_state(
            response_cache,
            catalyrst_market::http::response_cache::middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-market listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
