use std::collections::{BTreeMap, BTreeSet};

use parking_lot::Mutex;

use crate::crdt::{entity_number, CrdtMessage};
use crate::jsruntime::{self, Command, JsRuntimeHandle};

/// Scene entity ids travel the wire packed as `number | generation << 16`
/// (bevy `dcl_component/src/lib.rs` `SceneEntityId::as_proto_u32`, `@dcl/ecs`
/// `entity.ts`), so only `2^16` entity **numbers** exist. A client range whose
/// end runs past this hands the client ids that collide with the generation
/// field -- they are not representable and must never be allocated.
pub const ENTITY_NUMBER_SPACE: u32 = 1 << 16;

/// Entity numbers the renderer owns *locally on every client*: ROOT(0),
/// PLAYER(1), CAMERA(2), WORLD_ORIGIN(5) and the foreign-player block `6..=405`
/// (bevy `dcl_component/src/lib.rs`, `SceneEntityId::{ROOT, PLAYER, CAMERA,
/// WORLD_ORIGIN, FOREIGN_PLAYER_RANGE}`). Nothing on the server may author
/// these into the shared CRDT state whatever `reservedLocalEntities` says --
/// they resolve to a *different* entity on each client, so a network write
/// there is a cross-boundary breach (e.g. forcing every player's avatar
/// transform, which `RestrictedActions` is deliberately stubbed out to prevent,
/// see `jsruntime/scene_thread.rs` `op_empty_promise`).
///
/// `@dcl/ecs` `entity.ts` `RESERVED_STATIC_ENTITIES = 512` is the *default*
/// reservation; this constant is the part that is not configurable.
pub const RENDERER_LOCAL_ENTITIES: u32 = 406;

/// `SceneComponentId::TRANSFORM` -- bevy `dcl_component/src/lib.rs`.
pub const TRANSFORM_COMPONENT_ID: u32 = 1;

/// `DclTransformAndParent` is `3xf32` translation, `4xf32` rotation, `3xf32`
/// scale, then the parent id (bevy
/// `dcl_component/src/transform_and_parent.rs` `FromDclReader`/`ToDclWriter`),
/// so the parent is a little-endian `u32` at byte 40 of a 44-byte payload.
pub const TRANSFORM_PARENT_OFFSET: usize = 40;
/// Serialized length of `DclTransformAndParent`.
pub const TRANSFORM_PAYLOAD_LEN: usize = 44;

/// The range handed to a client for which no representable slot is free: it
/// owns nothing. `decode_client_batch` admits no entity against it (`e >= MAX
/// && e < MAX` is empty) and `reclaim_range` touches nothing, so an overflow
/// client is inert rather than aliasing somebody else's entities.
pub const EMPTY_RANGE: (u32, u32) = (u32::MAX, 0);

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
    /// First entity number a client range can start at.
    fn client_base(&self) -> u64 {
        self.reserved_local_entities as u64 + self.server_network_entities_limit as u64
    }

    /// How many client slots have a range that fits entirely inside the
    /// representable entity-number space. With the default config this is
    /// **126** (`(65536 - 1024) / 512`), i.e. indices `0..=125`; index 126 used
    /// to be handed `(65536, 512)`, which is off the end of the id space.
    pub fn max_client_slots(&self) -> u32 {
        let base = self.client_base();
        let limit = self.client_network_entities_limit as u64;
        if limit == 0 || base >= ENTITY_NUMBER_SPACE as u64 {
            return 0;
        }
        ((ENTITY_NUMBER_SPACE as u64 - base) / limit) as u32
    }

    /// The half-open range `[start, start+size)` for `index`, or `None` when
    /// the index has no representable slot. Callers that cannot refuse the
    /// connection must fall back to [`EMPTY_RANGE`] and say so loudly.
    pub fn try_range_for_client(&self, index: u32) -> Option<(u32, u32)> {
        if index >= self.max_client_slots() {
            return None;
        }
        let start = self.client_base() + index as u64 * self.client_network_entities_limit as u64;

        debug_assert!(
            start + self.client_network_entities_limit as u64 <= ENTITY_NUMBER_SPACE as u64
        );
        Some((start as u32, self.client_network_entities_limit))
    }

    pub fn range_for_client(&self, index: u32) -> (u32, u32) {
        self.try_range_for_client(index).unwrap_or(EMPTY_RANGE)
    }

    /// The band the scene itself writes: `[reserved, reserved + serverLimit)`.
    pub fn server_range(&self) -> (u32, u32) {
        (
            self.reserved_local_entities,
            self.server_network_entities_limit,
        )
    }

    /// Lowest entity number anything on this server may author into the shared
    /// state. Never below [`RENDERER_LOCAL_ENTITIES`], so a scene cannot open
    /// the renderer's local block up by declaring `reservedLocalEntities: 0`.
    pub fn network_floor(&self) -> u32 {
        self.reserved_local_entities.max(RENDERER_LOCAL_ENTITIES)
    }
}

