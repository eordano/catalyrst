use parking_lot::Mutex;

use crate::jsruntime::{self, Command, JsRuntimeHandle};

#[derive(Debug, Clone, Copy)]
pub struct ServerTransportConfig {
    pub reserved_local_entities: u32,

    pub server_network_entities_limit: u32,

    pub client_network_entities_limit: u32,
}

impl Default for ServerTransportConfig {
    fn default() -> Self {
        Self {
            reserved_local_entities: 512,
            server_network_entities_limit: 512,
            client_network_entities_limit: 512,
        }
    }
}

impl ServerTransportConfig {
    pub fn range_for_client(&self, index: u32) -> (u32, u32) {
        let offset = (index as u64).saturating_mul(self.client_network_entities_limit as u64);
        let start = (self.reserved_local_entities as u64)
            .saturating_add(self.server_network_entities_limit as u64)
            .saturating_add(offset);
        if start >= u32::MAX as u64 {
            return (u32::MAX, 0);
        }
        let start = start as u32;

        let size = self.client_network_entities_limit.min(u32::MAX - start);
        (start, size)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeLimits {
    pub js_heap_limit_mb: usize,
    pub js_tick_budget_ms: u64,
    pub js_shutdown_join_ms: u64,
    pub js_update_failure_cap: usize,
    pub client_inbound_max: usize,
    pub client_outbound_max: usize,
    pub crdt_max_components: usize,
    pub fetch_max_response_bytes: usize,
    pub fetch_max_body_bytes: usize,
    pub fetch_max_in_flight: usize,
    pub fetch_timeout_ms: u64,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            js_heap_limit_mb: 384,
            js_tick_budget_ms: 250,
            js_shutdown_join_ms: 2000,
            js_update_failure_cap: 30,
            client_inbound_max: 1024,
            client_outbound_max: 1024,
            crdt_max_components: 100_000,
            fetch_max_response_bytes: 2 * 1024 * 1024,
            fetch_max_body_bytes: 1024 * 1024,
            fetch_max_in_flight: 8,
            fetch_timeout_ms: 10_000,
        }
    }
}

pub struct InitState {
    pub start: u32,
    pub size: u32,
    pub reserved_local_entities: u32,
    pub crdt_state: Vec<u8>,
}

pub trait SceneRuntime: Send + Sync {
    fn scene_hash(&self) -> &str;

    fn allocate_client_index(&self) -> u32;

    fn on_client_open(
        &self,
        client_index: u32,
        outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState;

    fn on_client_crdt(&self, client_index: u32, body: &[u8]) -> Vec<Vec<u8>>;

    fn on_client_close(&self, client_index: u32);

    fn snapshot(&self) -> Vec<u8>;
}

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

    fn on_client_crdt(&self, client_index: u32, body: &[u8]) -> Vec<Vec<u8>> {
        let (start, size) = self.config.range_for_client(client_index);
        let msgs = crate::crdt::decode_client_batch(body, start, size);
        let accepted = self.engine.lock().apply_batch(&msgs);
        if accepted.is_empty() {
            vec![]
        } else {
            vec![crate::crdt::encode_batch(&accepted)]
        }
    }

    fn on_client_close(&self, client_index: u32) {
        let (start, size) = self.config.range_for_client(client_index);
        self.engine.lock().reclaim_range(start, size);
    }

    fn snapshot(&self) -> Vec<u8> {
        self.engine.lock().snapshot()
    }
}

pub struct JsRuntime {
    handle: JsRuntimeHandle,
}

impl JsRuntime {
    pub fn new(
        scene_hash: impl Into<String>,
        source: String,
        realm_name: String,
        limits: RuntimeLimits,
        static_crdt: Vec<u8>,
        storage: Option<jsruntime::StorageCtx>,
    ) -> Self {
        let handle = jsruntime::spawn(
            scene_hash.into(),
            source,
            realm_name,
            limits,
            static_crdt,
            storage,
        );
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

pub fn frame_crdt(body: &[u8]) -> Vec<u8> {
    crate::protocol::encode_message(crate::protocol::MessageType::Crdt, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ranges_match_upstream_example() {
        let cfg = ServerTransportConfig::default();

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

        let put = CrdtMessage::Put {
            entity: 1100,
            component_id: 1,
            timestamp: 5,
            data: vec![1, 2, 3],
        };
        let body = encode_batch(std::slice::from_ref(&put));
        let out = rt.on_client_crdt(idx, &body);

        assert_eq!(out.len(), 1);
        assert_eq!(crate::crdt::decode_batch(&out[0]), vec![put.clone()]);

        let stale = encode_batch(&[CrdtMessage::Put {
            entity: 1100,
            component_id: 1,
            timestamp: 4,
            data: vec![9],
        }]);
        assert!(rt.on_client_crdt(idx, &stale).is_empty());

        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let init = rt.on_client_open(rt.allocate_client_index(), tx);
        assert_eq!(crate::crdt::decode_batch(&init.crdt_state), vec![put]);
        assert_eq!(init.start, 1536);
    }

    #[test]
    fn relay_rejects_out_of_range_client_ops() {
        use crate::crdt::{encode_batch, CrdtMessage};
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let idx = rt.allocate_client_index();

        let out_of_range = encode_batch(&[
            CrdtMessage::Put {
                entity: 5,
                component_id: 1,
                timestamp: 1,
                data: vec![1],
            },
            CrdtMessage::Put {
                entity: 2048,
                component_id: 1,
                timestamp: 1,
                data: vec![2],
            },
            CrdtMessage::DeleteEntity {
                entity: 4_000_000_000,
            },
        ]);
        assert!(rt.on_client_crdt(idx, &out_of_range).is_empty());
        assert!(rt.snapshot().is_empty());

        let in_range = encode_batch(&[CrdtMessage::Put {
            entity: 1030,
            component_id: 1,
            timestamp: 1,
            data: vec![3],
        }]);
        assert_eq!(rt.on_client_crdt(idx, &in_range).len(), 1);
    }
}
