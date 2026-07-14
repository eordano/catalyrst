//! CRDT merge engine for SDK7 scene state.
//!
//! # Entity identifiers
//!
//! An SDK7 entity id is a **packed** `u32`: `number | (generation << 16)` -- the
//! low 16 bits are the entity NUMBER, the high 16 bits are the generation
//! ("version" upstream). See `@dcl/ecs` `engine/entity.ts` (`EntityUtils`,
//! `MASK_UPPER_16_ON_32`) and bevy-explorer `dcl_component::SceneEntityId`.
//! Everything that talks about *ranges* of entities -- the per-client write
//! range handed out in the `Init` message, `reclaim_range` -- is talking about
//! NUMBERS, never packed ids: upstream allocates a client range as
//! `reservedLocalEntities + serverLimit + index * clientLimit`
//! (`scene-state-server/src/adapters/scene.ts`) and bevy allocates from a
//! `RangeInclusive<u16>` of numbers (`CrdtContext::new_in_range`).
//!
//! # Removal
//!
//! Removal is tracked by a **version G-Set**: one high-water generation per
//! entity NUMBER, exactly `createVersionGSet` in `@dcl/ecs`
//! (`systems/crdt/gset.ts`). An entity is removed iff
//! `removed_generation(number) >= generation` -- `getEntityState` in
//! `engine/entity.ts` -- which is also what bevy's `CrdtContext::is_dead` does
//! (it stores `killed_generation + 1` and compares strictly greater).
//! `max` is commutative, idempotent and associative, so the removal set is a
//! function of the message multiset and never of arrival order, and it is
//! structurally bounded by `MAX_ENTITY_NUMBERS` -- there is no eviction and
//! therefore no resurrection.
//!
//! # Component storage
//!
//! Two disjoint stores, because SDK7 has two component kinds and a component id
//! belongs to exactly one of them:
//!
//! * `lww` -- last-write-wins elements (`lww-element-set-component-definition.ts`),
//!   fed by `PUT_COMPONENT` / `DELETE_COMPONENT`.
//! * `appends` -- grow-only value sets
//!   (`grow-only-value-set-component-definition.ts`), fed by `APPEND_VALUE`,
//!   which *pushes* onto a list. Every append on the wire carries timestamp 0
//!   (hardcoded in both references), so routing appends through LWW would make
//!   every append after the first collide at an equal timestamp.
//!
//! Routing is decided by the message type alone, so no per-cell "kind" flag
//! exists to be order-dependent.

use std::collections::{BTreeMap, VecDeque};

pub mod msg_type {
    pub const PUT_COMPONENT: u32 = 1;
    pub const DELETE_COMPONENT: u32 = 2;
    pub const DELETE_ENTITY: u32 = 3;
    pub const APPEND_VALUE: u32 = 4;
}

const HEADER_LEN: usize = 8;

/// Entity numbers live in the low 16 bits, so the removal G-Set has at most
/// this many entries -- a structural bound (~256 KiB), not a policy cap.
pub const MAX_ENTITY_NUMBERS: usize = u16::MAX as usize + 1;

/// Depth of one grow-only channel, i.e. how many appended payloads are retained
/// per `(entity, component)`. Matches bevy-explorer's `growonly::SET_SIZE` and
/// upstream's `ValueSetOptions::maxElements`; like both references the *newest*
/// values are kept. Retention is storage only: acceptance and relay of an
/// append never depend on it (see `CrdtEngine::apply`).
pub const MAX_APPEND_VALUES_PER_CHANNEL: usize = 100;

/// Low 16 bits of a packed entity id: the entity NUMBER.
#[inline]
pub fn entity_number(entity: u32) -> u16 {
    (entity & 0xffff) as u16
}

/// High 16 bits of a packed entity id: the generation (upstream: "version").
#[inline]
pub fn entity_generation(entity: u32) -> u16 {
    (entity >> 16) as u16
}

/// `number | (generation << 16)` -- `EntityUtils.toEntityId`.
#[inline]
pub fn pack_entity(number: u16, generation: u16) -> u32 {
    number as u32 | ((generation as u32) << 16)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrdtMessage {
    Put {
        entity: u32,
        component_id: u32,
        timestamp: u32,
        data: Vec<u8>,
    },
    DeleteComponent {
        entity: u32,
        component_id: u32,
        timestamp: u32,
    },
    DeleteEntity {
        entity: u32,
    },
    Append {
        entity: u32,
        component_id: u32,
        timestamp: u32,
        data: Vec<u8>,
    },
}

impl CrdtMessage {
    pub fn entity(&self) -> u32 {
        match self {
            CrdtMessage::Put { entity, .. }
            | CrdtMessage::DeleteComponent { entity, .. }
            | CrdtMessage::DeleteEntity { entity }
            | CrdtMessage::Append { entity, .. } => *entity,
        }
    }
}

#[inline]
fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let bytes = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().unwrap()))
}

