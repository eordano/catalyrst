pub mod config;
pub mod modules;
pub mod state;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;

pub use config::Config;
pub use state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let http = reqwest::Client::builder()
        .user_agent("catalyrst-explorer-api/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("failed to build reqwest client")?;

    Ok(Arc::new(AppStateInner {
        cfg: cfg.clone(),
        http,
        auth_api: Default::default(),
        feature_flags: Default::default(),
        runtime_config: Default::default(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(modules::realm_provider::routes())
        .merge(modules::auth_api::routes())
        .merge(modules::blocklist::routes())
        .merge(modules::builder_api::routes())
        .merge(modules::worlds_content_server::routes())
        .merge(modules::feature_flags::routes())
        .merge(modules::runtime_config::routes())
}
