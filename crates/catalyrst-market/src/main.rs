//! `catalyrst-market` — entry point. Mirrors the wiring in
//! `marketplace-server/src/index.ts` + `service.ts`: builds component
//! instances, wires routes, then binds & serves.

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use catalyrst_market::config::Config;
use catalyrst_market::handlers;
use catalyrst_market::ports::contracts::ContractsComponent;
use catalyrst_market::AppStateInner;

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

    // Read pool — handlers only need DAPPS_READ for now.
    let dapps_read = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&cfg.dapps_read_database_url)
        .await?;

    // Apply search_path so unqualified table refs land in the right schema.
    sqlx::query(&format!("SET search_path TO {}, public", cfg.dapps_read_schema))
        .execute(&dapps_read)
        .await
        .ok();

    let state = Arc::new(AppStateInner {
        contracts: ContractsComponent::new(dapps_read.clone()),
    });

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/v1/contracts", get(handlers::contracts::get_contracts))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-market listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