#[inline]
fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn decode_batch(buf: &[u8]) -> Vec<CrdtMessage> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + HEADER_LEN <= buf.len() {
        let len = match read_u32(buf, off) {
            Some(l) => l as usize,
            None => break,
        };
        let ty = match read_u32(buf, off + 4) {
            Some(t) => t,
            None => break,
        };

        // WIRE-FORMAT STRICTNESS -- drop-on-garbage, and it is the oracle's rule.
        //
        // A record whose declared length is < HEADER_LEN, or which overruns the
        // buffer, aborts the whole batch: every record after it is dropped. This
        // was recorded as a possible divergence (resynchronise on the next 8-byte
        // boundary instead), but `@dcl/ecs` settles it --
        // `CrdtMessageProtocol.validate` returns false as soon as the declared
        // length exceeds the bytes remaining, and `parseChunkMessage`'s
        // `while ((header = getHeader(buffer)))` loop simply ENDS there. There is
        // no resynchronisation anywhere in the reference reader, and there cannot
        // be: nothing in the format marks a record boundary, so "the next 8-byte
        // boundary" is a guess that can invent messages out of payload bytes.
        // bevy-explorer's reader now does the same (`dcl/src/interface/mod.rs`
        // `process_message_stream`), so all three agree.
        //
        // What WAS wrong is that it happened in silence; the drop is logged now.
        if len < HEADER_LEN || off + len > buf.len() {
            tracing::warn!(
                offset = off,
                declared_len = len,
                remaining = buf.len() - off,
                "CRDT framing error: unframeable record; discarding rest of batch"
            );
            break;
        }
        let body = &buf[off + HEADER_LEN..off + len];
        match ty {
            msg_type::PUT_COMPONENT | msg_type::APPEND_VALUE if body.len() >= 16 => {
                let entity = read_u32(body, 0).unwrap();
                let component_id = read_u32(body, 4).unwrap();
                let timestamp = read_u32(body, 8).unwrap();
                let data_len = read_u32(body, 12).unwrap() as usize;
                // EXACTLY the declared payload, not "at least". A record that
                // frames correctly but whose `data_len` UNDERSTATES its payload
                // used to be accepted with the value silently truncated to
                // `data_len` bytes, while bevy-explorer rejects the same record
                // outright (`dcl/src/interface/mod.rs`, defect 5b) -- so one peer
                // stored a value the other refused, and payload bytes are exactly
                // what breaks LWW ties. Both sides now drop the record.
                if 16 + data_len != body.len() {
                    tracing::warn!(
                        offset = off,
                        declared_data_len = data_len,
                        body_len = body.len(),
                        "CRDT framing error: record does not carry exactly the payload \
                         it declares; dropping the record"
                    );
                } else {
                    let data = body[16..16 + data_len].to_vec();
                    out.push(if ty == msg_type::PUT_COMPONENT {
                        CrdtMessage::Put {
                            entity,
                            component_id,
                            timestamp,
                            data,
                        }
                    } else {
                        CrdtMessage::Append {
                            entity,
                            component_id,
                            timestamp,
                            data,
                        }
                    });
                }
            }
            msg_type::DELETE_COMPONENT if body.len() >= 12 => {
                out.push(CrdtMessage::DeleteComponent {
                    entity: read_u32(body, 0).unwrap(),
                    component_id: read_u32(body, 4).unwrap(),
                    timestamp: read_u32(body, 8).unwrap(),
                });
            }
            msg_type::DELETE_ENTITY if body.len() >= 4 => {
                out.push(CrdtMessage::DeleteEntity {
                    entity: read_u32(body, 0).unwrap(),
                });
            }
            _ => {}
        }
        off += len;
    }
    out
}

pub fn encode_message(msg: &CrdtMessage, out: &mut Vec<u8>) {
    match msg {
        CrdtMessage::Put {
            entity,
            component_id,
            timestamp,
            data,
        }
        | CrdtMessage::Append {
            entity,
            component_id,
            timestamp,
            data,
        } => {
            let ty = if matches!(msg, CrdtMessage::Put { .. }) {
                msg_type::PUT_COMPONENT
            } else {
                msg_type::APPEND_VALUE
            };
            let len = (HEADER_LEN + 16 + data.len()) as u32;
            write_u32(out, len);
            write_u32(out, ty);
            write_u32(out, *entity);
            write_u32(out, *component_id);
            write_u32(out, *timestamp);
            write_u32(out, data.len() as u32);
            out.extend_from_slice(data);
        }
        CrdtMessage::DeleteComponent {
            entity,
            component_id,
            timestamp,
        } => {
            write_u32(out, (HEADER_LEN + 12) as u32);
            write_u32(out, msg_type::DELETE_COMPONENT);
            write_u32(out, *entity);
            write_u32(out, *component_id);
            write_u32(out, *timestamp);
        }
        CrdtMessage::DeleteEntity { entity } => {
            write_u32(out, (HEADER_LEN + 4) as u32);
            write_u32(out, msg_type::DELETE_ENTITY);
            write_u32(out, *entity);
        }
    }
}

pub fn encode_batch(msgs: &[CrdtMessage]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in msgs {
        encode_message(m, &mut out);
    }
    out
}

