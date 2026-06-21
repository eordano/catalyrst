use std::collections::HashMap;

use super::*;
use proptest::prelude::*;

const PARCEL: usize = 0;
const POS_X: usize = 1;
const POS_Y: usize = 2;
const POS_Z: usize = 3;
const VEL_X: usize = 4;
const VEL_Y: usize = 5;
const VEL_Z: usize = 6;
const ROT_Y: usize = 7;
const MOVE_BLEND: usize = 8;
const SLIDE_BLEND: usize = 9;
const HEAD_YAW: usize = 10;
const HEAD_PITCH: usize = 11;
const GLIDE: usize = 12;
const JUMP: usize = 13;
const POINT_X: usize = 14;
const POINT_Y: usize = 15;
const POINT_Z: usize = 16;

fn subject(id: u32, baseline: u32, new_seq: u32) -> BatchSubject {
    BatchSubject {
        subject_id: id,
        baseline_seq: baseline,
        new_seq,
        state_flags: 1,
        fields: [None; FIELD_COUNT],
    }
}

fn set(s: &mut BatchSubject, idx: usize, v: u32) {
    s.fields[idx] = Some(v);
}

fn grounded_run(id: u32, baseline: u32) -> BatchSubject {
    let mut s = subject(id, baseline, baseline + 1);
    set(&mut s, POS_X, 128);
    set(&mut s, POS_Y, 4200);
    set(&mut s, POS_Z, 200);
    set(&mut s, VEL_X, 40);
    set(&mut s, VEL_Z, 41);
    set(&mut s, ROT_Y, 64);
    set(&mut s, MOVE_BLEND, 10);
    s
}

fn sprint(id: u32, baseline: u32) -> BatchSubject {
    let mut s = grounded_run(id, baseline);
    set(&mut s, VEL_Y, 3);
    set(&mut s, SLIDE_BLEND, 7);
    s.state_flags = 1 | 8;
    s
}

fn rotate_in_place(id: u32, baseline: u32) -> BatchSubject {
    let mut s = subject(id, baseline, baseline + 1);
    set(&mut s, ROT_Y, 100);
    set(&mut s, HEAD_YAW, 55);
    set(&mut s, HEAD_PITCH, 40);
    s.state_flags = 1 | 32 | 64;
    s
}

fn aiming(id: u32, baseline: u32) -> BatchSubject {
    let mut s = subject(id, baseline, baseline + 1);
    set(&mut s, POS_X, 130);
    set(&mut s, POS_Z, 205);
    set(&mut s, POINT_X, 65000);
    set(&mut s, POINT_Y, 33);
    set(&mut s, POINT_Z, 131071);
    s.state_flags = 1 | 128;
    s
}

fn full(id: u32, baseline: u32) -> BatchSubject {
    let mut s = subject(id, baseline, baseline + 1);
    let vals = [
        (PARCEL, 99533),
        (POS_X, 255),
        (POS_Y, 8191),
        (POS_Z, 255),
        (VEL_X, 254),
        (VEL_Y, 255),
        (VEL_Z, 3),
        (ROT_Y, 127),
        (MOVE_BLEND, 31),
        (SLIDE_BLEND, 15),
        (HEAD_YAW, 127),
        (HEAD_PITCH, 127),
        (GLIDE, 3),
        (JUMP, 65535),
        (POINT_X, 131071),
        (POINT_Y, 127),
        (POINT_Z, 131071),
    ];
    for (idx, v) in vals {
        set(&mut s, idx, v);
    }
    s.state_flags = 0xBEEF;
    s
}

fn roundtrip_one(s: &BatchSubject) -> BatchSubject {
    let batches = encode_batches(
        777,
        std::slice::from_ref(s),
        MAX_BATCH_BYTES,
        SeqEncoding::Delta,
    );
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].subject_count, 1);
    let baseline = s.baseline_seq;
    let decoded = decode_batch(1, &batches[0].payload, |_| baseline).unwrap();
    assert_eq!(decoded.len(), 1);
    decoded.into_iter().next().unwrap()
}

