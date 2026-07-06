use super::*;

fn eval_4way_pbit_with_tiebreak(
    lo: &[ColorI; 2],
    hi: &[ColorI; 2],
    p: &CCParams,
    res: &mut CCResults,
    num_pixels: usize,
    pixels: &[ColorI],
) {
    const RATIO_NUM: u64 = 1;
    const RATIO_DEN: u64 = 8192;
    let pbit_options: [[u32; 2]; 4] = [[0, 0], [0, 1], [1, 0], [1, 1]];
    let lo_idx = [0usize, 0, 1, 1];
    let hi_idx = [0usize, 1, 0, 1];
    let mut errs = [u64::MAX; 4];
    let mut snapshots: [Option<CCResults>; 4] = [None, None, None, None];
    let baseline = res.clone();
    for k in 0..4 {
        let mut local = baseline.clone();
        let e = evaluate_solution(
            &lo[lo_idx[k]],
            &hi[hi_idx[k]],
            &pbit_options[k],
            p,
            &mut local,
            num_pixels,
            pixels,
        );
        errs[k] = e;
        snapshots[k] = Some(local);
    }
    let min_err = *errs.iter().min().unwrap();
    let tol = min_err.saturating_mul(RATIO_NUM) / RATIO_DEN;
    let band = min_err.saturating_add(tol);
    let mut winner = 0usize;
    let mut best_rank: (u32, u32) = (0, 0);
    let mut found = false;
    for k in 0..4 {
        if errs[k] <= band {
            let rank = (pbit_options[k][0], pbit_options[k][1]);
            if !found || rank > best_rank {
                winner = k;
                best_rank = rank;
                found = true;
            }
        }
    }
    let winner_snap = snapshots[winner].take().unwrap();
    if winner_snap.best_overall_err < res.best_overall_err {
        *res = winner_snap;
    }
}

fn fix_degenerate_endpoints(
    mode: usize,
    tmin: &mut ColorI,
    tmax: &mut ColorI,
    xl: &Vec4F,
    xh: &Vec4F,
    iscale: i32,
) {
    if mode == 1 || mode == 4 {
        for i in 0..3 {
            if tmin.c[i] == tmax.c[i] && (xl.c[i] - xh.c[i]).abs() > 0.0 {
                if tmin.c[i] > (iscale >> 1) {
                    if tmin.c[i] > 0 {
                        tmin.c[i] -= 1;
                    } else if tmax.c[i] < iscale {
                        tmax.c[i] += 1;
                    }
                } else if tmax.c[i] < iscale {
                    tmax.c[i] += 1;
                } else if tmin.c[i] > 0 {
                    tmin.c[i] -= 1;
                }
                if mode == 4 {
                    if tmin.c[i] > (iscale >> 1) {
                        if tmax.c[i] < iscale {
                            tmax.c[i] += 1;
                        } else if tmin.c[i] > 0 {
                            tmin.c[i] -= 1;
                        }
                    } else if tmin.c[i] > 0 {
                        tmin.c[i] -= 1;
                    } else if tmax.c[i] < iscale {
                        tmax.c[i] += 1;
                    }
                }
            }
        }
    }
}

