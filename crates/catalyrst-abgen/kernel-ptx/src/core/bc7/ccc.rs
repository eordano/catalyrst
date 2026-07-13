use super::*;

pub(super) fn color_cell_compression(
    mode: usize,
    p: &CCParams,
    res: &mut CCResults,
    cp: &Params,
    num_pixels: usize,
    pixels: &[ColorI],
    refinement: bool,
    t: &OptTables,
) -> u64 {
    res.best_overall_err = u64::MAX;

    if (mode <= 2) || (mode == 4) || (mode >= 6) {
        let cr = pixels[0].c[0];
        let cg = pixels[0].c[1];
        let cb = pixels[0].c[2];
        let ca = pixels[0].c[3];
        let mut all_same = true;
        for i in 1..num_pixels {
            if cr != pixels[i].c[0]
                || cg != pixels[i].c[1]
                || cb != pixels[i].c[2]
                || ca != pixels[i].c[3]
            {
                all_same = false;
                break;
            }
        }
        if all_same {
            let (r, g, b, a) = (cr as usize, cg as usize, cb as usize, ca as usize);
            return match mode {
                0 => pack_mode0_to_one_color(p, t, res, r, g, b, num_pixels, pixels),
                1 => pack_mode1_to_one_color(p, t, res, r, g, b, num_pixels, pixels),
                6 => pack_mode6_to_one_color(p, t, res, r, g, b, a, num_pixels, pixels),
                7 => pack_mode7_to_one_color(p, t, res, r, g, b, a, num_pixels, pixels),
                _ => pack_mode24_to_one_color(p, t, res, r, g, b, num_pixels, pixels),
            };
        }
    }

    let mut mean = Vec4F::default();
    for i in 0..num_pixels {
        for c in 0..4 {
            mean.c[c] += pixels[i].c[c] as f32;
        }
    }
    let inv_n = 1.0 / (num_pixels as i32 as f32);
    let mut mean_scaled = Vec4F::default();
    for c in 0..4 {
        mean_scaled.c[c] = mean.c[c] * inv_n;
    }
    let inv_n255 = 1.0 / (num_pixels as i32 as f32 * 255.0);
    for c in 0..4 {
        mean.c[c] *= inv_n255;
        mean.c[c] = saturate(mean.c[c]);
    }

    let mut axis: Vec4F;
    if p.has_alpha {
        let mut v = Vec4F::default();
        for i in 0..num_pixels {
            let mut color = Vec4F {
                c: [
                    pixels[i].c[0] as f32,
                    pixels[i].c[1] as f32,
                    pixels[i].c[2] as f32,
                    pixels[i].c[3] as f32,
                ],
            };
            for c in 0..4 {
                color.c[c] -= mean_scaled.c[c];
            }
            let a = Vec4F {
                c: [
                    color.c[0] * color.c[0],
                    color.c[1] * color.c[0],
                    color.c[2] * color.c[0],
                    color.c[3] * color.c[0],
                ],
            };
            let b = Vec4F {
                c: [
                    color.c[0] * color.c[1],
                    color.c[1] * color.c[1],
                    color.c[2] * color.c[1],
                    color.c[3] * color.c[1],
                ],
            };
            let cc = Vec4F {
                c: [
                    color.c[0] * color.c[2],
                    color.c[1] * color.c[2],
                    color.c[2] * color.c[2],
                    color.c[3] * color.c[2],
                ],
            };
            let d = Vec4F {
                c: [
                    color.c[0] * color.c[3],
                    color.c[1] * color.c[3],
                    color.c[2] * color.c[3],
                    color.c[3] * color.c[3],
                ],
            };
            let mut nrm = if i != 0 { v } else { color };
            vec4f_normalize(&mut nrm);
            v.c[0] += vec4f_dot(&a, &nrm);
            v.c[1] += vec4f_dot(&b, &nrm);
            v.c[2] += vec4f_dot(&cc, &nrm);
            v.c[3] += vec4f_dot(&d, &nrm);
        }
        axis = v;
        vec4f_normalize(&mut axis);
    } else {
        let mut cov = [0f32; 6];
        for i in 0..num_pixels {
            let r = pixels[i].c[0] as f32 - mean_scaled.c[0];
            let g = pixels[i].c[1] as f32 - mean_scaled.c[1];
            let b = pixels[i].c[2] as f32 - mean_scaled.c[2];
            cov[0] += r * r;
            cov[1] += r * g;
            cov[2] += r * b;
            cov[3] += g * g;
            cov[4] += g * b;
            cov[5] += b * b;
        }
        let mut vfr = 0.9f32;
        let mut vfg = 1.0f32;
        let mut vfb = 0.7f32;
        for _ in 0..3 {
            let r = vfr * cov[0] + vfg * cov[1] + vfb * cov[2];
            let g = vfr * cov[1] + vfg * cov[3] + vfb * cov[4];
            let b = vfr * cov[2] + vfg * cov[4] + vfb * cov[5];
            let mut m = r.abs().max(g.abs()).max(b.abs());
            let (mut rr, mut gg, mut bb) = (r, g, b);
            if m > 1e-10 {
                m = 1.0 / m;
                rr = r * m;
                gg = g * m;
                bb = b * m;
            }
            vfr = rr;
            vfg = gg;
            vfb = bb;
        }
        let mut len = vfr * vfr + vfg * vfg + vfb * vfb;
        if len < 1e-10 {
            axis = Vec4F::default();
        } else {
            len = 1.0 / super::sqrtf(len);
            vfr *= len;
            vfg *= len;
            vfb *= len;
            axis = Vec4F {
                c: [vfr, vfg, vfb, 0.0],
            };
        }
    }

    if vec4f_dot(&axis, &axis) < 0.5 {
        if p.perceptual {
            axis = Vec4F {
                c: [0.213, 0.715, 0.072, if p.has_alpha { 0.715 } else { 0.0 }],
            };
        } else {
            axis = Vec4F {
                c: [1.0, 1.0, 1.0, if p.has_alpha { 1.0 } else { 0.0 }],
            };
        }
        vec4f_normalize(&mut axis);
    }

    let mut l = 1e9f32;
    let mut h = -1e9f32;
    for i in 0..num_pixels {
        let mut q = Vec4F {
            c: [
                pixels[i].c[0] as f32,
                pixels[i].c[1] as f32,
                pixels[i].c[2] as f32,
                pixels[i].c[3] as f32,
            ],
        };
        for c in 0..4 {
            q.c[c] -= mean_scaled.c[c];
        }
        let d = vec4f_dot(&q, &axis);
        l = l.min(d);
        h = h.max(d);
    }
    l *= 1.0 / 255.0;
    h *= 1.0 / 255.0;

    let mut min_color = Vec4F::default();
    let mut max_color = Vec4F::default();
    for c in 0..4 {
        min_color.c[c] = saturate(mean.c[c] + axis.c[c] * l);
        max_color.c[c] = saturate(mean.c[c] + axis.c[c] * h);
    }
    let white = Vec4F { c: [1.0; 4] };
    if vec4f_dot(&min_color, &white) > vec4f_dot(&max_color, &white) {
        core::mem::swap(&mut min_color, &mut max_color);
    }

    if find_optimal_solution(
        mode,
        &min_color,
        &max_color,
        p,
        res,
        cp.pbit_search,
        num_pixels,
        pixels,
    ) == 0
    {
        return 0;
    }
    if !refinement {
        return res.best_overall_err;
    }

    for _ in 0..cp.refinement_passes {
        let mut xl = Vec4F::default();
        let mut xh = Vec4F::default();
        if p.has_alpha {
            compute_lsq_endpoints_rgba(
                num_pixels,
                &res.selectors,
                p.psel_weightsx,
                &mut xl,
                &mut xh,
                pixels,
            );
        } else {
            compute_lsq_endpoints_rgb(
                num_pixels,
                &res.selectors,
                p.psel_weightsx,
                &mut xl,
                &mut xh,
                pixels,
            );
            xl.c[3] = 255.0;
            xh.c[3] = 255.0;
        }
        for c in 0..4 {
            xl.c[c] *= 1.0 / 255.0;
            xh.c[c] *= 1.0 / 255.0;
        }
        if find_optimal_solution(mode, &xl, &xh, p, res, cp.pbit_search, num_pixels, pixels) == 0 {
            return 0;
        }
    }

    if cp.uber_level > 0 {
        let mut selectors_temp = [0i32; 16];
        selectors_temp[..num_pixels].copy_from_slice(&res.selectors[..num_pixels]);
        let max_selector = p.num_selector_weights as i32 - 1;
        let mut min_sel = 16u32;
        let mut max_sel = 0u32;
        for i in 0..num_pixels {
            let s = selectors_temp[i] as u32;
            min_sel = min_sel.min(s);
            max_sel = max_sel.max(s);
        }
        let mut selectors_temp1 = [0i32; 16];

        let run_ls = |sel1: &[i32; 16], res: &mut CCResults| -> bool {
            let mut xl = Vec4F::default();
            let mut xh = Vec4F::default();
            if p.has_alpha {
                compute_lsq_endpoints_rgba(
                    num_pixels,
                    sel1,
                    p.psel_weightsx,
                    &mut xl,
                    &mut xh,
                    pixels,
                );
            } else {
                compute_lsq_endpoints_rgb(
                    num_pixels,
                    sel1,
                    p.psel_weightsx,
                    &mut xl,
                    &mut xh,
                    pixels,
                );
                xl.c[3] = 255.0;
                xh.c[3] = 255.0;
            }
            for c in 0..4 {
                xl.c[c] *= 1.0 / 255.0;
                xh.c[c] *= 1.0 / 255.0;
            }
            find_optimal_solution(mode, &xl, &xh, p, res, cp.pbit_search, num_pixels, pixels) != 0
        };

        if cp.uber1_mask & 1 != 0 {
            for i in 0..num_pixels {
                let mut s = selectors_temp[i] as u32;
                if s == min_sel && s < p.num_selector_weights - 1 {
                    s += 1;
                }
                selectors_temp1[i] = s as i32;
            }
            if !run_ls(&selectors_temp1, res) {
                return 0;
            }
        }
        if cp.uber1_mask & 2 != 0 {
            for i in 0..num_pixels {
                let mut s = selectors_temp[i] as u32;
                if s == max_sel && s > 0 {
                    s -= 1;
                }
                selectors_temp1[i] = s as i32;
            }
            if !run_ls(&selectors_temp1, res) {
                return 0;
            }
        }
        if cp.uber1_mask & 4 != 0 {
            for i in 0..num_pixels {
                let mut s = selectors_temp[i] as u32;
                if s == min_sel && s < p.num_selector_weights - 1 {
                    s += 1;
                } else if s == max_sel && s > 0 {
                    s -= 1;
                }
                selectors_temp1[i] = s as i32;
            }
            if !run_ls(&selectors_temp1, res) {
                return 0;
            }
        }

        let uber_err_thresh = ((num_pixels as u32) * 56) >> 4;
        if cp.uber_level >= 2 && res.best_overall_err > uber_err_thresh as u64 {
            let q = if cp.uber_level >= 4 {
                (cp.uber_level - 2) as i32
            } else {
                1
            };
            let mut ly = -q;
            while ly <= 1 {
                let mut hy = max_selector - 1;
                while hy <= max_selector + q {
                    if !(ly == 0 && hy == max_selector) {
                        for i in 0..num_pixels {
                            selectors_temp1[i] = ((max_selector as f32
                                * (selectors_temp[i] as f32 - ly as f32)
                                / (hy as f32 - ly as f32)
                                + 0.5)
                                .floor())
                            .clamp(0.0, max_selector as f32)
                                as i32;
                        }
                        let mut xl = Vec4F::default();
                        let mut xh = Vec4F::default();
                        if p.has_alpha {
                            compute_lsq_endpoints_rgba(
                                num_pixels,
                                &selectors_temp1,
                                p.psel_weightsx,
                                &mut xl,
                                &mut xh,
                                pixels,
                            );
                        } else {
                            compute_lsq_endpoints_rgb(
                                num_pixels,
                                &selectors_temp1,
                                p.psel_weightsx,
                                &mut xl,
                                &mut xh,
                                pixels,
                            );
                            xl.c[3] = 255.0;
                            xh.c[3] = 255.0;
                        }
                        for c in 0..4 {
                            xl.c[c] *= 1.0 / 255.0;
                            xh.c[c] *= 1.0 / 255.0;
                        }
                        if find_optimal_solution(
                            mode,
                            &xl,
                            &xh,
                            p,
                            res,
                            cp.pbit_search && cp.uber_level >= 2,
                            num_pixels,
                            pixels,
                        ) == 0
                        {
                            return 0;
                        }
                    }
                    hy += 1;
                }
                ly += 1;
            }
        }
    }

    if (mode <= 2) || (mode == 4) || (mode >= 6) {
        let mut avg = CCResults::new();
        avg.best_overall_err = res.best_overall_err;
        let r = itrunc(0.5 + mean.c[0] * 255.0) as usize;
        let g = itrunc(0.5 + mean.c[1] * 255.0) as usize;
        let b = itrunc(0.5 + mean.c[2] * 255.0) as usize;
        let a = itrunc(0.5 + mean.c[3] * 255.0) as usize;
        let avg_err = match mode {
            0 => pack_mode0_to_one_color(p, t, &mut avg, r, g, b, num_pixels, pixels),
            1 => pack_mode1_to_one_color(p, t, &mut avg, r, g, b, num_pixels, pixels),
            6 => pack_mode6_to_one_color(p, t, &mut avg, r, g, b, a, num_pixels, pixels),
            7 => pack_mode7_to_one_color(p, t, &mut avg, r, g, b, a, num_pixels, pixels),
            _ => pack_mode24_to_one_color(p, t, &mut avg, r, g, b, num_pixels, pixels),
        };
        if avg_err < res.best_overall_err {
            res.best_overall_err = avg_err;
            res.low = avg.low;
            res.high = avg.high;
            res.pbits = avg.pbits;

            for i in 0..num_pixels {
                res.selectors[i] = avg.selectors[i];
            }
        }
    }

    res.best_overall_err
}

