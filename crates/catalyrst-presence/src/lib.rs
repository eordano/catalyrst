pub mod config;
pub mod handlers;
pub mod ports;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPool;

use crate::config::Config;
use crate::ports::collector::Collector;
use crate::ports::queries::QueriesComponent;
use crate::ports::upstream::UpstreamClient;

pub struct AppStateInner {
    pub queries: QueriesComponent,
    pub collector: Collector,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/current", get(handlers::history::current))
        .route("/current/scenes", get(handlers::history::current_scenes))
        .route("/current/worlds", get(handlers::history::current_worlds))
        .route("/scenes/history", get(handlers::history::scene_history))
        .route("/worlds/history", get(handlers::history::world_history))
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = connect_pool(cfg).await?;
    let client = UpstreamClient::new(cfg).context("build upstream client")?;
    Ok(Arc::new(AppStateInner {
        queries: QueriesComponent::new(pool.clone()),
        collector: Collector::new(pool, client),
    }))
}

pub async fn build_collector(cfg: &Config) -> Result<Collector> {
    let pool = connect_pool(cfg).await?;
    let client = UpstreamClient::new(cfg).context("build upstream client")?;
    Ok(Collector::new(pool, client))
}

async fn connect_pool(cfg: &Config) -> Result<PgPool> {
    let pool = catalyrst_db::connect_pool(
        &cfg.database_url,
        &catalyrst_db::PoolSettings {
            max_connections: 5,
            ..catalyrst_db::PoolSettings::default()
        },
    )
    .await
    .context("failed to connect presence pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("presence migration failed")?;

    Ok(pool)
}
