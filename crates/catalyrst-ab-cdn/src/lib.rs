pub mod config;
pub mod handlers;
pub mod resolver;
pub mod serve;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::routing::get;
use axum::Router;

use crate::config::Config;

pub use state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    Ok(Arc::new(AppStateInner::new(PathBuf::from(&cfg.abgen_out_root))))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route(
            "/{*path}",
            get(handlers::dispatch).head(handlers::dispatch),
        )
}
