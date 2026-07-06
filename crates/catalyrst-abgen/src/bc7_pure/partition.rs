use super::*;

fn make_est_params(mode: usize, cp: &Params) -> CCParams {
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

#[derive(Clone, Copy, Default)]
pub(super) struct Solution {
    pub(super) index: u32,
    pub(super) err: u64,
}

pub(super) const SIMD_W: usize = 4;

fn estimate_partition_group(mode: usize, lanes: &[&[ColorI; 16]], cp: &Params) -> Vec<u32> {
    let n = lanes.len();
    let total_subsets = G_NUM_SUBSETS[mode];
    let total_partitions = cp.max_partitions_mode[mode].min(1u32 << G_PARTITION_BITS[mode]);
    if total_partitions <= 1 {
        return vec![0u32; n];
    }
    let params = make_est_params(mode, cp);
    let mut best_partition = vec![0u32; n];

    let lanes_f32 = lanes_f32_if_supported(lanes);
    let subset_tab = subset_idx_tables(total_subsets);
    #[cfg(target_arch = "x86_64")]
    if let Some(lfs) = &lanes_f32 {
        for lane in 0..n {
            best_partition[lane] = unsafe {
                est_partition_lane_vperm(
                    mode,
                    &params,
                    &lfs[lane],
                    subset_tab,
                    total_partitions,
                    total_subsets,
                )
            };
        }
        return best_partition;
    }
    let mut best_err = vec![u64::MAX; n];
    let mut retired = vec![false; n];
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
                    lanes_f32.as_ref().map(|v| &v[lane]),
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
) -> Vec<Vec<Solution>> {
    let n = lanes.len();
    let total_subsets = G_NUM_SUBSETS[mode];
    let total_partitions = cp.max_partitions_mode[mode].min(1u32 << G_PARTITION_BITS[mode]);
    if total_partitions <= 1 {
        return vec![vec![Solution { index: 0, err: 0 }]; n];
    } else if max_solutions_in >= total_partitions as i32 {
        let mut v = Vec::new();
        for i in 0..total_partitions as usize {
            v.push(Solution {
                index: i as u32,
                err: i as u64,
            });
        }
        return vec![v; n];
    }
    let mut max_solutions = max_solutions_in;
    const THRESH: i32 = 4;
    if total_subsets == 2 && max_solutions < THRESH {
        max_solutions = THRESH;
    }
    let params = make_est_params(mode, cp);

    let cap = max_solutions as usize;
    let mut sols: Vec<Vec<Solution>> = vec![vec![Solution::default(); cap]; n];
    let mut num_solutions = vec![0i32; n];

    let lanes_f32 = lanes_f32_if_supported(lanes);
    let subset_tab = subset_idx_tables(total_subsets);
    let mut i_at = vec![0i32; n];
    #[cfg(target_arch = "x86_64")]
    if let Some(lfs) = &lanes_f32 {
        let cb = BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX as u32;
        let phase1_end = if total_subsets == 2 {
            (cb + 1).min(total_partitions)
        } else {
            total_partitions
        };
        for lane in 0..n {
            i_at[lane] = unsafe {
                est_partition_list_lane_vperm(
                    mode,
                    &params,
                    &lfs[lane],
                    subset_tab,
                    0,
                    phase1_end,
                    total_subsets,
                    &mut sols[lane],
                    &mut num_solutions[lane],
                    max_solutions,
                )
            };
        }
        let stop =
            total_subsets == 2 && cb < total_partitions && i_at[..n].iter().all(|&i| i >= THRESH);
        if !stop && phase1_end < total_partitions {
            for lane in 0..n {
                unsafe {
                    est_partition_list_lane_vperm(
                        mode,
                        &params,
                        &lfs[lane],
                        subset_tab,
                        phase1_end,
                        total_partitions,
                        total_subsets,
                        &mut sols[lane],
                        &mut num_solutions[lane],
                        max_solutions,
                    )
                };
            }
        }
        let mut out = Vec::with_capacity(n);
        for lane in 0..n {
            let take = (num_solutions[lane]).min(max_solutions_in) as usize;
            out.push(sols[lane][..take].to_vec());
        }
        return out;
    }
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
                    lanes_f32.as_ref().map(|v| &v[lane]),
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
    let mut out = Vec::with_capacity(n);
    for lane in 0..n {
        let take = (num_solutions[lane]).min(max_solutions_in) as usize;
        out.push(sols[lane][..take].to_vec());
    }
    out
}

#[derive(Clone, Default)]
pub(super) struct PartitionPlan {
    pub(super) part0: u32,
    pub(super) part13: u32,
    pub(super) list13: Vec<Solution>,
    pub(super) use_list13: bool,
    pub(super) part2: u32,
    pub(super) list2: Vec<Solution>,
    pub(super) use_list2: bool,
    pub(super) list0: Vec<Solution>,
    pub(super) use_list0: bool,
    pub(super) list7: Vec<Solution>,
}

pub(super) fn build_partition_plans(lanes: &[&[ColorI; 16]], cp: &Params) -> Vec<PartitionPlan> {
    let n = lanes.len();
    let mut plans = vec![PartitionPlan::default(); n];

    if cp.use_mode[1] || cp.use_mode[3] {
        if cp.op_max_mode13 == 1 {
            let r = estimate_partition_group(1, lanes, cp);
            for l in 0..n {
                plans[l].part13 = r[l];
            }
        } else {
            let r = estimate_partition_list_group(1, lanes, cp, cp.op_max_mode13 as i32);
            for l in 0..n {
                plans[l].list13 = r[l].clone();
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
            let r = estimate_partition_list_group(0, lanes, cp, cp.op_max_mode0 as i32);
            for l in 0..n {
                plans[l].list0 = r[l].clone();
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
            let r = estimate_partition_list_group(2, lanes, cp, cp.op_max_mode2 as i32);
            for l in 0..n {
                plans[l].list2 = r[l].clone();
                plans[l].use_list2 = true;
            }
        }
    }
    if cp.use_mode7 {
        let r = estimate_partition_list_group(7, lanes, cp, cp.al_max_mode7 as i32);
        for l in 0..n {
            plans[l].list7 = r[l].clone();
        }
    }
    plans
}
