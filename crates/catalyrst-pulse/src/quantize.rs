//! Float quantization, byte-exact with upstream Pulse's
//! `Decentraland.Networking.Bitwise.Quantize` (src/Protocol/Generated/Quantize.cs).
//!
//! The one subtle parity point: C# `MathF.Round(float)` rounds half-to-even
//! (banker's rounding), whereas Rust `f32::round()` rounds half away from zero.
//! `round_half_even` below reproduces the C# behavior so encoded integers match
//! the generated `*.Bitwise.cs` output bit-for-bit.

/// Number of quantization steps for `bits` (C#: `(1u << bits) - 1`).
#[inline]
fn steps(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

/// Round half-to-even, matching C# `MathF.Round`. Inputs here are non-negative
/// (`t * steps`, with `t` in `[0,1]`).
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

/// Encode a float to its quantized integer (`Quantize.Encode`). Values outside
/// `[min, max]` are clamped.
pub fn encode(value: f32, min: f32, max: f32, bits: u32) -> u32 {
    let steps = steps(bits);
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    round_half_even(t * steps as f32) as u32
}

/// Decode a quantized integer back to a float (`Quantize.Decode`).
pub fn decode(encoded: u32, min: f32, max: f32, bits: u32) -> f32 {
    let steps = steps(bits);
    encoded as f32 / steps as f32 * (max - min) + min
}

#[cfg(test)]
mod tests {
    use super::*;

    // Banker's rounding is the parity-critical bit: half-to-even, NOT away-from-zero.
    #[test]
    fn round_is_half_to_even() {
        assert_eq!(round_half_even(0.5), 0.0);
        assert_eq!(round_half_even(1.5), 2.0);
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(4.5), 4.0);
        // non-midpoints round normally
        assert_eq!(round_half_even(2.4), 2.0);
        assert_eq!(round_half_even(2.6), 3.0);
    }

    #[test]
    fn endpoints_and_clamp() {
        let (min, max, bits) = (-1.0f32, 1.0f32, 10);
        let s = steps(bits);
        assert_eq!(encode(min, min, max, bits), 0);
        assert_eq!(encode(max, min, max, bits), s);
        // out of range clamps
        assert_eq!(encode(-5.0, min, max, bits), 0);
        assert_eq!(encode(5.0, min, max, bits), s);
        // decode of the endpoints is exact
        assert_eq!(decode(0, min, max, bits), min);
        assert_eq!(decode(s, min, max, bits), max);
    }

    #[test]
    fn midpoint_uses_banker_rounding() {
        // bits=1 -> steps=1; value at the exact midpoint maps to t*steps=0.5 -> 0 (even).
        assert_eq!(encode(0.5, 0.0, 1.0, 1), 0);
        // bits=2 -> steps=3; t=0.5 -> 1.5 -> 2 (even), where away-from-zero would give 2 too;
        // pick t giving 2.5: value=2.5/3 over [0,1] -> 2 (even), away-from-zero would give 3.
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
