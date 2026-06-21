#[inline]
fn steps(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

#[inline]
fn round_half_even(x: f32) -> f32 {
    let floor = x.floor();
    let diff = x - floor;
    if diff < 0.5 {
        floor
    } else if diff > 0.5 {
        floor + 1.0
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    }
}

pub fn encode(value: f32, min: f32, max: f32, bits: u32) -> u32 {
    let steps = steps(bits);
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    round_half_even(t * steps as f32) as u32
}

pub fn decode(encoded: u32, min: f32, max: f32, bits: u32) -> f32 {
    let steps = steps(bits);
    encoded as f32 / steps as f32 * (max - min) + min
}

/// Power-law sign+magnitude encoding: `(magnitude << 1) | sign` where the
/// (bits-1)-bit magnitude is `round((|v|/max)^(1/pow) * magSteps)`. Zero
/// canonicalizes to code 0 (a zero magnitude never sets the sign bit).
pub fn encode_power(value: f32, max: f32, pow: f32, bits: u32) -> u32 {
    let magnitude_steps = (1u32 << (bits - 1)) - 1;
    let t = (value.abs() / max).clamp(0.0, 1.0);
    let u = t.powf(1.0 / pow);
    let magnitude = round_half_even(u * magnitude_steps as f32) as u32;
    let sign = u32::from(value < 0.0 && magnitude != 0);
    (magnitude << 1) | sign
}

pub fn decode_power(encoded: u32, max: f32, pow: f32, bits: u32) -> f32 {
    let magnitude_steps = (1u32 << (bits - 1)) - 1;
    let u = (encoded >> 1) as f32 / magnitude_steps as f32;
    let magnitude = max * u.powf(pow);
    if encoded & 1 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_is_half_to_even() {
        assert_eq!(round_half_even(0.5), 0.0);
        assert_eq!(round_half_even(1.5), 2.0);
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(4.5), 4.0);

        assert_eq!(round_half_even(2.4), 2.0);
        assert_eq!(round_half_even(2.6), 3.0);
    }

    #[test]
    fn endpoints_and_clamp() {
        let (min, max, bits) = (-1.0f32, 1.0f32, 10);
        let s = steps(bits);
        assert_eq!(encode(min, min, max, bits), 0);
        assert_eq!(encode(max, min, max, bits), s);

        assert_eq!(encode(-5.0, min, max, bits), 0);
        assert_eq!(encode(5.0, min, max, bits), s);

        assert_eq!(decode(0, min, max, bits), min);
        assert_eq!(decode(s, min, max, bits), max);
    }

    #[test]
    fn midpoint_uses_banker_rounding() {
        assert_eq!(encode(0.5, 0.0, 1.0, 1), 0);

        let v = 2.5f32 / 3.0;
        assert_eq!(encode(v, 0.0, 1.0, 2), 2);
    }

    #[test]
    fn roundtrip_is_close() {
        let (min, max, bits) = (0.0f32, 100.0f32, 16);
        let q = encode(42.0, min, max, bits);
        assert!((decode(q, min, max, bits) - 42.0).abs() < 0.01);
    }

    const VEL: (f32, f32, u32) = (50.0, 2.0, 8);

    #[test]
    fn power_zero_canonicalizes_to_code_zero() {
        let (max, pow, bits) = VEL;
        assert_eq!(encode_power(0.0, max, pow, bits), 0);
        assert_eq!(encode_power(-0.0, max, pow, bits), 0);
        assert_eq!(decode_power(0, max, pow, bits), 0.0);
    }

    #[test]
    fn power_sign_never_set_with_zero_magnitude() {
        let (max, pow, bits) = VEL;

        assert_eq!(encode_power(-1e-6, max, pow, bits), 0);
    }

    #[test]
    fn power_endpoints_and_sign_layout() {
        let (max, pow, bits) = VEL;

        assert_eq!(encode_power(50.0, max, pow, bits), 254);
        assert_eq!(encode_power(-50.0, max, pow, bits), 255);
        assert_eq!(encode_power(100.0, max, pow, bits), 254, "clamped to max");
        assert_eq!(decode_power(254, max, pow, bits), 50.0);
        assert_eq!(decode_power(255, max, pow, bits), -50.0);
    }

    #[test]
    fn power_steps_match_upstream_velocity_constants() {
        let (max, pow, bits) = VEL;

        // VelocityXQuantizedStep: coarsest step, between the two largest magnitudes.
        let coarsest = decode_power(254, max, pow, bits) - decode_power(252, max, pow, bits);
        assert!((coarsest - 0.784_301_6).abs() < 1e-6);

        // Near-zero step: the smallest nonzero magnitude.
        let near_zero = decode_power(2, max, pow, bits);
        assert!((near_zero - 0.003_100_006).abs() < 1e-7);
    }

    #[test]
    fn power_roundtrip_negation_symmetry() {
        let (max, pow, bits) = VEL;
        for v in [0.1f32, 1.0, 5.0, 12.5, 49.9] {
            let pos = encode_power(v, max, pow, bits);
            let neg = encode_power(-v, max, pow, bits);
            assert_eq!(neg, pos | 1, "negative sets only the sign bit");
            assert_eq!(
                decode_power(neg, max, pow, bits),
                -decode_power(pos, max, pow, bits)
            );
        }
    }
}
