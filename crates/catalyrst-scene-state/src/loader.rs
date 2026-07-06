use std::sync::Arc;

use anyhow::Result;

use crate::delegation::{self, DelegationSlot};
use crate::jsruntime::{parse_origin, StorageCtx};
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

    let (hash, source, static_crdt, base_parcel) = if name == LOCAL_SCENE_NAME {
        let path = state
            .cfg
            .local_scene_path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LOCAL_SCENE_PATH not set"))?;
        let src = scene_fetcher::from_local(&path).await?;
        (LOCAL_SCENE_NAME.to_string(), src, Vec::new(), None)
    } else {
        let world_url = state
            .cfg
            .world_server_url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("WORLD_SERVER_URL not set"))?;
        let ws = scene_fetcher::from_world(
            &state.http,
            &world_url,
            name,
            state.cfg.fetch_max_body_bytes,
        )
        .await?;
        (ws.scene_hash, ws.code, ws.static_crdt, Some(ws.base_parcel))
    };

    let mut renewal: Option<tokio::task::JoinHandle<()>> = None;
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
            js_update_failure_cap: state.cfg.js_update_failure_cap,
            client_inbound_max: state.cfg.client_inbound_max,
            client_outbound_max: state.cfg.client_outbound_max,
            crdt_max_components: state.cfg.crdt_max_components,
            fetch_max_response_bytes: state.cfg.signed_fetch_max_response_bytes,
            fetch_max_body_bytes: state.cfg.signed_fetch_max_body_bytes,
            fetch_max_in_flight: state.cfg.signed_fetch_max_in_flight,
            fetch_timeout_ms: state.cfg.signed_fetch_timeout_ms,
        };
        let storage = match build_storage_ctx(state, name, &hash, base_parcel.as_deref()).await {
            Some((ctx, task)) => {
                renewal = task;
                Some(ctx)
            }
            None => None,
        };
        Arc::new(JsRuntime::new(
            hash.clone(),
            source,
            realm,
            limits,
            static_crdt,
            storage,
        ))
    };
    let scene = Arc::new(Scene::new_with_renewal(name, runtime, renewal));
    state.scenes.insert(name, scene);
    tracing::info!(name, %hash, "scene loaded");
    Ok(())
}

// The delegation slot is written by the renewal task and read by the fetch
// worker; the JS isolate never sees key material or signed headers. Local scenes
// have no world name/parcel, so they only get storage via a pre-minted
// STORAGE_DELEGATION.
async fn build_storage_ctx(
    state: &AppState,
    world: &str,
    scene_hash: &str,
    base_parcel: Option<&str>,
) -> Option<(StorageCtx, Option<tokio::task::JoinHandle<()>>)> {
    let raw = state.cfg.storage_url.as_deref()?;
    let origin = match parse_origin(raw, state.cfg.storage_allow_http) {
        Ok(o) => o,
        Err(reason) => {
            tracing::warn!(reason, "STORAGE_URL rejected; scene storage disabled");
            return None;
        }
    };

    let slot: DelegationSlot = Arc::new(parking_lot::Mutex::new(None));
    let mut renew_tx = None;
    let mut task = None;

    if let Some(encoded) = &state.cfg.storage_delegation {
        match delegation::parse_storage_delegation(encoded) {
            Some(d) => *slot.lock() = Some(d),
            None => {
                tracing::warn!("STORAGE_DELEGATION did not parse; storage requests fail closed")
            }
        }
    } else if let (Some(minter), Some(parcel)) = (&state.cfg.delegation_minter_url, base_parcel) {
        let token = state.cfg.delegation_minter_token.clone();
        match delegation::mint_from_minter(
            &state.http,
            minter,
            token.as_deref(),
            world,
            scene_hash,
            parcel,
        )
        .await
        {
            Ok(d) => *slot.lock() = Some(d),
            Err(_) => tracing::warn!(
                world,
                "initial storage delegation mint failed; storage requests fail closed until renewal succeeds"
            ),
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        renew_tx = Some(tx);
        task = Some(tokio::spawn(delegation::renewal_loop(
            state.http.clone(),
            minter.clone(),
            token,
            world.to_string(),
            scene_hash.to_string(),
            parcel.to_string(),
            Arc::clone(&slot),
            rx,
        )));
    } else {
        tracing::warn!(
            world,
            "STORAGE_URL set without DELEGATION_MINTER_URL or STORAGE_DELEGATION; storage requests fail closed"
        );
    }

    Some((
        StorageCtx {
            origin,
            allow_http_loopback: state.cfg.storage_allow_http,
            delegation: slot,
            renew_tx,
        },
        task,
    ))
}