#[test]
fn bitwriter_is_msb_first_and_byte_aligns_at_end() {
    let mut w = BitWriter::new();
    w.write_bits(0b101, 3);
    w.write_bits(0b1, 1);
    assert_eq!(w.bit_len(), 4);
    let bytes = w.into_bytes();
    assert_eq!(bytes, vec![0b1011_0000]);
}

#[test]
fn bitreader_mirrors_writer_across_byte_boundaries() {
    let mut w = BitWriter::new();
    w.write_bits(0x1FFFF, 17);
    w.write_bits(0xDEAD_BEEF, 32);
    w.write_bits(5, 6);
    let bytes = w.into_bytes();
    let mut r = BitReader::new(&bytes);
    assert_eq!(r.read_bits(17).unwrap(), 0x1FFFF);
    assert_eq!(r.read_bits(32).unwrap(), 0xDEAD_BEEF);
    assert_eq!(r.read_bits(6).unwrap(), 5);
    assert!(r.bits_remaining() < 8, "only pad bits remain");
}

#[test]
fn bitreader_reports_eof_past_the_end() {
    let bytes = [0xFFu8];
    let mut r = BitReader::new(&bytes);
    assert_eq!(r.read_bits(8).unwrap(), 0xFF);
    assert_eq!(r.read_bits(1), Err(BatchError::UnexpectedEof));
}

#[test]
fn grounded_run_is_exactly_109_bits_and_14_payload_bytes() {
    let s = grounded_run(4096, 5000);
    assert_eq!(
        s.present_field_count(),
        7,
        "grounded run changes exactly 7 fields"
    );
    assert_eq!(s.bit_len(SeqEncoding::Delta), 109);
    let batches = encode_batches(
        0,
        std::slice::from_ref(&s),
        MAX_BATCH_BYTES,
        SeqEncoding::Delta,
    );
    assert_eq!(batches[0].payload.len(), 14, "ceil(109/8) = 14 bytes");
    assert_eq!(roundtrip_one(&s), s, "grounded run round-trips");
}

#[test]
fn every_profile_roundtrips() {
    let baseline = 1234;
    for s in [
        grounded_run(1, baseline),
        sprint(2, baseline),
        rotate_in_place(3, baseline),
        aiming(4, baseline),
        full(5, baseline),
    ] {
        assert_eq!(roundtrip_one(&s), s, "profile round-trip failed: {s:?}");
    }
}

#[test]
fn webtransport_subject_id_survives_13_bits() {
    let s = grounded_run(8190, 10);
    assert_eq!(roundtrip_one(&s).subject_id, 8190);
}

#[test]
fn seq_escape_carries_absolute_sequence() {
    let mut s = grounded_run(7, 1000);
    s.new_seq = 1000 + 500;
    assert!(s.seq_delta() >= SEQ_DELTA_ESCAPE);
    let batches = encode_batches(
        0,
        std::slice::from_ref(&s),
        MAX_BATCH_BYTES,
        SeqEncoding::Delta,
    );
    let decoded = decode_batch(1, &batches[0].payload, |_| 999_999).unwrap();
    assert_eq!(decoded[0].new_seq, 1500);
    assert_eq!(decoded[0].subject_id, 7);
}

#[test]
fn seq_delta_boundary_62_stays_inline_63_escapes() {
    let mut inline = subject(1, 100, 162);
    set(&mut inline, POS_X, 1);
    assert!(inline.seq_delta() < SEQ_DELTA_ESCAPE);
    assert_eq!(roundtrip_one(&inline).new_seq, 162);

    let mut escaped = subject(1, 100, 163);
    set(&mut escaped, POS_X, 1);
    assert_eq!(escaped.seq_delta(), SEQ_DELTA_ESCAPE);
    let batches = encode_batches(
        0,
        std::slice::from_ref(&escaped),
        MAX_BATCH_BYTES,
        SeqEncoding::Delta,
    );
    let decoded = decode_batch(1, &batches[0].payload, |_| 0).unwrap();
    assert_eq!(decoded[0].new_seq, 163, "escape encodes the absolute seq");
}

