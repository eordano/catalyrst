//! Executable counterexamples from the Rocq CRDT development -- now regression
//! pins for the fixes.
//!
//! Every test here is a **refutation witness**: it asserts the behaviour a
//! property *says should hold*, and each one used to FAIL. The test names keep
//! the `refuted_*` prefix and the Coq lemma references so they stay greppable
//! from the formal ledger; what was refuted is the *old* implementation, not
//! the property. They all pass now and must keep passing.
//!
//! Two tests here (`malformed_record_discards_the_rest_of_the_batch` and
//! `record_must_carry_exactly_the_payload_it_declares`) lost the `refuted_`
//! prefix on purpose: D8's stated property was refuted BY THE ORACLE, not by the
//! code -- `@dcl/ecs` also stops at the first unframeable record -- so the test
//! asserts the oracle's rule instead. Nothing here is `#[ignore]`d.
//!
//! Run: `cargo test -p catalyrst-scene-state --test formal_refutations`
//!
//! The oracle for every A-vs-B disagreement is upstream `@dcl/ecs`
//! (`js-sdk-toolchain/packages/@dcl/ecs`); the relevant citations are inline.

use catalyrst_scene_state::crdt::{
    decode_batch, decode_client_batch, encode_batch, pack_entity, ApplyResult, CrdtEngine,
    CrdtMessage,
};
use catalyrst_scene_state::runtime::ServerTransportConfig;

fn put(entity: u32, comp: u32, ts: u32, data: &[u8]) -> CrdtMessage {
    CrdtMessage::Put {
        entity,
        component_id: comp,
        timestamp: ts,
        data: data.to_vec(),
    }
}

/// APPEND_VALUE **always** carries timestamp 0 on the wire -- hardcoded at
/// bevy-explorer `crates/dcl/src/crdt/mod.rs` (`writer.write(&SceneCrdtTimestamp(0))`
/// in `append_component`) and upstream `@dcl/ecs`
/// `grow-only-value-set-component-definition.ts`. Every helper below therefore
/// uses 0; that is not a simplification, it is the protocol.
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

fn del_entity(entity: u32) -> CrdtMessage {
    CrdtMessage::DeleteEntity { entity }
}

fn run(ops: &[CrdtMessage]) -> CrdtEngine {
    let mut e = CrdtEngine::new();
    for m in ops {
        e.apply(m);
    }
    e
}

fn snap(e: &CrdtEngine) -> Vec<CrdtMessage> {
    decode_batch(&e.snapshot())
}

/// Packs a scene entity the way `@dcl/ecs` does (`entity.ts`) and the way
/// bevy-explorer's `SceneEntityId::as_proto_u32` does: `id | (generation << 16)`.
/// This is now the crate's own `pack_entity`; the alias keeps the witnesses
/// readable.
fn pack(number: u16, generation: u16) -> u32 {
    pack_entity(number, generation)
}

// D-NEW-1  is_append is order dependent
// FIXED: `ComponentCell::is_append` no longer exists. PUT_COMPONENT and
//       APPEND_VALUE are routed by message type into two disjoint stores
//       (crdt.rs `lww` / `appends`), so there is no per-cell kind flag left to
//       depend on arrival order -- SDK7 component ids belong to exactly one
//       kind (`lww-element-set-` vs `grow-only-value-set-component-definition.ts`).

/// Two servers fed the **same multiset** of messages must serve the same
/// snapshot. They used not to: the tie at (timestamp 0, identical payload) was
/// `Ignored`, so the cell kept whichever of PUT/APPEND arrived *first*, and
/// `is_append` decided what every future joining client was told the component
/// *is* -- an LWW component or a grow-only event channel.
///
/// Was invisible to `all_orderings_converge_to_same_state` (crdt.rs) because
/// that test only ever compares values.
#[test]
fn refuted_is_append_must_not_depend_on_arrival_order() {
    let m_put = put(1, 1, 0, &[1]);
    let m_app = append(1, 1, &[1]);

    let order1 = snap(&run(&[m_put.clone(), m_app.clone()]));
    let order2 = snap(&run(&[m_app, m_put]));

    assert_eq!(
        order1, order2,
        "same message multiset, different snapshot message TYPE.\n\
         put-then-append -> {order1:?}\n\
         append-then-put -> {order2:?}\n\
         PUT and APPEND must be routed by message type into disjoint stores, so no\n\
         per-cell kind flag can depend on arrival order."
    );
}

