#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

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

const G_WEIGHTS2: [u32; 4] = [0, 21, 43, 64];
const G_WEIGHTS3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const G_WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

const G_WEIGHTS2X: [[f32; 4]; 4] = [
    [0.000000, 0.000000, 1.000000, 0.000000],
    [0.107666, 0.220459, 0.451416, 0.328125],
    [0.451416, 0.220459, 0.107666, 0.671875],
    [1.000000, 0.000000, 0.000000, 1.000000],
];
const G_WEIGHTS3X: [[f32; 4]; 8] = [
    [0.000000, 0.000000, 1.000000, 0.000000],
    [0.019775, 0.120850, 0.738525, 0.140625],
    [0.079102, 0.202148, 0.516602, 0.281250],
    [0.177979, 0.243896, 0.334229, 0.421875],
    [0.334229, 0.243896, 0.177979, 0.578125],
    [0.516602, 0.202148, 0.079102, 0.718750],
    [0.738525, 0.120850, 0.019775, 0.859375],
    [1.000000, 0.000000, 0.000000, 1.000000],
];
const G_WEIGHTS4X: [[f32; 4]; 16] = [
    [0.000000, 0.000000, 1.000000, 0.000000],
    [0.003906, 0.058594, 0.878906, 0.062500],
    [0.019775, 0.120850, 0.738525, 0.140625],
    [0.041260, 0.161865, 0.635010, 0.203125],
    [0.070557, 0.195068, 0.539307, 0.265625],
    [0.107666, 0.220459, 0.451416, 0.328125],
    [0.165039, 0.241211, 0.352539, 0.406250],
    [0.219727, 0.249023, 0.282227, 0.468750],
    [0.282227, 0.249023, 0.219727, 0.531250],
    [0.352539, 0.241211, 0.165039, 0.593750],
    [0.451416, 0.220459, 0.107666, 0.671875],
    [0.539307, 0.195068, 0.070557, 0.734375],
    [0.635010, 0.161865, 0.041260, 0.796875],
    [0.738525, 0.120850, 0.019775, 0.859375],
    [0.878906, 0.058594, 0.003906, 0.937500],
    [1.000000, 0.000000, 0.000000, 1.000000],
];

const BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX: usize = 34;

#[rustfmt::skip]
const G_PARTITION2: [u8; 64 * 16] = [
    0,0,1,1,0,0,1,1,0,0,1,1,0,0,1,1, 0,0,0,1,0,0,0,1,0,0,0,1,0,0,0,1,
    0,1,1,1,0,1,1,1,0,1,1,1,0,1,1,1, 0,0,0,1,0,0,1,1,0,0,1,1,0,1,1,1,
    0,0,0,0,0,0,0,1,0,0,0,1,0,0,1,1, 0,0,1,1,0,1,1,1,0,1,1,1,1,1,1,1,
    0,0,0,1,0,0,1,1,0,1,1,1,1,1,1,1, 0,0,0,0,0,0,0,1,0,0,1,1,0,1,1,1,
    0,0,0,0,0,0,0,0,0,0,0,1,0,0,1,1, 0,0,1,1,0,1,1,1,1,1,1,1,1,1,1,1,
    0,0,0,0,0,0,0,1,0,1,1,1,1,1,1,1, 0,0,0,0,0,0,0,0,0,0,0,1,0,1,1,1,
    0,0,0,1,0,1,1,1,1,1,1,1,1,1,1,1, 0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
    0,0,0,0,1,1,1,1,1,1,1,1,1,1,1,1, 0,0,0,0,0,0,0,0,0,0,0,0,1,1,1,1,
    0,0,0,0,1,0,0,0,1,1,1,0,1,1,1,1, 0,1,1,1,0,0,0,1,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,1,0,0,0,1,1,1,0, 0,1,1,1,0,0,1,1,0,0,0,1,0,0,0,0,
    0,0,1,1,0,0,0,1,0,0,0,0,0,0,0,0, 0,0,0,0,1,0,0,0,1,1,0,0,1,1,1,0,
    0,0,0,0,0,0,0,0,1,0,0,0,1,1,0,0, 0,1,1,1,0,0,1,1,0,0,1,1,0,0,0,1,
    0,0,1,1,0,0,0,1,0,0,0,1,0,0,0,0, 0,0,0,0,1,0,0,0,1,0,0,0,1,1,0,0,
    0,1,1,0,0,1,1,0,0,1,1,0,0,1,1,0, 0,0,1,1,0,1,1,0,0,1,1,0,1,1,0,0,
    0,0,0,1,0,1,1,1,1,1,1,0,1,0,0,0, 0,0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,
    0,1,1,1,0,0,0,1,1,0,0,0,1,1,1,0, 0,0,1,1,1,0,0,1,1,0,0,1,1,1,0,0,
    0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1, 0,0,0,0,1,1,1,1,0,0,0,0,1,1,1,1,
    0,1,0,1,1,0,1,0,0,1,0,1,1,0,1,0, 0,0,1,1,0,0,1,1,1,1,0,0,1,1,0,0,
    0,0,1,1,1,1,0,0,0,0,1,1,1,1,0,0, 0,1,0,1,0,1,0,1,1,0,1,0,1,0,1,0,
    0,1,1,0,1,0,0,1,0,1,1,0,1,0,0,1, 0,1,0,1,1,0,1,0,1,0,1,0,0,1,0,1,
    0,1,1,1,0,0,1,1,1,1,0,0,1,1,1,0, 0,0,0,1,0,0,1,1,1,1,0,0,1,0,0,0,
    0,0,1,1,0,0,1,0,0,1,0,0,1,1,0,0, 0,0,1,1,1,0,1,1,1,1,0,1,1,1,0,0,
    0,1,1,0,1,0,0,1,1,0,0,1,0,1,1,0, 0,0,1,1,1,1,0,0,1,1,0,0,0,0,1,1,
    0,1,1,0,0,1,1,0,1,0,0,1,1,0,0,1, 0,0,0,0,0,1,1,0,0,1,1,0,0,0,0,0,
    0,1,0,0,1,1,1,0,0,1,0,0,0,0,0,0, 0,0,1,0,0,1,1,1,0,0,1,0,0,0,0,0,
    0,0,0,0,0,0,1,0,0,1,1,1,0,0,1,0, 0,0,0,0,0,1,0,0,1,1,1,0,0,1,0,0,
    0,1,1,0,1,1,0,0,1,0,0,1,0,0,1,1, 0,0,1,1,0,1,1,0,1,1,0,0,1,0,0,1,
    0,1,1,0,0,0,1,1,1,0,0,1,1,1,0,0, 0,0,1,1,1,0,0,1,1,1,0,0,0,1,1,0,
    0,1,1,0,1,1,0,0,1,1,0,0,1,0,0,1, 0,1,1,0,0,0,1,1,0,0,1,1,1,0,0,1,
    0,1,1,1,1,1,1,0,1,0,0,0,0,0,0,1, 0,0,0,1,1,0,0,0,1,1,1,0,0,1,1,1,
    0,0,0,0,1,1,1,1,0,0,1,1,0,0,1,1, 0,0,1,1,0,0,1,1,1,1,1,1,0,0,0,0,
    0,0,1,0,0,0,1,0,1,1,1,0,1,1,1,0, 0,1,0,0,0,1,0,0,0,1,1,1,0,1,1,1,
];
#[rustfmt::skip]
const G_PARTITION3: [u8; 64 * 16] = [
    0,0,1,1,0,0,1,1,0,2,2,1,2,2,2,2, 0,0,0,1,0,0,1,1,2,2,1,1,2,2,2,1,
    0,0,0,0,2,0,0,1,2,2,1,1,2,2,1,1, 0,2,2,2,0,0,2,2,0,0,1,1,0,1,1,1,
    0,0,0,0,0,0,0,0,1,1,2,2,1,1,2,2, 0,0,1,1,0,0,1,1,0,0,2,2,0,0,2,2,
    0,0,2,2,0,0,2,2,1,1,1,1,1,1,1,1, 0,0,1,1,0,0,1,1,2,2,1,1,2,2,1,1,
    0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2, 0,0,0,0,1,1,1,1,1,1,1,1,2,2,2,2,
    0,0,0,0,1,1,1,1,2,2,2,2,2,2,2,2, 0,0,1,2,0,0,1,2,0,0,1,2,0,0,1,2,
    0,1,1,2,0,1,1,2,0,1,1,2,0,1,1,2, 0,1,2,2,0,1,2,2,0,1,2,2,0,1,2,2,
    0,0,1,1,0,1,1,2,1,1,2,2,1,2,2,2, 0,0,1,1,2,0,0,1,2,2,0,0,2,2,2,0,
    0,0,0,1,0,0,1,1,0,1,1,2,1,1,2,2, 0,1,1,1,0,0,1,1,2,0,0,1,2,2,0,0,
    0,0,0,0,1,1,2,2,1,1,2,2,1,1,2,2, 0,0,2,2,0,0,2,2,0,0,2,2,1,1,1,1,
    0,1,1,1,0,1,1,1,0,2,2,2,0,2,2,2, 0,0,0,1,0,0,0,1,2,2,2,1,2,2,2,1,
    0,0,0,0,0,0,1,1,0,1,2,2,0,1,2,2, 0,0,0,0,1,1,0,0,2,2,1,0,2,2,1,0,
    0,1,2,2,0,1,2,2,0,0,1,1,0,0,0,0, 0,0,1,2,0,0,1,2,1,1,2,2,2,2,2,2,
    0,1,1,0,1,2,2,1,1,2,2,1,0,1,1,0, 0,0,0,0,0,1,1,0,1,2,2,1,1,2,2,1,
    0,0,2,2,1,1,0,2,1,1,0,2,0,0,2,2, 0,1,1,0,0,1,1,0,2,0,0,2,2,2,2,2,
    0,0,1,1,0,1,2,2,0,1,2,2,0,0,1,1, 0,0,0,0,2,0,0,0,2,2,1,1,2,2,2,1,
    0,0,0,0,0,0,0,2,1,1,2,2,1,2,2,2, 0,2,2,2,0,0,2,2,0,0,1,2,0,0,1,1,
    0,0,1,1,0,0,1,2,0,0,2,2,0,2,2,2, 0,1,2,0,0,1,2,0,0,1,2,0,0,1,2,0,
    0,0,0,0,1,1,1,1,2,2,2,2,0,0,0,0, 0,1,2,0,1,2,0,1,2,0,1,2,0,1,2,0,
    0,1,2,0,2,0,1,2,1,2,0,1,0,1,2,0, 0,0,1,1,2,2,0,0,1,1,2,2,0,0,1,1,
    0,0,1,1,1,1,2,2,2,2,0,0,0,0,1,1, 0,1,0,1,0,1,0,1,2,2,2,2,2,2,2,2,
    0,0,0,0,0,0,0,0,2,1,2,1,2,1,2,1, 0,0,2,2,1,1,2,2,0,0,2,2,1,1,2,2,
    0,0,2,2,0,0,1,1,0,0,2,2,0,0,1,1, 0,2,2,0,1,2,2,1,0,2,2,0,1,2,2,1,
    0,1,0,1,2,2,2,2,2,2,2,2,0,1,0,1, 0,0,0,0,2,1,2,1,2,1,2,1,2,1,2,1,
    0,1,0,1,0,1,0,1,0,1,0,1,2,2,2,2, 0,2,2,2,0,1,1,1,0,2,2,2,0,1,1,1,
    0,0,0,2,1,1,1,2,0,0,0,2,1,1,1,2, 0,0,0,0,2,1,1,2,2,1,1,2,2,1,1,2,
    0,2,2,2,0,1,1,1,0,1,1,1,0,2,2,2, 0,0,0,2,1,1,1,2,1,1,1,2,0,0,0,2,
    0,1,1,0,0,1,1,0,0,1,1,0,2,2,2,2, 0,0,0,0,0,0,0,0,2,1,1,2,2,1,1,2,
    0,1,1,0,0,1,1,0,2,2,2,2,2,2,2,2, 0,0,2,2,0,0,1,1,0,0,1,1,0,0,2,2,
    0,0,2,2,1,1,2,2,1,1,2,2,0,0,2,2, 0,0,0,0,0,0,0,0,0,0,0,0,2,1,1,2,
    0,0,0,2,0,0,0,1,0,0,0,2,0,0,0,1, 0,2,2,2,1,2,2,2,0,2,2,2,1,2,2,2,
    0,1,0,1,2,2,2,2,2,2,2,2,2,2,2,2, 0,1,1,1,2,0,1,1,2,2,0,1,2,2,2,0,
];
#[rustfmt::skip]
const G_ANCHOR_2ND: [i32; 64] = [15,15,15,15,15,15,15,15,15,15,15,15,15,15,15,15,15,2,8,2,2,8,8,15,2,8,2,2,8,8,2,2,15,15,6,8,2,8,15,15,2,8,2,2,2,15,15,6,6,2,6,8,15,15,2,2,15,15,15,15,15,2,2,15];
#[rustfmt::skip]
const G_ANCHOR_3RD_1: [i32; 64] = [3,3,15,15,8,3,15,15,8,8,6,6,6,5,3,3,3,3,8,15,3,3,6,10,5,8,8,6,8,5,15,15,8,15,3,5,6,10,8,15,15,3,15,5,15,15,15,15,3,15,5,5,5,8,5,10,5,10,8,13,15,12,3,3];
#[rustfmt::skip]
const G_ANCHOR_3RD_2: [i32; 64] = [15,8,8,3,15,15,3,8,15,15,15,15,15,15,15,8,15,8,15,3,15,8,15,8,3,15,6,10,15,15,10,8,15,3,15,10,10,8,9,10,6,15,8,15,3,6,6,8,15,3,15,15,15,15,15,15,15,15,15,15,3,15,15,8];

