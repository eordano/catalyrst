pub mod config;
pub mod modules;
pub mod relay;
pub mod state;

pub use config::Config;
pub use state::{AppState, AppStateInner};

use anyhow::Result;
use axum::Router;
use std::sync::Arc;

pub fn api_router() -> Router<AppState> {
    modules::rpc::routes()
}

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("catalyrst-rpc/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(16)
        .build()?;

    for (network, url) in &cfg.upstreams {
        if url.trim().is_empty() {
            tracing::warn!(%network, "upstream URL is empty; requests for this network will fail");
        }
        if config::chain_id_for(network).is_none() {
            tracing::warn!(%network, "no known chain id for configured network");
        }
    }

    tracing::info!(
        networks = ?cfg.upstreams.keys().collect::<Vec<_>>(),
        "catalyrst-rpc wired"
    );

    Ok(Arc::new(AppStateInner { cfg, http }))
}
