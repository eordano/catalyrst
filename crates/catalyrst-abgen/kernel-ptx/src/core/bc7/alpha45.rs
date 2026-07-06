use super::*;

pub(super) fn handle_alpha_block_mode4(
    pixels: &[ColorI; 16],
    cp: &Params,
    params: &mut CCParams,
    lo_a: i32,
    hi_a: i32,
    opt4: &mut OptResults,
    mode4_err: &mut u64,
    t: &OptTables,
) {
    params.has_alpha = false;
    params.comp_bits = 5;
    params.has_pbits = false;
    params.endpoints_share_pbit = false;
    params.perceptual = cp.perceptual;

    for index_selector in 0..2usize {
        if cp.mode4_index_mask & (1 << index_selector) == 0 {
            continue;
        }
        if index_selector != 0 {
            params.psel_weights = &G_WEIGHTS3;
            params.psel_weightsx = &G_WEIGHTS3X;
            params.num_selector_weights = 8;
        } else {
            params.psel_weights = &G_WEIGHTS2;
            params.psel_weightsx = &G_WEIGHTS2X;
            params.num_selector_weights = 4;
        }
        let mut results = CCResults::new();
        let trial_err_color =
            color_cell_compression(4, params, &mut results, cp, 16, pixels, true, t);

        let mut la = ((lo_a + 2) >> 2).min(63);
        let mut ha = ((hi_a + 2) >> 2).min(63);
        if la == ha && lo_a != hi_a {
            if ha != 63 {
                ha += 1;
            } else if la != 0 {
                la -= 1;
            }
        }

        let mut best_alpha_err = u64::MAX;
        let mut best_la = 0i32;
        let mut best_ha = 0i32;
        let mut best_alpha_selectors = [0i32; 16];

        for pass in 0..2 {
            let mut vals = [0i32; 8];
            if index_selector == 0 {
                vals[0] = (la << 2) | (la >> 4);
                vals[7] = (ha << 2) | (ha >> 4);
                for i in 1..7 {
                    vals[i] = (vals[0] * (64 - G_WEIGHTS3[i] as i32)
                        + vals[7] * G_WEIGHTS3[i] as i32
                        + 32)
                        >> 6;
                }
            } else {
                vals[0] = (la << 2) | (la >> 4);
                vals[3] = (ha << 2) | (ha >> 4);
                let (w1, w2) = (21, 43);
                vals[1] = (vals[0] * (64 - w1) + vals[3] * w1 + 32) >> 6;
                vals[2] = (vals[0] * (64 - w2) + vals[3] * w2 + 32) >> 6;
            }
            let mut trial_alpha_err = 0u64;
            let mut trial_alpha_selectors = [0i32; 16];
            for i in 0..16 {
                let a = pixels[i].c[3];
                let mut s = 0i32;
                let mut be = iabs32(a - vals[0]);
                let mut e = iabs32(a - vals[1]);
                if e < be {
                    be = e;
                    s = 1;
                }
                e = iabs32(a - vals[2]);
                if e < be {
                    be = e;
                    s = 2;
                }
                e = iabs32(a - vals[3]);
                if e < be {
                    be = e;
                    s = 3;
                }
                if index_selector == 0 {
                    e = iabs32(a - vals[4]);
                    if e < be {
                        be = e;
                        s = 4;
                    }
                    e = iabs32(a - vals[5]);
                    if e < be {
                        be = e;
                        s = 5;
                    }
                    e = iabs32(a - vals[6]);
                    if e < be {
                        be = e;
                        s = 6;
                    }
                    e = iabs32(a - vals[7]);
                    if e < be {
                        be = e;
                        s = 7;
                    }
                }
                trial_alpha_err += (be * be) as u64 * params.weights[3] as u64;
                trial_alpha_selectors[i] = s;
            }
            if trial_alpha_err < best_alpha_err {
                best_alpha_err = trial_alpha_err;
                best_la = la;
                best_ha = ha;
                best_alpha_selectors = trial_alpha_selectors;
            }
            if pass == 0 {
                let mut xl = 0f32;
                let mut xh = 0f32;
                let sw = if index_selector != 0 {
                    &G_WEIGHTS2X[..]
                } else {
                    &G_WEIGHTS3X[..]
                };
                compute_lsq_endpoints_a(16, &trial_alpha_selectors, sw, &mut xl, &mut xh, pixels);
                if xl > xh {
                    core::mem::swap(&mut xl, &mut xh);
                }
                la = itrunc((xl * (63.0 / 255.0) + 0.5).floor()).clamp(0, 63);
                ha = itrunc((xh * (63.0 / 255.0) + 0.5).floor()).clamp(0, 63);
            }
        }

        if cp.uber_level > 0 {
            let d = (cp.uber_level as i32).min(3);
            for ld in -d..=d {
                for hd in -d..=d {
                    la = (best_la + ld).clamp(0, 63);
                    ha = (best_ha + hd).clamp(0, 63);
                    let mut vals = [0i32; 8];
                    if index_selector == 0 {
                        vals[0] = (la << 2) | (la >> 4);
                        vals[7] = (ha << 2) | (ha >> 4);
                        for i in 1..7 {
                            vals[i] = (vals[0] * (64 - G_WEIGHTS3[i] as i32)
                                + vals[7] * G_WEIGHTS3[i] as i32
                                + 32)
                                >> 6;
                        }
                    } else {
                        vals[0] = (la << 2) | (la >> 4);
                        vals[3] = (ha << 2) | (ha >> 4);
                        let (w1, w2) = (21, 43);
                        vals[1] = (vals[0] * (64 - w1) + vals[3] * w1 + 32) >> 6;
                        vals[2] = (vals[0] * (64 - w2) + vals[3] * w2 + 32) >> 6;
                    }
                    let mut trial_alpha_err = 0u64;
                    let mut trial_alpha_selectors = [0i32; 16];
                    for i in 0..16 {
                        let a = pixels[i].c[3];
                        let mut s = 0i32;
                        let mut be = iabs32(a - vals[0]);
                        let mut e = iabs32(a - vals[1]);
                        if e < be {
                            be = e;
                            s = 1;
                        }
                        e = iabs32(a - vals[2]);
                        if e < be {
                            be = e;
                            s = 2;
                        }
                        e = iabs32(a - vals[3]);
                        if e < be {
                            be = e;
                            s = 3;
                        }
                        if index_selector == 0 {
                            e = iabs32(a - vals[4]);
                            if e < be {
                                be = e;
                                s = 4;
                            }
                            e = iabs32(a - vals[5]);
                            if e < be {
                                be = e;
                                s = 5;
                            }
                            e = iabs32(a - vals[6]);
                            if e < be {
                                be = e;
                                s = 6;
                            }
                            e = iabs32(a - vals[7]);
                            if e < be {
                                be = e;
                                s = 7;
                            }
                        }
                        trial_alpha_err += (be * be) as u64 * params.weights[3] as u64;
                        trial_alpha_selectors[i] = s;
                    }
                    if trial_alpha_err < best_alpha_err {
                        best_alpha_err = trial_alpha_err;
                        best_la = la;
                        best_ha = ha;
                        best_alpha_selectors = trial_alpha_selectors;
                    }
                }
            }
        }

        let trial_err = trial_err_color + best_alpha_err;
        if trial_err < *mode4_err {
            *mode4_err = trial_err;
            opt4.mode = 4;
            opt4.index_selector = index_selector as u32;
            opt4.rotation = 0;
            opt4.partition = 0;
            opt4.low[0] = results.low;
            opt4.high[0] = results.high;
            opt4.low[0].c[3] = best_la;
            opt4.high[0].c[3] = best_ha;
            opt4.selectors = results.selectors;
            opt4.alpha_selectors = best_alpha_selectors;
        }
    }
}

