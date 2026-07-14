pub mod bake;
pub mod cdn;
pub mod config;
pub mod handlers;
pub mod key;
pub mod quarantine;
pub mod quarantine_list;
pub mod seed;
pub mod store;
pub mod supply;
pub mod zips;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;

use crate::bake::{BakeConfig, BakeQueue};
use crate::cdn::CdnClient;
use crate::config::Config;
use crate::quarantine::Quarantine;
use crate::quarantine_list::QuarantineList;
use crate::store::Store;
use crate::supply::Supply;

pub struct AppStateInner {
    pub store: Arc<Store>,
    pub supply: Supply,
    pub cdn: CdnClient,
    pub quarantine: Arc<Quarantine>,
    pub quarantine_list: QuarantineList,
    pub bake: Option<Arc<BakeQueue>>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let store = Arc::new(Store::new(cfg.store_root.clone(), cfg.store_max_bytes));
    store
        .init()
        .with_context(|| format!("initializing store at {}", cfg.store_root.display()))?;
    {
        let store = store.clone();
        tokio::task::spawn_blocking(move || {
            store.sweep_transient();
            let _ = store.evict_pass();
        })
        .await
        .context("boot sweep")?;
    }

    let quarantine = Arc::new(Quarantine::load(
        store.quarantine_path(),
        cfg.bake_max_failures,
        cfg.bake_quarantine_secs,
    ));
    let quarantine_list = QuarantineList::load(cfg.quarantine_list.clone());
    tracing::info!(
        path = %quarantine_list.path().display(),
        keys = quarantine_list.len(),
        "read-through quarantine list loaded"
    );
    let cdn = CdnClient::new(
        cfg.cdn_base.clone(),
        cfg.cdn_realm_segment.clone(),
        cfg.readthrough_timeout_secs,
    )?;
    let supply = Supply::new(store.clone());

    let bake = if cfg.bake_enabled {
        let queue = BakeQueue::new(
            BakeConfig {
                wrapper: cfg.bake_wrapper.clone(),
                bin: cfg.impost_bin.clone(),
                server: cfg.impost_server.clone(),
                content_server: cfg.impost_content_server.clone(),
                timeout: Duration::from_secs(cfg.bake_timeout_secs),
                queue_depth: cfg.bake_queue_depth,
            },
            store.clone(),
            quarantine.clone(),
        );
        queue.spawn_worker();
        Some(queue)
    } else {
        None
    };

    let periodic_store = store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(600));
        interval.tick().await;
        loop {
            interval.tick().await;
            let store = periodic_store.clone();
            let _ = tokio::task::spawn_blocking(move || {
                store.sweep_transient();
                let _ = store.evict_pass();
            })
            .await;
        }
    });

    Ok(Arc::new(AppStateInner {
        store,
        supply,
        cdn,
        quarantine,
        quarantine_list,
        bake,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new().route("/status", get(handlers::status)).route(
        "/imposters/realms/{realm}/{level}/{file}",
        get(handlers::imposter),
    )
}