fn find_optimal_solution(
    mode: usize,
    pxl: &Vec4F,
    pxh: &Vec4F,
    p: &CCParams,
    res: &mut CCResults,
    pbit_search: bool,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut xl = *pxl;
    let mut xh = *pxh;
    for i in 0..4 {
        xl.c[i] = saturate(xl.c[i]);
        xh.c[i] = saturate(xh.c[i]);
    }

    if p.has_pbits {
        let iscalep = (1i32 << (p.comp_bits + 1)) - 1;
        let scalep = iscalep as f32;
        let total_comps = if p.has_alpha { 4 } else { 3 };
        if pbit_search {
            if !p.endpoints_share_pbit {
                let mut lo = [ColorI::default(); 2];
                let mut hi = [ColorI::default(); 2];
                for pp in 0..2usize {
                    let p_i = pp as i32;
                    let mut xmin = ColorI::default();
                    let mut xmax = ColorI::default();
                    for c in 0..4 {
                        xmin.c[c] = itrunc((xl.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmin.c[c] = xmin.c[c].clamp(p_i, iscalep - 1 + p_i);
                        xmax.c[c] = itrunc((xh.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmax.c[c] = xmax.c[c].clamp(p_i, iscalep - 1 + p_i);
                    }
                    lo[pp] = xmin;
                    hi[pp] = xmax;
                    for c in 0..4 {
                        lo[pp].c[c] >>= 1;
                        hi[pp].c[c] >>= 1;
                    }
                }
                fix_degenerate_endpoints(mode, &mut lo[0], &mut hi[0], &xl, &xh, iscalep >> 1);
                fix_degenerate_endpoints(mode, &mut lo[1], &mut hi[1], &xl, &xh, iscalep >> 1);
                if mode == 6 {
                    eval_4way_pbit_with_tiebreak(&lo, &hi, p, res, num_pixels, pixels);
                } else {
                    evaluate_solution(&lo[0], &hi[0], &[0, 0], p, res, num_pixels, pixels);
                    evaluate_solution(&lo[0], &hi[1], &[0, 1], p, res, num_pixels, pixels);
                    evaluate_solution(&lo[1], &hi[0], &[1, 0], p, res, num_pixels, pixels);
                    evaluate_solution(&lo[1], &hi[1], &[1, 1], p, res, num_pixels, pixels);
                }
            } else {
                let mut lo = [ColorI::default(); 2];
                let mut hi = [ColorI::default(); 2];
                for pp in 0..2usize {
                    let p_i = pp as i32;
                    let mut xmin = ColorI::default();
                    let mut xmax = ColorI::default();
                    for c in 0..4 {
                        xmin.c[c] = itrunc((xl.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmin.c[c] = xmin.c[c].clamp(p_i, iscalep - 1 + p_i);
                        xmax.c[c] = itrunc((xh.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmax.c[c] = xmax.c[c].clamp(p_i, iscalep - 1 + p_i);
                    }
                    lo[pp] = xmin;
                    hi[pp] = xmax;
                    for c in 0..4 {
                        lo[pp].c[c] >>= 1;
                        hi[pp].c[c] >>= 1;
                    }
                }
                fix_degenerate_endpoints(mode, &mut lo[0], &mut hi[0], &xl, &xh, iscalep >> 1);
                fix_degenerate_endpoints(mode, &mut lo[1], &mut hi[1], &xl, &xh, iscalep >> 1);
                evaluate_solution(&lo[0], &hi[0], &[0, 0], p, res, num_pixels, pixels);
                evaluate_solution(&lo[1], &hi[1], &[1, 1], p, res, num_pixels, pixels);
            }
        } else {
            let mut best_pbits = [0u32; 2];
            let mut best_min = ColorI::default();
            let mut best_max = ColorI::default();
            if !p.endpoints_share_pbit {
                let mut best_err0 = 1e9f32;
                let mut best_err1 = 1e9f32;
                for pp in 0..2usize {
                    let p_i = pp as i32;
                    let mut xmin = ColorI::default();
                    let mut xmax = ColorI::default();
                    for c in 0..4 {
                        xmin.c[c] = itrunc((xl.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmin.c[c] = xmin.c[c].clamp(p_i, iscalep - 1 + p_i);
                        xmax.c[c] = itrunc((xh.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmax.c[c] = xmax.c[c].clamp(p_i, iscalep - 1 + p_i);
                    }
                    let sl = scale_color(&xmin, p);
                    let sh = scale_color(&xmax, p);
                    let mut err0 = 0f32;
                    let mut err1 = 0f32;
                    for i in 0..total_comps {
                        err0 += sq(sl.c[i] as f32 - xl.c[i] * 255.0);
                        err1 += sq(sh.c[i] as f32 - xh.c[i] * 255.0);
                    }
                    if err0 < best_err0 {
                        best_err0 = err0;
                        best_pbits[0] = pp as u32;
                        for c in 0..4 {
                            best_min.c[c] = xmin.c[c] >> 1;
                        }
                    }
                    if err1 < best_err1 {
                        best_err1 = err1;
                        best_pbits[1] = pp as u32;
                        for c in 0..4 {
                            best_max.c[c] = xmax.c[c] >> 1;
                        }
                    }
                }
            } else {
                let mut best_err = 1e9f32;
                for pp in 0..2usize {
                    let p_i = pp as i32;
                    let mut xmin = ColorI::default();
                    let mut xmax = ColorI::default();
                    for c in 0..4 {
                        xmin.c[c] = itrunc((xl.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmin.c[c] = xmin.c[c].clamp(p_i, iscalep - 1 + p_i);
                        xmax.c[c] = itrunc((xh.c[c] * scalep - p_i as f32) / 2.0 + 0.5) * 2 + p_i;
                        xmax.c[c] = xmax.c[c].clamp(p_i, iscalep - 1 + p_i);
                    }
                    let sl = scale_color(&xmin, p);
                    let sh = scale_color(&xmax, p);
                    let mut err = 0f32;
                    for i in 0..total_comps {
                        err += sq(sl.c[i] as f32 / 255.0 - xl.c[i])
                            + sq(sh.c[i] as f32 / 255.0 - xh.c[i]);
                    }
                    if err < best_err {
                        best_err = err;
                        best_pbits = [pp as u32, pp as u32];
                        for c in 0..4 {
                            best_min.c[c] = xmin.c[c] >> 1;
                            best_max.c[c] = xmax.c[c] >> 1;
                        }
                    }
                }
            }
            fix_degenerate_endpoints(mode, &mut best_min, &mut best_max, &xl, &xh, iscalep >> 1);
            if res.best_overall_err == u64::MAX
                || best_min.c != res.low.c
                || best_max.c != res.high.c
                || best_pbits[0] != res.pbits[0]
                || best_pbits[1] != res.pbits[1]
            {
                evaluate_solution(
                    &best_min,
                    &best_max,
                    &best_pbits,
                    p,
                    res,
                    num_pixels,
                    pixels,
                );
            }
        }
    } else {
        let iscale = (1i32 << p.comp_bits) - 1;
        let scale = iscale as f32;
        let mut tmin = ColorI::default();
        let mut tmax = ColorI::default();
        for c in 0..4 {
            tmin.c[c] = itrunc(xl.c[c] * scale + 0.5).clamp(0, 255);
            tmax.c[c] = itrunc(xh.c[c] * scale + 0.5).clamp(0, 255);
        }
        fix_degenerate_endpoints(mode, &mut tmin, &mut tmax, &xl, &xh, iscale);
        if res.best_overall_err == u64::MAX || tmin.c != res.low.c || tmax.c != res.high.c {
            evaluate_solution(&tmin, &tmax, &[0, 0], p, res, num_pixels, pixels);
        }
        if mode == 2 {
            let mut smin = tmin;
            let mut smax = tmax;
            for c in 0..3 {
                if smin.c[c] < iscale {
                    smin.c[c] += 1;
                }
                if smax.c[c] > 0 {
                    smax.c[c] -= 1;
                }
            }
            if smin.c != tmin.c || smax.c != tmax.c {
                evaluate_solution(&smin, &smax, &[0, 0], p, res, num_pixels, pixels);
            }
        }
    }
    res.best_overall_err
}

#[inline]
fn sq(s: f32) -> f32 {
    s * s
}

pub(super) fn color_cell_compression(
    mode: usize,
    p: &CCParams,
    res: &mut CCResults,
    cp: &Params,
    num_pixels: usize,
    pixels: &[ColorI],
    refinement: bool,
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
                0 => pack_mode0_to_one_color(p, res, r, g, b, num_pixels, pixels),
                1 => pack_mode1_to_one_color(p, res, r, g, b, num_pixels, pixels),
                6 => pack_mode6_to_one_color(p, res, r, g, b, a, num_pixels, pixels),
                7 => pack_mode7_to_one_color(p, res, r, g, b, a, num_pixels, pixels),
                _ => pack_mode24_to_one_color(p, res, r, g, b, num_pixels, pixels),
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
            len = 1.0 / len.sqrt();
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
        std::mem::swap(&mut min_color, &mut max_color);
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
            0 => pack_mode0_to_one_color(p, &mut avg, r, g, b, num_pixels, pixels),
            1 => pack_mode1_to_one_color(p, &mut avg, r, g, b, num_pixels, pixels),
            6 => pack_mode6_to_one_color(p, &mut avg, r, g, b, a, num_pixels, pixels),
            7 => pack_mode7_to_one_color(p, &mut avg, r, g, b, a, num_pixels, pixels),
            _ => pack_mode24_to_one_color(p, &mut avg, r, g, b, num_pixels, pixels),
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
