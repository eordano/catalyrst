pub mod client;
pub mod config;
pub mod handlers;
pub mod parse;
pub mod ports;
pub mod rows;
pub mod snapshot;
pub mod sync;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;

use crate::client::GovernanceClient;
use crate::config::Config;
use crate::ports::store::Store;
use crate::snapshot::SnapshotGate;

pub struct AppStateInner {
    pub store: Store,
    pub archives: crate::ports::archives::Archives,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/proposals", get(handlers::read::proposals))
        .route("/proposals/{id}/votes", get(handlers::read::proposal_votes))
        .route(
            "/proposals/{id}/comments",
            get(handlers::read::proposal_comments),
        )
        .route("/votes/engagement", get(handlers::read::engagement))
        .route("/activity", get(handlers::read::activity))
        .route("/projects", get(handlers::read::projects))
        .route("/projects/{id}", get(handlers::read::project_by_id))
        .route("/budgets", get(handlers::read::budgets))
        .route("/vestings", get(handlers::read::vestings))
        .route("/members", get(handlers::read::members))
}

pub fn write_router(gate: Arc<SnapshotGate>) -> Router {
    Router::new()
        .route("/proposals/{kind}", post(handlers::write::submit_proposal))
        .with_state(gate)
}

pub fn build_snapshot_gate(cfg: &Config) -> Arc<SnapshotGate> {
    let gate = SnapshotGate::build(cfg.snapshot.clone());
    gate.startup_log();
    Arc::new(gate)
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = build_pool(cfg).await?;
    let archives = crate::ports::archives::Archives::from_env().await;
    Ok(Arc::new(AppStateInner {
        store: Store::new(pool),
        archives,
    }))
}

async fn build_pool(cfg: &Config) -> Result<sqlx::PgPool> {
    let pool = catalyrst_db::connect_pool(
        &cfg.database_url,
        &catalyrst_db::PoolSettings {
            max_connections: 5,
            ..catalyrst_db::PoolSettings::default()
        },
    )
    .await
    .context("failed to connect governance pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("governance migration failed")?;
    Ok(pool)
}

pub fn build_client(cfg: &Config) -> Result<GovernanceClient> {
    GovernanceClient::new(cfg.api_url.clone())
}

pub fn spawn_sync_loop(state: AppState, client: GovernanceClient, window_hours: u32) {
    let interval = Duration::from_secs(window_hours.max(1) as u64 * 3600);
    tokio::spawn(async move {
        tracing::info!(window_hours, "governance sync loop started");
        loop {
            if let Err(e) = sync::sync(&client, &state.store, window_hours).await {
                tracing::error!(error = %e, "governance sync cycle failed");
            }
            tokio::time::sleep(interval).await;
        }
    });
}
