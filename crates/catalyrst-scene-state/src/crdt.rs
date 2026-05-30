//! SDK7 CRDT engine — the authoritative server-side state model.
//!
//! Port of the wire format + LWW-element-set semantics implemented across
//! `js-sdk-toolchain` `packages/@dcl/ecs/src/serialization/crdt/*` and
//! `systems/crdt/index.ts` (+ `engine/lww-element-set-component-definition.ts`).
//!
//! # Wire format (little-endian throughout)
//!
//! A CRDT batch is a concatenation of self-delimited messages. Every message
//! starts with an 8-byte header:
//!
//! ```text
//! offset size field
//! 0      4    length  (u32, total message bytes incl. this header)
//! 4      4    type    (u32, CrdtMessageType)
//! ```
//!
//! followed by a per-type body. The four message types this server processes
//! (the renderer/scene transport set — network messages 5-7 are LiveKit-only
//! and never reach this transport):
//!
//! - `PUT_COMPONENT` (1): `entity u32 | componentId u32 | timestamp u32 |
//!   dataLen u32 | data[dataLen]`
//! - `DELETE_COMPONENT` (2): `entity u32 | componentId u32 | timestamp u32`
//! - `DELETE_ENTITY` (3): `entity u32`
//! - `APPEND_VALUE` (4): same body layout as `PUT_COMPONENT` (grow-only log;
//!   for snapshot purposes we keep the *latest* appended value per
//!   (entity, component), which is sufficient for late-joiner state — the
//!   renderer treats GrowOnlyValueSet specially but the wire bytes are
//!   identical).
//!
//! # LWW merge (`engine/lww-element-set-component-definition.ts::crdtRuleForCurrentState`)
//!
//! State is keyed by `(entity, componentId)`. The winner is the message with
//! the highest Lamport `timestamp`; on a tie the message with the
//! lexicographically-greater data (length first, then bytes — see
//! `systems/crdt/utils.ts::dataCompare`) wins, with `DELETE_COMPONENT`
//! modelled as `data = null` which sorts *below* any present data. A
//! `DELETE_ENTITY` tombstones the entity: it and all its components are
//! dropped from state and any later op referencing that entity id is ignored
//! (matches `crdtSceneSystem.receiveMessages` `EntityState.Removed` skip).

use std::collections::{BTreeMap, BTreeSet};

/// Message type discriminants, matching `CrdtMessageType` upstream.
pub mod msg_type {
    pub const PUT_COMPONENT: u32 = 1;
    pub const DELETE_COMPONENT: u32 = 2;
    pub const DELETE_ENTITY: u32 = 3;
    pub const APPEND_VALUE: u32 = 4;
    // 5-7 are the network (LiveKit) variants; not seen on this transport.
}

const HEADER_LEN: usize = 8;

/// A single decoded CRDT operation.
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

/// Decode a full CRDT batch into its constituent messages. Unknown/garbage
/// message types are skipped using the header length (mirrors the
/// `incrementReadOffset(header.length)` default branch in `parseChunkMessage`).
/// Returns the messages parsed before any truncated/invalid trailer.
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
        // A length smaller than the header, or one that runs past the end of
        // the buffer, means a corrupt/partial frame: stop.
        if len < HEADER_LEN || off + len > buf.len() {
            break;
        }
        let body = &buf[off + HEADER_LEN..off + len];
        match ty {
            msg_type::PUT_COMPONENT | msg_type::APPEND_VALUE => {
                // entity u32 | component u32 | ts u32 | dataLen u32 | data
                if body.len() >= 16 {
                    let entity = read_u32(body, 0).unwrap();
                    let component_id = read_u32(body, 4).unwrap();
                    let timestamp = read_u32(body, 8).unwrap();
                    let data_len = read_u32(body, 12).unwrap() as usize;
                    if 16 + data_len <= body.len() {
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
            }
            msg_type::DELETE_COMPONENT => {
                if body.len() >= 12 {
                    out.push(CrdtMessage::DeleteComponent {
                        entity: read_u32(body, 0).unwrap(),
                        component_id: read_u32(body, 4).unwrap(),
                        timestamp: read_u32(body, 8).unwrap(),
                    });
                }
            }
            msg_type::DELETE_ENTITY => {
                if body.len() >= 4 {
                    out.push(CrdtMessage::DeleteEntity {
                        entity: read_u32(body, 0).unwrap(),
                    });
                }
            }
            _ => { /* unknown type: skip via len */ }
        }
        off += len;
    }
    out
}

