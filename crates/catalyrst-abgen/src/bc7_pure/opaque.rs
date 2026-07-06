use super::*;

pub(super) fn handle_opaque_block(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
    plan: &PartitionPlan,
) -> [u8; 16] {
    let mut opt_results = OptResults::new();
    let mut best_err = u64::MAX;

    if cp.use_mode[6] {
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS4;
        params.psel_weightsx = &G_WEIGHTS4X;
        params.num_selector_weights = 16;
        params.comp_bits = 7;
        params.has_pbits = true;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;
        let mut results6 = CCResults::new();
        best_err = color_cell_compression(6, &params, &mut results6, cp, 16, pixels, true);
        opt_results.mode = 6;
        opt_results.index_selector = 0;
        opt_results.rotation = 0;
        opt_results.partition = 0;
        opt_results.low[0] = results6.low;
        opt_results.high[0] = results6.high;
        opt_results.pbits[0] = results6.pbits;
        opt_results.selectors = results6.selectors;
    }

    let mut solutions2: Vec<Solution> = Vec::new();
    if cp.use_mode[1] || cp.use_mode[3] {
        if plan.use_list13 {
            solutions2 = plan.list13.clone();
        } else {
            solutions2.push(Solution {
                index: plan.part13,
                err: 0,
            });
        }
    }
    let num_solutions2 = solutions2.len();

    if cp.use_mode[1] {
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS3;
        params.psel_weightsx = &G_WEIGHTS3X;
        params.num_selector_weights = 8;
        params.comp_bits = 6;
        params.has_pbits = true;
        params.endpoints_share_pbit = true;
        params.perceptual = cp.perceptual;

        let run = |trial_partition: u32,
                   best_err: &mut u64,
                   opt: &mut OptResults,
                   refine_force: bool| {
            let part = &G_PARTITION2[(trial_partition as usize) * 16..];
            let mut sc = [[ColorI::default(); 16]; 2];
            let mut st = [0usize; 2];
            let mut spi = [[0usize; 16]; 2];
            let mut ssel = [[0i32; 16]; 2];
            let mut slow = [ColorI::default(); 2];
            let mut shigh = [ColorI::default(); 2];
            let mut spb = [[0u32; 2]; 2];
            for idx in 0..16 {
                let pp = part[idx] as usize;
                sc[pp][st[pp]] = pixels[idx];
                spi[pp][st[pp]] = idx;
                st[pp] += 1;
            }
            let mut trial_err = 0u64;
            let mut ok = true;
            for subset in 0..2 {
                let mut r = CCResults::new();
                let refine = (num_solutions2 <= 2) || refine_force;
                let err =
                    color_cell_compression(1, &params, &mut r, cp, st[subset], &sc[subset], refine);
                ssel[subset] = r.selectors;
                slow[subset] = r.low;
                shigh[subset] = r.high;
                spb[subset] = r.pbits;
                trial_err += err;
                if trial_err > *best_err {
                    ok = false;
                    break;
                }
            }
            if ok && trial_err < *best_err {
                *best_err = trial_err;
                opt.mode = 1;
                opt.index_selector = 0;
                opt.rotation = 0;
                opt.partition = trial_partition;
                for subset in 0..2 {
                    for i in 0..st[subset] {
                        opt.selectors[spi[subset][i]] = ssel[subset][i];
                    }
                    opt.low[subset] = slow[subset];
                    opt.high[subset] = shigh[subset];
                    opt.pbits[subset][0] = spb[subset][0];
                }
                return true;
            }
            false
        };
        for si in 0..num_solutions2 {
            run(solutions2[si].index, &mut best_err, &mut opt_results, false);
        }
        if num_solutions2 > 2 && opt_results.mode == 1 {
            let tp = opt_results.partition;
            run(tp, &mut best_err, &mut opt_results, true);
        }
    }

    if cp.use_mode[0] {
        let solutions3: Vec<Solution> = if plan.use_list0 {
            plan.list0.clone()
        } else {
            vec![Solution {
                index: plan.part0,
                err: 0,
            }]
        };
        let num_solutions3 = solutions3.len();
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS3;
        params.psel_weightsx = &G_WEIGHTS3X;
        params.num_selector_weights = 8;
        params.comp_bits = 4;
        params.has_pbits = true;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;

        for si in 0..num_solutions3 {
            let best_partition0 = solutions3[si].index;
            let part = &G_PARTITION3[(best_partition0 as usize) * 16..];
            let mut sc = [[ColorI::default(); 16]; 3];
            let mut st = [0usize; 3];
            let mut spi = [[0usize; 16]; 3];
            for idx in 0..16 {
                let pp = part[idx] as usize;
                sc[pp][st[pp]] = pixels[idx];
                spi[pp][st[pp]] = idx;
                st[pp] += 1;
            }
            let mut ssel = [[0i32; 16]; 3];
            let mut slow = [ColorI::default(); 3];
            let mut shigh = [ColorI::default(); 3];
            let mut spb = [[0u32; 2]; 3];
            let mut mode0_err = 0u64;
            let mut ok = true;
            for subset in 0..3 {
                let mut r = CCResults::new();
                let err =
                    color_cell_compression(0, &params, &mut r, cp, st[subset], &sc[subset], true);
                ssel[subset] = r.selectors;
                slow[subset] = r.low;
                shigh[subset] = r.high;
                spb[subset] = r.pbits;
                mode0_err += err;
                if mode0_err > best_err {
                    ok = false;
                    break;
                }
            }
            if ok && mode0_err < best_err {
                best_err = mode0_err;
                opt_results.mode = 0;
                opt_results.index_selector = 0;
                opt_results.rotation = 0;
                opt_results.partition = best_partition0;
                for subset in 0..3 {
                    for i in 0..st[subset] {
                        opt_results.selectors[spi[subset][i]] = ssel[subset][i];
                    }
                    opt_results.low[subset] = slow[subset];
                    opt_results.high[subset] = shigh[subset];
                    opt_results.pbits[subset] = spb[subset];
                }
            }
        }
    }

    if cp.use_mode[3] {
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS2;
        params.psel_weightsx = &G_WEIGHTS2X;
        params.num_selector_weights = 4;
        params.comp_bits = 7;
        params.has_pbits = true;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;

        let run = |trial_partition: u32,
                   best_err: &mut u64,
                   opt: &mut OptResults,
                   refine_force: bool| {
            let part = &G_PARTITION2[(trial_partition as usize) * 16..];
            let mut sc = [[ColorI::default(); 16]; 2];
            let mut st = [0usize; 2];
            let mut spi = [[0usize; 16]; 2];
            let mut ssel = [[0i32; 16]; 2];
            let mut slow = [ColorI::default(); 2];
            let mut shigh = [ColorI::default(); 2];
            let mut spb = [[0u32; 2]; 2];
            for idx in 0..16 {
                let pp = part[idx] as usize;
                sc[pp][st[pp]] = pixels[idx];
                spi[pp][st[pp]] = idx;
                st[pp] += 1;
            }
            let mut trial_err = 0u64;
            let mut ok = true;
            for subset in 0..2 {
                let mut r = CCResults::new();
                let refine = (num_solutions2 <= 2) || refine_force;
                let err =
                    color_cell_compression(3, &params, &mut r, cp, st[subset], &sc[subset], refine);
                ssel[subset] = r.selectors;
                slow[subset] = r.low;
                shigh[subset] = r.high;
                spb[subset] = r.pbits;
                trial_err += err;
                if trial_err > *best_err {
                    ok = false;
                    break;
                }
            }
            if ok && trial_err < *best_err {
                *best_err = trial_err;
                opt.mode = 3;
                opt.index_selector = 0;
                opt.rotation = 0;
                opt.partition = trial_partition;
                for subset in 0..2 {
                    for i in 0..st[subset] {
                        opt.selectors[spi[subset][i]] = ssel[subset][i];
                    }
                    opt.low[subset] = slow[subset];
                    opt.high[subset] = shigh[subset];
                    opt.pbits[subset] = spb[subset];
                }
                return true;
            }
            false
        };
        for si in 0..num_solutions2 {
            run(solutions2[si].index, &mut best_err, &mut opt_results, false);
        }
        if num_solutions2 > 2 && opt_results.mode == 3 {
            let tp = opt_results.partition;
            run(tp, &mut best_err, &mut opt_results, true);
        }
    }

    if !cp.perceptual && cp.use_mode[5] {
        for rotation in 0..4usize {
            if cp.mode5_rotation_mask & (1 << rotation) == 0 {
                continue;
            }
            let mut params5 = base.clone();
            if rotation != 0 {
                params5.weights.swap(rotation - 1, 3);
            }
            let mut rot_pixels = *pixels;
            let mut tlo = 255i32;
            let mut thi = 255i32;
            if rotation != 0 {
                tlo = 255;
                thi = 0;
                for i in 0..16 {
                    rot_pixels[i].c.swap(3, rotation - 1);
                    tlo = tlo.min(rot_pixels[i].c[3]);
                    thi = thi.max(rot_pixels[i].c[3]);
                }
            }
            let mut trial5 = OptResults::new();
            let mut trial_err = 0u64;
            handle_alpha_block_mode5(
                &rot_pixels,
                cp,
                &mut params5,
                tlo,
                thi,
                &mut trial5,
                &mut trial_err,
            );
            if trial_err < best_err {
                best_err = trial_err;
                opt_results = trial5;
                opt_results.rotation = rotation as u32;
            }
        }
    }

    if cp.use_mode[2] {
        let solutions3: Vec<Solution> = if plan.use_list2 {
            plan.list2.clone()
        } else {
            vec![Solution {
                index: plan.part2,
                err: 0,
            }]
        };
        let num_solutions3 = solutions3.len();
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS2;
        params.psel_weightsx = &G_WEIGHTS2X;
        params.num_selector_weights = 4;
        params.comp_bits = 5;
        params.has_pbits = false;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;

        for si in 0..num_solutions3 {
            let best_partition2 = solutions3[si].index;
            let part = &G_PARTITION3[(best_partition2 as usize) * 16..];
            let mut sc = [[ColorI::default(); 16]; 3];
            let mut st = [0usize; 3];
            let mut spi = [[0usize; 16]; 3];
            for idx in 0..16 {
                let pp = part[idx] as usize;
                sc[pp][st[pp]] = pixels[idx];
                spi[pp][st[pp]] = idx;
                st[pp] += 1;
            }
            let mut ssel = [[0i32; 16]; 3];
            let mut slow = [ColorI::default(); 3];
            let mut shigh = [ColorI::default(); 3];
            let mut mode2_err = 0u64;
            let mut ok = true;
            for subset in 0..3 {
                let mut r = CCResults::new();
                let err =
                    color_cell_compression(2, &params, &mut r, cp, st[subset], &sc[subset], true);
                ssel[subset] = r.selectors;
                slow[subset] = r.low;
                shigh[subset] = r.high;
                mode2_err += err;
                if mode2_err > best_err {
                    ok = false;
                    break;
                }
            }
            if ok && mode2_err < best_err {
                best_err = mode2_err;
                opt_results.mode = 2;
                opt_results.index_selector = 0;
                opt_results.rotation = 0;
                opt_results.partition = best_partition2;
                for subset in 0..3 {
                    for i in 0..st[subset] {
                        opt_results.selectors[spi[subset][i]] = ssel[subset][i];
                    }
                    opt_results.low[subset] = slow[subset];
                    opt_results.high[subset] = shigh[subset];
                }
            }
        }
    }

    if !cp.perceptual && cp.use_mode[4] {
        for rotation in 0..4usize {
            if cp.mode4_rotation_mask & (1 << rotation) == 0 {
                continue;
            }
            let mut params4 = base.clone();
            if rotation != 0 {
                params4.weights.swap(rotation - 1, 3);
            }
            let mut rot_pixels = *pixels;
            let mut tlo = 255i32;
            let mut thi = 255i32;
            if rotation != 0 {
                tlo = 255;
                thi = 0;
                for i in 0..16 {
                    rot_pixels[i].c.swap(3, rotation - 1);
                    tlo = tlo.min(rot_pixels[i].c[3]);
                    thi = thi.max(rot_pixels[i].c[3]);
                }
            }
            let mut trial4 = OptResults::new();
            let mut trial_err = best_err;
            handle_alpha_block_mode4(
                &rot_pixels,
                cp,
                &mut params4,
                tlo,
                thi,
                &mut trial4,
                &mut trial_err,
            );
            if trial_err < best_err {
                best_err = trial_err;
                opt_results.mode = 4;
                opt_results.index_selector = trial4.index_selector;
                opt_results.rotation = rotation as u32;
                opt_results.partition = 0;
                opt_results.low[0] = trial4.low[0];
                opt_results.high[0] = trial4.high[0];
                opt_results.selectors = trial4.selectors;
                opt_results.alpha_selectors = trial4.alpha_selectors;
            }
        }
    }

    encode_bc7_block_bits(&opt_results)
}

