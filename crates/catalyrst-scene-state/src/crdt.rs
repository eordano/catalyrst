use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_DELETED_ENTITIES: usize = 4096;

pub mod msg_type {
    pub const PUT_COMPONENT: u32 = 1;
    pub const DELETE_COMPONENT: u32 = 2;
    pub const DELETE_ENTITY: u32 = 3;
    pub const APPEND_VALUE: u32 = 4;
}

const HEADER_LEN: usize = 8;

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

        if len < HEADER_LEN || off + len > buf.len() {
            break;
        }
        let body = &buf[off + HEADER_LEN..off + len];
        match ty {
            msg_type::PUT_COMPONENT | msg_type::APPEND_VALUE if body.len() >= 16 => {
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

pub fn decode_client_batch(body: &[u8], start: u32, size: u32) -> Vec<CrdtMessage> {
    let end = start.saturating_add(size);
    decode_batch(body)
        .into_iter()
        .filter(|m| {
            let e = m.entity();
            e >= start && e < end
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

#[derive(Debug, Clone)]
struct ComponentCell {
    timestamp: u32,

    data: Option<Vec<u8>>,

    is_append: bool,
}

#[derive(Debug)]
pub struct CrdtEngine {
    components: BTreeMap<(u32, u32), ComponentCell>,

    deleted_entities: BTreeSet<u32>,

    deleted_order: VecDeque<u32>,

    max_components: usize,
}

impl Default for CrdtEngine {
    fn default() -> Self {
        Self {
            components: BTreeMap::new(),
            deleted_entities: BTreeSet::new(),
            deleted_order: VecDeque::new(),

            max_components: usize::MAX,
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

    pub fn with_cap(max_components: usize) -> Self {
        Self {
            max_components: max_components.max(1),
            ..Self::default()
        }
    }

    pub fn apply(&mut self, msg: &CrdtMessage) -> ApplyResult {
        let entity = msg.entity();
        if self.deleted_entities.contains(&entity) {
            return ApplyResult::Ignored;
        }

        match msg {
            CrdtMessage::DeleteEntity { entity } => {
                self.tombstone(*entity);

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

    fn tombstone(&mut self, entity: u32) {
        if self.deleted_entities.insert(entity) {
            self.deleted_order.push_back(entity);
            while self.deleted_order.len() > MAX_DELETED_ENTITIES {
                if let Some(oldest) = self.deleted_order.pop_front() {
                    self.deleted_entities.remove(&oldest);
                }
            }
        }
    }

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
                if self.components.len() >= self.max_components {
                    return ApplyResult::Ignored;
                }

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
                Ordering::Less => ApplyResult::Ignored,
                Ordering::Equal => match data_compare(data.as_deref(), cur.data.as_deref()) {
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

                    _ => ApplyResult::Ignored,
                },
            },
        }
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

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    pub fn deleted_count(&self) -> usize {
        self.deleted_entities.len()
    }

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
            self.tombstone(e);
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

        let del = CrdtMessage::DeleteComponent {
            entity: 1,
            component_id: 1,
            timestamp: 5,
        };
        assert_eq!(e.apply(&del), ApplyResult::Ignored);

        let del2 = CrdtMessage::DeleteComponent {
            entity: 1,
            component_id: 1,
            timestamp: 6,
        };
        assert_eq!(e.apply(&del2), ApplyResult::Applied);

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
            CrdtMessage::DeleteComponent {
                entity: 1,
                component_id: 2,
                timestamp: 3,
            },
            CrdtMessage::DeleteComponent {
                entity: 1,
                component_id: 2,
                timestamp: 4,
            },
            put(2, 9, 1, b"keep"),
        ];
        let perms = permutations(&ops);
        assert!(perms.len() >= 5040);
        let reference = apply_all(&ops);

        let decoded = decode_batch(&reference);
        assert!(decoded.contains(&put(1, 1, 7, b"z")));
        assert!(decoded.contains(&put(2, 9, 1, b"keep")));
        assert!(!decoded.iter().any(|m| m.entity() == 1
            && matches!(m, CrdtMessage::Put { component_id, .. } if *component_id == 2)));
        for p in &perms {
            assert_eq!(apply_all(p), reference, "ordering diverged: {p:?}");
        }
    }

    #[test]
    fn tombstone_cap_holds_under_delete_flood() {
        let mut e = CrdtEngine::new();
        for entity in 0..(MAX_DELETED_ENTITIES as u32 * 3) {
            e.apply(&CrdtMessage::DeleteEntity { entity });
        }
        assert_eq!(e.deleted_count(), MAX_DELETED_ENTITIES);

        assert_eq!(
            e.apply(&put(MAX_DELETED_ENTITIES as u32 * 3 - 1, 1, 1, b"x")),
            ApplyResult::Ignored
        );

        assert_eq!(e.apply(&put(0, 1, 1, b"ghost")), ApplyResult::Applied);
    }

    #[test]
    fn reclaim_range_respects_tombstone_cap() {
        let mut e = CrdtEngine::new();
        for entity in 0..(MAX_DELETED_ENTITIES as u32 + 100) {
            e.apply(&put(entity, 1, 1, b"a"));
        }
        e.reclaim_range(0, MAX_DELETED_ENTITIES as u32 + 100);
        assert_eq!(e.deleted_count(), MAX_DELETED_ENTITIES);
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
        assert_eq!(decoded, vec![put(2, 1, 1, b"survivor")]);
    }
}