// D1  APPEND_VALUE routed through LWW
// FIXED: crdt.rs `CrdtEngine::append` pushes onto a per-(entity, component)
//       grow-only channel and always accepts, mirroring upstream's
//       `grow-only-value-set-component-definition.ts` (`row.raw.push(...)`).

/// APPEND_VALUE is a grow-only event channel: PointerEventsResult,
/// AvatarEmoteCommand, VideoEvent. Every append carries timestamp 0, so when
/// appends went through LWW the second and later appends to one
/// (entity, component) collided at an equal timestamp and only the
/// `data_compare`-maximum survived. The rest were `Ignored` -- and `apply_batch`
/// therefore never relayed them to the other clients either.
///
/// This crate's own `lww_tie_breaks_on_data` already proves the mechanism;
/// nothing connected it to APPEND.
#[test]
fn refuted_appends_must_not_be_swallowed_at_equal_timestamp() {
    let mut e = CrdtEngine::new();
    let first = append(1, 1, &[2]);
    let second = append(1, 1, &[1]);

    assert_eq!(e.apply(&first), ApplyResult::Applied);
    assert_eq!(
        e.apply(&second),
        ApplyResult::Applied,
        "the second APPEND_VALUE to (entity 1, component 1) was Ignored.\n\
         Both appends carry timestamp 0 (that is the protocol), the payload [1] compares\n\
         below [2], so an LWW Equal arm drops it. Every emote / pointer result\n\
         after the first with a lesser payload is silently swallowed AND never relayed."
    );
    // both values are retained, in arrival order -- a list, not a cell
    assert_eq!(e.appended(1, 1), vec![vec![2u8], vec![1u8]]);
}

/// The relay half of the same defect, stated on the observable output
/// (`apply_batch`'s accepted sublist is exactly what reaches the other
/// clients).
#[test]
fn refuted_every_append_must_be_relayed() {
    let mut e = CrdtEngine::new();
    let batch = vec![append(1, 1, &[3]), append(1, 1, &[2]), append(1, 1, &[1])];
    let relayed = e.apply_batch(&batch);

    assert_eq!(
        relayed.len(),
        3,
        "only {} of 3 APPEND_VALUE messages were relayed: {relayed:?}",
        relayed.len()
    );
}

// D3  tombstone FIFO cap
// FIXED: crdt.rs no longer keeps a FIFO of packed ids with a 4096 cap. Removal
//       is a version G-Set (entity NUMBER -> highest generation removed), which
//       is upstream's own `createVersionGSet` (`@dcl/ecs systems/crdt/gset.ts`)
//       and is read exactly as `getEntityState` reads it (`removedVersion >= v`).
//       `max` is commutative and idempotent, so the removal set is a function of
//       the message multiset; and the u16 number space bounds it structurally, so
//       nothing is ever evicted.

/// The surviving tombstone set used to be a function of the *arrival order*,
/// not of the message multiset, so commutativity and convergence both broke:
/// the same 4097 deletes plus one PUT resurrected the entity in one order and
/// not in the other.
///
/// The old `tombstone_cap_holds_under_delete_flood` (crdt.rs) enshrined the
/// resurrection as expected behaviour; it is now
/// `tombstones_never_expire_under_delete_flood`.
#[test]
fn refuted_tombstone_cap_must_not_break_convergence() {
    // 4097 = the old MAX_DELETED_ENTITIES + 1, i.e. one more than the FIFO held.
    let n = 4097u32;
    let victim = 1u32;

    // order 1: victim deleted FIRST -> evicted by the flood
    let mut order1: Vec<CrdtMessage> = (1..=n).map(del_entity).collect();
    // order 2: same multiset, victim deleted LAST -> tombstone survives
    let mut order2: Vec<CrdtMessage> = (2..=n).map(del_entity).collect();
    order2.push(del_entity(victim));

    let probe = put(victim, 9, 7, &[4]);
    order1.push(probe.clone());
    order2.push(probe);

    let s1 = snap(&run(&order1));
    let s2 = snap(&run(&order2));

    assert_eq!(
        s1,
        s2,
        "same multiset of {} deletes + 1 put, different final state.\n\
         victim-deleted-first  -> {} messages   (tombstone evicted, PUT resurrects it)\n\
         victim-deleted-last   -> {} messages   (tombstone alive, PUT ignored)\n\
         removal must be a version G-Set, never a bounded FIFO of packed ids.",
        n,
        s1.len(),
        s2.len()
    );
    // and the victim really is still dead in both
    assert!(!s1
        .iter()
        .any(|m| matches!(m, CrdtMessage::Put { entity, .. } if *entity == victim)));
}

