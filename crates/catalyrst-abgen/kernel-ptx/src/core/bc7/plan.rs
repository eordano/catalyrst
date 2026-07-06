use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Solution {
    pub(super) index: u32,
    pub(super) err: u64,
}

pub(super) const SIMD_W: usize = 4;

pub(super) const SOL_CAP: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct SolutionList {
    pub(super) sols: [Solution; SOL_CAP],
    pub(super) len: usize,
}
impl SolutionList {
    pub(super) const fn new() -> Self {
        SolutionList {
            sols: [Solution { index: 0, err: 0 }; SOL_CAP],
            len: 0,
        }
    }
}

pub(super) fn estimate_partition_group(
    mode: usize,
    lanes: &[&[ColorI; 16]],
    cp: &Params,
) -> [u32; SIMD_W] {
    let n = lanes.len();
    let total_subsets = G_NUM_SUBSETS[mode];
    let total_partitions = cp.max_partitions_mode[mode].min(1u32 << G_PARTITION_BITS[mode]);
    let mut best_partition = [0u32; SIMD_W];
    if total_partitions <= 1 {
        return best_partition;
    }
    let params = make_est_params(mode, cp);

    let subset_tab = subset_idx_tables(total_subsets);
    let mut best_err = [u64::MAX; SIMD_W];
    let mut retired = [false; SIMD_W];
    for partition in 0..total_partitions {
        let si = &subset_tab[partition as usize];
        for lane in 0..n {
            if retired[lane] {
                continue;
            }

            let mut total_subset_err = 0u64;
            for subset in 0..total_subsets {
                let err = est_subset_err(
                    mode,
                    &params,
                    &si.idx[subset],
                    si.total[subset],
                    lanes[lane],
                );
                total_subset_err += err;
                if total_subset_err >= best_err[lane] {
                    break;
                }
            }
            if total_subset_err < best_err[lane] {
                best_err[lane] = total_subset_err;
                best_partition[lane] = partition;

                if best_err[lane] == 0 {
                    retired[lane] = true;
                }
            }

            if total_subsets == 2
                && partition as usize == BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
                && best_partition[lane] as usize != BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
            {
                retired[lane] = true;
            }
        }
        if retired[..n].iter().all(|&r| r) {
            break;
        }
    }
    best_partition
}

pub(super) fn estimate_partition_list_group(
    mode: usize,
    lanes: &[&[ColorI; 16]],
    cp: &Params,
    max_solutions_in: i32,
    out: &mut [SolutionList],
) {
    let n = lanes.len();
    let total_subsets = G_NUM_SUBSETS[mode];
    let total_partitions = cp.max_partitions_mode[mode].min(1u32 << G_PARTITION_BITS[mode]);
    if total_partitions <= 1 {
        for lane in 0..n {
            let mut l = SolutionList::new();
            l.sols[0] = Solution { index: 0, err: 0 };
            l.len = 1;
            out[lane] = l;
        }
        return;
    } else if max_solutions_in >= total_partitions as i32 {
        let mut l = SolutionList::new();
        let take = (total_partitions as usize).min(SOL_CAP);
        for i in 0..take {
            l.sols[i] = Solution {
                index: i as u32,
                err: i as u64,
            };
        }
        l.len = take;
        for lane in 0..n {
            out[lane] = l;
        }
        return;
    }
    let mut max_solutions = max_solutions_in;
    const THRESH: i32 = 4;
    if total_subsets == 2 && max_solutions < THRESH {
        max_solutions = THRESH;
    }
    if max_solutions > SOL_CAP as i32 {
        max_solutions = SOL_CAP as i32;
    }
    let params = make_est_params(mode, cp);

    let mut sols = [[Solution { index: 0, err: 0 }; SOL_CAP]; SIMD_W];
    let mut num_solutions = [0i32; SIMD_W];

    let subset_tab = subset_idx_tables(total_subsets);
    let mut i_at = [0i32; SIMD_W];
    for partition in 0..total_partitions {
        let si = &subset_tab[partition as usize];

        for lane in 0..n {
            let full = num_solutions[lane] == max_solutions;
            let thresh = if full {
                sols[lane][(max_solutions - 1) as usize].err
            } else {
                u64::MAX
            };
            let mut total_subset_err = 0u64;
            let mut pruned = false;
            for subset in 0..total_subsets {
                let err = est_subset_err(
                    mode,
                    &params,
                    &si.idx[subset],
                    si.total[subset],
                    lanes[lane],
                );
                total_subset_err += err;
                if total_subset_err >= thresh {
                    pruned = true;
                    break;
                }
            }
            if pruned {
                i_at[lane] = num_solutions[lane];
                continue;
            }
            let solutions = &mut sols[lane];
            let mut i = 0i32;
            while i < num_solutions[lane] {
                if total_subset_err < solutions[i as usize].err {
                    break;
                }
                i += 1;
            }
            if i < num_solutions[lane] {
                let mut solutions_to_move = (max_solutions - 1) - i;
                let num_elements_at_i = num_solutions[lane] - i;
                if solutions_to_move > num_elements_at_i {
                    solutions_to_move = num_elements_at_i;
                }
                let mut j = solutions_to_move - 1;
                while j >= 0 {
                    solutions[(i + j + 1) as usize] = solutions[(i + j) as usize];
                    j -= 1;
                }
            }
            if num_solutions[lane] < max_solutions {
                num_solutions[lane] += 1;
            }
            if i < num_solutions[lane] {
                solutions[i as usize].err = total_subset_err;
                solutions[i as usize].index = partition;
            }
            i_at[lane] = i;
        }

        if total_subsets == 2
            && partition as usize == BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
            && i_at[..n].iter().all(|&i| i >= THRESH)
        {
            break;
        }
    }
    for lane in 0..n {
        let take = (num_solutions[lane]).min(max_solutions_in) as usize;
        let mut l = SolutionList::new();
        l.sols[..take].copy_from_slice(&sols[lane][..take]);
        l.len = take;
        out[lane] = l;
    }
}

