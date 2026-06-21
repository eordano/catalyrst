//! Scene runtime abstraction — the heart of the state-sync core.
//!
//! # What the upstream server actually does
//!
//! scene-state-server is **not** a dumb CRDT relay. It is a headless host that
//! *executes the scene's own compiled SDK7 JavaScript* (`bin/game.js`) inside a
//! sandbox (`src/logic/scene-runtime/`), running an `onStart` + 30 Hz `onUpdate`
//! game loop. The scene calls `registerScene(serverConfig, observer)` to:
//!
//! 1. declare its entity-range policy ([`ServerTransportConfig`]): how many
//!    local entity ids to reserve, the server's own network-entity budget, and
//!    the per-client network-entity budget; and
//! 2. hand the server an `observer` callback invoked on every client
//!    open/close. The scene drives the multiplayer state by pulling each
//!    client's queued inbound CRDT messages (`client.getMessages()`) and
//!    pushing merged CRDT updates back (`client.sendCrdtMessage(bytes)`), all
//!    from inside its own `onUpdate` tick.
//!
//! The sandbox exposes a cut-down `~system/*` API surface
//! (`src/logic/scene-runtime/apis.ts`): `EngineApi.isServer() -> true`,
//! `Runtime.getRealm()`, no-op `UserIdentity`/`SignedFetch`, and **no** `fetch`
//! or `WebSocket`. The server keeps the authoritative CRDT snapshot in a single
//! `crdtState: Uint8Array` updated via the `updateCRDTState` host hook, and
//! ships that snapshot to every newly-joined client in the `Init` frame.
//!
//! # Why this is a trait, not an implementation
//!
//! Reproducing that behaviour in Rust requires embedding a JS engine
//! (`deno_core` / `rusty_v8`) and re-implementing the `~system/*` host API plus
//! the SDK7 CRDT engine. That is the bulk of the porting work and is deferred
//! (see `IMPLEMENTATION PLAN` in `lib.rs` and the crate report). To keep the
//! rest of the server (HTTP, WS, connection lifecycle, entity-range allocation,
//! state buffer) testable and shippable now, scene logic sits behind this
//! [`SceneRuntime`] trait.
//!
//! Two implementations exist:
//! - [`JsRuntime`] (default): embeds V8 and runs the scene's real `game.js`
//!   headlessly — `onStart` + 30 Hz `onUpdate`, the `~system/*` host API, and a
//!   real SDK7 CRDT engine doing LWW merges of client batches. See
//!   [`crate::jsruntime`] and [`crate::crdt`]. This is the faithful port of the
//!   upstream state-sync core.
//! - [`RelayRuntime`] (fallback, behind config): a transport-faithful,
//!   scene-logic-free relay used for scenes with no `game.js` (or when the JS
//!   runtime is explicitly disabled). It assigns entity ranges per the
//!   configured policy, keeps the CRDT snapshot as an opaque append buffer, and
//!   fans out inbound CRDT frames to the other connected clients.

use parking_lot::Mutex;

use crate::jsruntime::{self, Command, JsRuntimeHandle};

/// Entity-range + budget policy a scene declares via `registerScene`.
/// Port of `ServerTransportConfig` (`src/adapters/scene.ts`).
#[derive(Debug, Clone, Copy)]
pub struct ServerTransportConfig {
    /// Ids `[0, reserved_local_entities)` are reserved for the renderer's local
    /// (non-networked) entities and never handed to a client.
    pub reserved_local_entities: u32,
    /// Entity ids the server itself may create.
    pub server_network_entities_limit: u32,
    /// Entity ids each client may create within its assigned range.
    pub client_network_entities_limit: u32,
}

impl Default for ServerTransportConfig {
    fn default() -> Self {
        // Matches the upstream docs/limitations.md worked example: 512-wide
        // ranges, the n-th client getting [512*n, 512*(n+1)).
        Self {
            reserved_local_entities: 512,
            server_network_entities_limit: 512,
            client_network_entities_limit: 512,
        }
    }
}

impl ServerTransportConfig {
    /// The `(start, size)` network-entity range for the `index`-th client.
    ///
    /// Port of the arithmetic in `addSceneClient` upstream:
    /// `start = reserved_local + server_limit + index * client_limit`.
    ///
    /// All operands are client-influenced (the scene declares the limits via
    /// `registerScene`, and `index` grows with every connection), so the
    /// arithmetic is saturating: an overflowing config/index clamps the range
    /// to the top of the id space instead of silently wrapping (which would
    /// produce bogus/overlapping ranges and make `reclaim_range` tombstone
    /// another client's entities). A clamped range is empty (`size == 0` once
    /// `start` saturates to `u32::MAX`), so it allocates no usable ids — the
    /// scene has simply exhausted its entity space, which upstream also treats
    /// as a hard limit.
    pub fn range_for_client(&self, index: u32) -> (u32, u32) {
        let offset = (index as u64).saturating_mul(self.client_network_entities_limit as u64);
        let start = (self.reserved_local_entities as u64)
            .saturating_add(self.server_network_entities_limit as u64)
            .saturating_add(offset);
        if start >= u32::MAX as u64 {
            // No room left in the id space for this client.
            return (u32::MAX, 0);
        }
        let start = start as u32;
        // Clamp the window so start+size never overflows u32.
        let size = self.client_network_entities_limit.min(u32::MAX - start);
        (start, size)
    }
}