/// Who is authoring a batch of CRDT messages.
///
/// This is the authority model the server actually enforces. It is checked at
/// **every** boundary where messages reach the engine, not only where they are
/// queued for the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// The scene's own JS (`EngineApi.crdtSendToRenderer`, and the relay
    /// `client.sendCrdtMessage`). The scene is the authority for the scene, so
    /// it may write any *network* entity -- but not the renderer-local block,
    /// which belongs to each client's own engine.
    Server,
    /// A connected client, confined to the half-open range it was assigned when
    /// it connected. The range is persisted per-client, never recomputed from a
    /// config the scene can change underneath it.
    Client { start: u32, size: u32 },
}

impl Authority {
    /// May this author touch `entity`? `entity` is the PACKED id off the wire
    /// (`number | generation << 16`); `floor` is
    /// [`ServerTransportConfig::network_floor`].
    ///
    /// Both the floor and the client ranges are expressed in entity **NUMBERS**
    /// ([`ServerTransportConfig::try_range_for_client`] carves up
    /// [`ENTITY_NUMBER_SPACE`]), so the comparison is on `entity_number(entity)`
    /// -- the same convention [`crate::crdt::decode_client_batch`],
    /// [`crate::crdt::CrdtEngine::reclaim_range`] and
    /// [`AuthorityGuard::in_foreign_range`] use.
    ///
    /// Comparing the packed id, as this used to, was wrong in BOTH directions:
    ///
    /// - too strict: a recycled entity carries `generation >= 1`, which packs it
    ///   at `number + 65536` -- above every real range -- so a client could never
    ///   write to an entity it had recycled. `decode_client_batch` was fixed to
    ///   compare numbers; this check ran after it and re-imposed the old rule,
    ///   which left the recycled-entity defect live on the client path.
    /// - too lax: the renderer-local floor was bypassed by the same arithmetic.
    ///   Entity number 5 (`WORLD_ORIGIN`) at generation 1 packs to 65541, which
    ///   is `>= floor`, so a scene (or a client whose range happened to contain
    ///   65541) could author a renderer-local entity just by bumping the
    ///   generation.
    pub fn may_write(&self, floor: u32, entity: u32) -> bool {
        let number = entity_number(entity) as u32;
        if number < floor {
            return false;
        }
        match *self {
            Authority::Server => true,
            Authority::Client { start, size } => {
                let end = start.saturating_add(size);
                number >= start && number < end
            }
        }
    }
}

/// Reads `DclTransformAndParent::parent` out of an opaque component payload.
/// Returns `None` for any component that is not a transform, or for a payload
/// too short to contain one.
pub fn transform_parent(component_id: u32, data: &[u8]) -> Option<u32> {
    if component_id != TRANSFORM_COMPONENT_ID || data.len() < TRANSFORM_PAYLOAD_LEN {
        return None;
    }
    let bytes = &data[TRANSFORM_PARENT_OFFSET..TRANSFORM_PARENT_OFFSET + 4];
    Some(u32::from_le_bytes(bytes.try_into().unwrap()))
}

/// The single place a CRDT batch is checked against the authority of whoever
/// sent it. Both runtimes and every JS op route through this.
pub struct AuthorityGuard<'a> {
    pub authority: Authority,
    pub floor: u32,
    /// Ranges assigned to *other* live clients. Used for the scene-graph check;
    /// empty for [`Authority::Server`], which is allowed to relay.
    pub foreign_ranges: &'a [(u32, u32)],
}