/// Encode a single message back to the wire format (little-endian).
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

/// Encode a slice of messages into a single batch buffer.
pub fn encode_batch(msgs: &[CrdtMessage]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in msgs {
        encode_message(m, &mut out);
    }
    out
}

/// `dataCompare` port: length first, then byte-by-byte; `None` (a deleted
/// component, modelled as null) sorts below any present data.
/// Returns Ordering of `a` vs `b`.
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

/// State of a single component on a single entity.
#[derive(Debug, Clone)]
struct ComponentCell {
    timestamp: u32,
    /// `None` means the component was deleted (tombstone with a timestamp).
    data: Option<Vec<u8>>,
    /// True if the latest op was an APPEND_VALUE (re-emitted as APPEND_VALUE in
    /// snapshots so the renderer routes it to the grow-only set).
    is_append: bool,
}

/// The authoritative LWW-element-set CRDT engine for one scene.
///
/// Keyed by `(entity, componentId)`. Tracks per-entity tombstones so a
/// `DELETE_ENTITY` permanently masks the entity (and every later op for it).
#[derive(Debug)]
pub struct CrdtEngine {
    /// (entity, componentId) -> cell
    components: BTreeMap<(u32, u32), ComponentCell>,
    /// Entities that have been deleted (tombstones). Once present, all ops for
    /// that entity are ignored and its components are dropped.
    deleted_entities: BTreeSet<u32>,
    /// Hard cap on the number of live `(entity, component)` cells. A write that
    /// would introduce a *new* cell beyond this cap is rejected (treated as a
    /// no-op `Ignored`), so one scene/client can't drive unbounded memory
    /// growth in the authoritative state. Updates to already-present cells and
    /// deletes are always allowed (they don't grow the map).
    max_components: usize,
}

impl Default for CrdtEngine {
    fn default() -> Self {
        Self {
            components: BTreeMap::new(),
            deleted_entities: BTreeSet::new(),
            // Generous default; production overrides via CrdtEngine::with_cap.
            max_components: usize::MAX,
        }
    }
}

/// Outcome of applying a single message, used to decide whether to rebroadcast.
#[derive(Debug, PartialEq, Eq)]
pub enum ApplyResult {
    /// State changed; the message should be forwarded to other clients.
    Applied,
    /// Message was outdated / a no-op / referenced a deleted entity: state
    /// unchanged, do not rebroadcast.
    Ignored,
}

