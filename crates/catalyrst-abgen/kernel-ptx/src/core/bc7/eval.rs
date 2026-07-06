use super::*;

pub(super) fn evaluate_solution(
    low: &ColorI,
    high: &ColorI,
    pbits: &[u32; 2],
    p: &CCParams,
    res: &mut CCResults,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut qmin = *low;
    let mut qmax = *high;
    if p.has_pbits {
        let (min_pbit, max_pbit) = if p.endpoints_share_pbit {
            (pbits[0], pbits[0])
        } else {
            (pbits[0], pbits[1])
        };
        for i in 0..4 {
            qmin.c[i] = (low.c[i] << 1) | min_pbit as i32;
            qmax.c[i] = (high.c[i] << 1) | max_pbit as i32;
        }
    }
    let amin = scale_color(&qmin, p);
    let amax = scale_color(&qmax, p);
    let n = p.num_selector_weights as usize;
    let nc = if p.has_alpha { 4 } else { 3 };

    let mut total_errf = 0f32;
    let wr = p.weights[0] as f32;
    let wg = p.weights[1] as f32;
    let wb = p.weights[2] as f32;
    let wa = p.weights[3] as f32;

    let mut wc = [[0f32; 4]; 16];
    for j in 0..4 {
        wc[0][j] = amin.c[j] as f32;
        wc[n - 1][j] = amax.c[j] as f32;
    }

    for i in 1..(n - 1) {
        for j in 0..nc {
            wc[i][j] = ((wc[0][j] * (64.0 - p.psel_weights[i] as f32)
                + wc[n - 1][j] * p.psel_weights[i] as f32
                + 32.0)
                * (1.0 / 64.0))
                .floor();
        }
    }

    if !p.perceptual {
        if !p.has_alpha {
            if n == 16 {
                let lr = amin.c[0] as f32;
                let lg = amin.c[1] as f32;
                let lb = amin.c[2] as f32;
                let dr = amax.c[0] as f32 - lr;
                let dg = amax.c[1] as f32 - lg;
                let db = amax.c[2] as f32 - lb;
                let f = n as f32 / (dr * dr + dg * dg + db * db);
                let lr = lr * -dr;
                let lg = lg * -dg;
                let lb = lb * -db;
                total_errf = eval_solution_n16_rgb_scalar(
                    num_pixels,
                    pixels,
                    &wc,
                    wr,
                    wg,
                    wb,
                    dr,
                    dg,
                    db,
                    lr,
                    lg,
                    lb,
                    f,
                    n,
                    &mut res.selectors_temp,
                );
            } else {
                for i in 0..num_pixels {
                    let pr = pixels[i].c[0] as f32;
                    let pg = pixels[i].c[1] as f32;
                    let pb = pixels[i].c[2] as f32;

                    let mut errs = [0f32; 4];
                    for k in 0..4usize {
                        let d0 = wc[k][0] - pr;
                        let d1 = wc[k][1] - pg;
                        let d2 = wc[k][2] - pb;
                        errs[k] = wr * d0 * d0 + wg * d1 * d1 + wb * d2 * d2;
                    }
                    let mut best_err = errs[0].min(errs[1]).min(errs[2]).min(errs[3]);
                    let mut best_sel = if best_err == errs[1] { 1 } else { 0 };
                    if best_err == errs[2] {
                        best_sel = 2;
                    }
                    if best_err == errs[3] {
                        best_sel = 3;
                    }
                    if n == 8 {
                        let mut e2 = [0f32; 4];
                        for k in 0..4usize {
                            let d0 = wc[4 + k][0] - pr;
                            let d1 = wc[4 + k][1] - pg;
                            let d2 = wc[4 + k][2] - pb;
                            e2[k] = wr * d0 * d0 + wg * d1 * d1 + wb * d2 * d2;
                        }
                        best_err = best_err.min(e2[0].min(e2[1]).min(e2[2]).min(e2[3]));
                        if best_err == e2[0] {
                            best_sel = 4;
                        }
                        if best_err == e2[1] {
                            best_sel = 5;
                        }
                        if best_err == e2[2] {
                            best_sel = 6;
                        }
                        if best_err == e2[3] {
                            best_sel = 7;
                        }
                    }
                    total_errf += best_err;
                    res.selectors_temp[i] = best_sel;
                }
            }
        } else {
            if n == 16 {
                let lr = amin.c[0] as f32;
                let lg = amin.c[1] as f32;
                let lb = amin.c[2] as f32;
                let la = amin.c[3] as f32;
                let dr = amax.c[0] as f32 - lr;
                let dg = amax.c[1] as f32 - lg;
                let db = amax.c[2] as f32 - lb;
                let da = amax.c[3] as f32 - la;
                let f = n as f32 / (dr * dr + dg * dg + db * db + da * da);
                let lr = lr * -dr;
                let lg = lg * -dg;
                let lb = lb * -db;
                let la = la * -da;
                for i in 0..num_pixels {
                    let r = pixels[i].c[0] as f32;
                    let g = pixels[i].c[1] as f32;
                    let b = pixels[i].c[2] as f32;
                    let a = pixels[i].c[3] as f32;
                    let mut best_sel =
                        ((((r * dr + lr) + (g * dg + lg) + (b * db + lb) + (a * da + la)) * f)
                            + 0.5)
                            .floor();
                    best_sel = best_sel.clamp(1.0, (n - 1) as f32);
                    let best_sel0 = best_sel - 1.0;
                    let i0 = best_sel0 as i32 as usize;
                    let i1 = best_sel as i32 as usize;
                    let dr0 = wc[i0][0] - r;
                    let dg0 = wc[i0][1] - g;
                    let db0 = wc[i0][2] - b;
                    let da0 = wc[i0][3] - a;
                    let err0 = wr * dr0 * dr0 + wg * dg0 * dg0 + wb * db0 * db0 + wa * da0 * da0;
                    let dr1 = wc[i1][0] - r;
                    let dg1 = wc[i1][1] - g;
                    let db1 = wc[i1][2] - b;
                    let da1 = wc[i1][3] - a;
                    let err1 = wr * dr1 * dr1 + wg * dg1 * dg1 + wb * db1 * db1 + wa * da1 * da1;
                    let min_err = err0.min(err1);
                    total_errf += min_err;
                    res.selectors_temp[i] =
                        if min_err == err0 { best_sel0 } else { best_sel } as i32;
                }
            } else {
                for i in 0..num_pixels {
                    let pr = pixels[i].c[0] as f32;
                    let pg = pixels[i].c[1] as f32;
                    let pb = pixels[i].c[2] as f32;
                    let pa = pixels[i].c[3] as f32;
                    let mut errs = [0f32; 4];
                    for k in 0..4usize {
                        let d0 = wc[k][0] - pr;
                        let d1 = wc[k][1] - pg;
                        let d2 = wc[k][2] - pb;
                        let d3 = wc[k][3] - pa;
                        errs[k] = wr * d0 * d0 + wg * d1 * d1 + wb * d2 * d2 + wa * d3 * d3;
                    }
                    let mut best_err = errs[0].min(errs[1]).min(errs[2]).min(errs[3]);
                    let mut best_sel = if best_err == errs[1] { 1 } else { 0 };
                    if best_err == errs[2] {
                        best_sel = 2;
                    }
                    if best_err == errs[3] {
                        best_sel = 3;
                    }
                    if n == 8 {
                        let mut e2 = [0f32; 4];
                        for k in 0..4usize {
                            let d0 = wc[4 + k][0] - pr;
                            let d1 = wc[4 + k][1] - pg;
                            let d2 = wc[4 + k][2] - pb;
                            let d3 = wc[4 + k][3] - pa;
                            e2[k] = wr * d0 * d0 + wg * d1 * d1 + wb * d2 * d2 + wa * d3 * d3;
                        }
                        best_err = best_err.min(e2[0].min(e2[1]).min(e2[2]).min(e2[3]));
                        if best_err == e2[0] {
                            best_sel = 4;
                        }
                        if best_err == e2[1] {
                            best_sel = 5;
                        }
                        if best_err == e2[2] {
                            best_sel = 6;
                        }
                        if best_err == e2[3] {
                            best_sel = 7;
                        }
                    }
                    total_errf += best_err;
                    res.selectors_temp[i] = best_sel;
                }
            }
        }
    } else {
        let wgp = wg * PR_WEIGHT;
        let wbp = wb * PB_WEIGHT;
        let mut wy = [0f32; 16];
        let mut wcr = [0f32; 16];
        let mut wcb = [0f32; 16];

        for i in 0..16 {
            let r = wc[i][0];
            let g = wc[i][1];
            let b = wc[i][2];
            let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
            wy[i] = y;
            wcr[i] = r - y;
            wcb[i] = b - y;
        }
        if p.has_alpha {
            for i in 0..num_pixels {
                let r = pixels[i].c[0] as f32;
                let g = pixels[i].c[1] as f32;
                let b = pixels[i].c[2] as f32;
                let a = pixels[i].c[3] as f32;
                let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
                let cr = r - y;
                let cb = b - y;
                let mut best_err = 1e10f32;
                let mut best_sel = 0i32;
                for j in 0..n {
                    let dl = y - wy[j];
                    let dcr = cr - wcr[j];
                    let dcb = cb - wcb[j];
                    let da = a - wc[j][3];
                    let err = wr * dl * dl + wgp * dcr * dcr + wbp * dcb * dcb + wa * da * da;
                    if err < best_err {
                        best_err = err;
                        best_sel = j as i32;
                    }
                }
                total_errf += best_err;
                res.selectors_temp[i] = best_sel;
            }
        } else {
            for i in 0..num_pixels {
                let r = pixels[i].c[0] as f32;
                let g = pixels[i].c[1] as f32;
                let b = pixels[i].c[2] as f32;
                let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
                let cr = r - y;
                let cb = b - y;
                let mut best_err = 1e10f32;
                let mut best_sel = 0i32;
                for j in 0..n {
                    let dl = y - wy[j];
                    let dcr = cr - wcr[j];
                    let dcb = cb - wcb[j];
                    let err = wr * dl * dl + wgp * dcr * dcr + wbp * dcb * dcb;
                    if err < best_err {
                        best_err = err;
                        best_sel = j as i32;
                    }
                }
                total_errf += best_err;
                res.selectors_temp[i] = best_sel;
            }
        }
    }

    let total_err = total_errf as i64 as u64;
    if total_err < res.best_overall_err {
        res.best_overall_err = total_err;
        res.low = *low;
        res.high = *high;
        res.pbits = *pbits;
        for i in 0..num_pixels {
            res.selectors[i] = res.selectors_temp[i];
        }
    }
    total_err
}