/// Safety/backpressure limits handed to a scene runtime at construction.
/// Sourced from [`crate::config::Config`]; isolated into its own struct so the
/// runtime layer doesn't depend on the whole config surface.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeLimits {
    pub js_heap_limit_mb: usize,
    pub js_tick_budget_ms: u64,
    pub js_shutdown_join_ms: u64,
    pub client_inbound_max: usize,
    pub client_outbound_max: usize,
    pub crdt_max_components: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            js_heap_limit_mb: 384,
            js_tick_budget_ms: 250,
            js_shutdown_join_ms: 2000,
            client_inbound_max: 1024,
            client_outbound_max: 1024,
            crdt_max_components: 100_000,
        }
    }
}

/// Snapshot a late-joining client receives in its `Init` frame.
pub struct InitState {
    pub start: u32,
    pub size: u32,
    pub reserved_local_entities: u32,
    pub crdt_state: Vec<u8>,
}

/// The pluggable scene-state engine. One instance per loaded scene.
pub trait SceneRuntime: Send + Sync {
    /// Content hash identifying the loaded scene (or `"localScene"`).
    fn scene_hash(&self) -> &str;

    /// Allocate the client's index. The runtime owns index allocation because
    /// it must stay consistent with the entity-range arithmetic (the JS runtime
    /// reuses indices independently of the `Scene` roster's keys).
    fn allocate_client_index(&self) -> u32;

