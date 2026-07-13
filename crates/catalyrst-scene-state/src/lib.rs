#![allow(clippy::result_large_err)]

pub mod admin;
pub mod auth;
pub mod config;
pub mod crdt;
pub mod delegation;
pub mod handlers;
pub mod jsruntime;
pub mod loader;
pub mod protocol;
pub mod runtime;
pub mod scene;
pub mod scene_fetcher;
pub mod state;
pub mod ws;

pub use config::Config;
pub use state::{AppState, AppStateInner};

use std::sync::Arc;

use anyhow::Result;
use axum::Router;

use crate::scene::SceneManager;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("catalyrst-scene-state/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let state = Arc::new(AppStateInner {
        cfg: cfg.clone(),
        scenes: SceneManager::new(),
        http,
    });

    if cfg.local_scene_path.is_some() {
        if let Err(e) = loader::load_or_reload(&state, loader::LOCAL_SCENE_NAME).await {
            tracing::warn!(error = %e, "failed to load LOCAL_SCENE_PATH");
        }
    }

    tracing::info!(
        local_scene = cfg.local_scene_path.is_some(),
        world_server = cfg.world_server_url.is_some(),
        debugging_armed = cfg.debugging_secret.is_some(),
        "catalyrst-scene-state wired"
    );

    Ok(state)
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(handlers::routes())
        .merge(admin::routes())
        .merge(ws::routes())
}