const G_NUM_SUBSETS: [usize; 8] = [3, 2, 3, 2, 1, 1, 1, 2];
const G_PARTITION_BITS: [u32; 8] = [4, 6, 6, 6, 0, 0, 0, 6];
const G_COLOR_INDEX_BITCOUNT: [u32; 8] = [3, 3, 2, 2, 2, 2, 4, 2];
const G_ALPHA_INDEX_BITCOUNT: [i32; 8] = [0, 0, 0, 0, 3, 2, 4, 2];
const G_MODE_HAS_P_BITS: [i32; 8] = [1, 1, 0, 1, 0, 0, 1, 1];
const G_MODE_HAS_SHARED_P_BITS: [i32; 8] = [0, 1, 0, 0, 0, 0, 0, 0];
const G_COLOR_PRECISION_TABLE: [u32; 8] = [4, 6, 5, 7, 5, 7, 7, 5];
const G_ALPHA_PRECISION_TABLE: [u32; 8] = [0, 0, 0, 0, 6, 8, 7, 5];

const fn get_color_index_size(mode: usize, index_selector: u32) -> u32 {
    G_COLOR_INDEX_BITCOUNT[mode] + index_selector
}
const fn get_alpha_index_size(mode: usize, index_selector: u32) -> u32 {
    (G_ALPHA_INDEX_BITCOUNT[mode] - index_selector as i32) as u32
}
const fn mode_has_separate_alpha_selectors(mode: usize) -> bool {
    mode == 4 || mode == 5
}

const PR_WEIGHT: f32 = (0.5f32 / (1.0 - 0.2126)) * (0.5f32 / (1.0 - 0.2126));
const PB_WEIGHT: f32 = (0.5f32 / (1.0 - 0.0722)) * (0.5f32 / (1.0 - 0.0722));

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct EndpointErr {
    error: u16,
    lo: u8,
    hi: u8,
}

struct OptTables {
    mode0: [[[EndpointErr; 2]; 2]; 256],
    mode1: [[EndpointErr; 2]; 256],
    mode6: [[[EndpointErr; 2]; 2]; 256],
    mode7: [[[EndpointErr; 2]; 2]; 256],
    mode5: [u32; 256],
    mode4_3: [u32; 256],
    mode4_2: [u32; 256],
}

const MODE0_IDX: usize = 2;
const MODE1_IDX: usize = 2;
const MODE6_IDX: usize = 5;
const MODE5_IDX: usize = 1;
const MODE4_IDX3: usize = 2;
const MODE4_IDX2: usize = 1;
const MODE7_IDX: usize = 1;

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

