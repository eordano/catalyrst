pub const JUMP_OFFSETS: [usize; 5] = [16, 8, 4, 2, 1];

pub fn alpha_bleed_inplace(rgba: &mut [u8], w: u32, h: u32) {
    let w = w as usize;
    let h = h as usize;
    let n = w * h;
    debug_assert_eq!(rgba.len(), n * 4);

    let mut has_transparent = false;
    let mut has_opaque = false;
    for px in rgba.chunks_exact(4) {
        if px[3] == 0 {
            has_transparent = true;
        } else {
            has_opaque = true;
        }
        if has_transparent && has_opaque {
            break;
        }
    }
    if !(has_transparent && has_opaque) {
        return;
    }

    let mut seed: Vec<i32> = vec![-1; n];
    for i in 0..n {
        if rgba[i * 4 + 3] > 0 {
            seed[i] = i as i32;
        }
    }

    let l1 = |i: usize, s: i32| -> i32 {
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        let (sx, sy) = (s % w as i32, s / w as i32);
        (x - sx).abs() + (y - sy).abs()
    };

    for k in JUMP_OFFSETS {
        let snap = seed.clone();
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                if rgba[idx * 4 + 3] > 0 {
                    continue;
                }
                let mut best = seed[idx];
                let mut bestd = if best >= 0 { l1(idx, best) } else { i32::MAX };
                let taps = [
                    (x >= k).then(|| idx - k),
                    (x + k < w).then(|| idx + k),
                    (y >= k).then(|| idx - k * w),
                    (y + k < h).then(|| idx + k * w),
                ];
                for tap in taps.into_iter().flatten() {
                    let s = snap[tap];
                    if s >= 0 {
                        let d = l1(idx, s);
                        if d < bestd {
                            bestd = d;
                            best = s;
                        }
                    }
                }
                seed[idx] = best;
            }
        }
    }

    let snap_rgb: Vec<u8> = rgba.to_vec();
    for i in 0..n {
        if rgba[i * 4 + 3] == 0 && seed[i] >= 0 {
            let s = seed[i] as usize * 4;
            rgba[i * 4] = snap_rgb[s];
            rgba[i * 4 + 1] = snap_rgb[s + 1];
            rgba[i * 4 + 2] = snap_rgb[s + 2];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_when_fully_opaque() {
        let mut rgba = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let before = rgba.clone();
        alpha_bleed_inplace(&mut rgba, 2, 1);
        assert_eq!(rgba, before);
    }

    #[test]
    fn no_op_when_fully_transparent() {
        let mut rgba = vec![0, 0, 0, 0, 0, 0, 0, 0];
        alpha_bleed_inplace(&mut rgba, 2, 1);
        assert_eq!(rgba, vec![0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn single_seed_fills_plus_but_not_diagonals() {
        let mut rgba = vec![0u8; 3 * 3 * 4];
        rgba[4 * 4] = 100;
        rgba[4 * 4 + 1] = 200;
        rgba[4 * 4 + 2] = 50;
        rgba[4 * 4 + 3] = 255;
        alpha_bleed_inplace(&mut rgba, 3, 3);
        for (i, expected) in [
            (0usize, false),
            (1, true),
            (2, false),
            (3, true),
            (4, true),
            (5, true),
            (6, false),
            (7, true),
            (8, false),
        ] {
            if expected {
                assert_eq!(rgba[i * 4], 100, "R at {i}");
                assert_eq!(rgba[i * 4 + 1], 200, "G at {i}");
                assert_eq!(rgba[i * 4 + 2], 50, "B at {i}");
            } else {
                assert_eq!(&rgba[i * 4..i * 4 + 3], &[0, 0, 0], "corner at {i}");
            }
            let expected_a = if i == 4 { 255 } else { 0 };
            assert_eq!(rgba[i * 4 + 3], expected_a, "A at {i}");
        }
    }

    #[test]
    fn reach_caps_at_31() {
        let n = 40;
        let mut rgba = vec![0u8; n * 4];
        rgba[0] = 200;
        rgba[3] = 255;
        alpha_bleed_inplace(&mut rgba, n as u32, 1);
        for i in 0..n {
            let expected = if i <= 31 { 200 } else { 0 };
            assert_eq!(rgba[i * 4], expected, "R at {i}");
        }
    }

    #[test]
    fn nearest_seed_wins_no_averaging() {
        let n = 9;
        let mut rgba = vec![0u8; n * 4];
        rgba[0] = 200;
        rgba[3] = 255;
        rgba[(n - 1) * 4 + 2] = 90;
        rgba[(n - 1) * 4 + 3] = 255;
        alpha_bleed_inplace(&mut rgba, n as u32, 1);
        for i in 1..n - 1 {
            let (r, b) = (rgba[i * 4], rgba[i * 4 + 2]);
            assert!(
                (r == 200 && b == 0) || (r == 0 && b == 90),
                "pixel {i} must copy a seed exactly, got {r},{b}"
            );
            if i < 4 {
                assert_eq!(r, 200, "left of midpoint at {i}");
            }
            if i > 4 {
                assert_eq!(b, 90, "right of midpoint at {i}");
            }
        }
    }

    #[test]
    fn partial_alpha_pixels_are_seeds_and_keep_rgb() {
        let mut rgba = vec![0u8; 3 * 4];
        rgba[0] = 77;
        rgba[3] = 9;
        alpha_bleed_inplace(&mut rgba, 3, 1);
        assert_eq!(rgba[0], 77);
        assert_eq!(rgba[3], 9);
        assert_eq!(rgba[4], 77, "neighbor bled from partial-alpha seed");
        assert_eq!(rgba[7], 0, "alpha untouched");
    }

    #[test]
    fn alpha_zero_with_no_opaque_seed_is_noop() {
        let mut rgba = vec![10, 20, 30, 0, 40, 50, 60, 0];
        let before = rgba.clone();
        alpha_bleed_inplace(&mut rgba, 2, 1);
        assert_eq!(rgba, before);
    }
}