#[derive(Clone, Copy)]
pub(super) struct PartitionPlan {
    pub(super) part0: u32,
    pub(super) part13: u32,
    pub(super) list13: SolutionList,
    pub(super) use_list13: bool,
    pub(super) part2: u32,
    pub(super) list2: SolutionList,
    pub(super) use_list2: bool,
    pub(super) list0: SolutionList,
    pub(super) use_list0: bool,
    pub(super) list7: SolutionList,
}
impl PartitionPlan {
    pub(super) const fn new() -> Self {
        PartitionPlan {
            part0: 0,
            part13: 0,
            list13: SolutionList::new(),
            use_list13: false,
            part2: 0,
            list2: SolutionList::new(),
            use_list2: false,
            list0: SolutionList::new(),
            use_list0: false,
            list7: SolutionList::new(),
        }
    }
}

pub(super) fn build_partition_plans(
    lanes: &[&[ColorI; 16]],
    cp: &Params,
    plans: &mut [PartitionPlan],
) {
    let n = lanes.len();

    if cp.use_mode[1] || cp.use_mode[3] {
        if cp.op_max_mode13 == 1 {
            let r = estimate_partition_group(1, lanes, cp);
            for l in 0..n {
                plans[l].part13 = r[l];
            }
        } else {
            let mut r = [SolutionList::new(); SIMD_W];
            estimate_partition_list_group(1, lanes, cp, cp.op_max_mode13 as i32, &mut r);
            for l in 0..n {
                plans[l].list13 = r[l];
                plans[l].use_list13 = true;
            }
        }
    }
    if cp.use_mode[0] {
        if cp.op_max_mode0 == 1 {
            let r = estimate_partition_group(0, lanes, cp);
            for l in 0..n {
                plans[l].part0 = r[l];
            }
        } else {
            let mut r = [SolutionList::new(); SIMD_W];
            estimate_partition_list_group(0, lanes, cp, cp.op_max_mode0 as i32, &mut r);
            for l in 0..n {
                plans[l].list0 = r[l];
                plans[l].use_list0 = true;
            }
        }
    }
    if cp.use_mode[2] {
        if cp.op_max_mode2 == 1 {
            let r = estimate_partition_group(2, lanes, cp);
            for l in 0..n {
                plans[l].part2 = r[l];
            }
        } else {
            let mut r = [SolutionList::new(); SIMD_W];
            estimate_partition_list_group(2, lanes, cp, cp.op_max_mode2 as i32, &mut r);
            for l in 0..n {
                plans[l].list2 = r[l];
                plans[l].use_list2 = true;
            }
        }
    }
    if cp.use_mode7 {
        let mut r = [SolutionList::new(); SIMD_W];
        estimate_partition_list_group(7, lanes, cp, cp.al_max_mode7 as i32, &mut r);
        for l in 0..n {
            plans[l].list7 = r[l];
        }
    }
}

