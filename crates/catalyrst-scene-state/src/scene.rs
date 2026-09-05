use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::runtime::SceneRuntime;

pub struct Client {
    pub index: u32,
    pub address: String,

    pub tx: mpsc::Sender<Vec<u8>>,

    /// The entity range this client was **actually given** at open time.
    ///
    /// Persisted rather than recomputed, because the scene can rewrite the
    /// transport config from `onUpdate` (`registerScene`). Recomputing at close
    /// time made a departing client's reclaim either leak its entities or wipe
    /// a live neighbour's; recomputing at message time made two live clients
    /// answer to the same window.
    pub start: u32,
    pub size: u32,
}

pub struct Scene {
    pub name: String,
    pub runtime: Arc<dyn SceneRuntime>,
    clients: DashMap<u32, Arc<Client>>,

    renewal: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Scene {
    pub fn new(name: impl Into<String>, runtime: Arc<dyn SceneRuntime>) -> Self {
        Self::new_with_renewal(name, runtime, None)
    }

    pub fn new_with_renewal(
        name: impl Into<String>,
        runtime: Arc<dyn SceneRuntime>,
        renewal: Option<tokio::task::JoinHandle<()>>,
    ) -> Self {
        Self {
            name: name.into(),
            runtime,
            clients: DashMap::new(),
            renewal: Mutex::new(renewal),
        }
    }

    pub fn scene_hash(&self) -> String {
        self.runtime.scene_hash().to_string()
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn add_client(
        &self,
        address: String,
        tx: mpsc::Sender<Vec<u8>>,
    ) -> (Arc<Client>, crate::runtime::InitState) {
        let index = self.runtime.allocate_client_index();
        let init = self.runtime.on_client_open(index, tx.clone());
        let client = Arc::new(Client {
            index,
            address,
            tx,
            start: init.start,
            size: init.size,
        });
        self.clients.insert(index, Arc::clone(&client));
        (client, init)
    }

    /// The range `index` was assigned at open time, if it is still connected.
    pub fn client_range(&self, index: u32) -> Option<(u32, u32)> {
        self.clients.get(&index).map(|c| (c.start, c.size))
    }

    pub fn broadcast(&self, frame: &[u8], except: u32) {
        for entry in self.clients.iter() {
            if *entry.key() == except {
                continue;
            }

            let _ = entry.value().tx.try_send(frame.to_vec());
        }
    }

    pub fn remove_client(&self, index: u32) {
        self.clients.remove(&index);

        // Whatever the runtime reclaimed for the departing client is state the
        // rest of the room has to be told about: without this the players still
        // connected keep rendering a departed player's objects forever, while a
        // fresh joiner served from `snapshot()` never sees them.
        for body in self.runtime.on_client_close(index) {
            let frame = crate::runtime::frame_crdt(&body);
            self.broadcast(&frame, index);
        }
    }

    pub fn kick_all(&self) -> usize {
        let indices: Vec<u32> = self.clients.iter().map(|e| *e.key()).collect();
        let n = indices.len();
        for index in indices {
            self.remove_client(index);
        }
        n
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.runtime.snapshot()
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        if let Some(task) = self.renewal.lock().take() {
            task.abort();
        }
    }
}

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

    pub fn insert(&self, name: impl Into<String>, scene: Arc<Scene>) -> Option<Arc<Scene>> {
        self.scenes.lock().insert(name.into(), scene)
    }

    pub fn remove(&self, name: &str) -> Option<Arc<Scene>> {
        self.scenes.lock().remove(name)
    }

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

    /// D3. The assigned range travels with the client, so nothing downstream
    /// has to re-derive it from a config the scene can move.
    #[test]
    fn client_carries_the_range_it_was_given() {
        let scene = Scene::new(
            "localScene",
            Arc::new(RelayRuntime::new("h", ServerTransportConfig::default())),
        );
        let (tx, _rx) = mpsc::channel(16);
        let (a, ia) = scene.add_client("0x1".into(), tx.clone());
        let (b, ib) = scene.add_client("0x2".into(), tx);

        assert_eq!((a.start, a.size), (ia.start, ia.size));
        assert_eq!((b.start, b.size), (ib.start, ib.size));
        assert_eq!(scene.client_range(a.index), Some((1024, 512)));
        assert_eq!(scene.client_range(b.index), Some((1536, 512)));
        assert_ne!((a.start, a.size), (b.start, b.size));
    }

    /// The refusal condition `ws::handle_socket` checks: a client for which no
    /// representable range is left is handed `EMPTY_RANGE`, whose `size` is 0. It
    /// can author nothing (`decode_client_batch` admits nothing,
    /// `Authority::Client` rejects everything), so the socket layer now closes
    /// rather than serving a connected-but-mute client -- and the slot must come
    /// straight back, or one refusal would burn it forever.
    #[test]
    fn a_client_with_no_representable_range_gets_an_empty_one_and_frees_its_slot() {
        use crate::runtime::EMPTY_RANGE;

        // a config where the server band alone consumes the whole number space
        let cfg = ServerTransportConfig {
            reserved_local_entities: 512,
            server_network_entities_limit: u32::MAX - 512,
            client_network_entities_limit: 512,
        };
        assert_eq!(cfg.max_client_slots(), 0);

        let scene = Scene::new("localScene", Arc::new(RelayRuntime::new("h", cfg)));
        let (tx, _rx) = mpsc::channel(16);
        let (client, init) = scene.add_client("0x1".into(), tx.clone());
        assert_eq!((init.start, init.size), EMPTY_RANGE);
        assert_eq!(init.size, 0, "this is what ws.rs refuses on");

        // refusal path: remove_client returns the slot, so the next attempt is
        // handed index 0 again rather than walking the index space
        scene.remove_client(client.index);
        assert_eq!(scene.client_count(), 0);
        let (again, _) = scene.add_client("0x2".into(), tx);
        assert_eq!(again.index, client.index);
    }

    /// D4. `remove_client` must push the reclaim out to everyone still here.
    #[test]
    fn removing_a_client_broadcasts_the_reclaim() {
        use crate::crdt::{decode_batch, encode_batch, CrdtMessage};

        let scene = Scene::new(
            "localScene",
            Arc::new(RelayRuntime::new("h", ServerTransportConfig::default())),
        );
        let (tx_a, _rx_a) = mpsc::channel(16);
        let (tx_b, mut rx_b) = mpsc::channel(16);
        let (a, _) = scene.add_client("0x1".into(), tx_a);
        let (_b, _) = scene.add_client("0x2".into(), tx_b);

        let body = encode_batch(&[CrdtMessage::Put {
            entity: 1100,
            component_id: 7,
            timestamp: 1,
            data: vec![1],
        }]);
        assert_eq!(scene.runtime.on_client_crdt(a.index, &body).len(), 1);

        scene.remove_client(a.index);

        let frame = rx_b
            .try_recv()
            .expect("the remaining client must be told the departed range is gone");
        assert_eq!(frame[0], crate::protocol::MessageType::Crdt as u8);
        assert_eq!(
            decode_batch(&frame[1..]),
            vec![CrdtMessage::DeleteEntity { entity: 1100 }]
        );
    }
}
