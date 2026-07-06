use super::*;

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
                    std::mem::swap(&mut low[k].c[q], &mut high[k].c[q]);
                }
            } else {
                std::mem::swap(&mut low[k], &mut high[k]);
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
                std::mem::swap(&mut low[k].c[3], &mut high[k].c[3]);
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