fn build_opt_tables() -> Box<OptTables> {
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

#[cfg(test)]
fn build_opt_tables_reference() -> Box<OptTables> {
    let mut t = Box::new(OptTables {
        mode0: [[[EndpointErr::default(); 2]; 2]; 256],
        mode1: [[EndpointErr::default(); 2]; 256],
        mode6: [[[EndpointErr::default(); 2]; 2]; 256],
        mode7: [[[EndpointErr::default(); 2]; 2]; 256],
        mode5: [0u32; 256],
        mode4_3: [0u32; 256],
        mode4_2: [0u32; 256],
    });

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..16u32 {
                    let mut low = ((l << 1) | lp as u32) << 3;
                    low |= low >> 5;
                    for h in 0..16u32 {
                        let mut high = ((h << 1) | hp as u32) << 3;
                        high |= high >> 5;
                        let k = ((low * (64 - G_WEIGHTS3[MODE0_IDX])
                            + high * G_WEIGHTS3[MODE0_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode0[c as usize][hp][lp] = best;
            }
        }
    }

    for c in 0..256i32 {
        for lp in 0..2usize {
            let mut best = EndpointErr {
                error: u16::MAX,
                lo: 0,
                hi: 0,
            };
            for l in 0..64u32 {
                let mut low = ((l << 1) | lp as u32) << 1;
                low |= low >> 7;
                for h in 0..64u32 {
                    let mut high = ((h << 1) | lp as u32) << 1;
                    high |= high >> 7;
                    let k =
                        ((low * (64 - G_WEIGHTS3[MODE1_IDX]) + high * G_WEIGHTS3[MODE1_IDX] + 32)
                            >> 6) as i32;
                    let err = (k - c) * (k - c);
                    if err < best.error as i32 {
                        best.error = err as u16;
                        best.lo = l as u8;
                        best.hi = h as u8;
                    }
                }
            }
            t.mode1[c as usize][lp] = best;
        }
    }

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..128u32 {
                    let low = (l << 1) | lp as u32;
                    for h in 0..128u32 {
                        let high = (h << 1) | hp as u32;
                        let k = ((low * (64 - G_WEIGHTS4[MODE6_IDX])
                            + high * G_WEIGHTS4[MODE6_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode6[c as usize][hp][lp] = best;
            }
        }
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..128u32 {
            let mut low = l << 1;
            low |= low >> 7;
            for h in 0..128u32 {
                let mut high = h << 1;
                high |= high >> 7;
                let k = ((low * (64 - G_WEIGHTS2[MODE5_IDX]) + high * G_WEIGHTS2[MODE5_IDX] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode5[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..32u32 {
            let mut low = l << 3;
            low |= low >> 5;
            for h in 0..32u32 {
                let mut high = h << 3;
                high |= high >> 5;
                let k = ((low * (64 - G_WEIGHTS3[MODE4_IDX3]) + high * G_WEIGHTS3[MODE4_IDX3] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode4_3[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        let mut best = EndpointErr {
            error: u16::MAX,
            lo: 0,
            hi: 0,
        };
        for l in 0..32u32 {
            let mut low = l << 3;
            low |= low >> 5;
            for h in 0..32u32 {
                let mut high = h << 3;
                high |= high >> 5;
                let k = ((low * (64 - G_WEIGHTS2[MODE4_IDX2]) + high * G_WEIGHTS2[MODE4_IDX2] + 32)
                    >> 6) as i32;
                let err = (k - c) * (k - c);
                if err < best.error as i32 {
                    best.error = err as u16;
                    best.lo = l as u8;
                    best.hi = h as u8;
                }
            }
        }
        t.mode4_2[c as usize] = best.lo as u32 | ((best.hi as u32) << 8);
    }

    for c in 0..256i32 {
        for hp in 0..2usize {
            for lp in 0..2usize {
                let mut best = EndpointErr {
                    error: u16::MAX,
                    lo: 0,
                    hi: 0,
                };
                for l in 0..32u32 {
                    let mut low = ((l << 1) | lp as u32) << 2;
                    low |= low >> 6;
                    for h in 0..32u32 {
                        let mut high = ((h << 1) | hp as u32) << 2;
                        high |= high >> 6;
                        let k = ((low * (64 - G_WEIGHTS2[MODE7_IDX])
                            + high * G_WEIGHTS2[MODE7_IDX]
                            + 32)
                            >> 6) as i32;
                        let err = (k - c) * (k - c);
                        if err < best.error as i32 {
                            best.error = err as u16;
                            best.lo = l as u8;
                            best.hi = h as u8;
                        }
                    }
                }
                t.mode7[c as usize][hp][lp] = best;
            }
        }
    }
    t
}

use std::sync::OnceLock;
static OPT: OnceLock<Box<OptTables>> = OnceLock::new();
fn opt() -> &'static OptTables {
    OPT.get_or_init(build_opt_tables)
}

#[cfg(target_arch = "x86_64")]
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
#[cfg(target_arch = "x86_64")]
#[inline]
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| {
        if std::env::var_os("ABGEN_BC7_SCALAR").is_some() {
            return false;
        }
        std::is_x86_feature_detected!("avx2")
    })
}
#[cfg(not(target_arch = "x86_64"))]
#[inline]
fn has_avx2() -> bool {
    false
}

#[cfg(target_arch = "x86_64")]
static HAS_AVX512VL: OnceLock<bool> = OnceLock::new();
#[cfg(target_arch = "x86_64")]
#[inline]
fn has_avx512vl() -> bool {
    *HAS_AVX512VL.get_or_init(|| {
        if std::env::var_os("ABGEN_BC7_SCALAR").is_some()
            || std::env::var_os("ABGEN_BC7_NO512").is_some()
        {
            return false;
        }
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512vl")
    })
}
#[cfg(not(target_arch = "x86_64"))]
#[inline]
fn has_avx512vl() -> bool {
    false
}

#[derive(Clone, Copy, Default)]
struct ColorI {
    c: [i32; 4],
}
#[derive(Clone, Copy, Default)]
struct Vec4F {
    c: [f32; 4],
}

#[inline]
const fn saturate(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline]
fn vec4f_dot(a: &Vec4F, b: &Vec4F) -> f32 {
    a.c[0] * b.c[0] + a.c[1] * b.c[1] + a.c[2] * b.c[2] + a.c[3] * b.c[3]
}
#[inline]
fn vec4f_normalize(v: &mut Vec4F) {
    let mut s = v.c[0] * v.c[0] + v.c[1] * v.c[1] + v.c[2] * v.c[2] + v.c[3] * v.c[3];
    if s != 0.0 {
        s = 1.0 / s.sqrt();
        v.c[0] *= s;
        v.c[1] *= s;
        v.c[2] *= s;
        v.c[3] *= s;
    }
}

#[inline]
const fn iabs32(v: i32) -> i32 {
    v.abs()
}

#[inline]
const fn itrunc(f: f32) -> i32 {
    f as i32
}

#[derive(Clone)]
struct CCParams {
    num_selector_weights: u32,
    psel_weights: &'static [u32],
    psel_weightsx: &'static [[f32; 4]],
    comp_bits: u32,
    weights: [u32; 4],
    has_alpha: bool,
    has_pbits: bool,
    endpoints_share_pbit: bool,
    perceptual: bool,
}
impl CCParams {
    const fn clear() -> Self {
        CCParams {
            num_selector_weights: 0,
            psel_weights: &G_WEIGHTS2,
            psel_weightsx: &G_WEIGHTS2X,
            comp_bits: 0,
            weights: [1, 1, 1, 1],
            has_alpha: false,
            has_pbits: false,
            endpoints_share_pbit: false,
            perceptual: false,
        }
    }
}

#[derive(Clone)]
struct CCResults {
    best_overall_err: u64,
    low: ColorI,
    high: ColorI,
    pbits: [u32; 2],
    selectors: [i32; 16],
    selectors_temp: [i32; 16],
}
impl CCResults {
    fn new() -> Self {
        CCResults {
            best_overall_err: u64::MAX,
            low: ColorI::default(),
            high: ColorI::default(),
            pbits: [0, 0],
            selectors: [0; 16],
            selectors_temp: [0; 16],
        }
    }
}

#[inline]
fn scale_color(c: &ColorI, p: &CCParams) -> ColorI {
    let n = p.comp_bits + if p.has_pbits { 1 } else { 0 };
    let mut r = ColorI::default();
    for i in 0..4 {
        let mut v = (c.c[i] as u32) << (8 - n);
        v |= v >> n;
        r.c[i] = v as i32;
    }
    r
}

#[inline]
fn compute_color_distance_rgb(e1: &ColorI, e2: &ColorI, perceptual: bool, w: &[u32; 4]) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if !perceptual && has_avx2() {
            unsafe {
                return compute_color_distance_rgb_avx2(e1, e2, w);
            }
        }
    }
    compute_color_distance_rgb_scalar(e1, e2, perceptual, w)
}

#[inline]
fn compute_color_distance_rgb_scalar(
    e1: &ColorI,
    e2: &ColorI,
    perceptual: bool,
    w: &[u32; 4],
) -> u64 {
    if perceptual {
        let l1 = e1.c[0] as f32 * 0.2126 + e1.c[1] as f32 * 0.7152 + e1.c[2] as f32 * 0.0722;
        let cr1 = e1.c[0] as f32 - l1;
        let cb1 = e1.c[2] as f32 - l1;
        let l2 = e2.c[0] as f32 * 0.2126 + e2.c[1] as f32 * 0.7152 + e2.c[2] as f32 * 0.0722;
        let cr2 = e2.c[0] as f32 - l2;
        let cb2 = e2.c[2] as f32 - l2;
        let dl = l1 - l2;
        let dcr = cr1 - cr2;
        let dcb = cb1 - cb2;
        (w[0] as f32 * (dl * dl)
            + w[1] as f32 * PR_WEIGHT * (dcr * dcr)
            + w[2] as f32 * PB_WEIGHT * (dcb * dcb)) as i64 as u64
    } else {
        let dr = e1.c[0] as f32 - e2.c[0] as f32;
        let dg = e1.c[1] as f32 - e2.c[1] as f32;
        let db = e1.c[2] as f32 - e2.c[2] as f32;
        (w[0] as f32 * dr * dr + w[1] as f32 * dg * dg + w[2] as f32 * db * db) as i64 as u64
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_color_distance_rgb_avx2(e1: &ColorI, e2: &ColorI, w: &[u32; 4]) -> u64 {
    use std::arch::x86_64::*;
    let v1 = _mm_loadu_si128(e1.c.as_ptr() as *const __m128i);
    let v2 = _mm_loadu_si128(e2.c.as_ptr() as *const __m128i);
    let f1 = _mm_cvtepi32_ps(v1);
    let f2 = _mm_cvtepi32_ps(v2);
    let d = _mm_sub_ps(f1, f2);
    let d2 = _mm_mul_ps(d, d);
    let wi = _mm_loadu_si128(w.as_ptr() as *const __m128i);
    let wf = _mm_cvtepi32_ps(wi);
    let wd2 = _mm_mul_ps(wf, d2);

    let r = _mm_cvtss_f32(wd2);
    let g = _mm_cvtss_f32(_mm_shuffle_ps(wd2, wd2, 0b01_01_01_01));
    let b = _mm_cvtss_f32(_mm_shuffle_ps(wd2, wd2, 0b10_10_10_10));
    (r + g + b) as i64 as u64
}

#[inline]
fn compute_color_distance_rgba(e1: &ColorI, e2: &ColorI, perceptual: bool, w: &[u32; 4]) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if !perceptual && has_avx2() {
            unsafe {
                return compute_color_distance_rgba_avx2(e1, e2, w);
            }
        }
    }
    compute_color_distance_rgba_scalar(e1, e2, perceptual, w)
}

#[inline]
fn compute_color_distance_rgba_scalar(
    e1: &ColorI,
    e2: &ColorI,
    perceptual: bool,
    w: &[u32; 4],
) -> u64 {
    let da = e1.c[3] as f32 - e2.c[3] as f32;
    let a_err = w[3] as f32 * (da * da);
    if perceptual {
        let l1 = e1.c[0] as f32 * 0.2126 + e1.c[1] as f32 * 0.7152 + e1.c[2] as f32 * 0.0722;
        let cr1 = e1.c[0] as f32 - l1;
        let cb1 = e1.c[2] as f32 - l1;
        let l2 = e2.c[0] as f32 * 0.2126 + e2.c[1] as f32 * 0.7152 + e2.c[2] as f32 * 0.0722;
        let cr2 = e2.c[0] as f32 - l2;
        let cb2 = e2.c[2] as f32 - l2;
        let dl = l1 - l2;
        let dcr = cr1 - cr2;
        let dcb = cb1 - cb2;
        (w[0] as f32 * (dl * dl)
            + w[1] as f32 * PR_WEIGHT * (dcr * dcr)
            + w[2] as f32 * PB_WEIGHT * (dcb * dcb)
            + a_err) as i64 as u64
    } else {
        let dr = e1.c[0] as f32 - e2.c[0] as f32;
        let dg = e1.c[1] as f32 - e2.c[1] as f32;
        let db = e1.c[2] as f32 - e2.c[2] as f32;
        (w[0] as f32 * dr * dr + w[1] as f32 * dg * dg + w[2] as f32 * db * db + a_err) as i64
            as u64
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_color_distance_rgba_avx2(e1: &ColorI, e2: &ColorI, w: &[u32; 4]) -> u64 {
    use std::arch::x86_64::*;
    let v1 = _mm_loadu_si128(e1.c.as_ptr() as *const __m128i);
    let v2 = _mm_loadu_si128(e2.c.as_ptr() as *const __m128i);
    let f1 = _mm_cvtepi32_ps(v1);
    let f2 = _mm_cvtepi32_ps(v2);
    let d = _mm_sub_ps(f1, f2);
    let d2 = _mm_mul_ps(d, d);
    let wi = _mm_loadu_si128(w.as_ptr() as *const __m128i);
    let wf = _mm_cvtepi32_ps(wi);
    let wd2 = _mm_mul_ps(wf, d2);
    let r = _mm_cvtss_f32(wd2);
    let g = _mm_cvtss_f32(_mm_shuffle_ps(wd2, wd2, 0b01_01_01_01));
    let b = _mm_cvtss_f32(_mm_shuffle_ps(wd2, wd2, 0b10_10_10_10));
    let a = _mm_cvtss_f32(_mm_shuffle_ps(wd2, wd2, 0b11_11_11_11));
    (r + g + b + a) as i64 as u64
}

fn compute_lsq_endpoints_rgba(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_r, mut t_r) = (0f32, 0f32);
    let (mut q00_g, mut t_g) = (0f32, 0f32);
    let (mut q00_b, mut t_b) = (0f32, 0f32);
    let (mut q00_a, mut t_a) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_r += w * colors[i].c[0] as f32;
        t_r += colors[i].c[0] as f32;
        q00_g += w * colors[i].c[1] as f32;
        t_g += colors[i].c[1] as f32;
        q00_b += w * colors[i].c[2] as f32;
        t_b += colors[i].c[2] as f32;
        q00_a += w * colors[i].c[3] as f32;
        t_a += colors[i].c[3] as f32;
    }
    let q10_r = t_r - q00_r;
    let q10_g = t_g - q00_g;
    let q10_b = t_b - q00_b;
    let q10_a = t_a - q00_a;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    xl.c[0] = iz00 * q00_r + iz01 * q10_r;
    xh.c[0] = iz10 * q00_r + iz11 * q10_r;
    xl.c[1] = iz00 * q00_g + iz01 * q10_g;
    xh.c[1] = iz10 * q00_g + iz11 * q10_g;
    xl.c[2] = iz00 * q00_b + iz01 * q10_b;
    xh.c[2] = iz10 * q00_b + iz11 * q10_b;
    xl.c[3] = iz00 * q00_a + iz01 * q10_a;
    xh.c[3] = iz10 * q00_a + iz11 * q10_a;
}

fn compute_lsq_endpoints_rgb(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2() {
            unsafe {
                return compute_lsq_endpoints_rgb_avx2(n, sel, sw, xl, xh, colors);
            }
        }
    }
    compute_lsq_endpoints_rgb_scalar(n, sel, sw, xl, xh, colors)
}

fn compute_lsq_endpoints_rgb_scalar(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_r, mut t_r) = (0f32, 0f32);
    let (mut q00_g, mut t_g) = (0f32, 0f32);
    let (mut q00_b, mut t_b) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_r += w * colors[i].c[0] as f32;
        t_r += colors[i].c[0] as f32;
        q00_g += w * colors[i].c[1] as f32;
        t_g += colors[i].c[1] as f32;
        q00_b += w * colors[i].c[2] as f32;
        t_b += colors[i].c[2] as f32;
    }
    let q10_r = t_r - q00_r;
    let q10_g = t_g - q00_g;
    let q10_b = t_b - q00_b;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    xl.c[0] = iz00 * q00_r + iz01 * q10_r;
    xh.c[0] = iz10 * q00_r + iz11 * q10_r;
    xl.c[1] = iz00 * q00_g + iz01 * q10_g;
    xh.c[1] = iz10 * q00_g + iz11 * q10_g;
    xl.c[2] = iz00 * q00_b + iz01 * q10_b;
    xh.c[2] = iz10 * q00_b + iz11 * q10_b;
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_lsq_endpoints_rgb_avx2(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    use std::arch::x86_64::*;
    let mut z = _mm_setzero_ps();
    let mut q00 = _mm_setzero_ps();
    let mut t = _mm_setzero_ps();
    for i in 0..n {
        let s = sel[i] as usize;
        let sw_v = _mm_loadu_ps(sw[s].as_ptr());
        z = _mm_add_ps(z, sw_v);
        let w = _mm_shuffle_ps(sw_v, sw_v, 0b11_11_11_11);
        let ci = _mm_loadu_si128(colors[i].c.as_ptr() as *const __m128i);
        let cf = _mm_cvtepi32_ps(ci);
        q00 = _mm_add_ps(q00, _mm_mul_ps(w, cf));
        t = _mm_add_ps(t, cf);
    }
    let z00 = _mm_cvtss_f32(z);
    let z10 = _mm_cvtss_f32(_mm_shuffle_ps(z, z, 0b01_01_01_01));
    let z11 = _mm_cvtss_f32(_mm_shuffle_ps(z, z, 0b10_10_10_10));
    let q00_r = _mm_cvtss_f32(q00);
    let q00_g = _mm_cvtss_f32(_mm_shuffle_ps(q00, q00, 0b01_01_01_01));
    let q00_b = _mm_cvtss_f32(_mm_shuffle_ps(q00, q00, 0b10_10_10_10));
    let t_r = _mm_cvtss_f32(t);
    let t_g = _mm_cvtss_f32(_mm_shuffle_ps(t, t, 0b01_01_01_01));
    let t_b = _mm_cvtss_f32(_mm_shuffle_ps(t, t, 0b10_10_10_10));
    let q10_r = t_r - q00_r;
    let q10_g = t_g - q00_g;
    let q10_b = t_b - q00_b;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    xl.c[0] = iz00 * q00_r + iz01 * q10_r;
    xh.c[0] = iz10 * q00_r + iz11 * q10_r;
    xl.c[1] = iz00 * q00_g + iz01 * q10_g;
    xh.c[1] = iz10 * q00_g + iz11 * q10_g;
    xl.c[2] = iz00 * q00_b + iz01 * q10_b;
    xh.c[2] = iz10 * q00_b + iz11 * q10_b;
}

fn compute_lsq_endpoints_a(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut f32,
    xh: &mut f32,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_a, mut t_a) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_a += w * colors[i].c[3] as f32;
        t_a += colors[i].c[3] as f32;
    }
    let q10_a = t_a - q00_a;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    *xl = iz00 * q00_a + iz01 * q10_a;
    *xh = iz10 * q00_a + iz11 * q10_a;
}

fn pack_mode1_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..2 {
        let err =
            t.mode1[r][pp].error as u32 + t.mode1[g][pp].error as u32 + t.mode1[b][pp].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let er = &t.mode1[r][best_p];
    let eg = &t.mode1[g][best_p];
    let eb = &t.mode1[b][best_p];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, 0];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, 0];
    res.pbits = [best_p as u32, 0];
    for i in 0..num_pixels {
        res.selectors[i] = MODE1_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = ((res.low.c[i] as u32) << 1 | res.pbits[0]) << 1;
        low |= low >> 7;
        let mut high = ((res.high.c[i] as u32) << 1 | res.pbits[0]) << 1;
        high |= high >> 7;
        pc.c[i] =
            ((low * (64 - G_WEIGHTS3[MODE1_IDX]) + high * G_WEIGHTS3[MODE1_IDX] + 32) >> 6) as i32;
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

fn pack_mode24_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
    let (er, eg, eb) = if p.num_selector_weights == 8 {
        (t.mode4_3[r], t.mode4_3[g], t.mode4_3[b])
    } else {
        (t.mode4_2[r], t.mode4_2[g], t.mode4_2[b])
    };
    res.low.c = [
        (er & 0xFF) as i32,
        (eg & 0xFF) as i32,
        (eb & 0xFF) as i32,
        0,
    ];
    res.high.c = [(er >> 8) as i32, (eg >> 8) as i32, (eb >> 8) as i32, 0];
    let idx = if p.num_selector_weights == 8 {
        MODE4_IDX3
    } else {
        MODE4_IDX2
    };
    for i in 0..num_pixels {
        res.selectors[i] = idx as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = (res.low.c[i] as u32) << 3;
        low |= low >> 5;
        let mut high = (res.high.c[i] as u32) << 3;
        high |= high >> 5;
        pc.c[i] = if p.num_selector_weights == 8 {
            ((low * (64 - G_WEIGHTS3[MODE4_IDX3]) + high * G_WEIGHTS3[MODE4_IDX3] + 32) >> 6) as i32
        } else {
            ((low * (64 - G_WEIGHTS2[MODE4_IDX2]) + high * G_WEIGHTS2[MODE4_IDX2] + 32) >> 6) as i32
        };
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

fn pack_mode0_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let err = t.mode0[r][pp >> 1][pp & 1].error as u32
            + t.mode0[g][pp >> 1][pp & 1].error as u32
            + t.mode0[b][pp >> 1][pp & 1].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let er = &t.mode0[r][best_p >> 1][best_p & 1];
    let eg = &t.mode0[g][best_p >> 1][best_p & 1];
    let eb = &t.mode0[b][best_p >> 1][best_p & 1];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, 0];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, 0];
    res.pbits = [(best_p & 1) as u32, (best_p >> 1) as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE0_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = ((res.low.c[i] as u32) << 1 | res.pbits[0]) << 3;
        low |= low >> 5;
        let mut high = ((res.high.c[i] as u32) << 1 | res.pbits[1]) << 3;
        high |= high >> 5;
        pc.c[i] =
            ((low * (64 - G_WEIGHTS3[MODE0_IDX]) + high * G_WEIGHTS3[MODE0_IDX] + 32) >> 6) as i32;
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

fn pack_mode6_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let hi_p = pp >> 1;
        let lo_p = pp & 1;
        let err = t.mode6[r][hi_p][lo_p].error as u32
            + t.mode6[g][hi_p][lo_p].error as u32
            + t.mode6[b][hi_p][lo_p].error as u32
            + t.mode6[a][hi_p][lo_p].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let best_hi = best_p >> 1;
    let best_lo = best_p & 1;
    let er = &t.mode6[r][best_hi][best_lo];
    let eg = &t.mode6[g][best_hi][best_lo];
    let eb = &t.mode6[b][best_hi][best_lo];
    let ea = &t.mode6[a][best_hi][best_lo];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, ea.lo as i32];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, ea.hi as i32];
    res.pbits = [best_lo as u32, best_hi as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE6_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..4 {
        let low = (res.low.c[i] as u32) << 1 | res.pbits[0];
        let high = (res.high.c[i] as u32) << 1 | res.pbits[1];
        pc.c[i] =
            ((low * (64 - G_WEIGHTS4[MODE6_IDX]) + high * G_WEIGHTS4[MODE6_IDX] + 32) >> 6) as i32;
    }
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgba(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

fn pack_mode7_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let hi_p = pp >> 1;
        let lo_p = pp & 1;
        let err = t.mode7[r][hi_p][lo_p].error as u32
            + t.mode7[g][hi_p][lo_p].error as u32
            + t.mode7[b][hi_p][lo_p].error as u32
            + t.mode7[a][hi_p][lo_p].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let best_hi = best_p >> 1;
    let best_lo = best_p & 1;
    let er = &t.mode7[r][best_hi][best_lo];
    let eg = &t.mode7[g][best_hi][best_lo];
    let eb = &t.mode7[b][best_hi][best_lo];
    let ea = &t.mode7[a][best_hi][best_lo];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, ea.lo as i32];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, ea.hi as i32];
    res.pbits = [best_lo as u32, best_hi as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE7_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..4 {
        let low = (res.low.c[i] as u32) << 1 | res.pbits[0];
        let high = (res.high.c[i] as u32) << 1 | res.pbits[1];
        pc.c[i] =
            ((low * (64 - G_WEIGHTS2[MODE7_IDX]) + high * G_WEIGHTS2[MODE7_IDX] + 32) >> 6) as i32;
    }
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgba(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn build_wc_table_sse(wc: &mut [[f32; 4]; 16], psel: &[u32], n: usize, nc: usize) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_ps(wc[0].as_ptr());
    let b = _mm_loadu_ps(wc[n - 1].as_ptr());
    let v64 = _mm_set1_ps(64.0);
    let v32 = _mm_set1_ps(32.0);
    let inv64 = _mm_set1_ps(1.0 / 64.0);
    for i in 1..(n - 1) {
        let wv = _mm_set1_ps(psel[i] as f32);
        let iwv = _mm_sub_ps(v64, wv);
        let t = _mm_add_ps(_mm_add_ps(_mm_mul_ps(a, iwv), _mm_mul_ps(b, wv)), v32);
        let t = _mm_floor_ps(_mm_mul_ps(t, inv64));

        let t = if nc == 3 {
            _mm_insert_ps::<0b0000_1000>(t, t)
        } else {
            t
        };
        _mm_storeu_ps(wc[i].as_mut_ptr(), t);
    }
}

fn evaluate_solution(
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

    #[allow(unused_mut)]
    let mut wc_built = false;
    #[cfg(target_arch = "x86_64")]
    if has_avx2() {
        unsafe { build_wc_table_sse(&mut wc, p.psel_weights, n, nc) };
        wc_built = true;
    }
    if !wc_built {
        for i in 1..(n - 1) {
            for j in 0..nc {
                wc[i][j] = ((wc[0][j] * (64.0 - p.psel_weights[i] as f32)
                    + wc[n - 1][j] * p.psel_weights[i] as f32
                    + 32.0)
                    * (1.0 / 64.0))
                    .floor();
            }
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
                #[cfg(target_arch = "x86_64")]
                {
                    if has_avx2() {
                        unsafe {
                            total_errf = eval_solution_n16_rgb_avx2(
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
                        }
                    } else {
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
                    }
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
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
                }
            } else if has_avx2() && (n == 4 || n == 8) {
                #[cfg(target_arch = "x86_64")]
                {
                    total_errf = unsafe {
                        eval_discrete_avx2(
                            num_pixels,
                            pixels,
                            &wc,
                            wr,
                            wg,
                            wb,
                            wa,
                            false,
                            n,
                            &mut res.selectors_temp,
                        )
                    };
                }
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
            } else if has_avx2() && (n == 4 || n == 8) {
                #[cfg(target_arch = "x86_64")]
                {
                    total_errf = unsafe {
                        eval_discrete_avx2(
                            num_pixels,
                            pixels,
                            &wc,
                            wr,
                            wg,
                            wb,
                            wa,
                            true,
                            n,
                            &mut res.selectors_temp,
                        )
                    };
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
        #[cfg(target_arch = "x86_64")]
        let simd_done = if has_avx2() && (n == 4 || n == 8 || n == 16) {
            let mut wa4 = [0f32; 16];
            if p.has_alpha {
                for i in 0..16 {
                    wa4[i] = wc[i][3];
                }
            }
            total_errf = unsafe {
                eval_perceptual_avx2(
                    num_pixels,
                    pixels,
                    &wy,
                    &wcr,
                    &wcb,
                    &wa4,
                    wr,
                    wgp,
                    wbp,
                    wa,
                    p.has_alpha,
                    n,
                    &mut res.selectors_temp,
                )
            };
            true
        } else {
            false
        };
        #[cfg(not(target_arch = "x86_64"))]
        let simd_done = false;
        if simd_done {
        } else if p.has_alpha {
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
fn eval_solution_n16_rgb_scalar(
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

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn eval_solution_n16_rgb_avx2(
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
    use std::arch::x86_64::*;
    let w_v = _mm_setr_ps(wr, wg, wb, 0.0);
    let mut total_errf = 0f32;
    let n_minus_1 = (n - 1) as f32;
    for i in 0..num_pixels {
        let r = pixels[i].c[0] as f32;
        let g = pixels[i].c[1] as f32;
        let b = pixels[i].c[2] as f32;
        let mut best_sel = ((((r * dr + lr) + (g * dg + lg) + (b * db + lb)) * f) + 0.5).floor();
        best_sel = best_sel.clamp(1.0, n_minus_1);
        let best_sel0 = best_sel - 1.0;
        let i0 = best_sel0 as i32 as usize;
        let i1 = best_sel as i32 as usize;

        let pi = _mm_loadu_si128(pixels[i].c.as_ptr() as *const __m128i);
        let pi_v = _mm_cvtepi32_ps(pi);
        let wc0_v = _mm_loadu_ps(wc[i0].as_ptr());
        let wc1_v = _mm_loadu_ps(wc[i1].as_ptr());
        let d0_v = _mm_sub_ps(wc0_v, pi_v);
        let d1_v = _mm_sub_ps(wc1_v, pi_v);
        let t0 = _mm_mul_ps(_mm_mul_ps(w_v, d0_v), d0_v);
        let t1 = _mm_mul_ps(_mm_mul_ps(w_v, d1_v), d1_v);
        let r0 = _mm_cvtss_f32(t0);
        let g0 = _mm_cvtss_f32(_mm_shuffle_ps(t0, t0, 0b01_01_01_01));
        let b0 = _mm_cvtss_f32(_mm_shuffle_ps(t0, t0, 0b10_10_10_10));
        let err0 = r0 + g0 + b0;
        let r1 = _mm_cvtss_f32(t1);
        let g1 = _mm_cvtss_f32(_mm_shuffle_ps(t1, t1, 0b01_01_01_01));
        let b1 = _mm_cvtss_f32(_mm_shuffle_ps(t1, t1, 0b10_10_10_10));
        let err1 = r1 + g1 + b1;
        let min_err = err0.min(err1);
        total_errf += min_err;
        selectors_temp[i] = if min_err == err0 { best_sel0 } else { best_sel } as i32;
    }
    total_errf
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn eval_discrete_avx2(
    num_pixels: usize,
    pixels: &[ColorI],
    wc: &[[f32; 4]; 16],
    wr: f32,
    wg: f32,
    wb: f32,
    wa: f32,
    has_alpha: bool,
    n: usize,
    selectors_temp: &mut [i32; 16],
) -> f32 {
    use std::arch::x86_64::*;

    let mut wc0 = [0f32; 8];
    let mut wc1 = [0f32; 8];
    let mut wc2 = [0f32; 8];
    let mut wc3 = [0f32; 8];
    for k in 0..n {
        wc0[k] = wc[k][0];
        wc1[k] = wc[k][1];
        wc2[k] = wc[k][2];
        wc3[k] = wc[k][3];
    }
    let mut total_errf = 0f32;
    if n == 4 {
        let w0 = _mm_loadu_ps(wc0.as_ptr());
        let w1 = _mm_loadu_ps(wc1.as_ptr());
        let w2 = _mm_loadu_ps(wc2.as_ptr());
        let w3 = _mm_loadu_ps(wc3.as_ptr());
        let wrv = _mm_set1_ps(wr);
        let wgv = _mm_set1_ps(wg);
        let wbv = _mm_set1_ps(wb);
        let wav = _mm_set1_ps(wa);
        for i in 0..num_pixels {
            let d0 = _mm_sub_ps(w0, _mm_set1_ps(pixels[i].c[0] as f32));
            let d1 = _mm_sub_ps(w1, _mm_set1_ps(pixels[i].c[1] as f32));
            let d2 = _mm_sub_ps(w2, _mm_set1_ps(pixels[i].c[2] as f32));

            let mut err = _mm_add_ps(
                _mm_add_ps(
                    _mm_mul_ps(_mm_mul_ps(wrv, d0), d0),
                    _mm_mul_ps(_mm_mul_ps(wgv, d1), d1),
                ),
                _mm_mul_ps(_mm_mul_ps(wbv, d2), d2),
            );
            if has_alpha {
                let d3 = _mm_sub_ps(w3, _mm_set1_ps(pixels[i].c[3] as f32));
                err = _mm_add_ps(err, _mm_mul_ps(_mm_mul_ps(wav, d3), d3));
            }
            let m = _mm_min_ps(err, _mm_movehl_ps(err, err));
            let m = _mm_min_ss(m, _mm_shuffle_ps(m, m, 1));
            let best = _mm_cvtss_f32(m);
            let eq = _mm_movemask_ps(_mm_cmpeq_ps(err, _mm_set1_ps(best))) as u32;
            total_errf += best;
            selectors_temp[i] = (31 - eq.leading_zeros()) as i32;
        }
    } else {
        let w0 = _mm256_loadu_ps(wc0.as_ptr());
        let w1 = _mm256_loadu_ps(wc1.as_ptr());
        let w2 = _mm256_loadu_ps(wc2.as_ptr());
        let w3 = _mm256_loadu_ps(wc3.as_ptr());
        let wrv = _mm256_set1_ps(wr);
        let wgv = _mm256_set1_ps(wg);
        let wbv = _mm256_set1_ps(wb);
        let wav = _mm256_set1_ps(wa);
        for i in 0..num_pixels {
            let d0 = _mm256_sub_ps(w0, _mm256_set1_ps(pixels[i].c[0] as f32));
            let d1 = _mm256_sub_ps(w1, _mm256_set1_ps(pixels[i].c[1] as f32));
            let d2 = _mm256_sub_ps(w2, _mm256_set1_ps(pixels[i].c[2] as f32));
            let mut err = _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(wrv, d0), d0),
                    _mm256_mul_ps(_mm256_mul_ps(wgv, d1), d1),
                ),
                _mm256_mul_ps(_mm256_mul_ps(wbv, d2), d2),
            );
            if has_alpha {
                let d3 = _mm256_sub_ps(w3, _mm256_set1_ps(pixels[i].c[3] as f32));
                err = _mm256_add_ps(err, _mm256_mul_ps(_mm256_mul_ps(wav, d3), d3));
            }
            let best = hmin_ps256(err);
            let eq =
                _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_EQ_OQ>(err, _mm256_set1_ps(best))) as u32;
            total_errf += best;
            selectors_temp[i] = (31 - eq.leading_zeros()) as i32;
        }
    }
    total_errf
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn eval_perceptual_avx2(
    num_pixels: usize,
    pixels: &[ColorI],
    wy: &[f32; 16],
    wcr: &[f32; 16],
    wcb: &[f32; 16],
    wa4: &[f32; 16],
    wr: f32,
    wgp: f32,
    wbp: f32,
    wa: f32,
    has_alpha: bool,
    n: usize,
    selectors_temp: &mut [i32; 16],
) -> f32 {
    use std::arch::x86_64::*;
    let mut total_errf = 0f32;
    if n == 4 {
        let wyv = _mm_loadu_ps(wy.as_ptr());
        let wcrv = _mm_loadu_ps(wcr.as_ptr());
        let wcbv = _mm_loadu_ps(wcb.as_ptr());
        let wav4 = _mm_loadu_ps(wa4.as_ptr());
        let wrv = _mm_set1_ps(wr);
        let wgpv = _mm_set1_ps(wgp);
        let wbpv = _mm_set1_ps(wbp);
        let wav = _mm_set1_ps(wa);
        for i in 0..num_pixels {
            let r = pixels[i].c[0] as f32;
            let g = pixels[i].c[1] as f32;
            let b = pixels[i].c[2] as f32;
            let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
            let cr = r - y;
            let cb = b - y;
            let dl = _mm_sub_ps(_mm_set1_ps(y), wyv);
            let dcr = _mm_sub_ps(_mm_set1_ps(cr), wcrv);
            let dcb = _mm_sub_ps(_mm_set1_ps(cb), wcbv);

            let mut err = _mm_add_ps(
                _mm_add_ps(
                    _mm_mul_ps(_mm_mul_ps(wrv, dl), dl),
                    _mm_mul_ps(_mm_mul_ps(wgpv, dcr), dcr),
                ),
                _mm_mul_ps(_mm_mul_ps(wbpv, dcb), dcb),
            );
            if has_alpha {
                let a = pixels[i].c[3] as f32;
                let da = _mm_sub_ps(_mm_set1_ps(a), wav4);
                err = _mm_add_ps(err, _mm_mul_ps(_mm_mul_ps(wav, da), da));
            }
            let m = _mm_min_ps(err, _mm_movehl_ps(err, err));
            let m = _mm_min_ss(m, _mm_shuffle_ps(m, m, 1));
            let best = _mm_cvtss_f32(m);
            let eq = _mm_movemask_ps(_mm_cmpeq_ps(err, _mm_set1_ps(best)));
            total_errf += best;
            selectors_temp[i] = eq.trailing_zeros() as i32;
        }
    } else {
        let rows = n / 8;
        let mut wyv = [_mm256_setzero_ps(); 2];
        let mut wcrv = [_mm256_setzero_ps(); 2];
        let mut wcbv = [_mm256_setzero_ps(); 2];
        let mut wav4 = [_mm256_setzero_ps(); 2];
        for rrow in 0..rows {
            wyv[rrow] = _mm256_loadu_ps(wy.as_ptr().add(rrow * 8));
            wcrv[rrow] = _mm256_loadu_ps(wcr.as_ptr().add(rrow * 8));
            wcbv[rrow] = _mm256_loadu_ps(wcb.as_ptr().add(rrow * 8));
            wav4[rrow] = _mm256_loadu_ps(wa4.as_ptr().add(rrow * 8));
        }
        let wrv = _mm256_set1_ps(wr);
        let wgpv = _mm256_set1_ps(wgp);
        let wbpv = _mm256_set1_ps(wbp);
        let wav = _mm256_set1_ps(wa);
        for i in 0..num_pixels {
            let r = pixels[i].c[0] as f32;
            let g = pixels[i].c[1] as f32;
            let b = pixels[i].c[2] as f32;
            let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
            let cr = r - y;
            let cb = b - y;
            let yv = _mm256_set1_ps(y);
            let crv = _mm256_set1_ps(cr);
            let cbv = _mm256_set1_ps(cb);
            let mut errs = [_mm256_setzero_ps(); 2];
            for rrow in 0..rows {
                let dl = _mm256_sub_ps(yv, wyv[rrow]);
                let dcr = _mm256_sub_ps(crv, wcrv[rrow]);
                let dcb = _mm256_sub_ps(cbv, wcbv[rrow]);
                let mut err = _mm256_add_ps(
                    _mm256_add_ps(
                        _mm256_mul_ps(_mm256_mul_ps(wrv, dl), dl),
                        _mm256_mul_ps(_mm256_mul_ps(wgpv, dcr), dcr),
                    ),
                    _mm256_mul_ps(_mm256_mul_ps(wbpv, dcb), dcb),
                );
                if has_alpha {
                    let a = pixels[i].c[3] as f32;
                    let da = _mm256_sub_ps(_mm256_set1_ps(a), wav4[rrow]);
                    err = _mm256_add_ps(err, _mm256_mul_ps(_mm256_mul_ps(wav, da), da));
                }
                errs[rrow] = err;
            }
            let combined = if rows == 2 {
                _mm256_min_ps(errs[0], errs[1])
            } else {
                errs[0]
            };
            let best = hmin_ps256(combined);
            let bv = _mm256_set1_ps(best);
            let mut mask = _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_EQ_OQ>(errs[0], bv)) as u32;
            if rows == 2 {
                mask |= (_mm256_movemask_ps(_mm256_cmp_ps::<_CMP_EQ_OQ>(errs[1], bv)) as u32) << 8;
            }
            total_errf += best;
            selectors_temp[i] = mask.trailing_zeros() as i32;
        }
    }
    total_errf
}

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

fn color_cell_compression(
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

#[derive(Clone, Copy)]
struct SubsetIdx {
    idx: [[i32; 16]; 3],
    total: [usize; 3],
}

fn subset_idx_tables(total_subsets: usize) -> &'static [SubsetIdx; 64] {
    static T2: OnceLock<Box<[SubsetIdx; 64]>> = OnceLock::new();
    static T3: OnceLock<Box<[SubsetIdx; 64]>> = OnceLock::new();
    let build = |table: &'static [u8; 64 * 16]| -> Box<[SubsetIdx; 64]> {
        let mut out = Box::new(
            [SubsetIdx {
                idx: [[0i32; 16]; 3],
                total: [0usize; 3],
            }; 64],
        );
        for partition in 0..64 {
            let part = &table[partition * 16..partition * 16 + 16];
            let e = &mut out[partition];
            for (index, &pp) in part.iter().enumerate() {
                let pp = pp as usize;
                e.idx[pp][e.total[pp]] = index as i32;
                e.total[pp] += 1;
            }
        }
        out
    };
    if total_subsets == 3 {
        T3.get_or_init(|| build(&G_PARTITION3))
    } else {
        T2.get_or_init(|| build(&G_PARTITION2))
    }
}

fn ccc_est_idx(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2() {
            unsafe {
                return ccc_est_idx_avx2(mode, p, idxs, num_pixels, pixels);
            }
        }
    }
    ccc_est_idx_scalar(mode, p, idxs, num_pixels, pixels)
}

fn ccc_est_idx_scalar(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb) = (255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb) = (0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
    }
    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = (n as f32 - 1.0) / (high - low);
    let inv_n = 1.0 / (n as f32 - 1.0);
    let mut total_errf = 0f32;
    if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += dr * dr + dg * dg + db * db;
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn hmin_ps256(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let lo = _mm256_castps256_ps128(v);
    let hi = _mm256_extractf128_ps::<1>(v);
    let m = _mm_min_ps(lo, hi);
    let m = _mm_min_ps(m, _mm_movehl_ps(m, m));
    let m = _mm_min_ss(m, _mm_shuffle_ps(m, m, 1));
    _mm_cvtss_f32(m)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn hmax_ps256(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let lo = _mm256_castps256_ps128(v);
    let hi = _mm256_extractf128_ps::<1>(v);
    let m = _mm_max_ps(lo, hi);
    let m = _mm_max_ps(m, _mm_movehl_ps(m, m));
    let m = _mm_max_ss(m, _mm_shuffle_ps(m, m, 1));
    _mm_cvtss_f32(m)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn ccc_est_idx_avx2(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return ccc_est_idx_scalar(mode, p, idxs, num_pixels, pixels);
    }
    let base = pixels.as_ptr() as *const i32;
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        let idx = _mm256_slli_epi32::<2>(pix);
        rv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base, idx));
        gv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(1), idx));
        bv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(2), idx));

        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));

        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);

    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = (n as f32 - 1.0) / (high - low);
    let inv_n = 1.0 / (n as f32 - 1.0);

    let (wr, wg, wb) = if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
        )
    } else {
        (1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
            _mm256_mul_ps(fabv, bv[c]),
        );

        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);

        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let itr = _mm256_add_ps(srv, _mm256_mul_ps(dirv, s));
        let itg = _mm256_add_ps(sgv, _mm256_mul_ps(digv, s));
        let itb = _mm256_add_ps(sbv, _mm256_mul_ps(dibv, s));
        let dr = _mm256_sub_ps(itr, rv[c]);
        let dg = _mm256_sub_ps(itg, gv[c]);
        let db = _mm256_sub_ps(itb, bv[c]);

        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        for &t in &t_arr[..cnt] {
            total_errf += t;
        }
    }
    total_errf as i64 as u64
}