#[inline]
pub(super) fn eval_solution_n16_rgb_scalar(
    num_pixels: usize,
    pixels: &[ColorI],
    wc: &[[f32; 4]; 16],
    wr: f32,
    wg: f32,
    wb: f32,
    dr: f32,
    dg: f32,
    db: f32,
    lr: f32,
    lg: f32,
    lb: f32,
    f: f32,
    n: usize,
    selectors_temp: &mut [i32; 16],
) -> f32 {
    let mut total_errf = 0f32;
    for i in 0..num_pixels {
        let r = pixels[i].c[0] as f32;
        let g = pixels[i].c[1] as f32;
        let b = pixels[i].c[2] as f32;
        let mut best_sel = ((((r * dr + lr) + (g * dg + lg) + (b * db + lb)) * f) + 0.5).floor();
        best_sel = best_sel.clamp(1.0, (n - 1) as f32);
        let best_sel0 = best_sel - 1.0;
        let i0 = best_sel0 as i32 as usize;
        let i1 = best_sel as i32 as usize;
        let dr0 = wc[i0][0] - r;
        let dg0 = wc[i0][1] - g;
        let db0 = wc[i0][2] - b;
        let err0 = wr * dr0 * dr0 + wg * dg0 * dg0 + wb * db0 * db0;
        let dr1 = wc[i1][0] - r;
        let dg1 = wc[i1][1] - g;
        let db1 = wc[i1][2] - b;
        let err1 = wr * dr1 * dr1 + wg * dg1 * dg1 + wb * db1 * db1;
        let min_err = err0.min(err1);
        total_errf += min_err;
        selectors_temp[i] = if min_err == err0 { best_sel0 } else { best_sel } as i32;
    }
    total_errf
}

pub(super) fn eval_4way_pbit_with_tiebreak(
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

pub(super) fn fix_degenerate_endpoints(
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

pub(super) fn find_optimal_solution(
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
pub(super) fn sq(s: f32) -> f32 {
    s * s
}
