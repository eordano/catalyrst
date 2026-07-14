use catalyrst_pulse::quantize::{decode, decode_power, encode, encode_power};

const POS_MIN: f32 = 0.0;
const POS_MAX: f32 = 16.0;
const POS_BITS: u32 = 8;
const ROT_MIN: f32 = 0.0;
const ROT_MAX: f32 = 360.0;
const ROT_BITS: u32 = 7;
const VEL_MAX: f32 = 50.0;
const VEL_POW: f32 = 2.0;
const VEL_BITS: u32 = 8;
const POSY_MAX: f32 = 200.0;
const POSY_BITS: u32 = 13;
const POINT_MIN: f32 = -3000.0;
const POINT_MAX: f32 = 3000.0;
const POINT_BITS: u32 = 17;
const MBLEND_MAX: f32 = 3.0;
const MBLEND_BITS: u32 = 5;
const SBLEND_MAX: f32 = 1.0;
const SBLEND_BITS: u32 = 4;

#[test]
fn position_linear_vectors_match_upstream() {
    let vectors = [
        (0.0f32, 0u32),
        (16.0, 255),
        (-5.0, 0),
        (100.0, 255),
        (8.0, 128),
        (4.0, 64),
    ];
    for (v, code) in vectors {
        assert_eq!(encode(v, POS_MIN, POS_MAX, POS_BITS), code, "encode({v})");
    }
    assert_eq!(decode(0, POS_MIN, POS_MAX, POS_BITS), 0.0);
    assert_eq!(decode(255, POS_MIN, POS_MAX, POS_BITS), 16.0);
}

#[test]
fn rotation_linear_vectors_match_upstream() {
    let vectors = [(0.0f32, 0u32), (360.0, 127), (180.0, 64), (90.0, 32)];
    for (v, code) in vectors {
        assert_eq!(encode(v, ROT_MIN, ROT_MAX, ROT_BITS), code, "encode({v})");
    }
}

#[test]
fn velocity_power_vectors_match_upstream() {
    let vectors = [
        (0.0f32, 0u32),
        (50.0, 254),
        (-50.0, 255),
        (100.0, 254),
        (-100.0, 255),
        (-1e-6, 0),
        (12.5, 128),
        (-12.5, 129),
    ];
    for (v, code) in vectors {
        assert_eq!(
            encode_power(v, VEL_MAX, VEL_POW, VEL_BITS),
            code,
            "encode_power({v})"
        );
    }
    assert_eq!(decode_power(0, VEL_MAX, VEL_POW, VEL_BITS), 0.0);
    assert_eq!(decode_power(254, VEL_MAX, VEL_POW, VEL_BITS), 50.0);
    assert_eq!(decode_power(255, VEL_MAX, VEL_POW, VEL_BITS), -50.0);
}

#[test]
fn wide_and_negative_min_specs_match_upstream() {
    assert_eq!(encode(0.0, 0.0, POSY_MAX, POSY_BITS), 0);
    assert_eq!(encode(200.0, 0.0, POSY_MAX, POSY_BITS), 8191);
    assert_eq!(encode(100.0, 0.0, POSY_MAX, POSY_BITS), 4096);

    assert_eq!(encode(-3000.0, POINT_MIN, POINT_MAX, POINT_BITS), 0);
    assert_eq!(encode(3000.0, POINT_MIN, POINT_MAX, POINT_BITS), 131071);
    assert_eq!(encode(0.0, POINT_MIN, POINT_MAX, POINT_BITS), 65536);
    assert_eq!(encode(-1500.0, POINT_MIN, POINT_MAX, POINT_BITS), 32768);
    assert_eq!(
        encode(-9000.0, POINT_MIN, POINT_MAX, POINT_BITS),
        0,
        "clamped below min"
    );
    assert_eq!(
        encode(9000.0, POINT_MIN, POINT_MAX, POINT_BITS),
        131071,
        "clamped above max"
    );
    assert_eq!(decode(0, POINT_MIN, POINT_MAX, POINT_BITS), -3000.0);
    assert_eq!(decode(131071, POINT_MIN, POINT_MAX, POINT_BITS), 3000.0);

    assert_eq!(encode(0.0, 0.0, MBLEND_MAX, MBLEND_BITS), 0);
    assert_eq!(encode(3.0, 0.0, MBLEND_MAX, MBLEND_BITS), 31);
    assert_eq!(encode(1.5, 0.0, MBLEND_MAX, MBLEND_BITS), 16);
    assert_eq!(encode(1.0, 0.0, SBLEND_MAX, SBLEND_BITS), 15);
    assert_eq!(encode(0.5, 0.0, SBLEND_MAX, SBLEND_BITS), 8);
}

#[test]
fn zero_canonicalizes_regardless_of_sign() {
    assert_eq!(encode_power(0.0, VEL_MAX, VEL_POW, VEL_BITS), 0);
    assert_eq!(encode_power(-0.0, VEL_MAX, VEL_POW, VEL_BITS), 0);
    assert_eq!(encode_power(-1e-9, VEL_MAX, VEL_POW, VEL_BITS), 0);
}

#[test]
fn every_code_stays_within_field_width() {
    let pos_max_code = (1u32 << POS_BITS) - 1;
    let vel_max_code = (1u32 << VEL_BITS) - 1;
    let point_max_code = (1u32 << POINT_BITS) - 1;
    for i in -2000..=2000 {
        let v = i as f32 * 0.06;
        assert!(
            encode(v, POS_MIN, POS_MAX, POS_BITS) <= pos_max_code,
            "position code out of range for {v}"
        );
        assert!(
            encode_power(v, VEL_MAX, VEL_POW, VEL_BITS) <= vel_max_code,
            "velocity code out of range for {v}"
        );
        assert!(
            encode(v, POINT_MIN, POINT_MAX, POINT_BITS) <= point_max_code,
            "point_at code out of range for {v}"
        );
    }
}

#[test]
fn power_roundtrip_is_value_stable() {
    for code in 0u32..=255 {
        let v = decode_power(code, VEL_MAX, VEL_POW, VEL_BITS);
        let re = encode_power(v, VEL_MAX, VEL_POW, VEL_BITS);
        let v2 = decode_power(re, VEL_MAX, VEL_POW, VEL_BITS);
        assert!(
            (v - v2).abs() <= 1.0,
            "code {code}: {v} re-encoded to {re} = {v2}, drift {}",
            (v - v2).abs()
        );
    }
}