pub(super) fn handle_alpha_block_mode5(
    pixels: &[ColorI; 16],
    cp: &Params,
    params: &mut CCParams,
    mut lo_a: i32,
    mut hi_a: i32,
    opt5: &mut OptResults,
    mode5_err: &mut u64,
    t: &OptTables,
) {
    params.psel_weights = &G_WEIGHTS2;
    params.psel_weightsx = &G_WEIGHTS2X;
    params.num_selector_weights = 4;
    params.comp_bits = 7;
    params.has_alpha = false;
    params.has_pbits = false;
    params.endpoints_share_pbit = false;
    params.perceptual = cp.perceptual;

    let mut results5 = CCResults::new();
    *mode5_err = color_cell_compression(5, params, &mut results5, cp, 16, pixels, true, t);
    opt5.low[0] = results5.low;
    opt5.high[0] = results5.high;
    opt5.selectors = results5.selectors;

    if lo_a == hi_a {
        opt5.low[0].c[3] = lo_a;
        opt5.high[0].c[3] = hi_a;
        opt5.alpha_selectors = [0; 16];
    } else {
        let mut mode5_alpha_err = u64::MAX;
        for pass in 0..2 {
            let mut vals = [0i32; 4];
            vals[0] = lo_a;
            vals[3] = hi_a;
            let (w1, w2) = (21, 43);
            vals[1] = (vals[0] * (64 - w1) + vals[3] * w1 + 32) >> 6;
            vals[2] = (vals[0] * (64 - w2) + vals[3] * w2 + 32) >> 6;
            let mut trial_alpha_selectors = [0i32; 16];
            let mut trial_alpha_err = 0u64;
            for i in 0..16 {
                let a = pixels[i].c[3];
                let mut s = 0i32;
                let mut be = iabs32(a - vals[0]);
                let mut e = iabs32(a - vals[1]);
                if e < be {
                    be = e;
                    s = 1;
                }
                e = iabs32(a - vals[2]);
                if e < be {
                    be = e;
                    s = 2;
                }
                e = iabs32(a - vals[3]);
                if e < be {
                    be = e;
                    s = 3;
                }
                trial_alpha_selectors[i] = s;
                trial_alpha_err += (be * be) as u64 * params.weights[3] as u64;
            }
            if trial_alpha_err < mode5_alpha_err {
                mode5_alpha_err = trial_alpha_err;
                opt5.low[0].c[3] = lo_a;
                opt5.high[0].c[3] = hi_a;
                opt5.alpha_selectors = trial_alpha_selectors;
            }
            if pass == 0 {
                let mut xl = 0f32;
                let mut xh = 0f32;
                compute_lsq_endpoints_a(
                    16,
                    &trial_alpha_selectors,
                    &G_WEIGHTS2X,
                    &mut xl,
                    &mut xh,
                    pixels,
                );
                let mut new_lo = itrunc((xl + 0.5).floor()).clamp(0, 255);
                let mut new_hi = itrunc((xh + 0.5).floor()).clamp(0, 255);
                if new_lo > new_hi {
                    core::mem::swap(&mut new_lo, &mut new_hi);
                }
                if new_lo == lo_a && new_hi == hi_a {
                    break;
                }
                lo_a = new_lo;
                hi_a = new_hi;
            }
        }
        if cp.uber_level > 0 {
            let d = (cp.uber_level as i32).min(3);
            for ld in -d..=d {
                for hd in -d..=d {
                    lo_a = (opt5.low[0].c[3] + ld).clamp(0, 255);
                    hi_a = (opt5.high[0].c[3] + hd).clamp(0, 255);
                    let mut vals = [0i32; 4];
                    vals[0] = lo_a;
                    vals[3] = hi_a;
                    let (w1, w2) = (21, 43);
                    vals[1] = (vals[0] * (64 - w1) + vals[3] * w1 + 32) >> 6;
                    vals[2] = (vals[0] * (64 - w2) + vals[3] * w2 + 32) >> 6;
                    let mut trial_alpha_selectors = [0i32; 16];
                    let mut trial_alpha_err = 0u64;
                    for i in 0..16 {
                        let a = pixels[i].c[3];
                        let mut s = 0i32;
                        let mut be = iabs32(a - vals[0]);
                        let mut e = iabs32(a - vals[1]);
                        if e < be {
                            be = e;
                            s = 1;
                        }
                        e = iabs32(a - vals[2]);
                        if e < be {
                            be = e;
                            s = 2;
                        }
                        e = iabs32(a - vals[3]);
                        if e < be {
                            be = e;
                            s = 3;
                        }
                        trial_alpha_selectors[i] = s;
                        trial_alpha_err += (be * be) as u64 * params.weights[3] as u64;
                    }
                    if trial_alpha_err < mode5_alpha_err {
                        mode5_alpha_err = trial_alpha_err;
                        opt5.low[0].c[3] = lo_a;
                        opt5.high[0].c[3] = hi_a;
                        opt5.alpha_selectors = trial_alpha_selectors;
                    }
                }
            }
        }
        *mode5_err += mode5_alpha_err;
    }
    opt5.mode = 5;
    opt5.index_selector = 0;
    opt5.rotation = 0;
    opt5.partition = 0;
}