#[test]
fn empty_input_produces_no_batches() {
    assert!(encode_batches(0, &[], MAX_BATCH_BYTES, SeqEncoding::Delta).is_empty());
}

#[test]
fn wrapped_batch_never_exceeds_datagram_cap() {
    use crate::decentraland::pulse::{server_message, PlayerStateDeltaBatch, ServerMessage};
    use crate::transport::webtransport::config::DEFAULT_MAX_DATAGRAM_BYTES;
    use prost::Message as _;

    let subjects: Vec<BatchSubject> = (0..200u32)
        .map(|id| {
            let mut s = full(id, 0);
            s.baseline_seq = 0;
            s.new_seq = u32::MAX;
            s
        })
        .collect();
    let batches = encode_batches(u32::MAX, &subjects, MAX_BATCH_BYTES, SeqEncoding::Delta);
    assert!(batches.len() > 1, "expected at least one full-size split");
    for b in &batches {
        let msg = ServerMessage {
            message: Some(server_message::Message::PlayerStateDeltaBatch(
                PlayerStateDeltaBatch {
                    server_tick: b.server_tick,
                    subject_count: b.subject_count,
                    payload: b.payload.clone(),
                },
            )),
        };
        let len = msg.encode_to_vec().len();
        assert!(
            len <= DEFAULT_MAX_DATAGRAM_BYTES,
            "wrapped batch {len} B exceeds datagram cap {DEFAULT_MAX_DATAGRAM_BYTES}"
        );
    }
}

fn decode_all(batches: &[EncodedBatch], baselines: &HashMap<u32, u32>) -> Vec<BatchSubject> {
    let mut all = Vec::new();
    for b in batches {
        let decoded = decode_batch(b.subject_count, &b.payload, |id| {
            baselines.get(&id).copied().unwrap_or(0)
        })
        .unwrap();
        assert_eq!(decoded.len() as u32, b.subject_count);
        all.extend(decoded);
    }
    all
}

#[test]
fn mtu_split_stays_under_cap_and_reassembles() {
    let baseline = 500;
    let subjects: Vec<BatchSubject> = (0..60u32).map(|id| full(id, baseline)).collect();
    let baselines: HashMap<u32, u32> = subjects
        .iter()
        .map(|s| (s.subject_id, s.baseline_seq))
        .collect();

    let cap = 120;
    let batches = encode_batches(42, &subjects, cap, SeqEncoding::Delta);
    assert!(batches.len() > 1, "small cap must force a split");
    for b in &batches {
        assert!(
            b.payload.len() <= cap,
            "batch payload {} exceeds cap {cap}",
            b.payload.len()
        );
    }
    let total: u32 = batches.iter().map(|b| b.subject_count).sum();
    assert_eq!(total, 60);
    assert_eq!(decode_all(&batches, &baselines), subjects);
}

#[test]
fn oversized_lone_subject_still_ships_alone() {
    let s = full(1, 0);
    let batches = encode_batches(0, std::slice::from_ref(&s), 4, SeqEncoding::Delta);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].payload.len() > 4);
    assert_eq!(batches[0].subject_count, 1);
}