fn ccc_est_mode7_idx(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2() {
            unsafe {
                return ccc_est_mode7_idx_avx2(p, idxs, num_pixels, pixels);
            }
        }
    }
    ccc_est_mode7_idx_scalar(p, idxs, num_pixels, pixels)
}

fn ccc_est_mode7_idx_scalar(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb, mut la) = (255f32, 255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb, mut ha) = (0f32, 0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        let a = px.c[3] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        la = la.min(a);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
        ha = ha.max(a);
    }
    let n = 4f32;
    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = (n - 1.0) / (high - low);
    let inv_n = 1.0 / (n - 1.0);
    let mut total_errf = 0f32;
    if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        let wa = p.weights[3] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db + wa * da * da;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += dr * dr + dg * dg + db * db + da * da;
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn ccc_est_mode7_idx_avx2(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return ccc_est_mode7_idx_scalar(p, idxs, num_pixels, pixels);
    }
    let base = pixels.as_ptr() as *const i32;
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];
    let mut av = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        let idx = _mm256_slli_epi32::<2>(pix);
        rv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base, idx));
        gv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(1), idx));
        bv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(2), idx));
        av[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(3), idx));
        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));
        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        mina = _mm256_min_ps(mina, _mm256_blendv_ps(v255, av[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
        maxa = _mm256_max_ps(maxa, _mm256_blendv_ps(v0, av[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let la = hmin_ps256(mina);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);
    let ha = hmax_ps256(maxa);

    let n = 4f32;
    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = (n - 1.0) / (high - low);
    let inv_n = 1.0 / (n - 1.0);

    let (wr, wg, wb, wa) = if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
            p.weights[3] as f32,
        )
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let faav = _mm256_set1_ps(faa);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let sav = _mm256_set1_ps(sa);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let diav = _mm256_set1_ps(dia);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);
    let wav = _mm256_set1_ps(wa);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
                _mm256_mul_ps(fabv, bv[c]),
            ),
            _mm256_mul_ps(faav, av[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let dr = _mm256_sub_ps(_mm256_add_ps(srv, _mm256_mul_ps(dirv, s)), rv[c]);
        let dg = _mm256_sub_ps(_mm256_add_ps(sgv, _mm256_mul_ps(digv, s)), gv[c]);
        let db = _mm256_sub_ps(_mm256_add_ps(sbv, _mm256_mul_ps(dibv, s)), bv[c]);
        let da = _mm256_sub_ps(_mm256_add_ps(sav, _mm256_mul_ps(diav, s)), av[c]);

        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                    _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
                ),
                _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wav, da), da),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        for &t in &t_arr[..cnt] {
            total_errf += t;
        }
    }
    total_errf as i64 as u64
}