impl CrdtEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an engine that rejects new component cells beyond `max_components`.
    pub fn with_cap(max_components: usize) -> Self {
        Self {
            max_components: max_components.max(1),
            ..Self::default()
        }
    }

    /// Apply one decoded message, returning whether it changed authoritative
    /// state. Implements the `crdtRuleForCurrentState` LWW resolution.
    pub fn apply(&mut self, msg: &CrdtMessage) -> ApplyResult {
        let entity = msg.entity();
        if self.deleted_entities.contains(&entity) {
            // EntityState.Removed -> skip (and never resurrect).
            return ApplyResult::Ignored;
        }

        match msg {
            CrdtMessage::DeleteEntity { entity } => {
                self.deleted_entities.insert(*entity);
                // Drop every component of this entity.
                let keys: Vec<_> = self
                    .components
                    .range((*entity, 0)..=(*entity, u32::MAX))
                    .map(|(k, _)| *k)
                    .collect();
                for k in keys {
                    self.components.remove(&k);
                }
                ApplyResult::Applied
            }
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
                let is_append = matches!(msg, CrdtMessage::Append { .. });
                self.lww_set(
                    *entity,
                    *component_id,
                    *timestamp,
                    Some(data.clone()),
                    is_append,
                )
            }
            CrdtMessage::DeleteComponent {
                entity,
                component_id,
                timestamp,
            } => self.lww_set(*entity, *component_id, *timestamp, None, false),
        }
    }

    /// Core LWW resolution for a component write (`data=None` == delete).
    fn lww_set(
        &mut self,
        entity: u32,
        component_id: u32,
        timestamp: u32,
        data: Option<Vec<u8>>,
        is_append: bool,
    ) -> ApplyResult {
        use std::cmp::Ordering;
        let key = (entity, component_id);
        match self.components.get(&key) {
            None => {
                // Reject introducing a new cell once the per-scene cap is hit,
                // so untrusted scene/client writes can't grow authoritative
                // state without bound. Updates to existing cells (the `Some`
                // arm) are unaffected. Note: a DELETE_COMPONENT for an unseen
                // cell still inserts a tombstone here (matching upstream
                // `currentTimestamp === undefined -> StateUpdatedTimestamp`),
                // which is also bounded by the same cap.
                if self.components.len() >= self.max_components {
                    return ApplyResult::Ignored;
                }
                // currentTimestamp === undefined -> StateUpdatedTimestamp
                self.components.insert(
                    key,
                    ComponentCell {
                        timestamp,
                        data,
                        is_append,
                    },
                );
                ApplyResult::Applied
            }
            Some(cur) => match timestamp.cmp(&cur.timestamp) {
                Ordering::Greater => {
                    self.components.insert(
                        key,
                        ComponentCell {
                            timestamp,
                            data,
                            is_append,
                        },
                    );
                    ApplyResult::Applied
                }
                Ordering::Less => ApplyResult::Ignored, // StateOutdatedTimestamp
                Ordering::Equal => {
                    // Same timestamp: resolve by data. Incoming wins iff its
                    // data is strictly greater than current.
                    match data_compare(data.as_deref(), cur.data.as_deref()) {
                        Ordering::Greater => {
                            self.components.insert(
                                key,
                                ComponentCell {
                                    timestamp,
                                    data,
                                    is_append,
                                },
                            );
                            ApplyResult::Applied // StateUpdatedData
                        }
                        // NoChanges / StateOutdatedData
                        _ => ApplyResult::Ignored,
                    }
                }
            },
        }
    }

    /// Apply a whole inbound batch, returning the subset of messages that
    /// actually changed state (to be forwarded to other clients). Preserves
    /// input order.
    pub fn apply_batch(&mut self, msgs: &[CrdtMessage]) -> Vec<CrdtMessage> {
        let mut accepted = Vec::new();
        for m in msgs {
            if self.apply(m) == ApplyResult::Applied {
                accepted.push(m.clone());
            }
        }
        accepted
    }

    /// Serialize the full authoritative state as a CRDT batch — the snapshot a
    /// late-joining client receives in its `Init` frame. Component PUT/APPEND
    /// ops are emitted with their winning timestamp; tombstoned components and
    /// deleted entities are not re-emitted (the client starts from an empty
    /// engine, so absence is the correct representation, matching upstream
    /// where `crdtState` is the accumulated component-state buffer).
    pub fn snapshot(&self) -> Vec<u8> {
        let mut msgs = Vec::new();
        for ((entity, component_id), cell) in &self.components {
            if let Some(data) = &cell.data {
                let m = if cell.is_append {
                    CrdtMessage::Append {
                        entity: *entity,
                        component_id: *component_id,
                        timestamp: cell.timestamp,
                        data: data.clone(),
                    }
                } else {
                    CrdtMessage::Put {
                        entity: *entity,
                        component_id: *component_id,
                        timestamp: cell.timestamp,
                        data: data.clone(),
                    }
                };
                msgs.push(m);
            }
        }
        encode_batch(&msgs)
    }

    /// Number of live (entity, component) cells — for metrics/tests.
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Remove every component of entities in the given network range and
    /// tombstone them. Used by `on_client_close` to GC a departed client's
    /// network entities (its assigned id window). Returns the DELETE_ENTITY
    /// messages to broadcast so peers drop the same entities.
    pub fn reclaim_range(&mut self, start: u32, size: u32) -> Vec<CrdtMessage> {
        let end = start.saturating_add(size);
        let entities: BTreeSet<u32> = self
            .components
            .range((start, 0)..(end, 0))
            .map(|((e, _), _)| *e)
            .collect();
        let mut out = Vec::new();
        for e in entities {
            let keys: Vec<_> = self
                .components
                .range((e, 0)..=(e, u32::MAX))
                .map(|(k, _)| *k)
                .collect();
            for k in keys {
                self.components.remove(&k);
            }
            self.deleted_entities.insert(e);
            out.push(CrdtMessage::DeleteEntity { entity: e });
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

    #[test]
    fn put_roundtrips_through_wire() {
        let m = put(513, 1, 7, &[0xde, 0xad, 0xbe, 0xef]);
        let mut buf = Vec::new();
        encode_message(&m, &mut buf);
        // header(8) + body(16) + data(4) = 28
        assert_eq!(buf.len(), 28);
        assert_eq!(&buf[0..4], &28u32.to_le_bytes());
        assert_eq!(&buf[4..8], &msg_type::PUT_COMPONENT.to_le_bytes());
        let decoded = decode_batch(&buf);
        assert_eq!(decoded, vec![m]);
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
        // same timestamp; "bbb" > "aaa" byte-wise (same length) => bbb wins.
        assert_eq!(e.apply(&put(1, 1, 5, b"aaa")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 1, 5, b"bbb")), ApplyResult::Applied);
        assert_eq!(e.apply(&put(1, 1, 5, b"aaa")), ApplyResult::Ignored);
        // longer data also wins on tie (length compared first).
        assert_eq!(e.apply(&put(1, 1, 5, b"zzzz")), ApplyResult::Applied);
        let snap = decode_batch(&e.snapshot());
        assert_eq!(snap, vec![put(1, 1, 5, b"zzzz")]);
    }

    #[test]
    fn delete_component_is_lower_than_data_on_tie() {
        let mut e = CrdtEngine::new();
        e.apply(&put(1, 1, 5, b"x"));
        // delete with same timestamp must NOT beat present data (null < data).
        let del = CrdtMessage::DeleteComponent {
            entity: 1,
            component_id: 1,
            timestamp: 5,
        };
        assert_eq!(e.apply(&del), ApplyResult::Ignored);
        // delete with higher timestamp wins.
        let del2 = CrdtMessage::DeleteComponent {
            entity: 1,
            component_id: 1,
            timestamp: 6,
        };
        assert_eq!(e.apply(&del2), ApplyResult::Applied);
        // tombstone is not re-emitted in the snapshot.
        assert!(e.snapshot().is_empty());
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
        assert_eq!(e.component_count(), 1); // only entity 2 remains
                                            // future op for deleted entity is ignored and never resurrects it.
        assert_eq!(e.apply(&put(1, 1, 99, b"z")), ApplyResult::Ignored);
        assert_eq!(e.component_count(), 1);
    }

    #[test]
    fn apply_batch_returns_only_accepted() {
        let mut e = CrdtEngine::new();
        let batch = vec![
            put(1, 1, 5, b"a"),     // applied
            put(1, 1, 4, b"older"), // ignored (older)
            put(2, 1, 1, b"b"),     // applied
        ];
        let accepted = e.apply_batch(&batch);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0], put(1, 1, 5, b"a"));
        assert_eq!(accepted[1], put(2, 1, 1, b"b"));
    }

    #[test]
    fn reclaim_range_tombstones_and_emits_deletes() {
        let mut e = CrdtEngine::new();
        e.apply(&put(1100, 1, 5, b"a")); // in range [1024,1536)
        e.apply(&put(1101, 1, 5, b"b"));
        e.apply(&put(2000, 1, 5, b"c")); // out of range
        let deletes = e.reclaim_range(1024, 512);
        assert_eq!(deletes.len(), 2);
        assert!(deletes.contains(&CrdtMessage::DeleteEntity { entity: 1100 }));
        assert!(deletes.contains(&CrdtMessage::DeleteEntity { entity: 1101 }));
        assert_eq!(e.component_count(), 1); // only 2000 remains
    }

    #[test]
    fn append_value_survives_in_snapshot_as_append() {
        let mut e = CrdtEngine::new();
        let a = CrdtMessage::Append {
            entity: 1,
            component_id: 9,
            timestamp: 1,
            data: vec![7, 7],
        };
        assert_eq!(e.apply(&a), ApplyResult::Applied);
        let snap = decode_batch(&e.snapshot());
        assert_eq!(snap, vec![a]);
    }

    #[test]
    fn truncated_trailer_is_ignored() {
        let mut batch = encode_batch(&[put(1, 1, 1, b"ok")]);
        batch.extend_from_slice(&[0x05, 0x00]); // partial header
        assert_eq!(decode_batch(&batch), vec![put(1, 1, 1, b"ok")]);
    }

    /// Convergence (the core CRDT property): applying the *same* set of
    /// conflicting ops in *any* order must reach the *same* authoritative state.
    /// This is what guarantees two WS clients on the same scene end identical
    /// after concurrent, conflicting writes to the same (entity, component).
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
        // A pile of conflicting ops on overlapping (entity, component) keys:
        // ties on timestamp (data tie-break), strict-newer wins, deletes,
        // and a delete-component tombstone vs present data on a tie.
        let ops = vec![
            put(1, 1, 5, b"aaa"),
            put(1, 1, 5, b"bbb"), // ties ts=5 with above -> "bbb" (greater) wins
            put(1, 1, 7, b"z"),   // strictly newer -> wins regardless of data
            put(1, 2, 3, b"c2"),
            CrdtMessage::DeleteComponent {
                entity: 1,
                component_id: 2,
                timestamp: 3,
            }, // tie ts=3 with put -> null < data, loses
            CrdtMessage::DeleteComponent {
                entity: 1,
                component_id: 2,
                timestamp: 4,
            }, // newer -> tombstone wins
            put(2, 9, 1, b"keep"),
        ];
        let perms = permutations(&ops);
        assert!(perms.len() >= 5040); // 7! orderings
        let reference = apply_all(&ops);
        // The winner of (1,1) is ts=7 "z"; (1,2) is tombstoned (absent); (2,9) is "keep".
        let decoded = decode_batch(&reference);
        assert!(decoded.contains(&put(1, 1, 7, b"z")));
        assert!(decoded.contains(&put(2, 9, 1, b"keep")));
        assert!(!decoded.iter().any(|m| m.entity() == 1
            && matches!(m, CrdtMessage::Put { component_id, .. } if *component_id == 2)));
        for p in &perms {
            assert_eq!(
                apply_all(p),
                reference,
                "ordering diverged: {p:?}"
            );
        }
    }

    #[test]
    fn delete_entity_convergence_independent_of_order() {
        // DELETE_ENTITY must mask all ops for that entity regardless of arrival
        // order (tombstone is permanent / never resurrects).
        let ops = vec![
            put(1, 1, 5, b"a"),
            CrdtMessage::DeleteEntity { entity: 1 },
            put(1, 1, 99, b"resurrect?"), // even with a far-future ts, stays masked
            put(2, 1, 1, b"survivor"),
        ];
        let reference = apply_all(&ops);
        for p in permutations(&ops) {
            assert_eq!(apply_all(&p), reference, "delete-entity order diverged: {p:?}");
        }
        // Entity 1 fully gone; only entity 2 survives.
        let decoded = decode_batch(&reference);
        assert_eq!(decoded, vec![put(2, 1, 1, b"survivor")]);
    }
}
