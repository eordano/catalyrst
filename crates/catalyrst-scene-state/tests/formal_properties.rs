//! Property-based encodings of the CRDT theorems that were machine-checked in
//! a (since-retired) Rocq development. These must PASS, forever. If one of them
//! ever fails, the Rust has drifted out from under a proof; this file is the
//! durable form of that proof.
//!
//! Run: `cargo test -p catalyrst-scene-state --test formal_properties`
//!
//! Scope discipline, inherited verbatim from `Props.v`: the LWW fragment below
//! is PUT_COMPONENT + DELETE_COMPONENT only, one component-id space, no caps,
//! no APPEND, no DELETE_ENTITY, well-formed frames. Every fragment was excluded
//! because it was REFUTED at the time (see `formal_refutations.rs`), not
//! because it was inconvenient; the refutations are now fixed and pinned there,
//! and this file stays scoped to the fragment the proofs actually cover.

use catalyrst_scene_state::crdt::{
    decode_batch, encode_batch, ApplyResult, CrdtEngine, CrdtMessage,
};
use proptest::prelude::*;

const E: u32 = 1;
const C: u32 = 1;

fn put(entity: u32, comp: u32, ts: u32, data: &[u8]) -> CrdtMessage {
    CrdtMessage::Put {
        entity,
        component_id: comp,
        timestamp: ts,
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

/// A value in the order Core.v proves strictly total: `None` is the tombstone
/// (bottom), `Some(bytes)` is a payload.
fn write(entity: u32, comp: u32, ts: u32, v: Option<&[u8]>) -> CrdtMessage {
    match v {
        Some(d) => put(entity, comp, ts, d),
        None => del_comp(entity, comp, ts),
    }
}

fn build(ops: &[CrdtMessage]) -> CrdtEngine {
    let mut e = CrdtEngine::new();
    for m in ops {
        e.apply(m);
    }
    e
}

/// Does the engine accept `new` over `cur` when the timestamps are EQUAL?
/// This is exactly the `Ordering::Equal` arm of `lww_set`, i.e. the value
/// order under test, observed only through the public API.
fn wins(new: Option<&[u8]>, cur: Option<&[u8]>) -> bool {
    let mut e = build(&[write(E, C, 5, cur)]);
    e.apply(&write(E, C, 5, new)) == ApplyResult::Applied
}

/// Everything an outside observer can learn about an engine: the snapshot it
/// serves, plus how it answers a fixed battery of probe writes. The snapshot
/// now carries component tombstones (so it is state-determining on its own),
/// but the probes are kept: they are the only check that the *timestamp* on a
/// tombstone is honoured, and they would catch a regression that dropped
/// tombstones from the snapshot again.
fn observable(ops: &[CrdtMessage], probes: &[CrdtMessage]) -> (Vec<CrdtMessage>, Vec<bool>) {
    let snapshot = decode_batch(&build(ops).snapshot());
    let answers = probes
        .iter()
        .map(|p| build(ops).apply(p) == ApplyResult::Applied)
        .collect();
    (snapshot, answers)
}

// Code: crdt.rs data_compare + the lww_set Equal arm.

/// Sample values: the tombstone, the empty payload, and payloads chosen to
/// exercise both the length rule and the lexicographic rule.
fn value() -> impl Strategy<Value = Option<Vec<u8>>> {
    prop_oneof![
        Just(None),
        proptest::collection::vec(0u8..4, 0..4).prop_map(Some),
    ]
}

proptest! {
    /// Irreflexivity: a write can never displace an identical write at the
    /// same timestamp. This is what makes retransmits free.
    #[test]
    fn value_order_is_irreflexive(a in value()) {
        prop_assert!(!wins(a.as_deref(), a.as_deref()));
    }

    /// Trichotomy: for any two values at the same timestamp exactly one of
    /// `a` wins, `b` wins, or they are equal. Without this the LWW tie-break
    /// would not be deterministic.
    #[test]
    fn value_order_is_trichotomous(a in value(), b in value()) {
        let ab = wins(a.as_deref(), b.as_deref());
        let ba = wins(b.as_deref(), a.as_deref());
        let eq = a == b;
        prop_assert_eq!(u8::from(ab) + u8::from(ba) + u8::from(eq), 1);
    }

    /// Transitivity.
    #[test]
    fn value_order_is_transitive(a in value(), b in value(), c in value()) {
        if wins(a.as_deref(), b.as_deref()) && wins(b.as_deref(), c.as_deref()) {
            prop_assert!(wins(a.as_deref(), c.as_deref()));
        }
    }
}

/// `Some []` is strictly above the tombstone: an empty payload beats a
/// DELETE_COMPONENT at the same timestamp, and not the other way round.
/// Both implementations and `@dcl/ecs` agree, and the whole delete/undelete
/// protocol rests on it.
#[test]
fn empty_payload_is_strictly_above_the_tombstone() {
    assert!(wins(Some(&[]), None));
    assert!(!wins(None, Some(&[])));
}

proptest! {
    /// Idempotence: re-delivering a message is a no-op on both the state and
    /// the relayed output. Retransmits and snapshot re-sends are free.
    #[test]
    fn lww_step_is_idempotent(ts in 0u32..6, v in value()) {
        let m = write(E, C, ts, v.as_deref());
        let mut e = build(std::slice::from_ref(&m));
        let before = decode_batch(&e.snapshot());
        prop_assert_eq!(e.apply(&m), ApplyResult::Ignored);
        prop_assert_eq!(decode_batch(&e.snapshot()), before);
    }

    /// On acceptance the cell holds EXACTLY the new timestamp and the new
    /// value -- no merge of old and new content.
    /// Coq: Core.lww_step_agree (the B half).
    #[test]
    fn accepted_write_lands_exactly(ts0 in 0u32..4, v0 in value(), ts1 in 0u32..6, v1 in value()) {
        let seed = write(E, C, ts0, v0.as_deref());
        let next = write(E, C, ts1, v1.as_deref());
        let mut e = build(&[seed]);
        if e.apply(&next) == ApplyResult::Applied {
            // a DELETE_COMPONENT lands as a tombstone carrying its timestamp,
            // exactly as upstream's `createDumpLwwFunctionFromCrdt` dumps it
            let expect = match v1.as_deref() {
                Some(d) => vec![put(E, C, ts1, d)],
                None => vec![del_comp(E, C, ts1)],
            };
            prop_assert_eq!(decode_batch(&e.snapshot()), expect);
        }
    }

    /// Commutativity of the single cell on the (timestamp, value) projection:
    /// the join-semilattice fragment. The projection used to be load-bearing --
    /// the old `ComponentCell` was NOT commutative because of its `is_append`
    /// flag -- but PUT and APPEND now live in disjoint stores, so the whole cell
    /// commutes; see `refuted_is_append_must_not_depend_on_arrival_order`.
    #[test]
    fn lww_cell_is_commutative_on_timestamp_and_value(
        t1 in 0u32..4, v1 in value(), t2 in 0u32..4, v2 in value()
    ) {
        let m1 = write(E, C, t1, v1.as_deref());
        let m2 = write(E, C, t2, v2.as_deref());
        let probes: Vec<_> = (0..6).map(|t| put(E, C, t, b"probe")).collect();
        prop_assert_eq!(
            observable(&[m1.clone(), m2.clone()], &probes),
            observable(&[m2, m1], &probes)
        );
    }

    /// Associativity, via all six orders of three writes to one cell.
    #[test]
    fn lww_cell_is_associative(
        t1 in 0u32..4, v1 in value(),
        t2 in 0u32..4, v2 in value(),
        t3 in 0u32..4, v3 in value(),
    ) {
        let m = [
            write(E, C, t1, v1.as_deref()),
            write(E, C, t2, v2.as_deref()),
            write(E, C, t3, v3.as_deref()),
        ];
        let probes: Vec<_> = (0..6).map(|t| put(E, C, t, b"probe")).collect();
        let reference = observable(&m, &probes);
        for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let seq: Vec<_> = order.iter().map(|i| m[*i].clone()).collect();
            prop_assert_eq!(observable(&seq, &probes), reference.clone());
        }
    }
}

// Covers the highest-traffic path: Transform, MeshRenderer, Material.

fn lww_message() -> impl Strategy<Value = CrdtMessage> {
    (1u32..3, 1u32..3, 0u32..4, value()).prop_map(|(e, c, t, v)| write(e, c, t, v.as_deref()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Any permutation of a PUT/DELETE_COMPONENT-only message list yields the
    /// same observable state. This is the fragment that is genuinely a CRDT.
    #[test]
    fn lww_fragment_is_order_independent(
        ops in proptest::collection::vec(lww_message(), 0..8),
        perm in proptest::collection::vec(any::<u8>(), 0..8),
    ) {
        let mut pool: Vec<CrdtMessage> = ops.clone();
        let mut shuffled = Vec::with_capacity(pool.len());
        let mut i = 0usize;
        while !pool.is_empty() {
            let k = if perm.is_empty() { 0 } else { perm[i % perm.len()] as usize % pool.len() };
            shuffled.push(pool.remove(k));
            i += 1;
        }

        let mut probes = Vec::new();
        for e in 1u32..3 {
            for c in 1u32..3 {
                for t in 0u32..5 {
                    probes.push(put(e, c, t, b"probe"));
                }
            }
        }
        prop_assert_eq!(observable(&ops, &probes), observable(&shuffled, &probes));
    }
}

// Code: crdt.rs decode_batch `_ => {}` then `off += len`.

proptest! {
    /// A record with an unknown type (PUT_COMPONENT_NETWORK = 5,
    /// DELETE_COMPONENT_NETWORK = 6, DELETE_ENTITY_NETWORK = 7, and anything
    /// beyond) is skipped without consuming the record that follows it.
    ///
    /// Agreement with bevy-explorer here is real, but agreement is not
    /// correctness: a networked-scene server that discards every
    /// networked-entity message is silently no-op'ing the feature it exists
    /// for. That is a design finding, not a divergence -- see
    /// the crate-level docs.
    #[test]
    fn unknown_message_types_are_skipped_not_desynchronising(
        ty in 5u32..64,
        body in proptest::collection::vec(any::<u8>(), 0..24),
    ) {
        let mut buf = Vec::new();
        buf.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
        buf.extend_from_slice(&ty.to_le_bytes());
        buf.extend_from_slice(&body);
        let follower = put(9, 9, 9, b"after");
        buf.extend_from_slice(&encode_batch(std::slice::from_ref(&follower)));

        prop_assert_eq!(decode_batch(&buf), vec![follower]);
    }
}