impl AuthorityGuard<'_> {
    fn in_foreign_range(&self, entity_number: u32) -> bool {
        self.foreign_ranges.iter().any(|(start, size)| {
            let end = start.saturating_add(*size);
            entity_number >= *start && entity_number < end
        })
    }

    /// Is `msg` within this author's authority?
    ///
    /// Two rules:
    /// 1. the target entity must be writable by the author (range + floor);
    /// 2. for `DclTransformAndParent`, the *parent* named inside the payload
    ///    must not be an entity owned by a different client. Authority is over
    ///    component cells, and the scene graph lives inside a payload the
    ///    engine otherwise never parses -- without (2) an in-range write on your
    ///    own entity can graft it onto a neighbour's.
    ///
    /// TODO(owner-decision): (2) is the fail-closed reading of "authority
    /// covers the scene graph". It rejects cross-client parenting outright;
    /// parenting to ROOT, to any renderer-local entity, to the server band and
    /// within your own range all stay legal. If a game mode genuinely needs
    /// client-to-client parenting (hand-off / carry mechanics), that wants an
    /// explicit transfer op rather than re-opening this.
    pub fn admits(&self, msg: &CrdtMessage) -> bool {
        if !self.authority.may_write(self.floor, msg.entity()) {
            return false;
        }
        let (component_id, data) = match msg {
            CrdtMessage::Put {
                component_id, data, ..
            }
            | CrdtMessage::Append {
                component_id, data, ..
            } => (*component_id, data.as_slice()),
            _ => return true,
        };
        match transform_parent(component_id, data) {
            // Ranges are expressed in entity *numbers*; a packed parent carries
            // its generation in the high 16 bits, so compare on the number.
            Some(parent) => !self.in_foreign_range(parent & 0xFFFF),
            None => true,
        }
    }

    pub fn filter(&self, msgs: Vec<CrdtMessage>) -> Vec<CrdtMessage> {
        msgs.into_iter().filter(|m| self.admits(m)).collect()
    }
}

/// Hands out client slot indices and takes them back on disconnect.
///
/// Indices are a bounded resource: only
/// [`ServerTransportConfig::max_client_slots`] of them have a representable
/// entity range. The previous `AtomicU32::fetch_add` never freed one, so 127
/// *cumulative* connections (reconnects included) walked off the end, and at
/// `2^32` it wrapped and reissued index 0 to a new client on top of the
/// incumbent's live entities. Reusing the lowest free slot makes both
/// unreachable.
#[derive(Debug, Default)]
pub struct ClientSlots {
    in_use: Mutex<BTreeSet<u32>>,
}

impl ClientSlots {
    /// Lowest index not currently held. Only indices below
    /// `max_client_slots()` have a usable range; a higher one is still returned
    /// (and is still unique, so it cannot alias another client) when every slot
    /// is concurrently occupied.
    pub fn acquire(&self) -> u32 {
        let mut used = self.in_use.lock();
        let mut index = 0u32;
        while used.contains(&index) {
            match index.checked_add(1) {
                Some(next) => index = next,
                None => break,
            }
        }
        used.insert(index);
        index
    }

    pub fn release(&self, index: u32) {
        self.in_use.lock().remove(&index);
    }

    pub fn clear(&self) {
        self.in_use.lock().clear();
    }

    pub fn live(&self) -> usize {
        self.in_use.lock().len()
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

    /// Tear down `client_index`. Returns already-encoded CRDT bodies that the
    /// caller **must broadcast to the remaining clients** -- the reclaim of a
    /// departed client's range is state the room has to be told about, or the
    /// players still connected keep rendering a ghost forever while a fresh
    /// joiner (served from `snapshot()`) does not see it.
    #[must_use]
    fn on_client_close(&self, client_index: u32) -> Vec<Vec<u8>>;

    fn snapshot(&self) -> Vec<u8>;
}

pub struct RelayRuntime {
    scene_hash: String,
    config: ServerTransportConfig,
    engine: Mutex<crate::crdt::CrdtEngine>,
    slots: ClientSlots,
    /// Range each live client was actually given, so nothing is ever recomputed
    /// from a config read at a different time than the one the client was told.
    assigned: Mutex<BTreeMap<u32, (u32, u32)>>,
}

impl RelayRuntime {
    pub fn new(scene_hash: impl Into<String>, config: ServerTransportConfig) -> Self {
        Self {
            scene_hash: scene_hash.into(),
            config,
            engine: Mutex::new(crate::crdt::CrdtEngine::new()),
            slots: ClientSlots::default(),
            assigned: Mutex::new(BTreeMap::new()),
        }
    }