pub(super) fn handle_block_solid(cr: usize, cg: usize, cb: usize, ca: i32) -> [u8; 16] {
    let t = opt();
    let er = t.mode5[cr];
    let eg = t.mode5[cg];
    let eb = t.mode5[cb];
    let mut opt_r = OptResults::new();
    opt_r.mode = 5;
    opt_r.low[0] = ColorI {
        c: [
            (er & 0xFF) as i32,
            (eg & 0xFF) as i32,
            (eb & 0xFF) as i32,
            ca,
        ],
    };
    opt_r.high[0] = ColorI {
        c: [(er >> 8) as i32, (eg >> 8) as i32, (eb >> 8) as i32, ca],
    };
    opt_r.index_selector = 0;
    opt_r.rotation = 0;
    opt_r.partition = 0;
    for i in 0..16 {
        opt_r.selectors[i] = MODE5_IDX as i32;
        opt_r.alpha_selectors[i] = 0;
    }
    encode_bc7_block_bits(&opt_r)
}

pub(super) fn handle_opaque_block_mode6(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
) -> [u8; 16] {
    let mut opt_results = OptResults::new();
    let mut params = base.clone();
    params.psel_weights = &G_WEIGHTS4;
    params.psel_weightsx = &G_WEIGHTS4X;
    params.num_selector_weights = 16;
    params.comp_bits = 7;
    params.has_pbits = true;
    params.endpoints_share_pbit = false;
    params.perceptual = cp.perceptual;
    let mut results6 = CCResults::new();
    color_cell_compression(6, &params, &mut results6, cp, 16, pixels, true);
    opt_results.mode = 6;
    opt_results.index_selector = 0;
    opt_results.rotation = 0;
    opt_results.partition = 0;
    opt_results.low[0] = results6.low;
    opt_results.high[0] = results6.high;
    opt_results.pbits[0] = results6.pbits;
    opt_results.selectors = results6.selectors;
    encode_bc7_block_mode6(&opt_results)
}