/// Decode a batch received from client `index` and keep only the messages the
/// client is allowed to write.
///
/// `start`/`size` describe a range of entity **NUMBERS** (see the module docs),
/// so the filter compares `entity_number(id)`, not the packed id. Comparing the
/// packed id would reject every write to a recycled entity -- `@dcl/ecs` recycles
/// a freed number with `version + 1` (`entity.ts::generateEntity`), and real
/// ranges live far below 2^16, so any generation >= 1 packs above every range.
pub fn decode_client_batch(body: &[u8], start: u32, size: u32) -> Vec<CrdtMessage> {
    let end = start.saturating_add(size);
    decode_batch(body)
        .into_iter()
        .filter(|m| {
            let number = entity_number(m.entity()) as u32;
            number >= start && number < end
        })
        .collect()
}

fn data_compare(a: Option<&[u8]>, b: Option<&[u8]>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => {
            if x.len() != y.len() {
                return x.len().cmp(&y.len());
            }
            x.cmp(y)
        }
    }
}

/// `(entity number, entity generation, component id)`.
///
/// Deliberately *not* `(packed entity, component)`: keying on the split id makes
/// every generation of one entity number a contiguous key range, which is what
/// makes "kill number `n` up to generation `g`" a single range scan.
type CellKey = (u16, u16, u32);

#[inline]
fn cell_key(entity: u32, component_id: u32) -> CellKey {
    (
        entity_number(entity),
        entity_generation(entity),
        component_id,
    )
}

