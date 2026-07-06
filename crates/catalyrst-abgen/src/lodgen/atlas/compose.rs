use anyhow::{anyhow, Result};
use image::RgbaImage;
use std::collections::HashMap;

use crate::lodgen::model::{AlphaClass, LodImage, LodModel, LodPrimitive};

use super::pack::expand_axis;
use super::tile::{Bucket, Tile, UvMap};
use super::{JPEG_QUALITY, NATIVE_SOLID_DIM};

pub(super) fn compose(
    tiles: &[Tile],
    crops: &[Option<[u32; 4]>],
    rects: &[(u32, u32, u32, u32)],
    canvas: u32,
    padding: u32,
) -> Vec<u8> {
    let s = canvas as usize;
    let mut out = vec![0u8; s * s * 4];
    for ((t, crop), &(x, y, w, h)) in tiles.iter().zip(crops.iter()).zip(rects.iter()) {
        let px = t.render_cropped(*crop, w, h);
        let x0 = x.saturating_sub(padding) as usize;
        let y0 = y.saturating_sub(padding) as usize;
        let x1 = (x + w + padding).min(canvas) as usize;
        let y1 = (y + h + padding).min(canvas) as usize;
        for cy in y0..y1 {
            let sy = (cy as i64 - y as i64).clamp(0, h as i64 - 1) as usize;
            for cx in x0..x1 {
                let sx = (cx as i64 - x as i64).clamp(0, w as i64 - 1) as usize;
                let d = (cy * s + cx) * 4;
                let src = (sy * w as usize + sx) * 4;
                out[d..d + 4].copy_from_slice(&px[src..src + 4]);
            }
        }
    }
    out
}

fn fill_background(rgba: &mut [u8], size: u32) {
    let w = size as usize;
    let n = w * w;
    let mut has_transparent = false;
    let mut has_opaque = false;
    for px in rgba.chunks_exact(4) {
        if px[3] == 0 {
            has_transparent = true;
        } else {
            has_opaque = true;
        }
        if has_transparent && has_opaque {
            break;
        }
    }
    if !(has_transparent && has_opaque) {
        return;
    }
    let mut seed: Vec<i32> = vec![-1; n];
    for i in 0..n {
        if rgba[i * 4 + 3] > 0 {
            seed[i] = i as i32;
        }
    }
    let l1 = |i: usize, s: i32| -> i32 {
        let (x, y) = ((i % w) as i32, (i / w) as i32);
        let (sx, sy) = (s % w as i32, s / w as i32);
        (x - sx).abs() + (y - sy).abs()
    };
    let mut offsets = Vec::new();
    let mut k = (w / 2).max(1);
    loop {
        offsets.push(k);
        if k == 1 {
            break;
        }
        k /= 2;
    }
    for _round in 0..8 {
        for &k in &offsets {
            let snap = seed.clone();
            for y in 0..w {
                for x in 0..w {
                    let idx = y * w + x;
                    if rgba[idx * 4 + 3] > 0 {
                        continue;
                    }
                    let mut best = seed[idx];
                    let mut bestd = if best >= 0 { l1(idx, best) } else { i32::MAX };
                    let taps = [
                        (x >= k).then(|| idx - k),
                        (x + k < w).then(|| idx + k),
                        (y >= k).then(|| idx - k * w),
                        (y + k < w).then(|| idx + k * w),
                    ];
                    for tap in taps.into_iter().flatten() {
                        let s = snap[tap];
                        if s >= 0 {
                            let d = l1(idx, s);
                            if d < bestd {
                                bestd = d;
                                best = s;
                            }
                        }
                    }
                    seed[idx] = best;
                }
            }
        }
        if seed.iter().all(|&s| s >= 0) {
            break;
        }
    }
    let snap_rgb: Vec<u8> = rgba.to_vec();
    for i in 0..n {
        if rgba[i * 4 + 3] == 0 && seed[i] >= 0 {
            let s = seed[i] as usize * 4;
            rgba[i * 4] = snap_rgb[s];
            rgba[i * 4 + 1] = snap_rgb[s + 1];
            rgba[i * 4 + 2] = snap_rgb[s + 2];
        }
    }
}