struct LaneF32 {
    r: [f32; 16],
    g: [f32; 16],
    b: [f32; 16],
    a: [f32; 16],
}

impl LaneF32 {
    fn new(pixels: &[ColorI; 16]) -> Self {
        let mut l = LaneF32 {
            r: [0.0; 16],
            g: [0.0; 16],
            b: [0.0; 16],
            a: [0.0; 16],
        };
        for i in 0..16 {
            l.r[i] = pixels[i].c[0] as f32;
            l.g[i] = pixels[i].c[1] as f32;
            l.b[i] = pixels[i].c[2] as f32;
            l.a[i] = pixels[i].c[3] as f32;
        }
        l
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn ccc_est_idx_vperm(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let r0 = _mm256_loadu_ps(lf.r.as_ptr());
    let r1 = _mm256_loadu_ps(lf.r.as_ptr().add(8));
    let g0 = _mm256_loadu_ps(lf.g.as_ptr());
    let g1 = _mm256_loadu_ps(lf.g.as_ptr().add(8));
    let b0 = _mm256_loadu_ps(lf.b.as_ptr());
    let b1 = _mm256_loadu_ps(lf.b.as_ptr().add(8));
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(r0, pix, r1);
        gv[c] = _mm256_permutex2var_ps(g0, pix, g1);
        bv[c] = _mm256_permutex2var_ps(b0, pix, b1);
        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));
        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);

    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = (n as f32 - 1.0) / (high - low);
    let inv_n = 1.0 / (n as f32 - 1.0);

    let (wr, wg, wb) = if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
        )
    } else {
        (1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
            _mm256_mul_ps(fabv, bv[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let itr = _mm256_add_ps(srv, _mm256_mul_ps(dirv, s));
        let itg = _mm256_add_ps(sgv, _mm256_mul_ps(digv, s));
        let itb = _mm256_add_ps(sbv, _mm256_mul_ps(dibv, s));
        let dr = _mm256_sub_ps(itr, rv[c]);
        let dg = _mm256_sub_ps(itg, gv[c]);
        let db = _mm256_sub_ps(itb, bv[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        if cnt == 8 {
            total_errf += t_arr[0];
            total_errf += t_arr[1];
            total_errf += t_arr[2];
            total_errf += t_arr[3];
            total_errf += t_arr[4];
            total_errf += t_arr[5];
            total_errf += t_arr[6];
            total_errf += t_arr[7];
        } else {
            for &t in &t_arr[..cnt] {
                total_errf += t;
            }
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn ccc_est_mode7_idx_vperm(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let r0 = _mm256_loadu_ps(lf.r.as_ptr());
    let r1 = _mm256_loadu_ps(lf.r.as_ptr().add(8));
    let g0 = _mm256_loadu_ps(lf.g.as_ptr());
    let g1 = _mm256_loadu_ps(lf.g.as_ptr().add(8));
    let b0 = _mm256_loadu_ps(lf.b.as_ptr());
    let b1 = _mm256_loadu_ps(lf.b.as_ptr().add(8));
    let a0 = _mm256_loadu_ps(lf.a.as_ptr());
    let a1 = _mm256_loadu_ps(lf.a.as_ptr().add(8));
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];
    let mut av = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(r0, pix, r1);
        gv[c] = _mm256_permutex2var_ps(g0, pix, g1);
        bv[c] = _mm256_permutex2var_ps(b0, pix, b1);
        av[c] = _mm256_permutex2var_ps(a0, pix, a1);
        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));
        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        mina = _mm256_min_ps(mina, _mm256_blendv_ps(v255, av[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
        maxa = _mm256_max_ps(maxa, _mm256_blendv_ps(v0, av[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let la = hmin_ps256(mina);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);
    let ha = hmax_ps256(maxa);

    let n = 4f32;
    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = (n - 1.0) / (high - low);
    let inv_n = 1.0 / (n - 1.0);

    let (wr, wg, wb, wa) = if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
            p.weights[3] as f32,
        )
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let faav = _mm256_set1_ps(faa);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let sav = _mm256_set1_ps(sa);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let diav = _mm256_set1_ps(dia);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);
    let wav = _mm256_set1_ps(wa);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
                _mm256_mul_ps(fabv, bv[c]),
            ),
            _mm256_mul_ps(faav, av[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let dr = _mm256_sub_ps(_mm256_add_ps(srv, _mm256_mul_ps(dirv, s)), rv[c]);
        let dg = _mm256_sub_ps(_mm256_add_ps(sgv, _mm256_mul_ps(digv, s)), gv[c]);
        let db = _mm256_sub_ps(_mm256_add_ps(sbv, _mm256_mul_ps(dibv, s)), bv[c]);
        let da = _mm256_sub_ps(_mm256_add_ps(sav, _mm256_mul_ps(diav, s)), av[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                    _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
                ),
                _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wav, da), da),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        if cnt == 8 {
            total_errf += t_arr[0];
            total_errf += t_arr[1];
            total_errf += t_arr[2];
            total_errf += t_arr[3];
            total_errf += t_arr[4];
            total_errf += t_arr[5];
            total_errf += t_arr[6];
            total_errf += t_arr[7];
        } else {
            for &t in &t_arr[..cnt] {
                total_errf += t;
            }
        }
    }
    total_errf as i64 as u64
}

#[inline]
fn est_subset_err(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
    lf: Option<&LaneF32>,
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    if let Some(lf) = lf {
        unsafe {
            return if mode == 7 {
                ccc_est_mode7_idx_vperm(p, idxs, num_pixels, lf)
            } else {
                ccc_est_idx_vperm(mode, p, idxs, num_pixels, lf)
            };
        }
    }
    let _ = lf;
    if mode == 7 {
        ccc_est_mode7_idx(p, idxs, num_pixels, pixels)
    } else {
        ccc_est_idx(mode, p, idxs, num_pixels, pixels)
    }
}

fn lanes_f32_if_supported(lanes: &[&[ColorI; 16]]) -> Option<Vec<LaneF32>> {
    if has_avx512vl() && has_avx2() {
        Some(lanes.iter().map(|p| LaneF32::new(p)).collect())
    } else {
        None
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct EstPreRgb {
    r0: std::arch::x86_64::__m256,
    r1: std::arch::x86_64::__m256,
    g0: std::arch::x86_64::__m256,
    g1: std::arch::x86_64::__m256,
    b0: std::arch::x86_64::__m256,
    b1: std::arch::x86_64::__m256,
    v255: std::arch::x86_64::__m256,
    v0: std::arch::x86_64::__m256,
    wrv: std::arch::x86_64::__m256,
    wgv: std::arch::x86_64::__m256,
    wbv: std::arch::x86_64::__m256,
    invnv: std::arch::x86_64::__m256,
    halfv: std::arch::x86_64::__m256,
    onev: std::arch::x86_64::__m256,
    nm1: f32,
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_pre_rgb(mode: usize, p: &CCParams, lf: &LaneF32) -> EstPreRgb {
    use std::arch::x86_64::*;
    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let nm1 = n as f32 - 1.0;
    let inv_n = 1.0 / nm1;
    let (wr, wg, wb) = if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
        )
    } else {
        (1.0, 1.0, 1.0)
    };
    EstPreRgb {
        r0: _mm256_loadu_ps(lf.r.as_ptr()),
        r1: _mm256_loadu_ps(lf.r.as_ptr().add(8)),
        g0: _mm256_loadu_ps(lf.g.as_ptr()),
        g1: _mm256_loadu_ps(lf.g.as_ptr().add(8)),
        b0: _mm256_loadu_ps(lf.b.as_ptr()),
        b1: _mm256_loadu_ps(lf.b.as_ptr().add(8)),
        v255: _mm256_set1_ps(255.0),
        v0: _mm256_setzero_ps(),
        wrv: _mm256_set1_ps(wr),
        wgv: _mm256_set1_ps(wg),
        wbv: _mm256_set1_ps(wb),
        invnv: _mm256_set1_ps(inv_n),
        halfv: _mm256_set1_ps(0.5),
        onev: _mm256_set1_ps(1.0),
        nm1,
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct EstPreRgba {
    r0: std::arch::x86_64::__m256,
    r1: std::arch::x86_64::__m256,
    g0: std::arch::x86_64::__m256,
    g1: std::arch::x86_64::__m256,
    b0: std::arch::x86_64::__m256,
    b1: std::arch::x86_64::__m256,
    a0: std::arch::x86_64::__m256,
    a1: std::arch::x86_64::__m256,
    v255: std::arch::x86_64::__m256,
    v0: std::arch::x86_64::__m256,
    wrv: std::arch::x86_64::__m256,
    wgv: std::arch::x86_64::__m256,
    wbv: std::arch::x86_64::__m256,
    wav: std::arch::x86_64::__m256,
    invnv: std::arch::x86_64::__m256,
    halfv: std::arch::x86_64::__m256,
    onev: std::arch::x86_64::__m256,
    nm1: f32,
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_pre_rgba(p: &CCParams, lf: &LaneF32) -> EstPreRgba {
    use std::arch::x86_64::*;
    let n = 4f32;
    let nm1 = n - 1.0;
    let inv_n = 1.0 / nm1;
    let (wr, wg, wb, wa) = if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
            p.weights[3] as f32,
        )
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };
    EstPreRgba {
        r0: _mm256_loadu_ps(lf.r.as_ptr()),
        r1: _mm256_loadu_ps(lf.r.as_ptr().add(8)),
        g0: _mm256_loadu_ps(lf.g.as_ptr()),
        g1: _mm256_loadu_ps(lf.g.as_ptr().add(8)),
        b0: _mm256_loadu_ps(lf.b.as_ptr()),
        b1: _mm256_loadu_ps(lf.b.as_ptr().add(8)),
        a0: _mm256_loadu_ps(lf.a.as_ptr()),
        a1: _mm256_loadu_ps(lf.a.as_ptr().add(8)),
        v255: _mm256_set1_ps(255.0),
        v0: _mm256_setzero_ps(),
        wrv: _mm256_set1_ps(wr),
        wgv: _mm256_set1_ps(wg),
        wbv: _mm256_set1_ps(wb),
        wav: _mm256_set1_ps(wa),
        invnv: _mm256_set1_ps(inv_n),
        halfv: _mm256_set1_ps(0.5),
        onev: _mm256_set1_ps(1.0),
        nm1,
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
#[inline]
unsafe fn subset_err_rgb_pre(pre: &EstPreRgb, idxs: &[i32; 16], num_pixels: usize) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(8);
    let last = nchunks - 1;

    let tailk: __mmask8 = ((1u32 << (num_pixels - last * 8)) - 1) as __mmask8;
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];

    let v255 = pre.v255;
    let v0 = pre.v0;
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(pre.r0, pix, pre.r1);
        gv[c] = _mm256_permutex2var_ps(pre.g0, pix, pre.g1);
        bv[c] = _mm256_permutex2var_ps(pre.b0, pix, pre.b1);
        let k = if c == last { tailk } else { 0xff };
        minr = _mm256_mask_min_ps(minr, k, minr, rv[c]);
        ming = _mm256_mask_min_ps(ming, k, ming, gv[c]);
        minb = _mm256_mask_min_ps(minb, k, minb, bv[c]);
        maxr = _mm256_mask_max_ps(maxr, k, maxr, rv[c]);
        maxg = _mm256_mask_max_ps(maxg, k, maxg, gv[c]);
        maxb = _mm256_mask_max_ps(maxb, k, maxb, bv[c]);
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);

    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = pre.nm1 / (high - low);

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
            _mm256_mul_ps(fabv, bv[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), pre.halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), pre.invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, pre.onev);
        let s = _mm256_blendv_ps(s1, pre.onev, gt);
        let itr = _mm256_add_ps(srv, _mm256_mul_ps(dirv, s));
        let itg = _mm256_add_ps(sgv, _mm256_mul_ps(digv, s));
        let itb = _mm256_add_ps(sbv, _mm256_mul_ps(dibv, s));
        let dr = _mm256_sub_ps(itr, rv[c]);
        let dg = _mm256_sub_ps(itg, gv[c]);
        let db = _mm256_sub_ps(itb, bv[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_mul_ps(pre.wrv, dr), dr),
                _mm256_mul_ps(_mm256_mul_ps(pre.wgv, dg), dg),
            ),
            _mm256_mul_ps(_mm256_mul_ps(pre.wbv, db), db),
        );

        let term = if c == last {
            _mm256_maskz_mov_ps(tailk, term)
        } else {
            term
        };
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        total_errf += t_arr[0];
        total_errf += t_arr[1];
        total_errf += t_arr[2];
        total_errf += t_arr[3];
        total_errf += t_arr[4];
        total_errf += t_arr[5];
        total_errf += t_arr[6];
        total_errf += t_arr[7];
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
#[inline]
unsafe fn subset_err_rgba_pre(pre: &EstPreRgba, idxs: &[i32; 16], num_pixels: usize) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(8);
    let last = nchunks - 1;

    let tailk: __mmask8 = ((1u32 << (num_pixels - last * 8)) - 1) as __mmask8;
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];
    let mut av = [_mm256_setzero_ps(); 2];

    let v255 = pre.v255;
    let v0 = pre.v0;
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(pre.r0, pix, pre.r1);
        gv[c] = _mm256_permutex2var_ps(pre.g0, pix, pre.g1);
        bv[c] = _mm256_permutex2var_ps(pre.b0, pix, pre.b1);
        av[c] = _mm256_permutex2var_ps(pre.a0, pix, pre.a1);
        let k = if c == last { tailk } else { 0xff };
        minr = _mm256_mask_min_ps(minr, k, minr, rv[c]);
        ming = _mm256_mask_min_ps(ming, k, ming, gv[c]);
        minb = _mm256_mask_min_ps(minb, k, minb, bv[c]);
        mina = _mm256_mask_min_ps(mina, k, mina, av[c]);
        maxr = _mm256_mask_max_ps(maxr, k, maxr, rv[c]);
        maxg = _mm256_mask_max_ps(maxg, k, maxg, gv[c]);
        maxb = _mm256_mask_max_ps(maxb, k, maxb, bv[c]);
        maxa = _mm256_mask_max_ps(maxa, k, maxa, av[c]);
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let la = hmin_ps256(mina);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);
    let ha = hmax_ps256(maxa);

    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = pre.nm1 / (high - low);

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let faav = _mm256_set1_ps(faa);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let sav = _mm256_set1_ps(sa);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let diav = _mm256_set1_ps(dia);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
                _mm256_mul_ps(fabv, bv[c]),
            ),
            _mm256_mul_ps(faav, av[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), pre.halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), pre.invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, pre.onev);
        let s = _mm256_blendv_ps(s1, pre.onev, gt);
        let dr = _mm256_sub_ps(_mm256_add_ps(srv, _mm256_mul_ps(dirv, s)), rv[c]);
        let dg = _mm256_sub_ps(_mm256_add_ps(sgv, _mm256_mul_ps(digv, s)), gv[c]);
        let db = _mm256_sub_ps(_mm256_add_ps(sbv, _mm256_mul_ps(dibv, s)), bv[c]);
        let da = _mm256_sub_ps(_mm256_add_ps(sav, _mm256_mul_ps(diav, s)), av[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(pre.wrv, dr), dr),
                    _mm256_mul_ps(_mm256_mul_ps(pre.wgv, dg), dg),
                ),
                _mm256_mul_ps(_mm256_mul_ps(pre.wbv, db), db),
            ),
            _mm256_mul_ps(_mm256_mul_ps(pre.wav, da), da),
        );

        let term = if c == last {
            _mm256_maskz_mov_ps(tailk, term)
        } else {
            term
        };
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        total_errf += t_arr[0];
        total_errf += t_arr[1];
        total_errf += t_arr[2];
        total_errf += t_arr[3];
        total_errf += t_arr[4];
        total_errf += t_arr[5];
        total_errf += t_arr[6];
        total_errf += t_arr[7];
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_partition_lane_vperm(
    mode: usize,
    p: &CCParams,
    lf: &LaneF32,
    tab: &[SubsetIdx; 64],
    total_partitions: u32,
    total_subsets: usize,
) -> u32 {
    debug_assert!(mode != 7);
    let pre = est_pre_rgb(mode, p, lf);
    let mut best_err = u64::MAX;
    let mut best_partition = 0u32;
    for partition in 0..total_partitions {
        let si = &tab[partition as usize];
        let mut total_subset_err = 0u64;
        for subset in 0..total_subsets {
            let err = subset_err_rgb_pre(&pre, &si.idx[subset], si.total[subset]);
            total_subset_err += err;
            if total_subset_err >= best_err {
                break;
            }
        }
        if total_subset_err < best_err {
            best_err = total_subset_err;
            best_partition = partition;
            if best_err == 0 {
                break;
            }
        }
        if total_subsets == 2
            && partition as usize == BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
            && best_partition as usize != BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
        {
            break;
        }
    }
    best_partition
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_partition_list_lane_vperm(
    mode: usize,
    p: &CCParams,
    lf: &LaneF32,
    tab: &[SubsetIdx; 64],
    part_lo: u32,
    part_hi: u32,
    total_subsets: usize,
    solutions: &mut [Solution],
    num_solutions: &mut i32,
    max_solutions: i32,
) -> i32 {
    let pre_rgb = if mode != 7 {
        Some(est_pre_rgb(mode, p, lf))
    } else {
        None
    };
    let pre_rgba = if mode == 7 {
        Some(est_pre_rgba(p, lf))
    } else {
        None
    };
    let mut i_at = 0i32;
    for partition in part_lo..part_hi {
        let si = &tab[partition as usize];
        let full = *num_solutions == max_solutions;
        let thresh = if full {
            solutions[(max_solutions - 1) as usize].err
        } else {
            u64::MAX
        };
        let mut total_subset_err = 0u64;
        let mut pruned = false;
        for subset in 0..total_subsets {
            let err = if let Some(pre) = &pre_rgba {
                subset_err_rgba_pre(pre, &si.idx[subset], si.total[subset])
            } else {
                subset_err_rgb_pre(
                    pre_rgb.as_ref().unwrap_unchecked(),
                    &si.idx[subset],
                    si.total[subset],
                )
            };
            total_subset_err += err;
            if total_subset_err >= thresh {
                pruned = true;
                break;
            }
        }
        if pruned {
            i_at = *num_solutions;
            continue;
        }
        let mut i = 0i32;
        while i < *num_solutions {
            if total_subset_err < solutions[i as usize].err {
                break;
            }
            i += 1;
        }
        if i < *num_solutions {
            let mut solutions_to_move = (max_solutions - 1) - i;
            let num_elements_at_i = *num_solutions - i;
            if solutions_to_move > num_elements_at_i {
                solutions_to_move = num_elements_at_i;
            }
            let mut j = solutions_to_move - 1;
            while j >= 0 {
                solutions[(i + j + 1) as usize] = solutions[(i + j) as usize];
                j -= 1;
            }
        }
        if *num_solutions < max_solutions {
            *num_solutions += 1;
        }
        if i < *num_solutions {
            solutions[i as usize].err = total_subset_err;
            solutions[i as usize].index = partition;
        }
        i_at = i;
    }
    i_at
}

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
struct Solution {
    index: u32,
    err: u64,
}

const SIMD_W: usize = 4;

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

fn estimate_partition_list_group(
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
struct PartitionPlan {
    part0: u32,
    part13: u32,
    list13: Vec<Solution>,
    use_list13: bool,
    part2: u32,
    list2: Vec<Solution>,
    use_list2: bool,
    list0: Vec<Solution>,
    use_list0: bool,
    list7: Vec<Solution>,
}

fn build_partition_plans(lanes: &[&[ColorI; 16]], cp: &Params) -> Vec<PartitionPlan> {
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

#[derive(Clone)]
struct OptResults {
    mode: usize,
    partition: u32,
    selectors: [i32; 16],
    alpha_selectors: [i32; 16],
    low: [ColorI; 3],
    high: [ColorI; 3],
    pbits: [[u32; 2]; 3],
    rotation: u32,
    index_selector: u32,
}
impl OptResults {
    fn new() -> Self {
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

fn encode_bc7_block_bits(res: &OptResults) -> [u8; 16] {
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

fn encode_bc7_block_mode6(res: &OptResults) -> [u8; 16] {
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

fn handle_alpha_block_mode4(
    pixels: &[ColorI; 16],
    cp: &Params,
    params: &mut CCParams,
    lo_a: i32,
    hi_a: i32,
    opt4: &mut OptResults,
    mode4_err: &mut u64,
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
        let trial_err_color = color_cell_compression(4, params, &mut results, cp, 16, pixels, true);

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
                    std::mem::swap(&mut xl, &mut xh);
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

fn handle_alpha_block_mode5(
    pixels: &[ColorI; 16],
    cp: &Params,
    params: &mut CCParams,
    mut lo_a: i32,
    mut hi_a: i32,
    opt5: &mut OptResults,
    mode5_err: &mut u64,
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
    *mode5_err = color_cell_compression(5, params, &mut results5, cp, 16, pixels, true);
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
                    std::mem::swap(&mut new_lo, &mut new_hi);
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

fn handle_alpha_block(
    pixels: &[ColorI; 16],
    cp: &Params,
    base: &CCParams,
    lo_a: i32,
    hi_a: i32,
    plan: &PartitionPlan,
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
        let mode6_err = color_cell_compression(6, &params6, &mut results6, cp, 16, pixels, true);
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
        let num_solutions = solutions.len();
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
                solutions[solution_index].index,
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

fn handle_block_solid(cr: usize, cg: usize, cb: usize, ca: i32) -> [u8; 16] {
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

fn handle_opaque_block_mode6(pixels: &[ColorI; 16], cp: &Params, base: &CCParams) -> [u8; 16] {
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

pub fn compute_default_mip_count(width: u32, height: u32) -> i32 {
    let m = width.max(height).max(1);
    (31 - m.leading_zeros()) as i32 + 1
}

pub fn compute_mip_chain_size(width: u32, height: u32, mip_count: i32) -> usize {
    let mut total = 0usize;
    for m in 0..mip_count {
        let mw = (width >> m).max(1);
        let mh = (height >> m).max(1);
        let bx = (mw.div_ceil(4).max(1)) as usize;
        let by = (mh.div_ceil(4).max(1)) as usize;
        total += bx * by * 16;
    }
    total
}

pub fn encode_rgba32_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
) -> (Vec<u8>, i32) {
    let w = width as usize;
    let h = height as usize;
    assert_eq!(rgba.len(), w * h * 4);
    let flipped: Vec<u8> = if flip {
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = &rgba[(h - 1 - y) * w * 4..(h - 1 - y) * w * 4 + w * 4];
            out[y * w * 4..y * w * 4 + w * 4].copy_from_slice(src);
        }
        out
    } else {
        rgba.to_vec()
    };

    let mip_count = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));

    let mut cur: Vec<f32> = vec![0f32; w * h * 4];
    for i in 0..(w * h) {
        let r = flipped[i * 4];
        let g = flipped[i * 4 + 1];
        let b = flipped[i * 4 + 2];
        let a = flipped[i * 4 + 3] as f32;
        if srgb {
            cur[i * 4] = srgb_to_linear_u8(r);
            cur[i * 4 + 1] = srgb_to_linear_u8(g);
            cur[i * 4 + 2] = srgb_to_linear_u8(b);
            cur[i * 4 + 3] = a;
        } else {
            cur[i * 4] = r as f32;
            cur[i * 4 + 1] = g as f32;
            cur[i * 4 + 2] = b as f32;
            cur[i * 4 + 3] = a;
        }
    }
    let mut cw = w;
    let mut ch = h;

    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mip_count {
        let mut level = vec![0u8; cw * ch * 4];
        for i in 0..(cw * ch) {
            if srgb {
                level[i * 4] = linear_to_srgb_u8(cur[i * 4]);
                level[i * 4 + 1] = linear_to_srgb_u8(cur[i * 4 + 1]);
                level[i * 4 + 2] = linear_to_srgb_u8(cur[i * 4 + 2]);
            } else {
                level[i * 4] = round_half_up_u8(cur[i * 4]);
                level[i * 4 + 1] = round_half_up_u8(cur[i * 4 + 1]);
                level[i * 4 + 2] = round_half_up_u8(cur[i * 4 + 2]);
            }
            level[i * 4 + 3] = round_half_up_u8(cur[i * 4 + 3]);
        }
        parts.extend_from_slice(&level);
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}

const SRGB_TO_LINEAR_U8_BITS: [u32; 256] = [
    0x00000000, 0x399f22b4, 0x3a1f22b4, 0x3a6eb40e, 0x3a9f22b4, 0x3ac6eb61, 0x3aeeb40e, 0x3b0b3e5d,
    0x3b1f22b4, 0x3b33070b, 0x3b46eb61, 0x3b5b518d, 0x3b70f18d, 0x3b83e1c6, 0x3b8fe616, 0x3b9c87fc,
    0x3ba9c9b7, 0x3bb7ad6e, 0x3bc63549, 0x3bd56361, 0x3be539c1, 0x3bf5ba70, 0x3c0373b5, 0x3c0c6152,
    0x3c15a703, 0x3c1f45be, 0x3c293e6b, 0x3c3391f7, 0x3c3e4149, 0x3c494d43, 0x3c54b6c7, 0x3c607eb2,
    0x3c6ca5df, 0x3c792d22, 0x3c830aa8, 0x3c89af9e, 0x3c9085db, 0x3c978dc4, 0x3c9ec7c2, 0x3ca63434,
    0x3cadd37d, 0x3cb5a601, 0x3cbdac20, 0x3cc5e639, 0x3cce54ab, 0x3cd6f7d5, 0x3cdfd010, 0x3ce8ddb9,
    0x3cf2212c, 0x3cfb9ac1, 0x3d02a569, 0x3d0798dc, 0x3d0ca7e6, 0x3d11d2af, 0x3d171962, 0x3d1c7c2e,
    0x3d21fb3c, 0x3d2796b2, 0x3d2d4ebb, 0x3d332381, 0x3d39152b, 0x3d3f23e4, 0x3d454fd1, 0x3d4b991c,
    0x3d51ffee, 0x3d58846a, 0x3d5f26b8, 0x3d65e6fe, 0x3d6cc564, 0x3d73c20f, 0x3d7add29, 0x3d810b68,
    0x3d84b795, 0x3d887330, 0x3d8c3e4a, 0x3d9018f6, 0x3d940345, 0x3d97fd49, 0x3d9c0716, 0x3da020bb,
    0x3da44a4b, 0x3da883d7, 0x3daccd70, 0x3db12728, 0x3db59112, 0x3dba0b3a, 0x3dbe95b5, 0x3dc33092,
    0x3dc7dbe2, 0x3dcc97b6, 0x3dd1641f, 0x3dd6412c, 0x3ddb2eef, 0x3de02d78, 0x3de53cd5, 0x3dea5d19,
    0x3def8e52, 0x3df4d091, 0x3dfa23e8, 0x3dff8861, 0x3e027f07, 0x3e054280, 0x3e080ea3, 0x3e0ae377,
    0x3e0dc106, 0x3e10a754, 0x3e13966a, 0x3e168e51, 0x3e198f0f, 0x3e1c98ac, 0x3e1fab30, 0x3e22c6a3,
    0x3e25eb09, 0x3e29186c, 0x3e2c4ed1, 0x3e2f8e41, 0x3e32d6c5, 0x3e362861, 0x3e39831e, 0x3e3ce702,
    0x3e405416, 0x3e43ca5f, 0x3e4749e4, 0x3e4ad2ae, 0x3e4e64c2, 0x3e520027, 0x3e55a4e5, 0x3e595303,
    0x3e5d0a8b, 0x3e60cb7c, 0x3e6495e0, 0x3e6869bf, 0x3e6c4720, 0x3e702e0c, 0x3e741e84, 0x3e781890,
    0x3e7c1c38, 0x3e8014c2, 0x3e82203c, 0x3e84308d, 0x3e8645ba, 0x3e885fc5, 0x3e8a7eb1, 0x3e8ca283,
    0x3e8ecb3d, 0x3e90f8e1, 0x3e932b74, 0x3e9562f8, 0x3e979f71, 0x3e99e0e2, 0x3e9c274d, 0x3e9e72b7,
    0x3ea0c322, 0x3ea31891, 0x3ea57308, 0x3ea7d28a, 0x3eaa3718, 0x3eaca0b7, 0x3eaf0f69, 0x3eb18333,
    0x3eb3fc18, 0x3eb67a18, 0x3eb8fd37, 0x3ebb8579, 0x3ebe12e1, 0x3ec0a571, 0x3ec33d2d, 0x3ec5da17,
    0x3ec87c33, 0x3ecb2383, 0x3ecdd00b, 0x3ed081cd, 0x3ed338cc, 0x3ed5f50b, 0x3ed8b68d, 0x3edb7d55,
    0x3ede4965, 0x3ee11ac1, 0x3ee3f16b, 0x3ee6cd67, 0x3ee9aeb7, 0x3eec955d, 0x3eef815d, 0x3ef272ba,
    0x3ef56976, 0x3ef86594, 0x3efb6717, 0x3efe6e01, 0x3f00bd2d, 0x3f02460e, 0x3f03d1a7, 0x3f055ff9,
    0x3f06f106, 0x3f0884cf, 0x3f0a1b55, 0x3f0bb49b, 0x3f0d50a0, 0x3f0eef67, 0x3f1090f1, 0x3f12353e,
    0x3f13dc51, 0x3f15862b, 0x3f1732cd, 0x3f18e239, 0x3f1a946f, 0x3f1c4971, 0x3f1e0141, 0x3f1fbbdf,
    0x3f21794d, 0x3f23398d, 0x3f24fca0, 0x3f26c286, 0x3f288b42, 0x3f2a56d4, 0x3f2c253d, 0x3f2df680,
    0x3f2fca9f, 0x3f31a197, 0x3f337b6c, 0x3f355820, 0x3f3737b3, 0x3f391a26, 0x3f3aff7c, 0x3f3ce7b4,
    0x3f3ed2d2, 0x3f40c0d5, 0x3f42b1be, 0x3f44a590, 0x3f469c4b, 0x3f4895f1, 0x3f4a9282, 0x3f4c9201,
    0x3f4e946e, 0x3f5099cb, 0x3f52a218, 0x3f54ad57, 0x3f56bb8a, 0x3f58ccb1, 0x3f5ae0cd, 0x3f5cf7e0,
    0x3f5f11ec, 0x3f612eee, 0x3f634eef, 0x3f6571ea, 0x3f6797e3, 0x3f69c0d6, 0x3f6beccd, 0x3f6e1bc0,
    0x3f704db8, 0x3f7282af, 0x3f74baae, 0x3f76f5ae, 0x3f7933b8, 0x3f7b74c6, 0x3f7db8e0, 0x3f800000,
];

#[doc(hidden)]
pub const fn srgb_to_linear_u8(c: u8) -> f32 {
    f32::from_bits(SRGB_TO_LINEAR_U8_BITS[c as usize])
}

const SRGB_U8_LIN_THRESHOLD_BITS: [u32; 255] = [
    0x391f22b3, 0x39eeb40e, 0x3a46eb61, 0x3a8b3e5d, 0x3ab3070b, 0x3adacfb7, 0x3b014c32, 0x3b153089,
    0x3b2914df, 0x3b3cf936, 0x3b50f2d1, 0x3b65fb99, 0x3b7c3404, 0x3b89d060, 0x3b962333, 0x3ba314bd,
    0x3bb0a731, 0x3bbedcb6, 0x3bcdb76c, 0x3bdd3966, 0x3bed64ae, 0x3bfe3b46, 0x3c07df91, 0x3c10f918,
    0x3c1a6b33, 0x3c2436c8, 0x3c2e5cc8, 0x3c38de1a, 0x3c43bba4, 0x3c4ef647, 0x3c5a8ee1, 0x3c668653,
    0x3c72dd73, 0x3c7f9511, 0x3c865703, 0x3c8d1490, 0x3c940395, 0x3c9b247b, 0x3ca277a8, 0x3ca9fd79,
    0x3cb1b654, 0x3cb9a29a, 0x3cc1c2aa, 0x3cca16e3, 0x3cd29fa4, 0x3cdb5d4d, 0x3ce45033, 0x3ced78b6,
    0x3cf6d72f, 0x3d0035fc, 0x3d051bb6, 0x3d0a1cee, 0x3d0f39d1, 0x3d14728a, 0x3d19c745, 0x3d1f382b,
    0x3d24c56b, 0x3d2a6f25, 0x3d303587, 0x3d3618b9, 0x3d3c18e5, 0x3d423633, 0x3d4870cb, 0x3d4ec8d5,
    0x3d553e75, 0x3d5bd1d5, 0x3d62831a, 0x3d69526b, 0x3d703fef, 0x3d774bcf, 0x3d7e7628, 0x3d82df93,
    0x3d869374, 0x3d8a56cb, 0x3d8e29ac, 0x3d920c27, 0x3d95fe4f, 0x3d9a0036, 0x3d9e11ec, 0x3da23384,
    0x3da66510, 0x3daaa6a1, 0x3daef847, 0x3db35a17, 0x3db7cc1d, 0x3dbc4e6b, 0x3dc0e115, 0x3dc58429,
    0x3dca37b9, 0x3dcefbd6, 0x3dd3d090, 0x3dd8b5f6, 0x3dddac19, 0x3de2b30a, 0x3de7cad9, 0x3decf396,
    0x3df22d50, 0x3df7781a, 0x3dfcd3fe, 0x3e012088, 0x3e03dfaf, 0x3e06a77c, 0x3e0977f7, 0x3e0c5127,
    0x3e0f3314, 0x3e121dc6, 0x3e151144, 0x3e180d95, 0x3e1b12c2, 0x3e1e20d1, 0x3e2137ca, 0x3e2457b7,
    0x3e27809a, 0x3e2ab27c, 0x3e2ded67, 0x3e313160, 0x3e347e6f, 0x3e37d49d, 0x3e3b33ed, 0x3e3e9c68,
    0x3e420e14, 0x3e4588fa, 0x3e490d21, 0x3e4c9a8f, 0x3e50314b, 0x3e53d15c, 0x3e577aca, 0x3e5b2d9a,
    0x3e5ee9d4, 0x3e62af7d, 0x3e667e9f, 0x3e6a5742, 0x3e6e3965, 0x3e722512, 0x3e761a53, 0x3e7a192c,
    0x3e7e21a5, 0x3e8119e2, 0x3e8327c7, 0x3e853a88, 0x3e875223, 0x3e896e9f, 0x3e8b8ffd, 0x3e8db642,
    0x3e8fe171, 0x3e92118c, 0x3e944697, 0x3e968095, 0x3e98bf89, 0x3e9b0377, 0x3e9d4c62, 0x3e9f9a4c,
    0x3ea1ed38, 0x3ea4452b, 0x3ea6a226, 0x3ea9042e, 0x3eab6b44, 0x3eadd76c, 0x3eb048aa, 0x3eb2bf02,
    0x3eb53a71, 0x3eb7bb00, 0x3eba40b1, 0x3ebccb85, 0x3ebf5b81, 0x3ec1f0a6, 0x3ec48af9, 0x3ec72a7c,
    0x3ec9cf32, 0x3ecc791d, 0x3ecf2842, 0x3ed1dca2, 0x3ed49641, 0x3ed75521, 0x3eda1945, 0x3edce2b1,
    0x3edfb168, 0x3ee2856a, 0x3ee55ebd, 0x3ee83d62, 0x3eeb215d, 0x3eee0ab0, 0x3ef0f95e, 0x3ef3ed6a,
    0x3ef6e6d7, 0x3ef9e5a5, 0x3efce9df, 0x3efff37e, 0x3f018145, 0x3f030b82, 0x3f049877, 0x3f062827,
    0x3f07ba91, 0x3f094fba, 0x3f0ae79f, 0x3f0c8245, 0x3f0e1faa, 0x3f0fbfd2, 0x3f1162be, 0x3f13086e,
    0x3f14b0e4, 0x3f165c22, 0x3f180a29, 0x3f19baf9, 0x3f1b6e96, 0x3f1d24fe, 0x3f1ede35, 0x3f209a3c,
    0x3f225912, 0x3f241abb, 0x3f25df37, 0x3f27a688, 0x3f2970ae, 0x3f2b3dab, 0x3f2d0d83, 0x3f2ee033,
    0x3f30b5bd, 0x3f328e24, 0x3f346968, 0x3f36478c, 0x3f38288f, 0x3f3a0c73, 0x3f3bf33a, 0x3f3ddce5,
    0x3f3fc974, 0x3f41b8ea, 0x3f43ab48, 0x3f45a08e, 0x3f4798bf, 0x3f4993da, 0x3f4b91e2, 0x3f4d92d8,
    0x3f4f96bd, 0x3f519d91, 0x3f53a757, 0x3f55b410, 0x3f57c3bd, 0x3f59d65e, 0x3f5bebf6, 0x3f5e0485,
    0x3f60200d, 0x3f623e90, 0x3f64600b, 0x3f668486, 0x3f68abfa, 0x3f6ad671, 0x3f6d03e3, 0x3f6f345b,
    0x3f7167d0, 0x3f739e4d, 0x3f75d7cb, 0x3f781452, 0x3f7a53db, 0x3f7c9671, 0x3f7edc0e,
];

#[doc(hidden)]
pub fn linear_to_srgb_u8(lin: f32) -> u8 {
    if lin <= 0.0 || lin.is_nan() {
        return 0;
    }
    if lin >= 1.0 {
        return 255;
    }

    let bits = lin.to_bits();

    SRGB_U8_LIN_THRESHOLD_BITS.partition_point(|&t| t <= bits) as u8
}

#[doc(hidden)]
pub fn round_half_up_u8(v: f32) -> u8 {
    let r = (v + 0.5).floor();
    if r <= 0.0 {
        0
    } else if r >= 255.0 {
        255
    } else {
        r as u8
    }
}

fn scanline_to_blocks(rgba: &[u8], width: usize, height: usize) -> (Vec<u8>, usize) {
    let bw = width / 4;
    let bh = height / 4;
    let row_bytes = width * 4;
    let mut out = vec![0u8; bw * bh * 64];
    let mut o = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let base = by * 4 * row_bytes + bx * 16;
            for r in 0..4 {
                let start = base + r * row_bytes;
                out[o..o + 16].copy_from_slice(&rgba[start..start + 16]);
                o += 16;
            }
        }
    }
    (out, bw * bh)
}

fn pad_to_block_size(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let pw = (w + 3) & !3;
    let ph = (h + 3) & !3;
    if pw == w && ph == h {
        return (rgba.to_vec(), w, h);
    }

    let mut out = vec![0u8; pw * ph * 4];
    for y in 0..ph {
        let sy = y % h;
        for x in 0..pw {
            let sx = x % w;
            let s = (sy * w + sx) * 4;
            let d = (y * pw + x) * 4;
            out[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    (out, pw, ph)
}

#[doc(hidden)]
pub fn box_halve(arr: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let c = 4usize;
    let nh = (h / 2).max(1);
    let nw = (w / 2).max(1);
    let fh = if h > 1 { 2 } else { 1 };
    let fw = if w > 1 { 2 } else { 1 };
    let denom = (fh * fw) as f32;
    let mut out = vec![0f32; nh * nw * c];
    let row_stride = w * c;
    for ny in 0..nh {
        for nx in 0..nw {
            for ch in 0..c {
                let mut acc = 0f32;
                for dy in 0..fh {
                    for dx in 0..fw {
                        let y = ny * fh + dy;
                        let x = nx * fw + dx;
                        acc += arr[y * row_stride + x * c + ch];
                    }
                }
                out[(ny * nw + nx) * c + ch] = acc / denom;
            }
        }
    }
    (out, nw, nh)
}

pub fn encode_bc7_mip_chain_with_profile(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: Bc7Profile,
) -> (Vec<u8>, i32) {
    let w = width as usize;
    let h = height as usize;
    assert_eq!(rgba.len(), w * h * 4);
    let flipped: Vec<u8> = if flip {
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = &rgba[(h - 1 - y) * w * 4..(h - 1 - y) * w * 4 + w * 4];
            out[y * w * 4..y * w * 4 + w * 4].copy_from_slice(src);
        }
        out
    } else {
        rgba.to_vec()
    };

    let mip_count = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));
    let params = match profile {
        Bc7Profile::Slow => Params::slow(perceptual),
        Bc7Profile::Basic => Params::basic(perceptual),
    };

    let mut cur: Vec<f32> = vec![0f32; w * h * 4];
    for i in 0..(w * h) {
        let r = flipped[i * 4];
        let g = flipped[i * 4 + 1];
        let b = flipped[i * 4 + 2];
        let a = flipped[i * 4 + 3] as f32;
        if srgb {
            cur[i * 4] = srgb_to_linear_u8(r);
            cur[i * 4 + 1] = srgb_to_linear_u8(g);
            cur[i * 4 + 2] = srgb_to_linear_u8(b);
            cur[i * 4 + 3] = a;
        } else {
            cur[i * 4] = r as f32;
            cur[i * 4 + 1] = g as f32;
            cur[i * 4 + 2] = b as f32;
            cur[i * 4 + 3] = a;
        }
    }
    let mut cw = w;
    let mut ch = h;

    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mip_count {
        let mut level = vec![0u8; cw * ch * 4];
        for i in 0..(cw * ch) {
            if srgb {
                level[i * 4] = linear_to_srgb_u8(cur[i * 4]);
                level[i * 4 + 1] = linear_to_srgb_u8(cur[i * 4 + 1]);
                level[i * 4 + 2] = linear_to_srgb_u8(cur[i * 4 + 2]);
            } else {
                level[i * 4] = round_half_up_u8(cur[i * 4]);
                level[i * 4 + 1] = round_half_up_u8(cur[i * 4 + 1]);
                level[i * 4 + 2] = round_half_up_u8(cur[i * 4 + 2]);
            }
            level[i * 4 + 3] = round_half_up_u8(cur[i * 4 + 3]);
        }
        let (padded, pw, ph) = pad_to_block_size(&level, cw, ch);
        let (blocks, n) = scanline_to_blocks(&padded, pw, ph);
        let comp = encode_blocks(&blocks, n, &params);
        parts.extend_from_slice(&comp);
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}

#[cfg(test)]
mod opt_table_tests {
    use super::*;

    #[test]
    fn fast_opt_tables_match_reference() {
        let fast = build_opt_tables();
        let reference = build_opt_tables_reference();
        assert!(fast.mode0 == reference.mode0, "mode0 mismatch");
        assert!(fast.mode1 == reference.mode1, "mode1 mismatch");
        assert!(fast.mode6 == reference.mode6, "mode6 mismatch");
        assert!(fast.mode7 == reference.mode7, "mode7 mismatch");
        assert!(fast.mode5 == reference.mode5, "mode5 mismatch");
        assert!(fast.mode4_3 == reference.mode4_3, "mode4_3 mismatch");
        assert!(fast.mode4_2 == reference.mode4_2, "mode4_2 mismatch");
    }
}
