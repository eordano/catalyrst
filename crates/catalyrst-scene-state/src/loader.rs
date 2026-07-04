use std::sync::Arc;

use anyhow::Result;

use crate::runtime::{JsRuntime, RelayRuntime, RuntimeLimits, SceneRuntime, ServerTransportConfig};
use crate::scene::Scene;
use crate::scene_fetcher;
use crate::state::AppState;

pub const LOCAL_SCENE_NAME: &str = "localScene";

pub async fn load_or_reload(state: &AppState, name: &str) -> Result<()> {
    if let Some(prev) = state.scenes.remove(name) {
        tracing::info!(name, "stopping existing scene");

        drop(prev);
    }

    let (hash, source, static_crdt) = if name == LOCAL_SCENE_NAME {
        let path = state
            .cfg
            .local_scene_path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LOCAL_SCENE_PATH not set"))?;
        let src = scene_fetcher::from_local(&path).await?;
        (LOCAL_SCENE_NAME.to_string(), src, Vec::new())
    } else {
        let world_url = state
            .cfg
            .world_server_url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("WORLD_SERVER_URL not set"))?;
        let ws = scene_fetcher::from_world(&state.http, &world_url, name).await?;
        (ws.scene_hash, ws.code, ws.static_crdt)
    };

    let runtime: Arc<dyn SceneRuntime> = if state.cfg.disable_js_runtime || source.trim().is_empty()
    {
        tracing::info!(name, %hash, "loading with RelayRuntime (JS disabled or empty source)");
        Arc::new(RelayRuntime::new(
            hash.clone(),
            ServerTransportConfig::default(),
        ))
    } else {
        let realm = state
            .cfg
            .realm_name
            .clone()
            .unwrap_or_else(|| "dcl-one".to_string());
        tracing::info!(name, %hash, "loading with JsRuntime (server-side game.js)");
        let limits = RuntimeLimits {
            js_heap_limit_mb: state.cfg.js_heap_limit_mb,
            js_tick_budget_ms: state.cfg.js_tick_budget_ms,
            js_shutdown_join_ms: state.cfg.js_shutdown_join_ms,
            client_inbound_max: state.cfg.client_inbound_max,
            client_outbound_max: state.cfg.client_outbound_max,
            crdt_max_components: state.cfg.crdt_max_components,
        };
        Arc::new(JsRuntime::new(
            hash.clone(),
            source,
            realm,
            limits,
            static_crdt,
        ))
    };
    let scene = Arc::new(Scene::new(name, runtime));
    state.scenes.insert(name, scene);
    tracing::info!(name, %hash, "scene loaded");
    Ok(())
}
