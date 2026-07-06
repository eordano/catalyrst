#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

mod alpha45;
mod ccc;
mod cell;
mod eval;
mod handlers;
mod plan;
#[cfg(not(target_arch = "nvptx64"))]
mod probe_mod;
mod tables;

#[cfg(not(target_arch = "nvptx64"))]
pub use probe_mod::probe;

use super::sqrtf;
use alpha45::*;
use ccc::*;
use cell::*;
use eval::*;
use handlers::*;
use plan::*;
use tables::*;

#[derive(Clone)]
#[repr(C)]
pub struct Params {
    pub max_partitions_mode: [u32; 8],
    pub weights: [u32; 4],
    pub uber_level: u32,
    pub refinement_passes: u32,
    pub mode4_rotation_mask: u32,
    pub mode4_index_mask: u32,
    pub mode5_rotation_mask: u32,
    pub uber1_mask: u32,
    pub perceptual: bool,
    pub pbit_search: bool,
    pub mode6_only: bool,

    pub op_max_mode13: u32,
    pub op_max_mode0: u32,
    pub op_max_mode2: u32,
    pub use_mode: [bool; 7],

    pub al_max_mode7: u32,
    pub mode67_weight_mul: [u32; 4],
    pub use_mode4: bool,
    pub use_mode5: bool,
    pub use_mode6: bool,
    pub use_mode7: bool,
    pub use_mode4_rotation: bool,
    pub use_mode5_rotation: bool,
}

impl Params {
    pub const fn slow(perceptual: bool) -> Self {
        let weights = if perceptual {
            [128, 64, 16, 256]
        } else {
            [1, 1, 1, 1]
        };
        Params {
            max_partitions_mode: [16, 64, 64, 64, 0, 0, 0, 64],
            weights,
            uber_level: 0,
            refinement_passes: 1,
            mode4_rotation_mask: 0xF,
            mode4_index_mask: 3,
            mode5_rotation_mask: 0xF,
            uber1_mask: 7,
            perceptual,
            pbit_search: true,
            mode6_only: false,
            op_max_mode13: 1,
            op_max_mode0: 1,
            op_max_mode2: 1,
            use_mode: [true; 7],
            al_max_mode7: 2,
            mode67_weight_mul: [1, 1, 1, 1],
            use_mode4: true,
            use_mode5: true,
            use_mode6: true,
            use_mode7: true,
            use_mode4_rotation: true,
            use_mode5_rotation: true,
        }
    }