    fn foreign_ranges(&self, except: u32) -> Vec<(u32, u32)> {
        self.assigned
            .lock()
            .iter()
            .filter(|(index, _)| **index != except)
            .map(|(_, range)| *range)
            .collect()
    }
}

impl SceneRuntime for RelayRuntime {
    fn scene_hash(&self) -> &str {
        &self.scene_hash
    }

    fn allocate_client_index(&self) -> u32 {
        self.slots.acquire()
    }

    fn on_client_open(
        &self,
        client_index: u32,
        _outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState {
        let (start, size) = match self.config.try_range_for_client(client_index) {
            Some(range) => range,
            None => {
                tracing::error!(
                    scene = %self.scene_hash,
                    index = client_index,
                    slots = self.config.max_client_slots(),
                    "no representable entity range left; client admitted owning nothing"
                );
                EMPTY_RANGE
            }
        };
        self.assigned.lock().insert(client_index, (start, size));
        InitState {
            start,
            size,
            reserved_local_entities: self.config.reserved_local_entities,
            crdt_state: self.engine.lock().snapshot(),
        }
    }

    fn on_client_crdt(&self, client_index: u32, body: &[u8]) -> Vec<Vec<u8>> {
        let Some((start, size)) = self.assigned.lock().get(&client_index).copied() else {
            tracing::warn!(
                scene = %self.scene_hash,
                index = client_index,
                "crdt from a client with no assigned range; dropped"
            );
            return vec![];
        };
        let foreign = self.foreign_ranges(client_index);
        let guard = AuthorityGuard {
            authority: Authority::Client { start, size },
            floor: self.config.network_floor(),
            foreign_ranges: &foreign,
        };
        let msgs = guard.filter(crate::crdt::decode_client_batch(body, start, size));
        if msgs.is_empty() {
            return vec![];
        }
        let accepted = self.engine.lock().apply_batch(&msgs);
        if accepted.is_empty() {
            vec![]
        } else {
            vec![crate::crdt::encode_batch(&accepted)]
        }
    }

    fn on_client_close(&self, client_index: u32) -> Vec<Vec<u8>> {
        let assigned = self.assigned.lock().remove(&client_index);
        self.slots.release(client_index);
        let Some((start, size)) = assigned else {
            return vec![];
        };
        let deletes = self.engine.lock().reclaim_range(start, size);
        if deletes.is_empty() {
            vec![]
        } else {
            vec![crate::crdt::encode_batch(&deletes)]
        }
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
        self.handle.acquire_client_index()
    }

    fn on_client_open(
        &self,
        client_index: u32,
        outbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> InitState {
        self.handle.shared.outbound.insert(client_index, outbound);
        let cfg = *self.handle.shared.config.lock();
        let (start, size) = match cfg.try_range_for_client(client_index) {
            Some(range) => range,
            None => {
                tracing::error!(
                    scene = %self.handle.scene_hash,
                    index = client_index,
                    slots = cfg.max_client_slots(),
                    "no representable entity range left; client admitted owning nothing"
                );
                EMPTY_RANGE
            }
        };

        let _ = self.handle.tx.send(Command::ClientOpen {
            index: client_index,
            start,
            size,
        });
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

    fn on_client_close(&self, client_index: u32) -> Vec<Vec<u8>> {
        let _ = self.handle.tx.send(Command::ClientClose {
            index: client_index,
        });
        self.handle.shared.outbound.remove(&client_index);

        // The slot is normally released by the scene thread *after* it has
        // reclaimed the range, so a reconnect cannot be handed the departing
        // client's range while its entities are still live. If the thread is
        // gone nobody would ever release it, so do it here.
        if !self
            .handle
            .shared
            .running
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.handle.shared.slots.release(client_index);
        }

        // The JS thread broadcasts its own reclaim through each remaining
        // client's outbound queue (`scene_thread.rs::deliver_client_events`).
        vec![]
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
    use crate::crdt::{decode_batch, encode_batch, CrdtMessage};

    fn open(rt: &RelayRuntime) -> u32 {
        let index = rt.allocate_client_index();
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let _ = rt.on_client_open(index, tx);
        index
    }

    /// Live component state in the snapshot, ignoring whatever tombstone
    /// bookkeeping `crdt.rs` chooses to carry alongside it.
    fn live(rt: &RelayRuntime) -> Vec<CrdtMessage> {
        decode_batch(&rt.snapshot())
            .into_iter()
            .filter(|m| matches!(m, CrdtMessage::Put { .. } | CrdtMessage::Append { .. }))
            .collect()
    }

    fn transform_with_parent(parent: u32) -> Vec<u8> {
        let mut data = vec![0u8; TRANSFORM_PAYLOAD_LEN];
        data[TRANSFORM_PARENT_OFFSET..TRANSFORM_PARENT_OFFSET + 4]
            .copy_from_slice(&parent.to_le_bytes());
        data
    }

    #[test]
    fn client_ranges_match_upstream_example() {
        let cfg = ServerTransportConfig::default();

        assert_eq!(cfg.range_for_client(0), (1024, 512));
        assert_eq!(cfg.range_for_client(1), (1536, 512));
        assert_eq!(cfg.range_for_client(2), (2048, 512));
    }

    /// D2. `range_for_client(126)` used to return `(65536, 512)` -- off the end
    /// of the 2^16 entity-number space. The last representable slot is 125.
    #[test]
    fn ranges_never_run_off_the_entity_number_space() {
        let cfg = ServerTransportConfig::default();
        assert_eq!(cfg.max_client_slots(), 126);

        for index in 0..cfg.max_client_slots() {
            let (start, size) = cfg.try_range_for_client(index).expect("slot must exist");
            assert!(
                start as u64 + size as u64 <= ENTITY_NUMBER_SPACE as u64,
                "index {index} -> ({start}, {size}) runs past the id space"
            );
            assert_eq!(size, cfg.client_network_entities_limit);
        }
        assert_eq!(cfg.try_range_for_client(125), Some((65024, 512)));
        assert_eq!(cfg.try_range_for_client(126), None);
        assert_eq!(cfg.range_for_client(126), EMPTY_RANGE);

        // The far-out cases from the model -- index 8388605 used to get a short
        // 511-wide range and >= 8388606 a silently muted one. Every index past
        // the last slot is now explicitly, loudly the empty range.
        assert_eq!(cfg.try_range_for_client(8_388_605), None);
        assert_eq!(cfg.try_range_for_client(8_388_606), None);
        assert_eq!(cfg.try_range_for_client(u32::MAX), None);
    }

    /// A degenerate config must produce no slot at all rather than a zero-width
    /// or overlapping one.
    #[test]
    fn degenerate_configs_have_no_slots() {
        let zero_clients = ServerTransportConfig {
            client_network_entities_limit: 0,
            ..Default::default()
        };
        assert_eq!(zero_clients.max_client_slots(), 0);
        assert_eq!(zero_clients.range_for_client(0), EMPTY_RANGE);

        let huge_server = ServerTransportConfig {
            reserved_local_entities: 512,
            server_network_entities_limit: u32::MAX - 512,
            client_network_entities_limit: 512,
        };
        assert_eq!(huge_server.max_client_slots(), 0);
        assert_eq!(huge_server.range_for_client(0), EMPTY_RANGE);
    }

    /// D2. Indices are a bounded resource and must come back on disconnect --
    /// otherwise 127 *cumulative* connections exhaust the space, and the old
    /// wrapping `fetch_add` reissued index 0 at `2^32`.
    #[test]
    fn client_slots_are_reused_after_disconnect() {
        let slots = ClientSlots::default();
        let a = slots.acquire();
        let b = slots.acquire();
        assert_eq!((a, b), (0, 1));

        slots.release(a);
        assert_eq!(slots.acquire(), 0, "a freed slot must be handed out again");
        assert_eq!(slots.acquire(), 2);
        assert_eq!(slots.live(), 3);
    }

    #[test]
    fn relay_reuses_the_index_of_a_departed_client() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let first = open(&rt);
        assert_eq!(first, 0);
        let _ = rt.on_client_close(first);

        let again = open(&rt);
        assert_eq!(
            again, 0,
            "reconnects must recycle slots; 127 cumulative connections used to \
             walk off the end of the entity-number space"
        );
    }

    #[test]
    fn relay_merges_and_snapshots() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let idx = open(&rt);
        assert_eq!(idx, 0);

        let put = CrdtMessage::Put {
            entity: 1100,
            component_id: 7,
            timestamp: 5,
            data: vec![1, 2, 3],
        };
        let body = encode_batch(std::slice::from_ref(&put));
        let out = rt.on_client_crdt(idx, &body);

        assert_eq!(out.len(), 1);
        assert_eq!(decode_batch(&out[0]), vec![put.clone()]);

        let stale = encode_batch(&[CrdtMessage::Put {
            entity: 1100,
            component_id: 7,
            timestamp: 4,
            data: vec![9],
        }]);
        assert!(rt.on_client_crdt(idx, &stale).is_empty());

        let second = open(&rt);
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let init = rt.on_client_open(second, tx);
        let joined: Vec<CrdtMessage> = decode_batch(&init.crdt_state)
            .into_iter()
            .filter(|m| matches!(m, CrdtMessage::Put { .. }))
            .collect();
        assert_eq!(joined, vec![put]);
        assert_eq!(init.start, 1536);
    }

    #[test]
    fn relay_rejects_out_of_range_client_ops() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let idx = open(&rt);

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
        assert!(live(&rt).is_empty());

        let in_range = encode_batch(&[CrdtMessage::Put {
            entity: 1030,
            component_id: 7,
            timestamp: 1,
            data: vec![3],
        }]);
        assert_eq!(rt.on_client_crdt(idx, &in_range).len(), 1);
    }