/// Output idempotence, which is a *different* property from state
/// idempotence: a retransmitted DELETE_ENTITY must not be broadcast a second
/// time. Once its tombstone is evicted it is `Applied` again and re-broadcast
/// to every client.
#[test]
fn refuted_resent_delete_entity_must_not_be_rebroadcast() {
    let mut e = CrdtEngine::new();
    let victim = 1u32;
    assert_eq!(e.apply(&del_entity(victim)), ApplyResult::Applied);

    // flood past the old FIFO cap (4096) so an evicting engine forgets the victim
    for entity in 2..=4097u32 {
        e.apply(&del_entity(entity));
    }

    assert_eq!(
        e.apply(&del_entity(victim)),
        ApplyResult::Ignored,
        "a re-sent DELETE_ENTITY was Applied and therefore re-broadcast to every client;\n\
         the engine must not be able to forget a delete it has already seen."
    );
}

// D5  component cap
// FIXED: crdt.rs `enforce_cell_cap` inserts first and then evicts the LARGEST
//       key, so the surviving set is exactly the `max_components` smallest keys
//       ever presented -- a function of the key multiset, never of arrival order
//       (a key among the N smallest can never be the largest of N+1 residents,
//       so once inserted it is never evicted, and every message for it merges).
//       Wired to production at jsruntime/handle.rs
//       (`CrdtEngine::with_cap(limits.crdt_max_components)`), default 100_000
//       (config.rs CRDT_MAX_COMPONENTS).

/// At `max_components`, *which* components exist must not become a function of
/// arrival order, or a busy scene at the ceiling gets a different world after a
/// server restart replays the same messages in a different order.
#[test]
fn refuted_component_cap_must_not_break_convergence() {
    let a = put(1, 1, 1, &[1]);
    let b = put(2, 2, 1, &[2]);

    let mut e1 = CrdtEngine::with_cap(1);
    e1.apply(&a);
    e1.apply(&b);

    let mut e2 = CrdtEngine::with_cap(1);
    e2.apply(&b);
    e2.apply(&a);

    let s1 = snap(&e1);
    let s2 = snap(&e2);
    assert_eq!(
        s1, s2,
        "same two PUTs, opposite order, different surviving component.\n\
         a-then-b -> {s1:?}\n\
         b-then-a -> {s2:?}\n\
         at the cap, eviction must be by key order, not by arrival order."
    );
}

// D6  snapshot() is lossy
// FIXED: crdt.rs snapshot() emits one DELETE_ENTITY per removed entity number
//       (at the G-Set high-water generation, which masks every earlier one on
//       replay) and DELETE_COMPONENT for every component tombstone -- the latter
//       is what upstream's `createDumpLwwFunctionFromCrdt` does, walking the
//       *timestamps* map and writing `DeleteComponent` whenever the data is
//       absent (`lww-element-set-component-definition.ts`).

fn replay(e: &CrdtEngine) -> CrdtEngine {
    let mut r = CrdtEngine::new();
    for m in decode_batch(&e.snapshot()) {
        r.apply(&m);
    }
    r
}

/// A late joiner is served `snapshot()` and nothing else, so `snapshot()` must
/// determine the state. It does not: a DELETE_COMPONENT tombstone is omitted,
/// so the joiner accepts a stale write the authoritative engine rejects -- the
/// deleted component stays visible on that client only, forever.
#[test]
fn refuted_snapshot_must_carry_component_tombstones() {
    let mut e = run(&[put(1, 1, 1, &[9]), del_comp(1, 1, 2)]);
    let mut r = replay(&e);

    let stale = put(1, 1, 1, &[9]);
    let on_server = e.apply(&stale);
    let on_joiner = r.apply(&stale);

    assert_eq!(
        on_joiner, on_server,
        "server says {on_server:?}, snapshot-replayed joiner says {on_joiner:?} for the same\n\
         stale PUT. A snapshot that omits cells with data = None leaves the joiner with no\n\
         record that the component was deleted at timestamp 2."
    );
}