pub(super) fn encode_atlas(class: AlphaClass, mut rgba: Vec<u8>, canvas: u32) -> Result<LodImage> {
    if class == AlphaClass::Opaque {
        fill_background(&mut rgba, canvas);
        let mut rgb = Vec::with_capacity(canvas as usize * canvas as usize * 3);
        for px in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&px[0..3]);
        }
        let img = image::RgbImage::from_raw(canvas, canvas, rgb)
            .ok_or_else(|| anyhow!("atlas rgb buffer"))?;
        let mut cur = std::io::Cursor::new(Vec::new());
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cur, JPEG_QUALITY);
        img.write_with_encoder(enc)?;
        Ok(LodImage {
            bytes: cur.into_inner(),
            mime: "image/jpeg".to_string(),
        })
    } else {
        fill_background(&mut rgba, canvas);
        let img = RgbaImage::from_raw(canvas, canvas, rgba)
            .ok_or_else(|| anyhow!("atlas rgba buffer"))?;
        let mut cur = std::io::Cursor::new(Vec::new());
        img.write_to(&mut cur, image::ImageFormat::Png)?;
        Ok(LodImage {
            bytes: cur.into_inner(),
            mime: "image/png".to_string(),
        })
    }
}

pub(super) fn native_crops(bucket: &Bucket, model: &LodModel) -> Vec<Option<[u32; 4]>> {
    let mut boxes: Vec<Option<[f64; 4]>> = vec![None; bucket.tiles.len()];
    for (pi, ti, uvmap) in &bucket.prims {
        let UvMap::Rect { shift, reps } = uvmap else {
            continue;
        };
        let prim = &model.primitives[*pi];
        for uv in &prim.uvs {
            let lu = ((uv[0] as f64 - shift[0]) / reps[0]).clamp(0.0, 1.0);
            let lv = ((uv[1] as f64 - shift[1]) / reps[1]).clamp(0.0, 1.0);
            let b = boxes[*ti].get_or_insert([1.0, 1.0, 0.0, 0.0]);
            b[0] = b[0].min(lu);
            b[1] = b[1].min(lv);
            b[2] = b[2].max(lu);
            b[3] = b[3].max(lv);
        }
    }
    bucket
        .tiles
        .iter()
        .zip(boxes.iter())
        .map(|(t, b)| {
            if t.is_solid() {
                return None;
            }
            let b = (*b)?;
            let (w, h) = (t.src_w, t.src_h);
            let mut x0 = ((b[0] * w as f64).floor() as u32).min(w.saturating_sub(1));
            let mut x1 = ((b[2] * w as f64).ceil() as u32).clamp(x0 + 1, w);
            let mut y0 = ((b[1] * h as f64).floor() as u32).min(h.saturating_sub(1));
            let mut y1 = ((b[3] * h as f64).ceil() as u32).clamp(y0 + 1, h);
            expand_axis(&mut x0, &mut x1, w, NATIVE_SOLID_DIM);
            expand_axis(&mut y0, &mut y1, h, NATIVE_SOLID_DIM);
            if (x0, y0, x1, y1) == (0, 0, w, h) {
                None
            } else {
                Some([x0, y0, x1 - x0, y1 - y0])
            }
        })
        .collect()
}

const NORMAL_WELD_GRID: f32 = 1_048_576.0;

pub(super) fn weld_primitive(p: &mut LodPrimitive) {
    let key_of = |i: usize| -> ([u32; 3], [u32; 3], [u32; 2]) {
        let pos = p.positions[i].map(|v| v.to_bits());
        let nor =
            p.normals[i].map(|v| ((v * NORMAL_WELD_GRID).round() / NORMAL_WELD_GRID).to_bits());
        let uv = p.uvs[i].map(|v| v.to_bits());
        (pos, nor, uv)
    };
    let n = p.positions.len();
    let mut remap: Vec<u32> = Vec::with_capacity(n);
    let mut keep: Vec<usize> = Vec::with_capacity(n);
    let mut seen: HashMap<([u32; 3], [u32; 3], [u32; 2]), u32> = HashMap::with_capacity(n);
    for i in 0..n {
        match seen.entry(key_of(i)) {
            std::collections::hash_map::Entry::Occupied(e) => remap.push(*e.get()),
            std::collections::hash_map::Entry::Vacant(e) => {
                let idx = keep.len() as u32;
                e.insert(idx);
                keep.push(i);
                remap.push(idx);
            }
        }
    }
    if keep.len() == n {
        return;
    }
    p.positions = keep.iter().map(|&i| p.positions[i]).collect();
    p.normals = keep.iter().map(|&i| p.normals[i]).collect();
    p.uvs = keep.iter().map(|&i| p.uvs[i]).collect();
    for idx in p.indices.iter_mut() {
        *idx = remap[*idx as usize];
    }
}