    pub fn basic(perceptual: bool) -> Self {
        let mut p = Self::slow(perceptual);
        p.uber_level = 1;
        p.pbit_search = false;
        p.al_max_mode7 = 1;
        if perceptual {
            p.use_mode[0] = false;
            p.use_mode[2] = false;
            p.use_mode[3] = false;
            p.use_mode[4] = false;
            p.use_mode[5] = false;
        } else {
            p.max_partitions_mode[1] = 32;
            p.max_partitions_mode[2] = 32;
            p.max_partitions_mode[3] = 32;
            p.max_partitions_mode[7] = 32;
            p.use_mode[2] = false;
        }
        p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bc7Profile {
    Slow,
    Basic,
}

fn apply_mode_tree_hint(pixels: &[ColorI; 16], cp: &Params) -> Option<Params> {
    let mut rgba = [[0i32; 4]; 16];
    for i in 0..16 {
        rgba[i] = pixels[i].c;
    }
    let feat = super::mode_tree::block_features(&rgba);
    let (mode, conf) = super::mode_tree::predict(&feat);
    let thr = 9000u16;
    let var_rgb = feat[0];
    let max_dr = feat[1];
    let mut p = cp.clone();
    match mode {
        5 if conf >= thr => {
            p.use_mode4 = false;
            p.use_mode6 = false;
            p.use_mode7 = false;
        }
        6 if conf >= thr && var_rgb >= 200 && max_dr >= 16 => {
            p.use_mode4 = false;
            p.use_mode5 = false;
            p.use_mode7 = false;
        }
        _ => return None,
    }
    Some(p)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct EndpointErr {
    pub error: u16,
    pub lo: u8,
    pub hi: u8,
}

#[repr(C)]
pub struct OptTables {
    pub mode0: [[[EndpointErr; 2]; 2]; 256],
    pub mode1: [[EndpointErr; 2]; 256],
    pub mode6: [[[EndpointErr; 2]; 2]; 256],
    pub mode7: [[[EndpointErr; 2]; 2]; 256],
    pub mode5: [u32; 256],
    pub mode4_3: [u32; 256],
    pub mode4_2: [u32; 256],
}

const MODE0_IDX: usize = 2;
const MODE1_IDX: usize = 2;
const MODE6_IDX: usize = 5;
const MODE5_IDX: usize = 1;
const MODE4_IDX3: usize = 2;
const MODE4_IDX2: usize = 1;
const MODE7_IDX: usize = 1;

#[cfg(not(target_arch = "nvptx64"))]
fn best_endpoints_per_c(
    lcount: u32,
    hcount: u32,
    expand_lo: impl Fn(u32) -> u32,
    expand_hi: impl Fn(u32) -> u32,
    weight: u32,
) -> [EndpointErr; 256] {
    let mut first_idx = [u32::MAX; 256];
    let mut first_lo = [0u8; 256];
    let mut first_hi = [0u8; 256];
    let mut idx = 0u32;
    for l in 0..lcount {
        let low_part = expand_lo(l) * (64 - weight);
        for h in 0..hcount {
            let k = ((low_part + expand_hi(h) * weight + 32) >> 6) as usize;
            debug_assert!(k < 256);
            if first_idx[k] == u32::MAX {
                first_idx[k] = idx;
                first_lo[k] = l as u8;
                first_hi[k] = h as u8;
            }
            idx += 1;
        }
    }
    let mut out = [EndpointErr::default(); 256];
    for c in 0..256usize {
        let mut done = false;
        for d in 0..256usize {
            let mut best_k = usize::MAX;
            let mut best_i = u32::MAX;
            if d <= c {
                let k = c - d;
                if first_idx[k] != u32::MAX {
                    best_k = k;
                    best_i = first_idx[k];
                }
            }

            if d > 0 && c + d < 256 {
                let k = c + d;
                if first_idx[k] < best_i {
                    best_k = k;
                }
            }
            if best_k != usize::MAX {
                out[c] = EndpointErr {
                    error: (d * d) as u16,
                    lo: first_lo[best_k],
                    hi: first_hi[best_k],
                };
                done = true;
                break;
            }
        }
        debug_assert!(done, "no reachable k for c={c}");
    }
    out
}

#[cfg(not(target_arch = "nvptx64"))]
pub fn build_opt_tables() -> Box<OptTables> {
    let mut t = Box::new(OptTables {
        mode0: [[[EndpointErr::default(); 2]; 2]; 256],
        mode1: [[EndpointErr::default(); 2]; 256],
        mode6: [[[EndpointErr::default(); 2]; 2]; 256],
        mode7: [[[EndpointErr::default(); 2]; 2]; 256],
        mode5: [0u32; 256],
        mode4_3: [0u32; 256],
        mode4_2: [0u32; 256],
    });

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                16,
                16,
                |l| {
                    let mut low = ((l << 1) | lp as u32) << 3;
                    low |= low >> 5;
                    low
                },
                |h| {
                    let mut high = ((h << 1) | hp) << 3;
                    high |= high >> 5;
                    high
                },
                G_WEIGHTS3[MODE0_IDX],
            );
            for c in 0..256usize {
                t.mode0[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    for lp in 0..2usize {
        let per_c = best_endpoints_per_c(
            64,
            64,
            |l| {
                let mut low = ((l << 1) | lp as u32) << 1;
                low |= low >> 7;
                low
            },
            |h| {
                let mut high = ((h << 1) | lp as u32) << 1;
                high |= high >> 7;
                high
            },
            G_WEIGHTS3[MODE1_IDX],
        );
        for c in 0..256usize {
            t.mode1[c][lp] = per_c[c];
        }
    }

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                128,
                128,
                |l| (l << 1) | lp as u32,
                |h| (h << 1) | hp,
                G_WEIGHTS4[MODE6_IDX],
            );
            for c in 0..256usize {
                t.mode6[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    {
        let per_c = best_endpoints_per_c(
            128,
            128,
            |l| {
                let mut low = l << 1;
                low |= low >> 7;
                low
            },
            |h| {
                let mut high = h << 1;
                high |= high >> 7;
                high
            },
            G_WEIGHTS2[MODE5_IDX],
        );
        for c in 0..256usize {
            t.mode5[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    {
        let per_c = best_endpoints_per_c(
            32,
            32,
            |l| {
                let mut low = l << 3;
                low |= low >> 5;
                low
            },
            |h| {
                let mut high = h << 3;
                high |= high >> 5;
                high
            },
            G_WEIGHTS3[MODE4_IDX3],
        );
        for c in 0..256usize {
            t.mode4_3[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    {
        let per_c = best_endpoints_per_c(
            32,
            32,
            |l| {
                let mut low = l << 3;
                low |= low >> 5;
                low
            },
            |h| {
                let mut high = h << 3;
                high |= high >> 5;
                high
            },
            G_WEIGHTS2[MODE4_IDX2],
        );
        for c in 0..256usize {
            t.mode4_2[c] = per_c[c].lo as u32 | ((per_c[c].hi as u32) << 8);
        }
    }

    for hp in 0..2u32 {
        for lp in 0..2usize {
            let per_c = best_endpoints_per_c(
                32,
                32,
                |l| {
                    let mut low = ((l << 1) | lp as u32) << 2;
                    low |= low >> 6;
                    low
                },
                |h| {
                    let mut high = ((h << 1) | hp) << 2;
                    high |= high >> 6;
                    high
                },
                G_WEIGHTS2[MODE7_IDX],
            );
            for c in 0..256usize {
                t.mode7[c][hp as usize][lp] = per_c[c];
            }
        }
    }

    t
}

pub const GROUP_WIDTH: usize = SIMD_W;

pub fn encode_group(
    rgba_block_major: &[u8],
    n: usize,
    params: &Params,
    tables: &OptTables,
    out: &mut [[u8; 16]],
) {
    let mut group = [[ColorI::default(); 16]; SIMD_W];
    for k in 0..n {
        group[k] = block_from_bytes(&rgba_block_major[k * 64..k * 64 + 64]);
    }
    compress_group(&group[..n], params, tables, out);
}

pub fn group_signature(rgba_block_major: &[u8], n: usize) -> u8 {
    let mut sig = 0u8;
    for k in 0..n {
        let px = block_from_bytes(&rgba_block_major[k * 64..k * 64 + 64]);
        let cls = match classify_block(&px) {
            BlockClass::Solid(_) => 0u8,
            BlockClass::Alpha(..) => 1u8,
            BlockClass::Opaque => 2u8,
        };
        sig |= cls << (k * 2);
    }
    sig
}

pub fn encode_block(rgba: &[u8; 64], params: &Params, tables: &OptTables) -> [u8; 16] {
    let group = [block_from_bytes(rgba)];
    let mut out = [[0u8; 16]; 1];
    compress_group(&group, params, tables, &mut out);
    out[0]
}

#[cfg(not(target_arch = "nvptx64"))]
pub fn encode_blocks(
    rgba_block_major: &[u8],
    num_blocks: usize,
    params: &Params,
    tables: &OptTables,
) -> Vec<u8> {
    assert_eq!(rgba_block_major.len(), num_blocks * 64);
    let group_bytes = SIMD_W * 64;
    let mut out = Vec::with_capacity(num_blocks * 16);
    for chunk in rgba_block_major.chunks(group_bytes) {
        let n = chunk.len() / 64;
        let mut group = [[ColorI::default(); 16]; SIMD_W];
        for k in 0..n {
            group[k] = block_from_bytes(&chunk[k * 64..k * 64 + 64]);
        }
        let mut blocks = [[0u8; 16]; SIMD_W];
        compress_group(&group[..n], params, tables, &mut blocks);
        for blk in blocks[..n].iter() {
            out.extend_from_slice(blk);
        }
    }
    out
}
