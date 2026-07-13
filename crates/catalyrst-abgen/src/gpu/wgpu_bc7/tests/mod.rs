use super::testsup::*;
use super::*;
use crate::gpu::corelib::bc7::{build_opt_tables, encode_group, group_signature, probe};
use crate::gpu::corelib::mips;
use crate::gpu::corelib::mode_tree::{self, TREE};
use crate::gpuhost::oracle::gen_texture;
use std::mem::offset_of;

mod encode;
mod layout;
mod mathops;
mod partition;
mod solver;

fn params4() -> [Params; 4] {
    [
        Params::slow(false),
        Params::slow(true),
        Params::basic(false),
        Params::basic(true),
    ]
}

fn lin_cpu(tex: &[u8], srgb: bool) -> Vec<f32> {
    let n = tex.len() / 4;
    let mut out = vec![0f32; n * 4];
    for i in 0..n {
        mips::linearize_pixel(&tex[i * 4..i * 4 + 4], srgb, &mut out[i * 4..i * 4 + 4]);
    }
    out
}

fn pyramid(tex: &[u8], w: u32, h: u32, srgb: bool) -> Vec<(Vec<f32>, usize, usize)> {
    let mut levels = vec![(lin_cpu(tex, srgb), w as usize, h as usize)];
    loop {
        let (cur, w, h) = levels.last().unwrap();
        if *w == 1 && *h == 1 {
            break;
        }
        let (next, nw, nh) = mips::box_halve(cur, *w, *h);
        levels.push((next, nw, nh));
    }
    levels
}

fn texture_blocks(tex: &[u8], w: u32, h: u32, srgb: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (level, lw, lh) in pyramid(tex, w, h, srgb) {
        let (bw, bh) = mips::level_block_dims(lw, lh);
        for by in 0..bh {
            for bx in 0..bw {
                let mut blk = [0u8; 64];
                mips::quantize_pack_block(&level, lw, lh, srgb, bx, by, &mut blk);
                out.extend_from_slice(&blk);
            }
        }
    }
    out
}

fn block_with(f: impl Fn(usize) -> [u8; 4]) -> [u8; 64] {
    let mut b = [0u8; 64];
    for i in 0..16 {
        b[i * 4..i * 4 + 4].copy_from_slice(&f(i));
    }
    b
}

fn solid_block(px: [u8; 4]) -> [u8; 64] {
    block_with(|_| px)
}

fn classify_cases() -> (Vec<[u8; 64]>, Vec<u32>) {
    let blocks = vec![
        solid_block([10, 20, 30, 255]),
        solid_block([10, 20, 30, 100]),
        block_with(|i| [40, 50, 60, if i == 0 { 0 } else { 255 }]),
        block_with(|i| [40, 50, 60, if i % 2 == 0 { 254 } else { 255 }]),
        block_with(|i| [i as u8 * 3, 200 - i as u8, i as u8, 255]),
        block_with(|i| [77, 88, if i == 15 { 100 } else { 99 }, 255]),
    ];
    let want = vec![0u32, 0, 1, 1, 2, 2];
    (blocks, want)
}

fn xs64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn weight_sets() -> Vec<[u32; 4]> {
    let mut sets: Vec<[u32; 4]> = Vec::new();
    for p in params4() {
        let mut muled = p.weights;
        for c in 0..4 {
            muled[c] *= p.mode67_weight_mul[c];
        }
        for w in [p.weights, muled] {
            if !sets.contains(&w) {
                sets.push(w);
            }
        }
    }
    sets.push([37, 5, 11, 3]);
    sets
}

fn corner_colors() -> Vec<[i32; 4]> {
    (0..16u32)
        .map(|m| {
            let mut c = [0i32; 4];
            for k in 0..4 {
                if (m >> k) & 1 == 1 {
                    c[k] = 255;
                }
            }
            c
        })
        .collect()
}

fn gen_block(st: &mut u64, strategy: usize, out: &mut [u8]) {
    let byte = |st: &mut u64| (xs64(st) % 256) as u8;
    match strategy % 9 {
        0 => {
            for b in out.iter_mut() {
                *b = byte(st);
            }
        }
        1 => {
            let px = [byte(st), byte(st), byte(st), byte(st)];
            for i in 0..16 {
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        2 => {
            let a = [byte(st), byte(st), byte(st), 255];
            let b = [byte(st), byte(st), byte(st), 255];
            for i in 0..16 {
                let px = if i % 2 == 0 { a } else { b };
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        3 => {
            let base = [
                (xs64(st) % 250) as u8,
                (xs64(st) % 250) as u8,
                (xs64(st) % 250) as u8,
                255,
            ];
            for i in 0..16 {
                for k in 0..3 {
                    out[i * 4 + k] = base[k] + (xs64(st) % 5) as u8;
                }
                out[i * 4 + 3] = 255;
            }
        }
        4 => {
            for i in 0..16u8 {
                let o = i as usize * 4;
                out[o] = i * 16;
                out[o + 1] = 255 - i * 8;
                out[o + 2] = i * 4;
                out[o + 3] = i * 17;
            }
        }
        5 => {
            for i in 0..16u8 {
                let o = i as usize * 4;
                out[o] = i * 15;
                out[o + 1] = 240 - i * 12;
                out[o + 2] = 30 + i * 9;
                out[o + 3] = 255;
            }
        }
        6 => {
            for i in 0..16 {
                for k in 0..3 {
                    out[i * 4 + k] = byte(st);
                }
                out[i * 4 + 3] = 255;
            }
        }
        7 => {
            let px = [byte(st), byte(st), byte(st), 255];
            for i in 0..16 {
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        _ => {
            let px = [byte(st), byte(st), byte(st), 255];
            for i in 0..16 {
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
            let j = (xs64(st) % 16) as usize;
            out[j * 4] = out[j * 4].wrapping_add(40);
            out[j * 4 + 3] = 200;
        }
    }
}

fn push_pixels(input: &mut Vec<u32>, px: &[[i32; 4]; 16]) {
    for row in px {
        for &q in row {
            input.push(q as u32);
        }
    }
}

fn px_from_block(blk: &[u8; 64]) -> [[i32; 4]; 16] {
    let mut px = [[0i32; 4]; 16];
    for i in 0..16 {
        for k in 0..4 {
            px[i][k] = blk[i * 4 + k] as i32;
        }
    }
    px
}

fn hint_code(cp: &Params, px: &[[i32; 4]; 16]) -> (u32, Params) {
    let (applied, gated) = probe::mode_tree_hint(px, cp);
    let code = if !applied {
        0
    } else if !gated.use_mode6 {
        1
    } else {
        2
    };
    (code, gated)
}