    /// D4. The departing client's reclaim has to reach the room, not be
    /// computed and dropped on the floor.
    #[test]
    fn relay_broadcasts_the_reclaim_of_a_departed_client() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let a = open(&rt);
        let _b = open(&rt);

        let body = encode_batch(&[
            CrdtMessage::Put {
                entity: 1100,
                component_id: 7,
                timestamp: 1,
                data: vec![1],
            },
            CrdtMessage::Put {
                entity: 1101,
                component_id: 7,
                timestamp: 1,
                data: vec![2],
            },
        ]);
        assert_eq!(rt.on_client_crdt(a, &body).len(), 1);

        let out = rt.on_client_close(a);
        assert_eq!(
            out.len(),
            1,
            "close must hand back the DeleteEntity batch for the room"
        );
        let mut deletes = decode_batch(&out[0]);
        deletes.sort_by_key(|m| m.entity());
        assert_eq!(
            deletes,
            vec![
                CrdtMessage::DeleteEntity { entity: 1100 },
                CrdtMessage::DeleteEntity { entity: 1101 },
            ]
        );
        assert!(live(&rt).is_empty());
    }

    /// D6. An in-range write whose transform payload names a parent inside
    /// another live client's range is a scene-graph write outside the author's
    /// authority.
    #[test]
    fn client_cannot_reparent_onto_another_clients_entity() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let a = open(&rt);
        let b = open(&rt);
        assert_eq!(rt.assigned.lock().get(&b).copied(), Some((1536, 512)));

