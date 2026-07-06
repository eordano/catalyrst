use image::RgbaImage;
use std::collections::HashMap;

use super::{MAX_REPEATS, MIN_TILE_DIM, UV_EPS};

enum TileKind {
    Solid([u8; 4]),
    Image(Vec<u8>),
}

pub(super) struct Tile {
    kind: TileKind,
    pub(super) src_w: u32,
    pub(super) src_h: u32,
    pub(super) w: u32,
    pub(super) h: u32,
    pub(super) hash: String,
}

impl Tile {
    pub(super) fn from_pixels(pixels: Vec<u8>, w: u32, h: u32) -> Tile {
        let mut hashed = Vec::with_capacity(pixels.len() + 8);
        hashed.extend_from_slice(&w.to_le_bytes());
        hashed.extend_from_slice(&h.to_le_bytes());
        hashed.extend_from_slice(&pixels);
        let hash = crate::hashes::sha256_hex(&hashed);
        Tile {
            kind: TileKind::Image(pixels),
            src_w: w,
            src_h: h,
            w,
            h,
            hash,
        }
    }

    fn solid(color: [u8; 4]) -> Tile {
        let mut hashed = b"solid:".to_vec();
        hashed.extend_from_slice(&color);
        let hash = crate::hashes::sha256_hex(&hashed);
        Tile {
            kind: TileKind::Solid(color),
            src_w: MIN_TILE_DIM,
            src_h: MIN_TILE_DIM,
            w: MIN_TILE_DIM,
            h: MIN_TILE_DIM,
            hash,
        }
    }

    pub(super) fn is_solid(&self) -> bool {
        matches!(self.kind, TileKind::Solid(_))
    }