#[derive(Clone)]
pub(super) struct OptResults {
    pub(super) mode: usize,
    pub(super) partition: u32,
    pub(super) selectors: [i32; 16],
    pub(super) alpha_selectors: [i32; 16],
    pub(super) low: [ColorI; 3],
    pub(super) high: [ColorI; 3],
    pub(super) pbits: [[u32; 2]; 3],
    pub(super) rotation: u32,
    pub(super) index_selector: u32,
}
impl OptResults {
    pub(super) fn new() -> Self {
        OptResults {
            mode: 0,
            partition: 0,
            selectors: [0; 16],
            alpha_selectors: [0; 16],
            low: [ColorI::default(); 3],
            high: [ColorI::default(); 3],
            pbits: [[0; 2]; 3],
            rotation: 0,
            index_selector: 0,
        }
    }
}

fn set_block_bits(bytes: &mut [u8; 16], mut val: u32, mut num_bits: u32, cur_ofs: &mut u32) {
    while num_bits != 0 {
        let n = (8 - (*cur_ofs & 7)).min(num_bits);
        bytes[(*cur_ofs >> 3) as usize] |= (val << (*cur_ofs & 7)) as u8;
        val >>= n;
        num_bits -= n;
        *cur_ofs += n;
    }
}

pub(super) fn encode_bc7_block_bits(res: &OptResults) -> [u8; 16] {
    let best_mode = res.mode;
    let total_subsets = G_NUM_SUBSETS[best_mode];
    let total_partitions = 1u32 << G_PARTITION_BITS[best_mode];

    let part: &[u8] = if total_subsets == 1 {
        &[0u8; 16]
    } else if total_subsets == 2 {
        &G_PARTITION2[(res.partition as usize) * 16..(res.partition as usize) * 16 + 16]
    } else {
        &G_PARTITION3[(res.partition as usize) * 16..(res.partition as usize) * 16 + 16]
    };

    let mut color_selectors = res.selectors;
    let mut alpha_selectors = res.alpha_selectors;
    let mut low = res.low;
    let mut high = res.high;
    let mut pbits = res.pbits;
    let mut anchor = [-1i32; 3];

    for k in 0..total_subsets {
        let mut anchor_index = 0usize;
        if k != 0 {
            if total_subsets == 3 && k == 1 {
                anchor_index = G_ANCHOR_3RD_1[res.partition as usize] as usize;
            } else if total_subsets == 3 && k == 2 {
                anchor_index = G_ANCHOR_3RD_2[res.partition as usize] as usize;
            } else {
                anchor_index = G_ANCHOR_2ND[res.partition as usize] as usize;
            }
        }
        anchor[k] = anchor_index as i32;
        let color_index_bits = get_color_index_size(best_mode, res.index_selector);
        let num_color_indices = 1i32 << color_index_bits;
        if color_selectors[anchor_index] & (num_color_indices >> 1) != 0 {
            for i in 0..16 {
                if part[i] as usize == k {
                    color_selectors[i] = (num_color_indices - 1) - color_selectors[i];
                }
            }
            if mode_has_separate_alpha_selectors(best_mode) {
                for q in 0..3 {
                    core::mem::swap(&mut low[k].c[q], &mut high[k].c[q]);
                }
            } else {
                core::mem::swap(&mut low[k], &mut high[k]);
            }
            if G_MODE_HAS_SHARED_P_BITS[best_mode] == 0 {
                pbits[k].swap(0, 1);
            }
        }
        if mode_has_separate_alpha_selectors(best_mode) {
            let alpha_index_bits = get_alpha_index_size(best_mode, res.index_selector);
            let num_alpha_indices = 1i32 << alpha_index_bits;
            if alpha_selectors[anchor_index] & (num_alpha_indices >> 1) != 0 {
                for i in 0..16 {
                    if part[i] as usize == k {
                        alpha_selectors[i] = (num_alpha_indices - 1) - alpha_selectors[i];
                    }
                }
                core::mem::swap(&mut low[k].c[3], &mut high[k].c[3]);
            }
        }
    }

    let mut block = [0u8; 16];
    let mut cur = 0u32;
    set_block_bits(&mut block, 1 << best_mode, best_mode as u32 + 1, &mut cur);
    if best_mode == 4 || best_mode == 5 {
        set_block_bits(&mut block, res.rotation, 2, &mut cur);
    }
    if best_mode == 4 {
        set_block_bits(&mut block, res.index_selector, 1, &mut cur);
    }
    if total_partitions > 1 {
        set_block_bits(
            &mut block,
            res.partition,
            if total_partitions == 64 { 6 } else { 4 },
            &mut cur,
        );
    }
    let total_comps = if best_mode >= 4 { 4 } else { 3 };
    for comp in 0..total_comps {
        for subset in 0..total_subsets {
            let prec = if comp == 3 {
                G_ALPHA_PRECISION_TABLE[best_mode]
            } else {
                G_COLOR_PRECISION_TABLE[best_mode]
            };
            set_block_bits(&mut block, low[subset].c[comp] as u32, prec, &mut cur);
            set_block_bits(&mut block, high[subset].c[comp] as u32, prec, &mut cur);
        }
    }
    if G_MODE_HAS_P_BITS[best_mode] != 0 {
        for subset in 0..total_subsets {
            set_block_bits(&mut block, pbits[subset][0], 1, &mut cur);
            if G_MODE_HAS_SHARED_P_BITS[best_mode] == 0 {
                set_block_bits(&mut block, pbits[subset][1], 1, &mut cur);
            }
        }
    }
    for y in 0..4 {
        for x in 0..4 {
            let idx = x + y * 4;
            let mut n = if res.index_selector != 0 {
                get_alpha_index_size(best_mode, res.index_selector)
            } else {
                get_color_index_size(best_mode, res.index_selector)
            };
            if idx as i32 == anchor[0] || idx as i32 == anchor[1] || idx as i32 == anchor[2] {
                n -= 1;
            }
            let val = if res.index_selector != 0 {
                alpha_selectors[idx]
            } else {
                color_selectors[idx]
            };
            set_block_bits(&mut block, val as u32, n, &mut cur);
        }
    }
    if mode_has_separate_alpha_selectors(best_mode) {
        for y in 0..4 {
            for x in 0..4 {
                let idx = x + y * 4;
                let mut n = if res.index_selector != 0 {
                    get_color_index_size(best_mode, res.index_selector)
                } else {
                    get_alpha_index_size(best_mode, res.index_selector)
                };
                if idx as i32 == anchor[0] || idx as i32 == anchor[1] || idx as i32 == anchor[2] {
                    n -= 1;
                }
                let val = if res.index_selector != 0 {
                    color_selectors[idx]
                } else {
                    alpha_selectors[idx]
                };
                set_block_bits(&mut block, val as u32, n, &mut cur);
            }
        }
    }
    block
}

