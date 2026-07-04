const C: usize = 4;

#[inline]
fn cubic_bc(x: f64, b: f64, c: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
            + (-18.0 + 12.0 * b + 6.0 * c) * x * x
            + (6.0 - 2.0 * b))
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x * x * x
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}

struct AxisPlan {
    taps: Vec<Vec<(usize, f64)>>,
}

fn axis_plan(n: usize, m: usize, b: f64, c: f64) -> AxisPlan {
    let ratio = n as f64 / m as f64;
    let scalew = if ratio > 1.0 { ratio } else { 1.0 };
    let support = 2.0;
    let mut taps = Vec::with_capacity(m);
    for d in 0..m {
        let center = (d as f64 + 0.5) * ratio - 0.5;
        let lo = (center - support * scalew).floor() as i64;
        let hi = (center + support * scalew).ceil() as i64;
        let mut row = Vec::with_capacity((hi - lo + 1) as usize);
        let mut p = lo;
        while p <= hi {
            let w = cubic_bc((center - p as f64) / scalew, b, c);
            if w != 0.0 {
                let pc = p.clamp(0, n as i64 - 1) as usize;
                row.push((pc, w));
            }
            p += 1;
        }
        taps.push(row);
    }
    AxisPlan { taps }
}

const MITCHELL_B: f64 = 1.0 / 3.0;
const MITCHELL_C: f64 = 1.0 / 3.0;
const BSPLINE_B: f64 = 1.0;
const BSPLINE_C: f64 = 0.0;

fn plan_for_axis(n: usize, m: usize) -> AxisPlan {
    if m < n {
        axis_plan(n, m, MITCHELL_B, MITCHELL_C)
    } else {
        axis_plan(n, m, BSPLINE_B, BSPLINE_C)
    }
}

pub fn box_downscale_rgba(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    debug_assert_eq!(src.len(), sw * sh * C);
    if (sw, sh) == (dw, dh) {
        return src.to_vec();
    }

    let hplan = if dw == sw {
        None
    } else {
        Some(plan_for_axis(sw, dw))
    };
    let inter_w = dw;
    let mut inter = vec![0f64; sh * inter_w * C];
    match &hplan {
        None => {
            for i in 0..sh * sw * C {
                inter[i] = src[i] as f64;
            }
        }
        Some(plan) => {
            let src_rs = sw * C;
            let dst_rs = inter_w * C;
            for y in 0..sh {
                let srow = y * src_rs;
                let drow = y * dst_rs;
                for (x, taps) in plan.taps.iter().enumerate() {
                    let mut acc = [0f64; C];
                    let mut wsum = 0f64;
                    for &(sx, w) in taps {
                        let o = srow + sx * C;
                        for ch in 0..C {
                            acc[ch] += w * src[o + ch] as f64;
                        }
                        wsum += w;
                    }
                    let o = drow + x * C;
                    for ch in 0..C {
                        inter[o + ch] = acc[ch] / wsum;
                    }
                }
            }
        }
    }

    let vplan = if dh == sh {
        None
    } else {
        Some(plan_for_axis(sh, dh))
    };
    let mut out = vec![0u8; dh * dw * C];
    let inter_rs = inter_w * C;
    let out_rs = dw * C;
    let finish = |v: f64| -> u8 { v.round().clamp(0.0, 255.0) as u8 };
    match &vplan {
        None => {
            for i in 0..dh * dw * C {
                out[i] = finish(inter[i]);
            }
        }
        Some(plan) => {
            for (y, taps) in plan.taps.iter().enumerate() {
                let drow = y * out_rs;
                for x in 0..inter_w {
                    let mut acc = [0f64; C];
                    let mut wsum = 0f64;
                    for &(sy, w) in taps {
                        let o = sy * inter_rs + x * C;
                        for ch in 0..C {
                            acc[ch] += w * inter[o + ch];
                        }
                        wsum += w;
                    }
                    let o = drow + x * C;
                    for ch in 0..C {
                        out[o + ch] = finish(acc[ch] / wsum);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_col(n_src: usize, n_dst: usize, pos: usize) -> Vec<u8> {
        let mut src = vec![0u8; 2 * n_src * C];
        for x in 0..2 {
            let o = (pos * 2 + x) * C;
            src[o] = 255;
            src[o + 1] = 255;
            src[o + 2] = 255;
            src[o + 3] = 255;
        }

        for y in 0..n_src {
            for x in 0..2 {
                src[(y * 2 + x) * C + 3] = 255;
            }
        }
        let out = box_downscale_rgba(&src, 2, n_src, 2, n_dst);

        (0..n_dst).map(|y| out[(y * 2) * C]).collect()
    }

    #[test]
    fn downscale_impulse_matches_unity() {
        let col = impulse_col(300, 256, 150);
        assert_eq!(col[127], 22);
        assert_eq!(col[128], 192);
        assert_eq!(col[129], 5);

        let col0 = impulse_col(300, 256, 0);
        assert_eq!(col0[0], 211);
        assert_eq!(col0[1], 5);

        let col9 = impulse_col(300, 256, 299);
        assert_eq!(col9[254], 5);
        assert_eq!(col9[255], 211);
    }

    #[test]
    fn upscale_impulse_matches_unity() {
        let col = impulse_col(200, 256, 100);
        assert_eq!(col[126], 2);
        assert_eq!(col[127], 58);
        assert_eq!(col[128], 167);
        assert_eq!(col[129], 94);
        assert_eq!(col[130], 7);
    }

    #[test]
    fn step_edge_byte_domain() {
        let mut src = vec![0u8; 2 * 300 * C];
        for y in 0..300 {
            let v = if y < 150 { 0 } else { 255 };
            for x in 0..2 {
                let o = (y * 2 + x) * C;
                src[o] = v;
                src[o + 1] = v;
                src[o + 2] = v;
                src[o + 3] = 255;
            }
        }
        let out = box_downscale_rgba(&src, 2, 300, 2, 256);
        let col: Vec<u8> = (0..256).map(|y| out[(y * 2) * C]).collect();
        assert_eq!(col[127], 19);
        assert_eq!(col[128], 236);
        assert_eq!(col[126], 0);
        assert_eq!(col[129], 255);
    }

    #[test]
    fn identity_passthrough() {
        let src = vec![7u8; 4 * 4 * C];
        let out = box_downscale_rgba(&src, 4, 4, 4, 4);
        assert_eq!(out, src);
    }
}