#[test]
fn from_delta_to_delta_preserves_present_fields() {
    let mut delta = PlayerStateDeltaTier0 {
        subject_id: 4096,
        baseline_seq: 10,
        new_seq: 11,
        server_tick: 999,
        ..Default::default()
    };
    delta.position_x = Some(128);
    delta.velocity_x = Some(40);
    delta.glide_state = Some(2);
    delta.jump_count = Some(7);
    delta.point_at_x = Some(65000);

    let subj = BatchSubject::from_delta(&delta, 0b1010);
    let round = roundtrip_one(&subj);
    let back = round.to_delta(999);

    assert_eq!(back.subject_id, delta.subject_id);
    assert_eq!(back.new_seq, delta.new_seq);
    assert_eq!(back.position_x, Some(128));
    assert_eq!(back.velocity_x, Some(40));
    assert_eq!(back.glide_state, Some(2));
    assert_eq!(back.jump_count, Some(7));
    assert_eq!(back.point_at_x, Some(65000));
    assert_eq!(
        back.state_flags,
        Some(0b1010),
        "flags always carried in a batch"
    );
    assert!(back.position_y.is_none(), "absent field stays absent");
}

#[test]
fn absolute_mode_roundtrips_and_is_self_describing() {
    for s in [
        grounded_run(1, 5000),
        sprint(2, 40),
        rotate_in_place(3, 7),
        aiming(4, 900),
        full(5, 12345),
    ] {
        let batches = encode_batches(
            1,
            std::slice::from_ref(&s),
            MAX_BATCH_BYTES,
            SeqEncoding::Absolute,
        );
        assert_eq!(batches.len(), 1);
        assert_eq!(
            peek_seq_encoding(&batches[0].payload).unwrap(),
            SeqEncoding::Absolute,
            "header bit advertises Absolute"
        );
        let decoded = decode_batch(1, &batches[0].payload, |_| 0xDEAD_BEEF).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].subject_id, s.subject_id);
        assert_eq!(decoded[0].new_seq, s.new_seq);
        assert_eq!(decoded[0].state_flags, s.state_flags);
        assert_eq!(decoded[0].fields, s.fields);
    }
}

#[test]
fn delta_header_is_advertised_and_matches_legacy_layout() {
    let s = grounded_run(9, 5000);
    let batches = encode_batches(
        1,
        std::slice::from_ref(&s),
        MAX_BATCH_BYTES,
        SeqEncoding::Delta,
    );
    assert_eq!(
        peek_seq_encoding(&batches[0].payload).unwrap(),
        SeqEncoding::Delta,
        "header bit advertises Delta"
    );
}

#[test]
fn absolute_costs_exactly_26_more_bits_per_subject() {
    let s = grounded_run(1, 5000);
    assert_eq!(s.bit_len(SeqEncoding::Delta), 109);
    assert_eq!(s.bit_len(SeqEncoding::Absolute), 135);
    assert_eq!(
        s.bit_len(SeqEncoding::Absolute) - s.bit_len(SeqEncoding::Delta),
        26
    );

    let subjects: Vec<BatchSubject> = (0..39u32).map(|id| grounded_run(id, 5000)).collect();
    let delta = encode_batches(1, &subjects, MAX_BATCH_BYTES, SeqEncoding::Delta);
    let absolute = encode_batches(1, &subjects, MAX_BATCH_BYTES, SeqEncoding::Absolute);
    assert_eq!(delta.len(), 1);
    assert_eq!(absolute.len(), 1);
    assert_eq!(delta[0].payload.len(), 532, "39-subject Delta payload");
    assert_eq!(
        absolute[0].payload.len(),
        659,
        "39-subject Absolute payload"
    );
}

