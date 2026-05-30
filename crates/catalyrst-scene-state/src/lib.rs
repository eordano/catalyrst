//! # catalyrst-scene-state
//!
//! Rust port of [`decentraland/scene-state-server`]. Hosts authoritative,
//! server-side state for SDK7 multiplayer scenes: an HTTP control surface plus a
//! per-scene WebSocket transport carrying CRDT state sync. Listens on port 5153.
//!
//! [`decentraland/scene-state-server`]: https://github.com/decentraland/scene-state-server
//!
//! ## Architecture map (upstream -> this crate)
//!
//! | upstream file                         | this crate            |
//! |---------------------------------------|-----------------------|
//! | `logic/protocol.ts`                   | [`protocol`]          |
//! | `controllers/handlers/ws-handler.ts`  | [`ws`] + [`auth`]     |
//! | `adapters/scene.ts`                   | [`runtime`] + [`scene`] |
//! | `adapters/wsRegistry.ts`              | [`scene::SceneManager`] |
//! | `logic/sceneFetcher.ts`               | [`scene_fetcher`]     |
//! | `controllers/handlers/debugging-handler.ts` | [`loader`] + [`handlers`] |
//! | `controllers/handlers/status-handler.ts` | [`handlers`]       |
//! | `logic/scene-runtime/*`               | [`jsruntime`] + [`crdt`] |
//!
//! ## IMPLEMENTATION STATUS — complete
//!
//! ### Transport + lifecycle (faithful to upstream)
//! - Binary WS wire protocol: `Auth`/`Init`/`Crdt` frames, byte-exact
//!   `encode_init_message` layout ([`protocol`]).
//! - Signed-fetch WS authentication over the Auth frame, reusing the workspace
//!   `catalyrst-crypto` auth-chain verifier ([`auth`]).
//! - Connection lifecycle: 404 on unloaded scene, 5s auth timeout, per-client
//!   integer index + entity-range allocation, `Init` snapshot send, Crdt
//!   handling, connection counter ([`ws`], [`scene`]).
//! - Entity-range policy arithmetic (`reserved + server + index*client`),
//!   matching `docs/limitations.md` ([`runtime::ServerTransportConfig`]).
//! - HTTP surface: `/ping`, `/status`, secret-gated `/debugging/reload`
//!   ([`handlers`]).
//! - Scene acquisition: local file + world content-server resolution
//!   ([`scene_fetcher`]).
//!
//! ### The state-sync core (the defining feature)
//!
//! scene-state-server **runs the scene's own compiled SDK7 JavaScript**
//! (`bin/game.js`) headlessly. This is now ported in full:
//!
//! - [`jsruntime`] — embeds V8 (the `v8` crate, i.e. `rusty_v8`; offline-build
//!   recipe in the crate README) on a dedicated per-scene thread. Reproduces
//!   `logic/scene-runtime/{sandbox,sdk7-runtime,apis}.ts`: the sandbox global
//!   surface (`module`/`exports`/`console`/`require`/`setImmediate`/
//!   `registerScene`/`updateCRDTState`, restricted `fetch`/`WebSocket`), the
//!   `~system/*` host modules (`EngineApi.crdtSendToRenderer`/`crdtGetState`/
//!   `isServer`→true/`sendBatch`, `Runtime.getRealm`/`getSceneInformation`/
//!   `readFile`, no-op `UserIdentity`/`SignedFetch`), and the `onStart` + 30 Hz
//!   `onUpdate` game loop with the per-client `sendCrdtMessage`/`getMessages`
//!   channels wired through `registerScene`'s observer.
//! - [`crdt`] — a real SDK7 CRDT engine: little-endian wire codec for
//!   `PUT_COMPONENT`/`DELETE_COMPONENT`/`DELETE_ENTITY`/`APPEND_VALUE`, plus
//!   LWW-element-set merge (timestamp, then `dataCompare` tiebreak) with a
//!   deleted-entity tombstone set and range reclaim. Used by both [`JsRuntime`]
//!   (authoritative merges of the scene's own + clients' batches) and the
//!   fallback [`RelayRuntime`].
//!
//! Two runtimes behind the [`runtime::SceneRuntime`] trait:
//! 1. [`JsRuntime`] **(default)** — runs the real `game.js`; the scene declares
//!    its `ServerTransportConfig` via `registerScene(...)`.
//! 2. [`RelayRuntime`] **(fallback, `DISABLE_JS_RUNTIME=1` or empty source)** —
//!    scene-logic-free; default 512/512/512 policy; real CRDT merge + late-
//!    joiner snapshot; entity-range reclaim on close.
//!
//! [`JsRuntime`]: crate::runtime::JsRuntime
//! [`RelayRuntime`]: crate::runtime::RelayRuntime
//!
//! ### Open questions inherited from upstream `docs/limitations.md`
//! - No reconnection/state-reconciliation story (upstream punts: tell the user
//!   to restart the scene).
//! - Fixed entity ranges exhaust `2^16` ids; range reuse across the whole scene
//!   lifetime is unimplemented (per-client GC on close is — see
//!   `crdt::CrdtEngine::reclaim_range`).

pub mod auth;
pub mod config;
pub mod crdt;
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

    // Load the local scene at startup if configured (upstream `main`).
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
        .merge(ws::routes())
}
