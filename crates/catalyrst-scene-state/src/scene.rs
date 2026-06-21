//! Loaded-scene registry and per-scene client roster.
//!
//! Port of `src/adapters/scene.ts` + `src/adapters/wsRegistry.ts` +
//! the `scenes: Map<string, ISceneComponent>` component (`src/types.ts`).
//!
//! Upstream keys scenes by *name* (`localScene`, or a world name like
//! `my-world.dcl.eth`) and assigns each connected client a monotonically
//! increasing integer index used both as its id and to compute its entity
//! range. We mirror that: [`SceneManager`] owns the name->[`Scene`] map, each
//! [`Scene`] owns a runtime + a roster of [`Client`] handles.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::runtime::SceneRuntime;

/// A connected client's server-side handle. The WS task owns the receive side;
/// this handle owns the send side, so the runtime/peers can push frames to it.
pub struct Client {
    pub index: u32,
    pub address: String,
    /// Outbound frames (already protocol-encoded) destined for this client.
    /// Bounded so a slow client can't make the queue grow without bound; a full
    /// queue drops the frame (best-effort fan-out) rather than blocking.
    pub tx: mpsc::Sender<Vec<u8>>,
}

pub struct Scene {
    pub name: String,
    pub runtime: Arc<dyn SceneRuntime>,
    clients: DashMap<u32, Arc<Client>>,
}

impl Scene {
    pub fn new(name: impl Into<String>, runtime: Arc<dyn SceneRuntime>) -> Self {
        Self {
            name: name.into(),
            runtime,
            clients: DashMap::new(),
        }
    }

    pub fn scene_hash(&self) -> String {
        self.runtime.scene_hash().to_string()
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Registers a freshly-authenticated client, returning its handle and the
    /// `Init` payload. The index is allocated by the runtime (so it stays
    /// consistent with entity-range arithmetic), and the runtime is handed the
    /// outbound sender so the scene can push frames asynchronously.
    pub fn add_client(
        &self,
        address: String,
        tx: mpsc::Sender<Vec<u8>>,
    ) -> (Arc<Client>, crate::runtime::InitState) {
        let index = self.runtime.allocate_client_index();
        let init = self.runtime.on_client_open(index, tx.clone());
        let client = Arc::new(Client { index, address, tx });
        self.clients.insert(index, Arc::clone(&client));
        (client, init)
    }

    /// Fan a frame out to every connected client except `except` (echo
    /// suppression). Frames are already protocol-encoded.
    pub fn broadcast(&self, frame: &[u8], except: u32) {
        for entry in self.clients.iter() {
            if *entry.key() == except {
                continue;
            }
            // Best-effort: a closed receiver just means that client is gone
            // (its WS task removes it on close); a full bounded queue means the
            // client is too slow — drop the frame rather than block the caller.
            let _ = entry.value().tx.try_send(frame.to_vec());
        }
    }

    pub fn remove_client(&self, index: u32) {
        self.clients.remove(&index);
        self.runtime.on_client_close(index);
    }

    /// Forcibly disconnect every connected client (admin kick-all). Removing a
    /// client from the roster drops the only [`Client::tx`] holder (plus, for
    /// the JS runtime, `on_client_close` drops the runtime's outbound sender),
    /// so the client's WS task sees its outbound channel close and breaks out
    /// of its relay loop, tearing down the socket. Returns the number kicked.
    pub fn kick_all(&self) -> usize {
        let indices: Vec<u32> = self.clients.iter().map(|e| *e.key()).collect();
        let n = indices.len();
        for index in indices {
            self.remove_client(index);
        }
        n
    }

    /// The current authoritative CRDT snapshot bytes (admin inspect).
    pub fn snapshot(&self) -> Vec<u8> {
        self.runtime.snapshot()
    }
}

/// The top-level scene registry. Equivalent to the upstream `scenes` map plus
/// the global WS connection counter.
pub struct SceneManager {
    scenes: Mutex<std::collections::HashMap<String, Arc<Scene>>>,
    connection_count: AtomicU32,
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneManager {
    pub fn new() -> Self {
        Self {
            scenes: Mutex::new(std::collections::HashMap::new()),
            connection_count: AtomicU32::new(0),
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<Scene>> {
        self.scenes.lock().get(name).cloned()
    }

    /// Insert or replace a loaded scene. Returns the previous scene if any
    /// (caller should stop it). Port of `loadOrReload`'s map mutation.
    pub fn insert(&self, name: impl Into<String>, scene: Arc<Scene>) -> Option<Arc<Scene>> {
        self.scenes.lock().insert(name.into(), scene)
    }

    pub fn remove(&self, name: &str) -> Option<Arc<Scene>> {
        self.scenes.lock().remove(name)
    }

    /// `(name:hash)` strings for the `/status` endpoint.
    pub fn loaded(&self) -> Vec<String> {
        self.scenes
            .lock()
            .iter()
            .map(|(name, scene)| format!("{name}:{}", scene.scene_hash()))
            .collect()
    }

    pub fn connections(&self) -> u32 {
        self.connection_count.load(Ordering::Relaxed)
    }

    pub fn on_ws_connected(&self) {
        self.connection_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_ws_closed(&self) {
        self.connection_count.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{RelayRuntime, ServerTransportConfig};

    #[test]
    fn add_client_assigns_increasing_indices() {
        let scene = Scene::new(
            "localScene",
            Arc::new(RelayRuntime::new("h", ServerTransportConfig::default())),
        );
        let (tx, _rx) = mpsc::channel(16);
        let (a, _ia) = scene.add_client("0x1".into(), tx.clone());
        let (b, _ib) = scene.add_client("0x2".into(), tx);
        assert_eq!(a.index, 0);
        assert_eq!(b.index, 1);
        assert_eq!(scene.client_count(), 2);
    }
}