        let graft = encode_batch(&[CrdtMessage::Put {
            entity: 1100, // client a's own entity: in range
            component_id: TRANSFORM_COMPONENT_ID,
            timestamp: 1,
            data: transform_with_parent(1600), // ...parented into client b's range
        }]);
        assert!(
            rt.on_client_crdt(a, &graft).is_empty(),
            "cross-client reparent must be rejected"
        );
        assert!(live(&rt).is_empty());

        // ROOT, renderer-local entities, the server band and the client's own
        // range all stay legal parents.
        for parent in [0u32, 1, 5, 600, 1100] {
            let ok = encode_batch(&[CrdtMessage::Put {
                entity: 1101,
                component_id: TRANSFORM_COMPONENT_ID,
                timestamp: parent + 2,
                data: transform_with_parent(parent),
            }]);
            assert_eq!(
                rt.on_client_crdt(a, &ok).len(),
                1,
                "parenting to {parent} is legitimate and must be admitted"
            );
        }
    }

    /// CROSS-AREA. `decode_client_batch` filters on the entity NUMBER (so a
    /// recycled entity -- `generation >= 1`, packed at `number + 65536` -- still
    /// belongs to its allocator) and `may_write` used to filter the same batch
    /// on the PACKED id. The stricter of the two won, so the recycled-entity fix
    /// in `crdt.rs` was dead on the only path that matters: a real client.
    ///
    /// Both checks are on numbers now, and this pins that they agree.
    #[test]
    fn a_client_may_write_its_own_recycled_entities() {
        let rt = RelayRuntime::new("localScene", ServerTransportConfig::default());
        let a = open(&rt);
        assert_eq!(rt.assigned.lock().get(&a).copied(), Some((1024, 512)));

        for generation in [0u16, 1, 2, u16::MAX] {
            let packed = crate::crdt::pack_entity(1100, generation);
            let body = encode_batch(&[CrdtMessage::Put {
                entity: packed,
                component_id: 7,
                timestamp: generation as u32 + 1,
                data: vec![generation as u8],
            }]);
            assert_eq!(
                rt.on_client_crdt(a, &body).len(),
                1,
                "entity number 1100 at generation {generation} (packed {packed}) is \
                 inside client a's range [1024, 1536) and must be admitted"
            );
        }

        // ...and a number OUTSIDE the range is still refused at every generation,
        // so the relaxation did not turn into a hole.
        for generation in [0u16, 1, u16::MAX] {
            let packed = crate::crdt::pack_entity(1600, generation);
            let body = encode_batch(&[CrdtMessage::Put {
                entity: packed,
                component_id: 7,
                timestamp: 1,
                data: vec![1],
            }]);
            assert!(
                rt.on_client_crdt(a, &body).is_empty(),
                "entity number 1600 is not client a's, whatever the generation"
            );
        }
    }

    /// The other direction of the same convention bug: the renderer-local floor
    /// is a NUMBER, so comparing the packed id let anyone author a
    /// renderer-local entity just by bumping its generation -- number 5 at
    /// generation 1 packs to 65541, comfortably above the floor.
    #[test]
    fn the_renderer_local_floor_cannot_be_bypassed_with_a_generation() {
        let cfg = ServerTransportConfig::default();
        let floor = cfg.network_floor();
        for number in [0u16, 1, 2, 5, 405] {
            for generation in [0u16, 1, 7, u16::MAX] {
                let packed = crate::crdt::pack_entity(number, generation);
                assert!(
                    !Authority::Server.may_write(floor, packed),
                    "the scene must not author renderer-local number {number} \
                     at generation {generation} (packed {packed})"
                );
                assert!(
                    !Authority::Client {
                        start: 0,
                        size: u32::MAX
                    }
                    .may_write(floor, packed),
                    "nor may a client, at any range"
                );
            }
        }
    }

    /// The renderer-local block belongs to each client's own engine and is
    /// never authorable over the network -- by anybody, at any config.
    #[test]
    fn nobody_may_author_the_renderer_local_block() {
        let cfg = ServerTransportConfig::default();
        let floor = cfg.network_floor();
        for entity in [0u32, 1, 2, 5, 405] {
            assert!(!Authority::Server.may_write(floor, entity));
            assert!(!Authority::Client {
                start: 0,
                size: u32::MAX
            }
            .may_write(floor, entity));
        }

        let wide_open = ServerTransportConfig {
            reserved_local_entities: 0,
            ..Default::default()
        };
        assert_eq!(wide_open.network_floor(), RENDERER_LOCAL_ENTITIES);
        assert!(!Authority::Server.may_write(wide_open.network_floor(), 1));
    }

    #[test]
    fn transform_parent_is_read_at_the_wire_offset() {
        assert_eq!(
            transform_parent(TRANSFORM_COMPONENT_ID, &transform_with_parent(2048)),
            Some(2048)
        );
        // not a transform
        assert_eq!(transform_parent(2, &transform_with_parent(2048)), None);
        // too short to carry a parent
        assert_eq!(transform_parent(TRANSFORM_COMPONENT_ID, &[0u8; 40]), None);
    }
}
