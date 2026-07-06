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
        let client = Arc::new(Client { index, address, tx });
        self.clients.insert(index, Arc::clone(&client));
        (client, init)
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
        self.runtime.on_client_close(index);
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

// Reloads replace the Scene in the manager; aborting here keeps replaced scenes
// from accumulating orphan delegation-renewal loops.
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
}