/// Same defect for entity tombstones: a deleted entity can be resurrected by
/// the joining client's own writes.
#[test]
fn refuted_snapshot_must_carry_entity_tombstones() {
    let mut e = run(&[del_entity(5)]);
    let mut r = replay(&e);

    let write = put(5, 1, 1, &[1]);
    let on_server = e.apply(&write);
    let on_joiner = r.apply(&write);

    assert_eq!(
        on_joiner, on_server,
        "server says {on_server:?}, snapshot-replayed joiner says {on_joiner:?}.\n\
         A snapshot that emits no DELETE_ENTITY records loses the removal set on every\n\
         reconnect, and the entity is resurrected client-side."
    );
}

/// The generation-aware half of the same property: a snapshot carries the
/// removal at its high-water generation, so a joiner masks every earlier
/// generation of that number too, and only the *next* generation is live.
#[test]
fn refuted_snapshot_must_carry_entity_tombstones_per_generation() {
    let e = run(&[del_entity(pack(5, 2))]);
    let r = replay(&e);

    for probe in [pack(5, 0), pack(5, 2)] {
        assert!(
            r.is_dead(probe),
            "snapshot-replayed joiner thinks entity number 5 generation {} is alive after \
             generation 2 of the same number was deleted",
            probe >> 16
        );
    }
    assert!(!r.is_dead(pack(5, 3)));
}

/// `snapshot()` is *state*, `APPEND_VALUE` is an *event*. Re-emitting a
/// grow-only value re-fires a stale emote / pointer result / video event at
/// every client that joins, as if it had just happened.
///
/// Upstream has two dumps that disagree: the per-component
/// `dumpCrdtStateToBuffer` re-emits every appended value, but the late-joiner
/// path that actually feeds a new peer
/// (`@dcl/sdk/src/network/state.ts::engineToCrdt`) forwards PUT_COMPONENT only
/// and drops everything else -- and every grow-only component
/// (PointerEventsResult, VideoEvent, AudioEvent, TweenState...) is in its
/// `NOT_SYNC_COMPONENTS` list, i.e. never synced at all. The safe reading is
/// taken here; see the owner-decision TODO on `crdt.rs::snapshot`.
#[test]
fn refuted_snapshot_must_not_replay_events() {
    let e = run(&[append(1, 1, &[5])]);
    let replayed = snap(&e);
    let events: Vec<_> = replayed
        .iter()
        .filter(|m| matches!(m, CrdtMessage::Append { .. }))
        .collect();

    assert!(
        events.is_empty(),
        "snapshot() re-emits {} APPEND_VALUE record(s): {events:?}",
        events.len()
    );
    // the value is still in the engine's grow-only channel, it just is not
    // re-broadcast as a fresh event to joiners
    assert_eq!(e.appended(1, 1), vec![vec![5u8]]);
}

// D4  client range filter vs packed entity ids   (highest ranked finding)
// FIXED: crdt.rs decode_client_batch / reclaim_range compare
//       `entity_number(id)` -- the low 16 bits -- against the number range, which
//       is what both references mean by a range: upstream hands out
//       `reservedLocalEntities + serverLimit + index * clientLimit`
//       (scene-state-server/src/adapters/scene.ts) and bevy allocates from a
//       `RangeInclusive<u16>` (`CrdtContext::new_in_range`). Ranges come from
//       runtime.rs ServerTransportConfig::range_for_client.

/// Pins the range arithmetic the two tests below depend on. Note this
/// corrects the input semantics used for the model: client **0** is
/// `[1024, 1536)`, not `[1536, 2048)` -- the `1536` in `runtime.rs`'s own test
/// is the *second* allocated index.
#[test]
fn client_zero_range_is_1024_512() {
    let cfg = ServerTransportConfig::default();
    assert_eq!(cfg.range_for_client(0), (1024, 512));
    assert_eq!(cfg.range_for_client(1), (1536, 512));
}

