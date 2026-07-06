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
}