#[test]
fn delta_drifts_under_loss_while_absolute_stays_exact() {
    fn packet(mode: SeqEncoding, baseline: u32, new_seq: u32) -> EncodedBatch {
        let mut s = subject(5, baseline, new_seq);
        set(&mut s, POS_X, 128);
        encode_batches(1, std::slice::from_ref(&s), MAX_BATCH_BYTES, mode)
            .into_iter()
            .next()
            .unwrap()
    }

    let mut last_known = 100u32;
    let b1 = packet(SeqEncoding::Delta, 100, 101);
    last_known = decode_batch(1, &b1.payload, |_| last_known).unwrap()[0].new_seq;
    assert_eq!(last_known, 101);
    let b3 = packet(SeqEncoding::Delta, 102, 103);
    let d3 = decode_batch(1, &b3.payload, |_| last_known).unwrap()[0].new_seq;
    assert_eq!(d3, 102, "truth is 103 but the delta reconstructs low by 1");
    assert!(d3 < 103);

    let b4 = packet(SeqEncoding::Delta, 103, 104);
    let d4 = decode_batch(1, &b4.payload, |_| last_known).unwrap()[0].new_seq;
    assert_eq!(d4, 102, "two drops -> low by 2 (truth 104)");

    last_known = 104;
    let b5 = packet(SeqEncoding::Delta, 104, 105);
    let d5 = decode_batch(1, &b5.payload, |_| last_known).unwrap()[0].new_seq;
    assert_eq!(
        d5, 105,
        "drift is transient — healed baseline restores exactness"
    );

    let a3 = packet(SeqEncoding::Absolute, 102, 103);
    let da3 = decode_batch(1, &a3.payload, |_| {
        panic!("absolute must not consult last-known")
    })
    .unwrap()[0]
        .new_seq;
    assert_eq!(
        da3, 103,
        "absolute is exact regardless of the dropped batch"
    );
}

fn field_strategy(width: u32) -> impl Strategy<Value = Option<u32>> {
    let hi = if width >= 32 {
        u32::MAX
    } else {
        (1u32 << width) - 1
    };
    prop::option::of(0u32..=hi)
}

prop_compose! {
    fn arb_subject()(
        id in 0u32..8192,
        baseline in any::<u32>(),
        seq_delta in any::<u32>(),
        state_flags in 0u32..=0xFFFF,
        f0 in field_strategy(FIELD_WIDTHS[0]),
        f1 in field_strategy(FIELD_WIDTHS[1]),
        f2 in field_strategy(FIELD_WIDTHS[2]),
        f3 in field_strategy(FIELD_WIDTHS[3]),
        f4 in field_strategy(FIELD_WIDTHS[4]),
        f5 in field_strategy(FIELD_WIDTHS[5]),
        f6 in field_strategy(FIELD_WIDTHS[6]),
        f7 in field_strategy(FIELD_WIDTHS[7]),
        f8 in field_strategy(FIELD_WIDTHS[8]),
        f9 in field_strategy(FIELD_WIDTHS[9]),
        f10 in field_strategy(FIELD_WIDTHS[10]),
        f11 in field_strategy(FIELD_WIDTHS[11]),
        f12 in field_strategy(FIELD_WIDTHS[12]),
        f13 in field_strategy(FIELD_WIDTHS[13]),
        f14 in field_strategy(FIELD_WIDTHS[14]),
        f15 in field_strategy(FIELD_WIDTHS[15]),
        f16 in field_strategy(FIELD_WIDTHS[16]),
    ) -> BatchSubject {
        BatchSubject {
            subject_id: id,
            baseline_seq: baseline,
            new_seq: baseline.wrapping_add(seq_delta),
            state_flags,
            fields: [f0, f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12, f13, f14, f15, f16],
        }
    }
}

fn arb_subjects() -> impl Strategy<Value = Vec<BatchSubject>> {
    prop::collection::vec(arb_subject(), 1..40).prop_map(|mut v| {
        for (i, s) in v.iter_mut().enumerate() {
            s.subject_id = i as u32;
        }
        v
    })
}

proptest! {
    #[test]
    fn arbitrary_batches_roundtrip(subjects in arb_subjects(), cap in 40usize..1400) {
        let baselines: HashMap<u32, u32> =
            subjects.iter().map(|s| (s.subject_id, s.baseline_seq)).collect();
        let batches = encode_batches(1, &subjects, cap, SeqEncoding::Delta);

        let total: u32 = batches.iter().map(|b| b.subject_count).sum();
        prop_assert_eq!(total as usize, subjects.len());
        for b in &batches {
            prop_assert!(b.payload.len() <= cap || b.subject_count == 1);
        }
        prop_assert_eq!(decode_all(&batches, &baselines), subjects);
    }
}