    /// Notify the runtime that a client connected. `outbound` is the encoded-
    /// frame sink the WS task drains; the runtime keeps it so the scene can push
    /// per-client `Crdt` frames asynchronously. Returns the `Init` payload.
    fn on_client_open(
        &self,
        client_index: u32,
        outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState;

    /// Ingest an inbound `Crdt` frame body from a client. The runtime merges it
    /// into authoritative state and returns the set of outbound `Crdt` bodies
    /// to broadcast to *other* clients now (the relay's synchronous fan-out
    /// path). The JS runtime returns empty here and instead pushes through the
    /// per-client `outbound` senders from inside the scene's `onUpdate`.
    fn on_client_crdt(&self, client_index: u32, body: &[u8]) -> Vec<Vec<u8>>;

    /// Handle a client disconnect (free its range, GC its network entities).
    fn on_client_close(&self, client_index: u32);

    /// The current authoritative CRDT snapshot (the same bytes a late-joining
    /// client receives in its `Init` frame). Read-only; used by the admin
    /// CRDT-inspect route.
    fn snapshot(&self) -> Vec<u8>;
}

/// Transport-faithful relay with no scene logic. Used as a fallback for scenes
/// with no `game.js` (or when V8 is disabled). Now backed by a real SDK7 CRDT
/// engine so the snapshot stays deduplicated head state (no longer an unbounded
/// append buffer) and inbound batches are LWW-merged before fan-out.
pub struct RelayRuntime {
    scene_hash: String,
    config: ServerTransportConfig,
    engine: Mutex<crate::crdt::CrdtEngine>,
    next_index: std::sync::atomic::AtomicU32,
}

impl RelayRuntime {
    pub fn new(scene_hash: impl Into<String>, config: ServerTransportConfig) -> Self {
        Self {
            scene_hash: scene_hash.into(),
            config,
            engine: Mutex::new(crate::crdt::CrdtEngine::new()),
            next_index: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl SceneRuntime for RelayRuntime {
    fn scene_hash(&self) -> &str {
        &self.scene_hash
    }

    fn allocate_client_index(&self) -> u32 {
        self.next_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    fn on_client_open(
        &self,
        client_index: u32,
        _outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState {
        let (start, size) = self.config.range_for_client(client_index);
        InitState {
            start,
            size,
            reserved_local_entities: self.config.reserved_local_entities,
            crdt_state: self.engine.lock().snapshot(),
        }
    }

    fn on_client_crdt(&self, _client_index: u32, body: &[u8]) -> Vec<Vec<u8>> {
        // Real SDK7 LWW merge: decode the batch, apply it, and fan out only the
        // accepted (state-changing) messages to other clients.
        let msgs = crate::crdt::decode_batch(body);
        let accepted = self.engine.lock().apply_batch(&msgs);
        if accepted.is_empty() {
            vec![]
        } else {
            vec![crate::crdt::encode_batch(&accepted)]
        }
    }

    fn on_client_close(&self, client_index: u32) {
        // Reclaim the client's network entity range + GC its entities.
        let (start, size) = self.config.range_for_client(client_index);
        self.engine.lock().reclaim_range(start, size);
    }

    fn snapshot(&self) -> Vec<u8> {
        self.engine.lock().snapshot()
    }
}

/// The real SDK7 server runtime: runs the scene's `game.js` in V8. Thin adapter
/// over [`JsRuntimeHandle`] implementing [`SceneRuntime`].
pub struct JsRuntime {
    handle: JsRuntimeHandle,
}

impl JsRuntime {
    /// Spawn the JS thread for `source`. `realm_name` is reported by
    /// `~system/Runtime.getRealm()`. `limits` bounds the isolate's heap,
    /// per-tick wall-clock, per-client inbound queue, and CRDT cell count.
    pub fn new(
        scene_hash: impl Into<String>,
        source: String,
        realm_name: String,
        limits: RuntimeLimits,
        static_crdt: Vec<u8>,
    ) -> Self {
        let handle = jsruntime::spawn(scene_hash.into(), source, realm_name, limits, static_crdt);
        Self { handle }
    }
}

impl SceneRuntime for JsRuntime {
    fn scene_hash(&self) -> &str {
        &self.handle.scene_hash
    }

    fn allocate_client_index(&self) -> u32 {
        self.handle.next_client_index()
    }

    fn on_client_open(
        &self,
        client_index: u32,
        outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState {
        // Register the per-client outbound sender so the scene can push frames
        // asynchronously from its onUpdate tick, then notify the JS thread.
        self.handle.shared.outbound.insert(client_index, outbound);
        let _ = self.handle.tx.send(Command::ClientOpen {
            index: client_index,
        });
        let cfg = *self.handle.shared.config.lock();
        let (start, size) = cfg.range_for_client(client_index);
        InitState {
            start,
            size,
            reserved_local_entities: cfg.reserved_local_entities,
            crdt_state: self.handle.shared.snapshot.lock().clone(),
        }
    }

    fn on_client_crdt(&self, client_index: u32, body: &[u8]) -> Vec<Vec<u8>> {
        // The scene owns the merge + fan-out: hand the batch to the JS thread.
        // The scene's onUpdate will pull it via client.getMessages() and push
        // results through the per-client outbound senders. Nothing to broadcast
        // synchronously here.
        let _ = self.handle.tx.send(Command::ClientCrdt {
            index: client_index,
            body: body.to_vec(),
        });
        vec![]
    }

    fn on_client_close(&self, client_index: u32) {
        let _ = self.handle.tx.send(Command::ClientClose {
            index: client_index,
        });
        self.handle.shared.outbound.remove(&client_index);
    }

    fn snapshot(&self) -> Vec<u8> {
        self.handle.shared.snapshot.lock().clone()
    }
}

/// Wrap a per-client outbound CRDT body into a `Crdt` WS frame. The JS runtime's
/// `SharedState.outbound` senders carry *unframed* CRDT bytes; the WS task wraps
/// them. Exposed so the WS layer and the runtime agree on the framing point.
pub fn frame_crdt(body: &[u8]) -> Vec<u8> {
    crate::protocol::encode_message(crate::protocol::MessageType::Crdt, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ranges_match_upstream_example() {
        let cfg = ServerTransportConfig::default(); // 512/512/512
                                                    // reserved(512) + server(512) + index*512
        assert_eq!(cfg.range_for_client(0), (1024, 512));
        assert_eq!(cfg.range_for_client(1), (1536, 512));
        assert_eq!(cfg.range_for_client(2), (2048, 512));
    }

    #[test]
    fn relay_merges_and_snapshots() {
        use crate::crdt::{encode_batch, CrdtMessage};
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let idx = rt.allocate_client_index();
        assert_eq!(idx, 0);

        // A real CRDT batch (PUT entity 1100, component 1, ts 5).
        let put = CrdtMessage::Put {
            entity: 1100,
            component_id: 1,
            timestamp: 5,
            data: vec![1, 2, 3],
        };
        let body = encode_batch(std::slice::from_ref(&put));
        let out = rt.on_client_crdt(idx, &body);
        // Accepted -> fanned out (a single merged batch).
        assert_eq!(out.len(), 1);
        assert_eq!(crate::crdt::decode_batch(&out[0]), vec![put.clone()]);

        // An older write for the same cell is rejected (no fan-out).
        let stale = encode_batch(&[CrdtMessage::Put {
            entity: 1100,
            component_id: 1,
            timestamp: 4,
            data: vec![9],
        }]);
        assert!(rt.on_client_crdt(idx, &stale).is_empty());

        // Late joiner gets the deduplicated head state.
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let init = rt.on_client_open(rt.allocate_client_index(), tx);
        assert_eq!(crate::crdt::decode_batch(&init.crdt_state), vec![put]);
        assert_eq!(init.start, 1536); // client index 1
    }
}