fn ccc_est_idx(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    ccc_est_idx_scalar(mode, p, idxs, num_pixels, pixels)
}

pub(super) fn ccc_est_idx_scalar(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb) = (255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb) = (0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
    }
    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = (n as f32 - 1.0) / (high - low);
    let inv_n = 1.0 / (n as f32 - 1.0);
    let mut total_errf = 0f32;
    if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += dr * dr + dg * dg + db * db;
        }
    }
    total_errf as i64 as u64
}

fn ccc_est_mode7_idx(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    ccc_est_mode7_idx_scalar(p, idxs, num_pixels, pixels)
}

pub(super) fn ccc_est_mode7_idx_scalar(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb, mut la) = (255f32, 255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb, mut ha) = (0f32, 0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        let a = px.c[3] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        la = la.min(a);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
        ha = ha.max(a);
    }
    let n = 4f32;
    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = (n - 1.0) / (high - low);
    let inv_n = 1.0 / (n - 1.0);
    let mut total_errf = 0f32;
    if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        let wa = p.weights[3] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db + wa * da * da;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += dr * dr + dg * dg + db * db + da * da;
        }
    }
    total_errf as i64 as u64
}

#[inline]
pub(super) fn est_subset_err(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    if mode == 7 {
        ccc_est_mode7_idx(p, idxs, num_pixels, pixels)
    } else {
        ccc_est_idx(mode, p, idxs, num_pixels, pixels)
    }
}

pub(super) fn make_est_params(mode: usize, cp: &Params) -> CCParams {
    let mut params = CCParams::clear();
    params.psel_weights = if G_COLOR_INDEX_BITCOUNT[mode] == 2 {
        &G_WEIGHTS2
    } else {
        &G_WEIGHTS3
    };
    params.num_selector_weights = 1 << G_COLOR_INDEX_BITCOUNT[mode];
    params.weights = cp.weights;
    if mode >= 6 {
        for c in 0..4 {
            params.weights[c] *= cp.mode67_weight_mul[c];
        }
    }
    params.perceptual = cp.perceptual;
    params
}