/// Client ranges are all far below 2^16, and a recycled entity packs its
/// generation into the high 16 bits, so **every** write to a recycled entity
/// packs to >= 65536 and is unconditionally outside its own client's range.
/// `@dcl/ecs` recycles entity numbers with generation + 1, so this fires as
/// soon as a scene deletes and re-creates an entity.
#[test]
fn refuted_client_range_must_accept_recycled_entities() {
    let (start, size) = ServerTransportConfig::default().range_for_client(0);
    let recycled = pack(start as u16, 1); // same NUMBER, generation 1
    let batch = encode_batch(&[put(recycled, 1, 1, b"x")]);

    let kept = decode_client_batch(&batch, start, size);
    assert_eq!(
        kept.len(),
        1,
        "client 0 owns entity number {start} but its generation-1 write packs to {recycled}\n\
         (= number | generation << 16), which is outside [{start}, {}).\n\
         Comparing the PACKED u32 against a NUMBER range silently drops every write to a\n\
         recycled entity. Proved for all ranges below 2^16.",
        start + size
    );
}

/// The other half: `reclaim_range` walks the same number range, so components
/// stored under a recycled (packed) entity id can never be freed -- a
/// permanent per-scene leak that also defeats the component cap.
#[test]
fn refuted_reclaim_range_must_free_recycled_entities() {
    let (start, size) = ServerTransportConfig::default().range_for_client(0);
    let recycled = pack(start as u16, 1);

    let mut e = CrdtEngine::new();
    assert_eq!(e.apply(&put(recycled, 1, 1, b"x")), ApplyResult::Applied);
    assert_eq!(e.component_count(), 1);

    let deletes = e.reclaim_range(start, size);
    assert_eq!(
        e.component_count(),
        0,
        "reclaim_range({start}, {size}) freed nothing (emitted {deletes:?}); the component\n\
         stored under packed entity {recycled} leaks for the lifetime of the scene."
    );
}

// D2  entity masking granularity
// FIXED: crdt.rs `is_dead` reads the version G-Set by entity NUMBER and returns
//       true iff `removed_generation >= generation` -- `getEntityState` in
//       `@dcl/ecs engine/entity.ts`, and equivalently bevy-explorer
//       `CrdtContext::is_dead` (which stores killed+1 and compares `>`).
//       Bevy already conformed; this engine did not.

/// Killing entity number 7 at generation 2 must also mask a stale in-flight
/// message for generation 0 of the same entity -- that is what `is_dead` does
/// in bevy-explorer and what `@dcl/ecs` means by recycling. Masking only the
/// exact packed u32 lets the stale write resurrect content on the server that
/// no client will ever render.
#[test]
fn refuted_entity_mask_must_cover_older_generations() {
    let mut e = CrdtEngine::new();
    assert_eq!(e.apply(&del_entity(pack(7, 2))), ApplyResult::Applied);

    assert_eq!(
        e.apply(&put(pack(7, 0), 1, 5, &[9])),
        ApplyResult::Ignored,
        "a stale generation-0 write for entity number 7 was Applied after generation 2 of the\n\
         same entity was deleted. Keying the removal set on the packed id ({}) never sees\n\
         that {} is the same entity, one recycle earlier.",
        pack(7, 2),
        pack(7, 0)
    );

    // ...and the recycled entity (generation 3) is a live, different entity
    assert_eq!(
        e.apply(&put(pack(7, 3), 1, 5, &[9])),
        ApplyResult::Applied,
        "recycling entity number 7 at generation 3 must produce a live entity"
    );
}

// D8  framing: one malformed record discards the rest of the batch
// Code: crdt.rs decode_batch -- `if len < HEADER_LEN || off + len > buf.len() { break }`

/// RESOLVED AGAINST THE ORACLE -- the property was the wrong one, so this test
/// now asserts the opposite of what it originally did.
///
/// The finding was that a record whose declared length is < 8 (or which
/// overruns the buffer) makes `decode_batch` `break`, discarding every
/// remaining record in the batch, where bevy-explorer's reader used to
/// resynchronise on the next 8-byte boundary.
///
/// `@dcl/ecs` settles it: `CrdtMessageProtocol.validate`
/// (`packages/@dcl/ecs/src/serialization/crdt/crdtMessageProtocol.ts`) returns
/// false the moment the declared length exceeds the bytes remaining, and
/// `parseChunkMessage`'s `while ((header = getHeader(buffer)))` loop ends right
/// there. The reference reader never resynchronises -- and it cannot: nothing in
/// the format marks a record boundary, so "the next 8-byte boundary" is a guess
/// that invents messages out of payload bytes. bevy-explorer now drops the rest
/// of the batch too, so all three agree.
///
/// What WAS a real defect is that the drop happened in silence; it is logged
/// now (`tracing::warn!` in `decode_batch`).
#[test]
fn malformed_record_discards_the_rest_of_the_batch() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes()); // declared length 0  (< HEADER_LEN)
    buf.extend_from_slice(&1u32.to_le_bytes()); // type PUT_COMPONENT
    let good = del_entity(5);
    buf.extend_from_slice(&encode_batch(std::slice::from_ref(&good)));

    let decoded = decode_batch(&buf);
    assert!(
        decoded.is_empty(),
        "an unframeable record must abort the batch, as @dcl/ecs does: {decoded:?}"
    );

    // control: without the bad record ahead of it, the same record parses
    assert_eq!(
        decode_batch(&encode_batch(std::slice::from_ref(&good))),
        vec![good]
    );
}

