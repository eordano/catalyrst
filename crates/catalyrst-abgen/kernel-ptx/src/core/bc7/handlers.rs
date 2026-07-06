use super::*;

fn handle_alpha_block(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
    lo_a: i32,
    hi_a: i32,
    plan: &PartitionPlan,
    t: &OptTables,
) -> [u8; 16] {
    let mut base = base.clone();
    base.perceptual = cp.perceptual;
    let base = &base;
    let mut opt_results = OptResults::new();
    let mut best_err = u64::MAX;

    if cp.use_mode4 {
        let num_rotations = if cp.perceptual || !cp.use_mode4_rotation {
            1
        } else {
            4
        };
        for rotation in 0..num_rotations {
            if cp.mode4_rotation_mask & (1 << rotation) == 0 {
                continue;
            }
            let mut params4 = base.clone();
            if rotation != 0 {
                params4.weights.swap(rotation - 1, 3);
            }
            let mut rot_pixels = *pixels;
            let mut tlo = lo_a;
            let mut thi = hi_a;
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
                t,
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

    if cp.use_mode6 {
        let mut params6 = base.clone();
        for c in 0..4 {
            params6.weights[c] *= cp.mode67_weight_mul[c];
        }
        params6.psel_weights = &G_WEIGHTS4;
        params6.psel_weightsx = &G_WEIGHTS4X;
        params6.num_selector_weights = 16;
        params6.comp_bits = 7;
        params6.has_pbits = true;
        params6.endpoints_share_pbit = false;
        params6.has_alpha = true;
        let mut results6 = CCResults::new();
        let mode6_err = color_cell_compression(6, &params6, &mut results6, cp, 16, pixels, true, t);
        if mode6_err < best_err {
            best_err = mode6_err;
            opt_results.mode = 6;
            opt_results.index_selector = 0;
            opt_results.rotation = 0;
            opt_results.partition = 0;
            opt_results.low[0] = results6.low;
            opt_results.high[0] = results6.high;
            opt_results.pbits[0] = results6.pbits;
            opt_results.selectors = results6.selectors;
        }
    }

    if cp.use_mode5 {
        let num_rotations = if cp.perceptual || !cp.use_mode5_rotation {
            1
        } else {
            4
        };
        for rotation in 0..num_rotations {
            if cp.mode5_rotation_mask & (1 << rotation) == 0 {
                continue;
            }
            let mut params5 = base.clone();
            if rotation != 0 {
                params5.weights.swap(rotation - 1, 3);
            }
            let mut rot_pixels = *pixels;
            let mut tlo = lo_a;
            let mut thi = hi_a;
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
                t,
            );
            if trial_err < best_err {
                best_err = trial_err;
                opt_results = trial5;
                opt_results.rotation = rotation as u32;
            }
        }
    }

    if cp.use_mode7 {
        let solutions = &plan.list7;
        let num_solutions = solutions.len;
        let mut params7 = base.clone();
        for c in 0..4 {
            params7.weights[c] *= cp.mode67_weight_mul[c];
        }
        params7.psel_weights = &G_WEIGHTS2;
        params7.psel_weightsx = &G_WEIGHTS2X;
        params7.num_selector_weights = 4;
        params7.comp_bits = 5;
        params7.has_pbits = true;
        params7.endpoints_share_pbit = false;
        params7.has_alpha = true;

        let run_partition =
            |trial_partition: u32, best_err: &mut u64, opt: &mut OptResults, refine_force: bool| {
                let part = &G_PARTITION2[(trial_partition as usize) * 16..];
                let mut subset_colors = [[ColorI::default(); 16]; 2];
                let mut subset_total = [0usize; 2];
                let mut subset_pixel_index = [[0usize; 16]; 2];
                let mut subset_selectors = [[0i32; 16]; 2];
                let mut subset_low = [ColorI::default(); 2];
                let mut subset_high = [ColorI::default(); 2];
                let mut subset_pbits = [[0u32; 2]; 2];
                for idx in 0..16 {
                    let pp = part[idx] as usize;
                    subset_colors[pp][subset_total[pp]] = pixels[idx];
                    subset_pixel_index[pp][subset_total[pp]] = idx;
                    subset_total[pp] += 1;
                }
                let mut trial_err = 0u64;
                let mut ok = true;
                for subset in 0..2 {
                    let mut results = CCResults::new();
                    let refine = (num_solutions <= 2) || refine_force;
                    let err = color_cell_compression(
                        7,
                        &params7,
                        &mut results,
                        cp,
                        subset_total[subset],
                        &subset_colors[subset],
                        refine,
                        t,
                    );
                    subset_selectors[subset] = results.selectors;
                    subset_low[subset] = results.low;
                    subset_high[subset] = results.high;
                    subset_pbits[subset] = results.pbits;
                    trial_err += err;
                    if trial_err > *best_err {
                        ok = false;
                        break;
                    }
                }
                if ok && trial_err < *best_err {
                    *best_err = trial_err;
                    opt.mode = 7;
                    opt.index_selector = 0;
                    opt.rotation = 0;
                    opt.partition = trial_partition;
                    for subset in 0..2 {
                        for i in 0..subset_total[subset] {
                            opt.selectors[subset_pixel_index[subset][i]] =
                                subset_selectors[subset][i];
                        }
                        opt.low[subset] = subset_low[subset];
                        opt.high[subset] = subset_high[subset];
                        opt.pbits[subset] = subset_pbits[subset];
                    }
                    return true;
                }
                false
            };

        for solution_index in 0..num_solutions {
            run_partition(
                solutions.sols[solution_index].index,
                &mut best_err,
                &mut opt_results,
                false,
            );
        }
        if num_solutions > 2 && opt_results.mode == 7 {
            let tp = opt_results.partition;
            run_partition(tp, &mut best_err, &mut opt_results, true);
        }
    }

    encode_bc7_block_bits(&opt_results)
}

fn handle_opaque_block(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
    plan: &PartitionPlan,
    t: &OptTables,
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
        best_err = color_cell_compression(6, &params, &mut results6, cp, 16, pixels, true, t);
        opt_results.mode = 6;
        opt_results.index_selector = 0;
        opt_results.rotation = 0;
        opt_results.partition = 0;
        opt_results.low[0] = results6.low;
        opt_results.high[0] = results6.high;
        opt_results.pbits[0] = results6.pbits;
        opt_results.selectors = results6.selectors;
    }

    let mut solutions2 = SolutionList::new();
    if cp.use_mode[1] || cp.use_mode[3] {
        if plan.use_list13 {
            solutions2 = plan.list13;
        } else {
            solutions2.sols[0] = Solution {
                index: plan.part13,
                err: 0,
            };
            solutions2.len = 1;
        }
    }
    let num_solutions2 = solutions2.len;

    if cp.use_mode[1] {
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS3;
        params.psel_weightsx = &G_WEIGHTS3X;
        params.num_selector_weights = 8;
        params.comp_bits = 6;
        params.has_pbits = true;
        params.endpoints_share_pbit = true;
        params.perceptual = cp.perceptual;

        let run =
            |trial_partition: u32, best_err: &mut u64, opt: &mut OptResults, refine_force: bool| {
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
                    let err = color_cell_compression(
                        1,
                        &params,
                        &mut r,
                        cp,
                        st[subset],
                        &sc[subset],
                        refine,
                        t,
                    );
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
            run(
                solutions2.sols[si].index,
                &mut best_err,
                &mut opt_results,
                false,
            );
        }
        if num_solutions2 > 2 && opt_results.mode == 1 {
            let tp = opt_results.partition;
            run(tp, &mut best_err, &mut opt_results, true);
        }
    }

    if cp.use_mode[0] {
        let mut solutions3 = SolutionList::new();
        if plan.use_list0 {
            solutions3 = plan.list0;
        } else {
            solutions3.sols[0] = Solution {
                index: plan.part0,
                err: 0,
            };
            solutions3.len = 1;
        }
        let num_solutions3 = solutions3.len;
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS3;
        params.psel_weightsx = &G_WEIGHTS3X;
        params.num_selector_weights = 8;
        params.comp_bits = 4;
        params.has_pbits = true;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;

        for si in 0..num_solutions3 {
            let best_partition0 = solutions3.sols[si].index;
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
                let err = color_cell_compression(
                    0,
                    &params,
                    &mut r,
                    cp,
                    st[subset],
                    &sc[subset],
                    true,
                    t,
                );
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

        let run =
            |trial_partition: u32, best_err: &mut u64, opt: &mut OptResults, refine_force: bool| {
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
                    let err = color_cell_compression(
                        3,
                        &params,
                        &mut r,
                        cp,
                        st[subset],
                        &sc[subset],
                        refine,
                        t,
                    );
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
            run(
                solutions2.sols[si].index,
                &mut best_err,
                &mut opt_results,
                false,
            );
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
                t,
            );
            if trial_err < best_err {
                best_err = trial_err;
                opt_results = trial5;
                opt_results.rotation = rotation as u32;
            }
        }
    }

    if cp.use_mode[2] {
        let mut solutions3 = SolutionList::new();
        if plan.use_list2 {
            solutions3 = plan.list2;
        } else {
            solutions3.sols[0] = Solution {
                index: plan.part2,
                err: 0,
            };
            solutions3.len = 1;
        }
        let num_solutions3 = solutions3.len;
        let mut params = base.clone();
        params.psel_weights = &G_WEIGHTS2;
        params.psel_weightsx = &G_WEIGHTS2X;
        params.num_selector_weights = 4;
        params.comp_bits = 5;
        params.has_pbits = false;
        params.endpoints_share_pbit = false;
        params.perceptual = cp.perceptual;

        for si in 0..num_solutions3 {
            let best_partition2 = solutions3.sols[si].index;
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
                let err = color_cell_compression(
                    2,
                    &params,
                    &mut r,
                    cp,
                    st[subset],
                    &sc[subset],
                    true,
                    t,
                );
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
                t,
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

pub(super) fn handle_block_solid(
    cr: usize,
    cg: usize,
    cb: usize,
    ca: i32,
    t: &OptTables,
) -> [u8; 16] {
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

fn handle_opaque_block_mode6(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
    t: &OptTables,
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
    color_cell_compression(6, &params, &mut results6, cp, 16, pixels, true, t);
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

#[derive(Clone, Copy, PartialEq)]
pub(super) enum BlockClass {
    Solid([i32; 4]),
    Alpha(i32, i32),
    Opaque,
}

pub(super) fn classify_block(pixels: &[ColorI; 16]) -> BlockClass {
    let (mut lo_r, mut hi_r) = (255i32, 0i32);
    let (mut lo_g, mut hi_g) = (255i32, 0i32);
    let (mut lo_b, mut hi_b) = (255i32, 0i32);
    let (mut lo_a, mut hi_a) = (255f32, 0f32);
    for i in 0..16 {
        let r = pixels[i].c[0];
        let g = pixels[i].c[1];
        let b = pixels[i].c[2];
        let a = pixels[i].c[3];
        lo_r = lo_r.min(r);
        hi_r = hi_r.max(r);
        lo_g = lo_g.min(g);
        hi_g = hi_g.max(g);
        lo_b = lo_b.min(b);
        hi_b = hi_b.max(b);
        let fa = a as f32;
        lo_a = lo_a.min(fa);
        hi_a = hi_a.max(fa);
    }
    if lo_r == hi_r && lo_g == hi_g && lo_b == hi_b && lo_a == hi_a {
        BlockClass::Solid([lo_r, lo_g, lo_b, lo_a as i32])
    } else if lo_a < 255.0 {
        BlockClass::Alpha(lo_a as i32, hi_a as i32)
    } else {
        BlockClass::Opaque
    }
}

pub(super) fn compress_group(
    group: &[[ColorI; 16]],
    cp: &Params,
    t: &OptTables,
    out: &mut [[u8; 16]],
) {
    let n = group.len();
    let mut base = CCParams::clear();
    base.weights = cp.weights;

    let mut classes = [BlockClass::Opaque; SIMD_W];
    for i in 0..n {
        classes[i] = classify_block(&group[i]);
    }

    let mut alpha_idx = [0usize; SIMD_W];
    let mut alpha_n = 0usize;
    let mut opaque_idx = [0usize; SIMD_W];
    let mut opaque_n = 0usize;
    for i in 0..n {
        if matches!(classes[i], BlockClass::Alpha(..)) {
            alpha_idx[alpha_n] = i;
            alpha_n += 1;
        }
        if classes[i] == BlockClass::Opaque && !cp.mode6_only {
            opaque_idx[opaque_n] = i;
            opaque_n += 1;
        }
    }

    let mut plans = [PartitionPlan::new(); SIMD_W];
    if alpha_n != 0 && cp.use_mode7 {
        let mut lanes: [&[ColorI; 16]; SIMD_W] = [&group[0]; SIMD_W];
        for k in 0..alpha_n {
            lanes[k] = &group[alpha_idx[k]];
        }
        let mut r = [SolutionList::new(); SIMD_W];
        estimate_partition_list_group(7, &lanes[..alpha_n], cp, cp.al_max_mode7 as i32, &mut r);
        for k in 0..alpha_n {
            plans[alpha_idx[k]].list7 = r[k];
        }
    }
    if opaque_n != 0 {
        let mut lanes: [&[ColorI; 16]; SIMD_W] = [&group[0]; SIMD_W];
        for k in 0..opaque_n {
            lanes[k] = &group[opaque_idx[k]];
        }
        let mut sub_plans = [PartitionPlan::new(); SIMD_W];
        build_partition_plans(&lanes[..opaque_n], cp, &mut sub_plans);
        for k in 0..opaque_n {
            let i = opaque_idx[k];
            plans[i].part0 = sub_plans[k].part0;
            plans[i].part13 = sub_plans[k].part13;
            plans[i].list13 = sub_plans[k].list13;
            plans[i].use_list13 = sub_plans[k].use_list13;
            plans[i].part2 = sub_plans[k].part2;
            plans[i].list2 = sub_plans[k].list2;
            plans[i].use_list2 = sub_plans[k].use_list2;
            plans[i].list0 = sub_plans[k].list0;
            plans[i].use_list0 = sub_plans[k].use_list0;
        }
    }

    for i in 0..n {
        let pixels = &group[i];
        out[i] = match classes[i] {
            BlockClass::Solid(c) => {
                handle_block_solid(c[0] as usize, c[1] as usize, c[2] as usize, c[3], t)
            }
            BlockClass::Alpha(lo, hi) => {
                let gated = apply_mode_tree_hint(pixels, cp);
                handle_alpha_block(
                    pixels,
                    gated.as_ref().unwrap_or(cp),
                    &base,
                    lo,
                    hi,
                    &plans[i],
                    t,
                )
            }
            BlockClass::Opaque => {
                if cp.mode6_only {
                    handle_opaque_block_mode6(pixels, cp, &base, t)
                } else {
                    handle_opaque_block(pixels, cp, &base, &plans[i], t)
                }
            }
        };
    }
}

pub(super) fn block_from_bytes(rgba16: &[u8]) -> [ColorI; 16] {
    let mut pixels = [ColorI::default(); 16];
    for i in 0..16 {
        pixels[i] = ColorI {
            c: [
                rgba16[i * 4] as i32,
                rgba16[i * 4 + 1] as i32,
                rgba16[i * 4 + 2] as i32,
                rgba16[i * 4 + 3] as i32,
            ],
        };
    }
    pixels
}