    pub(super) fn render_cropped(&self, crop: Option<[u32; 4]>, w: u32, h: u32) -> Vec<u8> {
        match &self.kind {
            TileKind::Solid(c) => {
                let mut px = Vec::with_capacity(w as usize * h as usize * 4);
                for _ in 0..w as usize * h as usize {
                    px.extend_from_slice(c);
                }
                px
            }
            TileKind::Image(src) => {
                let (cx, cy, cw, ch) = match crop {
                    Some([x, y, cw, ch]) => (x, y, cw, ch),
                    None => (0, 0, self.src_w, self.src_h),
                };
                let window: Vec<u8> = if (cx, cy, cw, ch) == (0, 0, self.src_w, self.src_h) {
                    src.clone()
                } else {
                    let mut out = Vec::with_capacity(cw as usize * ch as usize * 4);
                    for row in cy..cy + ch {
                        let start = (row as usize * self.src_w as usize + cx as usize) * 4;
                        out.extend_from_slice(&src[start..start + cw as usize * 4]);
                    }
                    out
                };
                if (w, h) == (cw, ch) {
                    window
                } else {
                    crate::resize::box_downscale_rgba(
                        &window,
                        cw as usize,
                        ch as usize,
                        w as usize,
                        h as usize,
                    )
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum TileKey {
    Image {
        hash: String,
        tint: [u64; 4],
        reps: (u32, u32),
    },
    Solid([u8; 4]),
}

pub(super) enum UvMap {
    Rect { shift: [f64; 2], reps: [f64; 2] },
    Center,
}

pub(super) enum UvPlan {
    Rect { shift: [f64; 2], reps: (u32, u32) },
    Fallback { span: [f64; 2] },
}

#[derive(Default)]
pub(super) struct Bucket {
    pub(super) tiles: Vec<Tile>,
    by_key: HashMap<TileKey, usize>,
    pub(super) prims: Vec<(usize, usize, UvMap)>,
    pub(super) refs: usize,
    pub(super) fallbacks: usize,
}

pub(super) fn tint_bits(c: [f64; 4]) -> [u64; 4] {
    c.map(|v| v.to_bits())
}

pub(super) fn solid_color(c: [f64; 4]) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub(super) fn tinted_pixels(img: &RgbaImage, tint: [f64; 4]) -> Vec<u8> {
    let t = tint.map(|v| v.clamp(0.0, 1.0));
    if t == [1.0; 4] {
        return img.as_raw().clone();
    }
    let mut out = Vec::with_capacity(img.as_raw().len());
    for px in img.as_raw().chunks_exact(4) {
        for ch in 0..4 {
            out.push((px[ch] as f64 * t[ch]).round().clamp(0.0, 255.0) as u8);
        }
    }
    out
}

pub(super) fn repeat_pixels(
    pixels: Vec<u8>,
    w: u32,
    h: u32,
    ru: u32,
    rv: u32,
) -> (Vec<u8>, u32, u32) {
    if ru == 1 && rv == 1 {
        return (pixels, w, h);
    }
    let nw = w * ru;
    let nh = h * rv;
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for y in 0..nh as usize {
        let sy = y % h as usize;
        for x in 0..nw as usize {
            let sx = x % w as usize;
            let d = (y * nw as usize + x) * 4;
            let s = (sy * w as usize + sx) * 4;
            out[d..d + 4].copy_from_slice(&pixels[s..s + 4]);
        }
    }
    (out, nw, nh)
}

pub(super) fn clamp_to_cap(pixels: Vec<u8>, w: u32, h: u32, cap: u32) -> (Vec<u8>, u32, u32) {
    if w <= cap && h <= cap {
        return (pixels, w, h);
    }
    let scale = cap as f64 / w.max(h) as f64;
    let nw = ((w as f64 * scale).floor() as u32).clamp(1, cap);
    let nh = ((h as f64 * scale).floor() as u32).clamp(1, cap);
    let out = crate::resize::box_downscale_rgba(
        &pixels,
        w as usize,
        h as usize,
        nw as usize,
        nh as usize,
    );
    (out, nw, nh)
}

pub(super) fn average_color(pixels: &[u8]) -> [u8; 4] {
    let n = (pixels.len() / 4).max(1) as f64;
    let mut acc = [0f64; 4];
    for px in pixels.chunks_exact(4) {
        for ch in 0..4 {
            acc[ch] += px[ch] as f64;
        }
    }
    acc.map(|v| (v / n).round().clamp(0.0, 255.0) as u8)
}

pub(super) fn solid_tile(color: [u8; 4]) -> Tile {
    Tile::solid(color)
}

pub(super) fn uv_plan(uvs: &[[f32; 2]]) -> UvPlan {
    let mut mn = [f64::INFINITY; 2];
    let mut mx = [f64::NEG_INFINITY; 2];
    for uv in uvs {
        for a in 0..2 {
            let v = uv[a] as f64;
            mn[a] = mn[a].min(v);
            mx[a] = mx[a].max(v);
        }
    }
    if !mn.iter().chain(mx.iter()).all(|v| v.is_finite()) {
        return UvPlan::Fallback {
            span: [f64::NAN, f64::NAN],
        };
    }
    let shift = [mn[0].floor(), mn[1].floor()];
    let smax = [mx[0] - shift[0], mx[1] - shift[1]];
    let reps = [
        (((smax[0] - UV_EPS).ceil() as i64).max(1)),
        (((smax[1] - UV_EPS).ceil() as i64).max(1)),
    ];
    if reps[0] > MAX_REPEATS || reps[1] > MAX_REPEATS {
        return UvPlan::Fallback { span: smax };
    }
    UvPlan::Rect {
        shift,
        reps: (reps[0] as u32, reps[1] as u32),
    }
}

pub(super) fn intern_tile(bucket: &mut Bucket, key: TileKey, make: impl FnOnce() -> Tile) -> usize {
    if let Some(&i) = bucket.by_key.get(&key) {
        return i;
    }
    let i = bucket.tiles.len();
    bucket.tiles.push(make());
    bucket.by_key.insert(key, i);
    i
}