pub(super) fn encode_bc7_block_mode6(res: &OptResults) -> [u8; 16] {
    let (low, high, pbits);
    let invert_selectors: u32;
    if res.selectors[0] & 8 != 0 {
        invert_selectors = 15;
        low = res.high[0];
        high = res.low[0];
        pbits = [res.pbits[0][1], res.pbits[0][0]];
    } else {
        invert_selectors = 0;
        low = res.low[0];
        high = res.high[0];
        pbits = [res.pbits[0][0], res.pbits[0][1]];
    }
    let mut l: u64 = 1 << 6;
    l |= (low.c[0] as u64) << 7;
    l |= (high.c[0] as u64) << 14;
    l |= (low.c[1] as u64) << 21;
    l |= (high.c[1] as u64) << 28;
    l |= (low.c[2] as u64) << 35;
    l |= (high.c[2] as u64) << 42;
    l |= (low.c[3] as u64) << 49;
    l |= (high.c[3] as u64) << 56;
    l |= (pbits[0] as u64) << 63;
    let mut h: u64 = pbits[1] as u64;
    for (i, sel) in res.selectors.iter().enumerate() {
        let v = (invert_selectors ^ (*sel as u32)) as u64;

        let shift = if i == 0 { 1 } else { i * 4 };
        h |= v << shift;
    }
    let mut block = [0u8; 16];
    block[0..8].copy_from_slice(&l.to_le_bytes());
    block[8..16].copy_from_slice(&h.to_le_bytes());
    block
}
