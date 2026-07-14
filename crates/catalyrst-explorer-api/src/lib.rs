#![allow(clippy::result_large_err)]

pub mod config;
pub mod modules;
pub mod state;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;

pub use config::Config;
pub use state::{AppState, AppStateInner};

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let http = catalyrst_commons::http::try_http_client(
        &catalyrst_commons::http::HttpClientCfg::default()
            .with_total_timeout(std::time::Duration::from_secs(15))
            .following_redirects(10),
    )
    .context("failed to build reqwest client")?;

    let denylist = Arc::new(modules::blocklist::read_denylist(&cfg.blocklist_path).await);

    Ok(Arc::new(AppStateInner {
        cfg: cfg.clone(),
        http,
        auth_api: Default::default(),
        feature_flags: Default::default(),
        runtime_config: Default::default(),
        onboarding: Default::default(),
        denylist: parking_lot::RwLock::new(denylist),
        catalyst_status_cache: catalyrst_commons::cache::TtlCell::new("catalyst-status"),
        hot_scenes_cache: catalyrst_commons::cache::TtlCell::new("hot-scenes"),
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
        .merge(modules::onboarding::routes())
}