#[derive(Debug, Clone)]
struct LwwCell {
    timestamp: u32,
    /// `None` is the component tombstone (a DELETE_COMPONENT at `timestamp`).
    data: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct CrdtEngine {
    /// Last-write-wins cells. A cell only ever exists for a live entity.
    lww: BTreeMap<CellKey, LwwCell>,

    /// Grow-only channels: appended payloads in arrival order, newest last.
    appends: BTreeMap<CellKey, VecDeque<Vec<u8>>>,

    /// Version G-Set: entity number -> highest generation known removed.
    /// `deleted[n] >= g` means `pack_entity(n, g)` is removed.
    deleted: BTreeMap<u16, u16>,

    max_components: usize,

    max_append_channels: usize,
}

impl Default for CrdtEngine {
    fn default() -> Self {
        Self {
            lww: BTreeMap::new(),
            appends: BTreeMap::new(),
            deleted: BTreeMap::new(),
            max_components: usize::MAX,
            max_append_channels: usize::MAX,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,

    Ignored,
}

impl CrdtEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cap the number of stored cells.
    ///
    /// The budget is shared: at most `max_components` LWW cells, and at most
    /// `max_components / MAX_APPEND_VALUES_PER_CHANNEL` grow-only channels, so
    /// the number of retained payloads is bounded by `max_components` on each
    /// side. Eviction is by key order (see `enforce_cell_cap`), never by arrival
    /// order.
    pub fn with_cap(max_components: usize) -> Self {
        let max_components = max_components.max(1);
        Self {
            max_components,
            max_append_channels: (max_components / MAX_APPEND_VALUES_PER_CHANNEL).max(1),
            ..Self::default()
        }
    }

    /// `getEntityState(entity) == Removed` -- the entity number has a recorded
    /// removal at this generation or a later one.
    pub fn is_dead(&self, entity: u32) -> bool {
        match self.deleted.get(&entity_number(entity)) {
            Some(&removed) => removed >= entity_generation(entity),
            None => false,
        }
    }

    /// Highest generation recorded removed for this entity number, if any.
    pub fn removed_generation(&self, number: u16) -> Option<u16> {
        self.deleted.get(&number).copied()
    }

    pub fn apply(&mut self, msg: &CrdtMessage) -> ApplyResult {
        // A message for a removed entity is dropped, whatever its kind -- this is
        // `if (entityState === EntityState.Removed) continue` in
        // `@dcl/ecs systems/crdt/index.ts`. It also makes a re-sent
        // DELETE_ENTITY `Ignored`, so it is not re-broadcast.
        if self.is_dead(msg.entity()) {
            return ApplyResult::Ignored;
        }

        match msg {
            CrdtMessage::DeleteEntity { entity } => {
                if self.tombstone(*entity) {
                    ApplyResult::Applied
                } else {
                    ApplyResult::Ignored
                }
            }
            CrdtMessage::Put {
                entity,
                component_id,
                timestamp,
                data,
            } => self.lww_set(*entity, *component_id, *timestamp, Some(data.clone())),
            CrdtMessage::DeleteComponent {
                entity,
                component_id,
                timestamp,
            } => self.lww_set(*entity, *component_id, *timestamp, None),
            CrdtMessage::Append {
                entity,
                component_id,
                data,
                ..
            } => self.append(*entity, *component_id, data),
        }
    }

    /// Record the removal of `entity` and of every earlier generation of the
    /// same entity number. Returns whether this raised the G-Set high-water
    /// mark, i.e. whether the message carried new information.
    fn tombstone(&mut self, entity: u32) -> bool {
        let number = entity_number(entity);
        let generation = entity_generation(entity);
        if let Some(&removed) = self.deleted.get(&number) {
            if removed >= generation {
                return false;
            }
        }
        self.deleted.insert(number, generation);
        self.purge(number, generation);
        true
    }

    /// Drop every stored cell for generations `0..=generation` of `number`.
    fn purge(&mut self, number: u16, generation: u16) {
        let lo: CellKey = (number, 0, 0);
        let hi: CellKey = (number, generation, u32::MAX);
        let dead: Vec<CellKey> = self.lww.range(lo..=hi).map(|(k, _)| *k).collect();
        for k in dead {
            self.lww.remove(&k);
        }
        let dead: Vec<CellKey> = self.appends.range(lo..=hi).map(|(k, _)| *k).collect();
        for k in dead {
            self.appends.remove(&k);
        }
    }

    /// Keep the store within `max_components` by evicting the **largest** key,
    /// which makes the surviving key set exactly the `max_components` smallest
    /// keys ever presented -- a function of the message multiset, not of arrival
    /// order. Returns whether the key just inserted survived.
    fn enforce_cell_cap(&mut self, inserted: CellKey) -> bool {
        if self.lww.len() <= self.max_components {
            return true;
        }
        let largest = match self.lww.keys().next_back() {
            Some(k) => *k,
            None => return true,
        };
        self.lww.remove(&largest);
        largest != inserted
    }

    fn lww_set(
        &mut self,
        entity: u32,
        component_id: u32,
        timestamp: u32,
        data: Option<Vec<u8>>,
    ) -> ApplyResult {
        use std::cmp::Ordering;
        let key = cell_key(entity, component_id);
        match self.lww.get(&key) {
            None => {
                self.lww.insert(key, LwwCell { timestamp, data });
                if self.enforce_cell_cap(key) {
                    ApplyResult::Applied
                } else {
                    ApplyResult::Ignored
                }
            }
            Some(cur) => {
                let accept = match timestamp.cmp(&cur.timestamp) {
                    Ordering::Greater => true,
                    Ordering::Less => false,
                    // Equal timestamps tie-break on the payload, `None` (the
                    // component tombstone) below every payload -- `dataCompare`
                    // in `@dcl/ecs systems/crdt/utils.ts`.
                    Ordering::Equal => {
                        data_compare(data.as_deref(), cur.data.as_deref()) == Ordering::Greater
                    }
                };
                if accept {
                    self.lww.insert(key, LwwCell { timestamp, data });
                    ApplyResult::Applied
                } else {
                    ApplyResult::Ignored
                }
            }
        }
    }

    /// Push onto the grow-only channel for `(entity, component_id)`.
    ///
    /// APPEND_VALUE is an event, not a value: upstream's
    /// `GrowOnlyValueSetComponentDefinition` pushes onto a list and every append
    /// on the wire carries timestamp 0, so appends must never be merged by LWW.
    /// Acceptance is unconditional (the caller relays whatever is `Applied`),
    /// retention is best-effort and bounded.
    fn append(&mut self, entity: u32, component_id: u32, data: &[u8]) -> ApplyResult {
        let key = cell_key(entity, component_id);
        if !self.appends.contains_key(&key) && self.appends.len() >= self.max_append_channels {
            // Same key-ordered rule as `enforce_cell_cap`: which channels exist
            // is a function of the key set, not of arrival order.
            match self.appends.keys().next_back() {
                Some(&largest) if largest > key => {
                    self.appends.remove(&largest);
                }
                Some(_) => return ApplyResult::Applied,
                None => {}
            }
        }
        let channel = self.appends.entry(key).or_default();
        if channel.len() >= MAX_APPEND_VALUES_PER_CHANNEL {
            channel.pop_front();
        }
        channel.push_back(data.to_vec());
        ApplyResult::Applied
    }

    pub fn apply_batch(&mut self, msgs: &[CrdtMessage]) -> Vec<CrdtMessage> {
        let mut accepted = Vec::new();
        for m in msgs {
            if self.apply(m) == ApplyResult::Applied {
                accepted.push(m.clone());
            }
        }
        accepted
    }

    /// The full state, as the batch a late joiner is served.
    ///
    /// It must *determine* the state: replaying it into an empty engine yields
    /// an engine that answers every subsequent message identically. So it
    /// carries, in a deterministic (key) order:
    ///
    /// 1. one `DELETE_ENTITY` per removed entity number, at the G-Set high-water
    ///    generation (which masks every earlier generation on replay);
    /// 2. `PUT_COMPONENT` for live cells and `DELETE_COMPONENT` for component
    ///    tombstones -- upstream's `createDumpLwwFunctionFromCrdt` walks the
    ///    *timestamps* map and emits `DeleteComponent.write` for every entity
    ///    whose data is absent, precisely so a joiner cannot accept a write the
    ///    authoritative replica rejects.
    ///
    /// TODO(owner-decision: authority model / event replay) grow-only channels
    /// are deliberately NOT re-emitted. Upstream has two dumps that disagree:
    /// the per-component `dumpCrdtStateToBuffer` re-emits every appended value
    /// as APPEND_VALUE, while the late-joiner path that actually feeds a new
    /// peer (`@dcl/sdk/src/network/state.ts::engineToCrdt`) forwards
    /// PUT_COMPONENT only and drops everything else -- and every grow-only
    /// component (PointerEventsResult, VideoEvent, AudioEvent, TweenState...)
    /// is in its `NOT_SYNC_COMPONENTS` list. Replaying them would re-fire stale
    /// emotes / pointer results at every joining client as if they had just
    /// happened, so the safe reading is taken here. Flipping this back is a
    /// one-line change plus `snapshot_omits_grow_only_events`.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut msgs = Vec::with_capacity(self.deleted.len() + self.lww.len());
        for (&number, &generation) in &self.deleted {
            msgs.push(CrdtMessage::DeleteEntity {
                entity: pack_entity(number, generation),
            });
        }
        for (&(number, generation, component_id), cell) in &self.lww {
            let entity = pack_entity(number, generation);
            msgs.push(match &cell.data {
                Some(data) => CrdtMessage::Put {
                    entity,
                    component_id,
                    timestamp: cell.timestamp,
                    data: data.clone(),
                },
                None => CrdtMessage::DeleteComponent {
                    entity,
                    component_id,
                    timestamp: cell.timestamp,
                },
            });
        }
        encode_batch(&msgs)
    }

