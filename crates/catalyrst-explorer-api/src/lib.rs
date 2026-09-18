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

    let denylist = modules::blocklist::read_denylist(&cfg.blocklist_path).await;

    let state = Arc::new(AppStateInner {
        cfg: cfg.clone(),
        http,
        auth_api: Default::default(),
        feature_flags: Default::default(),
        runtime_config: Default::default(),
        onboarding: Default::default(),
        denylist: parking_lot::RwLock::new(modules::blocklist::DenylistCache::new(denylist)),
        denylist_write: tokio::sync::Mutex::new(()),
        catalyst_status_cache: modules::swr::SwrCell::new("catalyst-status"),
        external_catalyst_cache: catalyrst_commons::cache::TtlMap::bounded(
            "external-catalyst",
            modules::realm_provider::CATALYST_STATUS_TTL,
            64,
        ),
        hot_scenes_cache: modules::swr::SwrCell::new("hot-scenes"),
        world_doc_cache: catalyrst_commons::cache::TtlMap::bounded(
            "world-docs",
            modules::worlds_content_server::WORLD_DOC_TTL,
            4096,
        ),
        contents_cache: catalyrst_commons::cache::TtlMap::bounded(
            "worlds-contents",
            modules::worlds_content_server::CONTENTS_TTL,
            modules::worlds_content_server::CONTENTS_MAX_ENTRIES,
        ),
    });
    modules::auth_api::spawn_identity_sweeper(Arc::downgrade(&state));
    Ok(state)
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