/// CROSS-IMPLEMENTATION. A record that frames correctly but whose `data_len`
/// understates its payload used to be accepted here with the value silently
/// truncated to `data_len` bytes, while bevy-explorer rejects it outright
/// (defect 5b there). One peer stored a value the other refused -- and payload
/// bytes are exactly what breaks LWW ties, so the two could disagree about a
/// component while both considered the record valid. Both sides drop it now.
#[test]
fn record_must_carry_exactly_the_payload_it_declares() {
    let good = del_entity(5);

    // a PUT record: 8 header + 16 fields + 4 payload bytes, but data_len says 0
    let mut buf = Vec::new();
    buf.extend_from_slice(&(8u32 + 16 + 4).to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // PUT_COMPONENT
    buf.extend_from_slice(&1000u32.to_le_bytes()); // entity
    buf.extend_from_slice(&7u32.to_le_bytes()); // component
    buf.extend_from_slice(&1u32.to_le_bytes()); // timestamp
    buf.extend_from_slice(&0u32.to_le_bytes()); // data_len -- understates by 4
    buf.extend_from_slice(&[9, 9, 9, 9]);
    buf.extend_from_slice(&encode_batch(std::slice::from_ref(&good)));

    let decoded = decode_batch(&buf);
    assert_eq!(
        decoded,
        vec![good],
        "the mis-declared record must be dropped, and -- because it still FRAMES \
         correctly -- the well-formed record after it must survive"
    );
}

/// `reclaim_range` walks the same NUMBER range, so a recycled entity's cells
/// are freed and its number is tombstoned when the owning client leaves.
/// Comparing packed ids leaked them for the lifetime of the scene *and* left
/// the number untombstoned, so the next client on that range inherited stale
/// state.
#[test]
fn reclaim_range_frees_and_tombstones_recycled_numbers() {
    let (start, size) = ServerTransportConfig::default().range_for_client(0);
    let recycled = pack(start as u16, 2);

    let mut e = CrdtEngine::new();
    assert_eq!(e.apply(&put(recycled, 1, 1, b"x")), ApplyResult::Applied);
    let deletes = e.reclaim_range(start, size);

    assert_eq!(deletes, vec![del_entity(recycled)]);
    assert_eq!(e.component_count(), 0);
    assert!(e.is_dead(pack(start as u16, 0)));
    assert!(e.is_dead(recycled));
    assert!(!e.is_dead(pack(start as u16, 3)));
}

/// The whole point of the snapshot: replaying it must produce a replica that
/// answers every subsequent message exactly as the authoritative engine does.
#[test]
fn snapshot_determines_state_across_all_message_kinds() {
    let ops = vec![
        put(1100, 1, 4, &[9]),
        del_comp(1100, 1, 5),
        put(1101, 2, 1, &[1]),
        append(1101, 7, &[3]),
        del_entity(pack(1102, 2)),
    ];
    let e = run(&ops);
    let r = replay(&e);
    assert_eq!(r.snapshot(), e.snapshot(), "snapshot is not a fixed point");

    for probe in [
        put(1100, 1, 4, &[9]),
        put(1100, 1, 6, &[9]),
        del_comp(1101, 2, 9),
        put(pack(1102, 0), 3, 1, &[1]),
        put(pack(1102, 2), 3, 1, &[1]),
        put(pack(1102, 3), 3, 1, &[1]),
    ] {
        let on_server = run(&ops).apply(&probe);
        let on_joiner = replay(&run(&ops)).apply(&probe);
        assert_eq!(on_joiner, on_server, "joiner diverged on {probe:?}");
    }
}