    /// Number of stored LWW cells, including component tombstones.
    pub fn component_count(&self) -> usize {
        self.lww.len()
    }

    /// Number of entity NUMBERS with a recorded removal.
    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }

    /// Number of grow-only channels currently retained.
    pub fn append_channel_count(&self) -> usize {
        self.appends.len()
    }

    /// Payloads retained on one grow-only channel, oldest first.
    pub fn appended(&self, entity: u32, component_id: u32) -> Vec<Vec<u8>> {
        self.appends
            .get(&cell_key(entity, component_id))
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Release every entity NUMBER in `[start, start + size)` -- the range of a
    /// departing client -- and return the DELETE_ENTITY messages to relay.
    ///
    /// The bounds are entity numbers (see the module docs), so an entity that
    /// has been recycled at least once is reclaimed like any other; comparing
    /// packed ids leaked every such entity for the lifetime of the scene. Each
    /// number is killed at the highest generation seen for it, which by the
    /// G-Set rule also removes every earlier generation.
    pub fn reclaim_range(&mut self, start: u32, size: u32) -> Vec<CrdtMessage> {
        let end = start.saturating_add(size);
        let lo = start.min(MAX_ENTITY_NUMBERS as u32);
        let hi = end.min(MAX_ENTITY_NUMBERS as u32);
        if lo >= hi {
            return Vec::new();
        }
        let lo_key: CellKey = (lo as u16, 0, 0);
        let hi_key: CellKey = ((hi - 1) as u16, u16::MAX, u32::MAX);

        // Highest generation seen per number, across both stores.
        let mut victims: BTreeMap<u16, u16> = BTreeMap::new();
        for (&(number, generation, _), _) in self.lww.range(lo_key..=hi_key) {
            let slot = victims.entry(number).or_insert(generation);
            *slot = (*slot).max(generation);
        }
        for (&(number, generation, _), _) in self.appends.range(lo_key..=hi_key) {
            let slot = victims.entry(number).or_insert(generation);
            *slot = (*slot).max(generation);
        }

        let mut out = Vec::with_capacity(victims.len());
        for (number, generation) in victims {
            let entity = pack_entity(number, generation);
            self.tombstone(entity);
            out.push(CrdtMessage::DeleteEntity { entity });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(entity: u32, comp: u32, ts: u32, data: &[u8]) -> CrdtMessage {
        CrdtMessage::Put {
            entity,
            component_id: comp,
            timestamp: ts,
            data: data.to_vec(),
        }
    }

    /// Every APPEND_VALUE on the wire carries timestamp 0 -- that is the
    /// protocol, not a simplification.
    fn append(entity: u32, comp: u32, data: &[u8]) -> CrdtMessage {
        CrdtMessage::Append {
            entity,
            component_id: comp,
            timestamp: 0,
            data: data.to_vec(),
        }
    }

    fn del_comp(entity: u32, comp: u32, ts: u32) -> CrdtMessage {
        CrdtMessage::DeleteComponent {
            entity,
            component_id: comp,
            timestamp: ts,
        }
    }

    #[test]
    fn put_roundtrips_through_wire() {
        let m = put(513, 1, 7, &[0xde, 0xad, 0xbe, 0xef]);
        let mut buf = Vec::new();
        encode_message(&m, &mut buf);

        assert_eq!(buf.len(), 28);
        assert_eq!(&buf[0..4], &28u32.to_le_bytes());
        assert_eq!(&buf[4..8], &msg_type::PUT_COMPONENT.to_le_bytes());
        let decoded = decode_batch(&buf);
        assert_eq!(decoded, vec![m]);
    }

    #[test]
    fn entity_packing_matches_upstream() {
        // `EntityUtils.toEntityId` / `fromEntityId`, @dcl/ecs engine/entity.ts.
        assert_eq!(pack_entity(1024, 0), 1024);
        assert_eq!(pack_entity(1024, 1), 1024 + 65536);
        assert_eq!(entity_number(1024 + 65536), 1024);
        assert_eq!(entity_generation(1024 + 65536), 1);
        assert_eq!(entity_number(u32::MAX), u16::MAX);
        assert_eq!(entity_generation(u32::MAX), u16::MAX);
    }

    #[test]
    fn decode_handles_concatenated_messages() {
        let a = put(1, 1, 1, &[1]);
        let b = CrdtMessage::DeleteEntity { entity: 2 };
        let c = CrdtMessage::DeleteComponent {
            entity: 1,
            component_id: 5,
            timestamp: 3,
        };
        let batch = encode_batch(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(decode_batch(&batch), vec![a, b, c]);
    }

    #[test]
    fn lww_higher_timestamp_wins() {
        let mut e = CrdtEngine::new();
        assert_eq!(e.apply(&put(1, 1, 5, b"old")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 1, 4, b"older")), ApplyResult::Ignored);
        assert_eq!(e.apply(&put(1, 1, 6, b"new")), ApplyResult::Applied);
        let snap = decode_batch(&e.snapshot());
        assert_eq!(snap, vec![put(1, 1, 6, b"new")]);
    }

    #[test]
    fn lww_tie_breaks_on_data() {
        let mut e = CrdtEngine::new();

        assert_eq!(e.apply(&put(1, 1, 5, b"aaa")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 1, 5, b"bbb")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 1, 5, b"aaa")), ApplyResult::Ignored);

        assert_eq!(e.apply(&put(1, 1, 5, b"zzzz")), ApplyResult::Applied);
        let snap = decode_batch(&e.snapshot());
        assert_eq!(snap, vec![put(1, 1, 5, b"zzzz")]);
    }

    #[test]
    fn delete_component_is_lower_than_data_on_tie() {
        let mut e = CrdtEngine::new();
        e.apply(&put(1, 1, 5, b"x"));

        let del = del_comp(1, 1, 5);
        assert_eq!(e.apply(&del), ApplyResult::Ignored);

        let del2 = del_comp(1, 1, 6);
        assert_eq!(e.apply(&del2), ApplyResult::Applied);

        // The tombstone survives in the snapshot: a joiner that did not learn
        // about the delete would accept a stale write this engine rejects.
        assert_eq!(decode_batch(&e.snapshot()), vec![del_comp(1, 1, 6)]);
    }

    #[test]
    fn delete_entity_drops_components_and_masks_future_ops() {
        let mut e = CrdtEngine::new();
        e.apply(&put(1, 1, 5, b"a"));
        e.apply(&put(1, 2, 5, b"b"));
        e.apply(&put(2, 1, 5, b"c"));
        assert_eq!(e.component_count(), 3);
        assert_eq!(
            e.apply(&CrdtMessage::DeleteEntity { entity: 1 }),
            ApplyResult::Applied
        );
        assert_eq!(e.component_count(), 1);

        assert_eq!(e.apply(&put(1, 1, 99, b"z")), ApplyResult::Ignored);
        assert_eq!(e.component_count(), 1);
    }

    #[test]
    fn apply_batch_returns_only_accepted() {
        let mut e = CrdtEngine::new();
        let batch = vec![
            put(1, 1, 5, b"a"),
            put(1, 1, 4, b"older"),
            put(2, 1, 1, b"b"),
        ];
        let accepted = e.apply_batch(&batch);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0], put(1, 1, 5, b"a"));
        assert_eq!(accepted[1], put(2, 1, 1, b"b"));
    }

    #[test]
    fn reclaim_range_tombstones_and_emits_deletes() {
        let mut e = CrdtEngine::new();
        e.apply(&put(1100, 1, 5, b"a"));
        e.apply(&put(1101, 1, 5, b"b"));
        e.apply(&put(2000, 1, 5, b"c"));
        let deletes = e.reclaim_range(1024, 512);
        assert_eq!(deletes.len(), 2);
        assert!(deletes.contains(&CrdtMessage::DeleteEntity { entity: 1100 }));
        assert!(deletes.contains(&CrdtMessage::DeleteEntity { entity: 1101 }));
        assert_eq!(e.component_count(), 1);
    }

    /// D4: `reclaim_range` used to compare packed ids, so a recycled entity was
    /// never freed and never tombstoned -- a permanent per-scene leak.
    #[test]
    fn reclaim_range_frees_recycled_entities() {
        let mut e = CrdtEngine::new();
        let recycled = pack_entity(1100, 3);
        assert_eq!(e.apply(&put(recycled, 1, 5, b"a")), ApplyResult::Applied);
        assert_eq!(e.apply(&append(recycled, 9, b"ev")), ApplyResult::Applied);

        let deletes = e.reclaim_range(1024, 512);
        assert_eq!(
            deletes,
            vec![CrdtMessage::DeleteEntity { entity: recycled }]
        );
        assert_eq!(e.component_count(), 0);
        assert_eq!(e.append_channel_count(), 0);
        assert!(e.is_dead(pack_entity(1100, 0)));
        assert!(e.is_dead(recycled));
        assert!(!e.is_dead(pack_entity(1100, 4)));
    }

    /// D1: the per-client range is a range of entity NUMBERS.
    #[test]
    fn client_range_accepts_recycled_entities() {
        let recycled = pack_entity(1024, 1);
        let batch = encode_batch(&[
            put(recycled, 1, 1, b"x"),
            put(pack_entity(1536, 1), 1, 1, b"out"),
        ]);
        assert_eq!(
            decode_client_batch(&batch, 1024, 512),
            vec![put(recycled, 1, 1, b"x")]
        );
    }

    /// D3: appends are a grow-only channel, not an LWW cell. Every append is
    /// retained and relayed even though they all share timestamp 0.
    #[test]
    fn appends_accumulate_and_are_all_relayed() {
        let mut e = CrdtEngine::new();
        let batch = vec![append(1, 9, &[3]), append(1, 9, &[2]), append(1, 9, &[1])];
        let relayed = e.apply_batch(&batch);
        assert_eq!(relayed, batch);
        assert_eq!(e.appended(1, 9), vec![vec![3], vec![2], vec![1]]);
        // and they do not collide with the LWW cell of the same component id
        assert_eq!(e.component_count(), 0);
    }

    #[test]
    fn append_channel_is_bounded_and_keeps_newest() {
        let mut e = CrdtEngine::new();
        for i in 0..(MAX_APPEND_VALUES_PER_CHANNEL + 5) {
            e.apply(&append(1, 9, &[i as u8]));
        }
        let kept = e.appended(1, 9);
        assert_eq!(kept.len(), MAX_APPEND_VALUES_PER_CHANNEL);
        assert_eq!(kept[0], vec![5u8]);
    }

    /// D6/D7: the snapshot is state, not a replay of events, and its message
    /// kinds come from the store a cell lives in -- never from arrival order.
    #[test]
    fn snapshot_omits_grow_only_events() {
        let mut e = CrdtEngine::new();
        e.apply(&append(1, 9, &[5]));
        e.apply(&put(1, 9, 3, b"lww"));
        let snap = decode_batch(&e.snapshot());
        assert_eq!(snap, vec![put(1, 9, 3, b"lww")]);
    }

    #[test]
    fn truncated_trailer_is_ignored() {
        let mut batch = encode_batch(&[put(1, 1, 1, b"ok")]);
        batch.extend_from_slice(&[0x05, 0x00]);
        assert_eq!(decode_batch(&batch), vec![put(1, 1, 1, b"ok")]);
    }

    fn apply_all(ops: &[CrdtMessage]) -> Vec<u8> {
        let mut e = CrdtEngine::new();
        for m in ops {
            e.apply(m);
        }
        e.snapshot()
    }

    fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut out = Vec::new();
        for i in 0..items.len() {
            let mut rest = items.to_vec();
            let head = rest.remove(i);
            for mut p in permutations(&rest) {
                p.insert(0, head.clone());
                out.push(p);
            }
        }
        out
    }

    #[test]
    fn all_orderings_converge_to_same_state() {
        let ops = vec![
            put(1, 1, 5, b"aaa"),
            put(1, 1, 5, b"bbb"),
            put(1, 1, 7, b"z"),
            put(1, 2, 3, b"c2"),
            del_comp(1, 2, 3),
            del_comp(1, 2, 4),
            put(2, 9, 1, b"keep"),
        ];
        let perms = permutations(&ops);
        assert!(perms.len() >= 5040);
        let reference = apply_all(&ops);

        let decoded = decode_batch(&reference);
        assert!(decoded.contains(&put(1, 1, 7, b"z")));
        assert!(decoded.contains(&put(2, 9, 1, b"keep")));
        assert!(decoded.contains(&del_comp(1, 2, 4)));
        assert!(!decoded.iter().any(|m| m.entity() == 1
            && matches!(m, CrdtMessage::Put { component_id, .. } if *component_id == 2)));
        for p in &perms {
            assert_eq!(apply_all(p), reference, "ordering diverged: {p:?}");
        }
    }

    /// D2/D4: removal is a version G-Set, so nothing is ever evicted and a
    /// deleted entity can never be resurrected by a later write.
    #[test]
    fn tombstones_never_expire_under_delete_flood() {
        let mut e = CrdtEngine::new();
        let n = 12_288u32;
        for entity in 0..n {
            e.apply(&CrdtMessage::DeleteEntity { entity });
        }
        assert_eq!(e.deleted_count(), n as usize);

        assert_eq!(e.apply(&put(n - 1, 1, 1, b"x")), ApplyResult::Ignored);
        assert_eq!(
            e.apply(&put(0, 1, 1, b"ghost")),
            ApplyResult::Ignored,
            "the first entity deleted must still be dead after {n} deletes"
        );
        // and the removal set is bounded structurally, by the u16 number space
        assert!(e.deleted_count() <= MAX_ENTITY_NUMBERS);
    }

    /// D2: killing generation `g` of a number masks every earlier generation --
    /// `getEntityState` (`removedVersion >= v`) upstream, `is_dead` in bevy.
    #[test]
    fn entity_mask_covers_older_generations() {
        let mut e = CrdtEngine::new();
        assert_eq!(
            e.apply(&CrdtMessage::DeleteEntity {
                entity: pack_entity(7, 2)
            }),
            ApplyResult::Applied
        );

        assert_eq!(
            e.apply(&put(pack_entity(7, 0), 1, 5, &[9])),
            ApplyResult::Ignored
        );
        assert_eq!(
            e.apply(&put(pack_entity(7, 2), 1, 5, &[9])),
            ApplyResult::Ignored
        );
        // the next generation is a different, live entity
        assert_eq!(
            e.apply(&put(pack_entity(7, 3), 1, 5, &[9])),
            ApplyResult::Applied
        );
    }

    /// A re-sent DELETE_ENTITY carries no new information and must not be
    /// relayed a second time.
    #[test]
    fn resent_delete_entity_is_not_rebroadcast() {
        let mut e = CrdtEngine::new();
        let victim = CrdtMessage::DeleteEntity { entity: 1 };
        assert_eq!(e.apply(&victim), ApplyResult::Applied);
        for entity in 2..5_000u32 {
            e.apply(&CrdtMessage::DeleteEntity { entity });
        }
        assert_eq!(e.apply(&victim), ApplyResult::Ignored);
    }

    #[test]
    fn reclaim_range_tombstones_every_number_in_range() {
        let mut e = CrdtEngine::new();
        let n = 4_196u32;
        for entity in 0..n {
            e.apply(&put(entity, 1, 1, b"a"));
        }
        let deletes = e.reclaim_range(0, n);
        assert_eq!(deletes.len(), n as usize);
        assert_eq!(e.deleted_count(), n as usize);
        assert_eq!(e.component_count(), 0);
    }

    #[test]
    fn decode_client_batch_rejects_out_of_range_ops() {
        let batch = encode_batch(&[
            put(1024, 1, 1, b"in-low"),
            put(1535, 1, 1, b"in-high"),
            put(1536, 1, 1, b"out-high"),
            put(1023, 1, 1, b"out-low"),
            CrdtMessage::DeleteEntity {
                entity: 4_000_000_000,
            },
            CrdtMessage::DeleteEntity { entity: 1100 },
        ]);
        let kept = decode_client_batch(&batch, 1024, 512);
        assert_eq!(
            kept,
            vec![
                put(1024, 1, 1, b"in-low"),
                put(1535, 1, 1, b"in-high"),
                CrdtMessage::DeleteEntity { entity: 1100 },
            ]
        );

        assert!(decode_client_batch(&batch, 1024, 0).is_empty());
        assert!(decode_client_batch(&batch, u32::MAX, 512).is_empty());
    }

    #[test]
    fn delete_entity_convergence_independent_of_order() {
        let ops = vec![
            put(1, 1, 5, b"a"),
            CrdtMessage::DeleteEntity { entity: 1 },
            put(1, 1, 99, b"resurrect?"),
            put(2, 1, 1, b"survivor"),
        ];
        let reference = apply_all(&ops);
        for p in permutations(&ops) {
            assert_eq!(
                apply_all(&p),
                reference,
                "delete-entity order diverged: {p:?}"
            );
        }

        let decoded = decode_batch(&reference);
        assert_eq!(
            decoded,
            vec![
                CrdtMessage::DeleteEntity { entity: 1 },
                put(2, 1, 1, b"survivor"),
            ]
        );
    }

    /// D6: replaying a snapshot into an empty engine must reproduce an engine
    /// that answers every message the same way.
    #[test]
    fn snapshot_is_state_determining() {
        fn server() -> CrdtEngine {
            let mut e = CrdtEngine::new();
            e.apply(&put(1100, 1, 1, &[9]));
            e.apply(&del_comp(1100, 1, 2));
            e.apply(&put(1101, 1, 1, &[1]));
            e.apply(&CrdtMessage::DeleteEntity {
                entity: pack_entity(1102, 2),
            });
            e
        }
        fn joiner() -> CrdtEngine {
            let mut r = CrdtEngine::new();
            for m in decode_batch(&server().snapshot()) {
                r.apply(&m);
            }
            r
        }

        assert_eq!(joiner().snapshot(), server().snapshot());

        for probe in [
            put(1100, 1, 1, &[9]),                 // stale write under a tombstone
            put(1100, 1, 3, &[9]),                 // fresh write over a tombstone
            put(pack_entity(1102, 0), 4, 7, &[1]), // dead: older generation
            put(pack_entity(1102, 2), 4, 7, &[1]), // dead: the killed generation
            put(pack_entity(1102, 3), 4, 7, &[1]), // live: the recycled entity
            del_comp(1101, 1, 1),
        ] {
            assert_eq!(
                joiner().apply(&probe),
                server().apply(&probe),
                "snapshot-replayed joiner diverged on {probe:?}"
            );
        }
    }

    /// D5: at the cap, which cells exist is the set of smallest keys, not a
    /// function of arrival order.
    #[test]
    fn component_cap_is_order_independent() {
        let a = put(1, 1, 1, &[1]);
        let b = put(2, 2, 1, &[2]);

        let mut e1 = CrdtEngine::with_cap(1);
        e1.apply(&a);
        e1.apply(&b);

        let mut e2 = CrdtEngine::with_cap(1);
        e2.apply(&b);
        e2.apply(&a);

        assert_eq!(e1.snapshot(), e2.snapshot());
        assert_eq!(decode_batch(&e1.snapshot()), vec![a]);
    }

    #[test]
    fn component_cap_keeps_updating_surviving_cells() {
        let mut e = CrdtEngine::with_cap(2);
        assert_eq!(e.apply(&put(1, 1, 1, b"a")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 2, 1, b"b")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 3, 1, b"c")), ApplyResult::Ignored);
        assert_eq!(e.apply(&put(1, 1, 2, b"a2")), ApplyResult::Applied);
        assert_eq!(e.component_count(), 2);
    }
}
