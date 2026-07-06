#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

mod alpha;
mod bits;
mod ccc;
mod color;
mod est_simd;
mod estimate;
mod evaluate;
mod mip;
mod opaque;
mod opt_tables;
mod pack;
mod partition;
mod tables;

pub use mip::*;

use alpha::*;
use bits::*;
use ccc::*;
use color::*;
use est_simd::*;
use estimate::*;
use evaluate::*;
use opaque::*;
use opt_tables::*;
use pack::*;
use partition::*;
use std::sync::OnceLock;
use tables::*;

#[derive(Clone)]
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
    let feat = crate::bc7_mode_tree::block_features(&rgba);
    let (mode, conf) = crate::bc7_mode_tree::predict(&feat);
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

#[derive(Clone, Copy, PartialEq)]
enum BlockClass {
    Solid([i32; 4]),
    Alpha(i32, i32),
    Opaque,
}

fn classify_block(pixels: &[ColorI; 16]) -> BlockClass {
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

fn compress_group(group: &[[ColorI; 16]], cp: &Params) -> Vec<[u8; 16]> {
    let mut base = CCParams::clear();
    base.weights = cp.weights;

    let classes: Vec<BlockClass> = group.iter().map(classify_block).collect();

    let alpha_idx: Vec<usize> = (0..group.len())
        .filter(|&i| matches!(classes[i], BlockClass::Alpha(..)))
        .collect();
    let opaque_idx: Vec<usize> = (0..group.len())
        .filter(|&i| classes[i] == BlockClass::Opaque && !cp.mode6_only)
        .collect();

    let mut plans: Vec<PartitionPlan> = vec![PartitionPlan::default(); group.len()];
    if !alpha_idx.is_empty() && cp.use_mode7 {
        let lanes: Vec<&[ColorI; 16]> = alpha_idx.iter().map(|&i| &group[i]).collect();
        let r = estimate_partition_list_group(7, &lanes, cp, cp.al_max_mode7 as i32);
        for (k, &i) in alpha_idx.iter().enumerate() {
            plans[i].list7 = r[k].clone();
        }
    }
    if !opaque_idx.is_empty() {
        let lanes: Vec<&[ColorI; 16]> = opaque_idx.iter().map(|&i| &group[i]).collect();
        let sub_plans = build_partition_plans(&lanes, cp);
        for (k, &i) in opaque_idx.iter().enumerate() {
            plans[i].part0 = sub_plans[k].part0;
            plans[i].part13 = sub_plans[k].part13;
            plans[i].list13 = sub_plans[k].list13.clone();
            plans[i].use_list13 = sub_plans[k].use_list13;
            plans[i].part2 = sub_plans[k].part2;
            plans[i].list2 = sub_plans[k].list2.clone();
            plans[i].use_list2 = sub_plans[k].use_list2;
            plans[i].list0 = sub_plans[k].list0.clone();
            plans[i].use_list0 = sub_plans[k].use_list0;
        }
    }

    let mut out = Vec::with_capacity(group.len());
    for (i, pixels) in group.iter().enumerate() {
        let blk = match classes[i] {
            BlockClass::Solid(c) => {
                handle_block_solid(c[0] as usize, c[1] as usize, c[2] as usize, c[3])
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
                )
            }
            BlockClass::Opaque => {
                if cp.mode6_only {
                    handle_opaque_block_mode6(pixels, cp, &base)
                } else {
                    handle_opaque_block(pixels, cp, &base, &plans[i])
                }
            }
        };
        out.push(blk);
    }
    out
}

fn block_from_bytes(rgba16: &[u8]) -> [ColorI; 16] {
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

pub fn encode_blocks(rgba_block_major: &[u8], num_blocks: usize, params: &Params) -> Vec<u8> {
    assert_eq!(rgba_block_major.len(), num_blocks * 64);

    use rayon::prelude::*;
    let group_bytes = SIMD_W * 64;
    let parts: Vec<Vec<[u8; 16]>> = rgba_block_major
        .par_chunks(group_bytes)
        .map(|chunk| {
            let n = chunk.len() / 64;
            let mut group: Vec<[ColorI; 16]> = Vec::with_capacity(n);
            for k in 0..n {
                group.push(block_from_bytes(&chunk[k * 64..k * 64 + 64]));
            }
            compress_group(&group, params)
        })
        .collect();
    let mut out = Vec::with_capacity(num_blocks * 16);
    for part in parts {
        for blk in part {
            out.extend_from_slice(&blk);
        }
    }

    if let Some(path) = bc7_capture_path() {
        use std::io::Write;
        let mut rec = Vec::with_capacity(num_blocks * 80);
        for i in 0..num_blocks {
            rec.extend_from_slice(&out[i * 16..i * 16 + 16]);
            rec.extend_from_slice(&rgba_block_major[i * 64..i * 64 + 64]);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _g = BC7_CAPTURE_LOCK.lock().unwrap();
            let _ = f.write_all(&rec);
        }
    }
    out
}

static BC7_CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn bc7_capture_path() -> Option<std::path::PathBuf> {
    static P: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    P.get_or_init(|| std::env::var_os("ABGEN_BC7_CAPTURE").map(std::path::PathBuf::from))
        .clone()
}
