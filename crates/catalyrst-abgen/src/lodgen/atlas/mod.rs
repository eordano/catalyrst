use anyhow::{anyhow, bail, Result};
use image::RgbaImage;
use rayon::prelude::*;

use super::model::{AlphaClass, LodImage, LodMaterial, LodModel, LodPrimitive};

mod compose;
mod pack;
#[cfg(test)]
mod tests;
mod tile;

use compose::{compose, encode_atlas, native_crops, weld_primitive};
#[cfg(test)]
use pack::pack_skyline;
use pack::{pack_bucket, Packed};
use tile::{
    average_color, clamp_to_cap, intern_tile, repeat_pixels, solid_color, solid_tile, tint_bits,
    tinted_pixels, uv_plan, Bucket, Tile, TileKey, UvMap, UvPlan,
};

const MIN_TILE_DIM: u32 = 4;
const UV_EPS: f64 = 1e-4;
const MAX_REPEATS: i64 = 2;
const JPEG_QUALITY: u8 = 85;
const TARGET_OCCUPANCY: f64 = 0.75;
const SHRINK_STEP: f64 = 0.95;
const MAX_PACK_TRIES: u32 = 200;
const GROW_TRIES: u32 = 12;

const NATIVE_SOLID_DIM: u32 = 8;
const NATIVE_MIN_CANVAS: u32 = 8;

const CLASS_ORDER: [AlphaClass; 3] = [AlphaClass::Opaque, AlphaClass::Mask, AlphaClass::Blend];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasMode {
    FullBleed,
    Native,
}

pub fn class_material_name(class: AlphaClass) -> &'static str {
    match class {
        AlphaClass::Opaque => "TextureBakeResult-mat",
        AlphaClass::Mask => "TextureBakeResult-mat-cutout",
        AlphaClass::Blend => "TextureBakeResult-mat-transparent",
    }
}

fn class_tag(class: AlphaClass) -> &'static str {
    match class {
        AlphaClass::Opaque => "opaque",
        AlphaClass::Mask => "mask",
        AlphaClass::Blend => "blend",
    }
}

fn class_index(class: AlphaClass) -> usize {
    match class {
        AlphaClass::Opaque => 0,
        AlphaClass::Mask => 1,
        AlphaClass::Blend => 2,
    }
}

pub fn atlas(model: &LodModel, max_size: u32, padding: u32) -> Result<LodModel> {
    atlas_with(model, max_size, padding, AtlasMode::FullBleed)
}

pub fn atlas_with(
    model: &LodModel,
    max_size: u32,
    padding: u32,
    mode: AtlasMode,
) -> Result<LodModel> {
    if model.primitives.is_empty() {
        bail!("atlas: model has no primitives");
    }
    let mut max_pot = 1u32;
    while max_pot * 2 <= max_size {
        max_pot *= 2;
    }
    if max_pot <= 2 * padding + MIN_TILE_DIM {
        bail!("atlas: max size {max_size} too small for padding {padding}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();
    #[cfg(target_arch = "wasm32")]
    let atlas_ms: u128 = 0;
    let mut log = model.log.clone();
    let mut needed = vec![false; model.images.len()];
    for prim in &model.primitives {
        if prim.positions.is_empty() || prim.indices.len() < 3 {
            continue;
        }
        if let Some(idx) = model
            .materials
            .get(prim.material)
            .and_then(|m| m.image)
            .filter(|&i| i < needed.len())
        {
            needed[idx] = true;
        }
    }
    let decoded: Vec<Option<(RgbaImage, String)>> = needed
        .par_iter()
        .enumerate()
        .map(|(idx, &need)| {
            if !need {
                return None;
            }
            image::load_from_memory(&model.images[idx].bytes)
                .ok()
                .map(|d| {
                    (
                        d.to_rgba8(),
                        crate::hashes::sha256_hex(&model.images[idx].bytes),
                    )
                })
        })
        .collect();
    for idx in 0..decoded.len() {
        if needed[idx] && decoded[idx].is_none() {
            log.push(format!(
                "atlas: image {idx} failed to decode, using solid base-color tile"
            ));
        }
    }
    let mut buckets: [Bucket; 3] = std::array::from_fn(|_| Bucket::default());

    for (pi, prim) in model.primitives.iter().enumerate() {
        if prim.positions.is_empty() || prim.indices.len() < 3 {
            continue;
        }
        let mat = model
            .materials
            .get(prim.material)
            .ok_or_else(|| anyhow!("atlas: primitive {pi} references missing material"))?;
        let bucket = &mut buckets[class_index(mat.class)];
        bucket.refs += 1;
        let img_ref: Option<&(RgbaImage, String)> = match mat.image {
            Some(idx) if idx < model.images.len() => decoded[idx].as_ref(),
            _ => None,
        };
        match img_ref {
            None => {
                let color = solid_color(mat.base_color);
                let ti = intern_tile(bucket, TileKey::Solid(color), || solid_tile(color));
                bucket.prims.push((pi, ti, UvMap::Center));
            }
            Some((img, img_hash)) => match uv_plan(&prim.uvs) {
                UvPlan::Rect { shift, reps } => {
                    let key = TileKey::Image {
                        hash: img_hash.clone(),
                        tint: tint_bits(mat.base_color),
                        reps,
                    };
                    let ti = intern_tile(bucket, key, || {
                        let tinted = tinted_pixels(img, mat.base_color);
                        let (px, w, h) =
                            repeat_pixels(tinted, img.width(), img.height(), reps.0, reps.1);
                        let (px, w, h) = clamp_to_cap(px, w, h, max_pot);
                        Tile::from_pixels(px, w, h)
                    });
                    bucket.prims.push((
                        pi,
                        ti,
                        UvMap::Rect {
                            shift,
                            reps: [reps.0 as f64, reps.1 as f64],
                        },
                    ));
                }
                UvPlan::Fallback { span } => {
                    bucket.fallbacks += 1;
                    log.push(format!(
                        "atlas: WARN fallback prim {pi} material {:?}: uv span {:.3}x{:.3} exceeds {MAX_REPEATS}x{MAX_REPEATS} repeats, collapsed to average-color tile",
                        mat.name, span[0], span[1]
                    ));
                    let tinted = tinted_pixels(img, mat.base_color);
                    let color = average_color(&tinted);
                    let ti = intern_tile(bucket, TileKey::Solid(color), || solid_tile(color));
                    bucket.prims.push((pi, ti, UvMap::Center));
                }
            },
        }
    }

    let mut out = LodModel {
        root_name: model.root_name.clone(),
        ..Default::default()
    };
    type HeavyOut = (Packed, LodImage, Vec<Option<[u32; 4]>>);
    let heavy: Vec<Option<Result<HeavyOut>>> = CLASS_ORDER
        .par_iter()
        .zip(buckets.par_iter_mut())
        .map(|(&class, bucket)| {
            if bucket.prims.is_empty() {
                return None;
            }
            Some((|| {
                let mut crops = match mode {
                    AtlasMode::Native => native_crops(bucket, model),
                    AtlasMode::FullBleed => vec![None; bucket.tiles.len()],
                };
                let packed = pack_bucket(&mut bucket.tiles, &mut crops, mode, max_pot, padding)?;
                let canvas_px =
                    compose(&bucket.tiles, &crops, &packed.rects, packed.canvas, padding);
                let img = encode_atlas(class, canvas_px, packed.canvas)?;
                Ok((packed, img, crops))
            })())
        })
        .collect();
    let mut total_fallbacks = 0usize;
    for (ci, item) in heavy.into_iter().enumerate() {
        let Some(res) = item else {
            continue;
        };
        let (packed, img, crops) = res?;
        let class = CLASS_ORDER[ci];
        let bucket = &buckets[ci];
        let mime = img.mime.clone();
        let img_idx = out.images.len();
        out.images.push(img);
        let mat_idx = out.materials.len();
        out.materials.push(LodMaterial {
            name: class_material_name(class).to_string(),
            class,
            base_color: [1.0, 1.0, 1.0, 1.0],
            cutoff: 0.5,
            image: Some(img_idx),
            double_sided: false,
        });
        let s = packed.canvas as f64;
        let mut merged = LodPrimitive {
            material: mat_idx,
            ..Default::default()
        };
        for (pi, ti, uvmap) in &bucket.prims {
            let prim = &model.primitives[*pi];
            let base = merged.positions.len() as u32;
            merged.positions.extend_from_slice(&prim.positions);
            merged.normals.extend_from_slice(&prim.normals);
            let (rx, ry, rw, rh) = packed.rects[*ti];
            let tile = &bucket.tiles[*ti];
            let crop = crops[*ti];
            for uv in &prim.uvs {
                let (lu, lv) = match uvmap {
                    UvMap::Center => (0.5, 0.5),
                    UvMap::Rect { shift, reps } => {
                        let mut lu = ((uv[0] as f64 - shift[0]) / reps[0]).clamp(0.0, 1.0);
                        let mut lv = ((uv[1] as f64 - shift[1]) / reps[1]).clamp(0.0, 1.0);
                        if let Some([cx, cy, cw, ch]) = crop {
                            lu = ((lu * tile.src_w as f64 - cx as f64) / cw as f64).clamp(0.0, 1.0);
                            lv = ((lv * tile.src_h as f64 - cy as f64) / ch as f64).clamp(0.0, 1.0);
                        }
                        (lu, lv)
                    }
                };
                merged.uvs.push([
                    ((rx as f64 + lu * rw as f64) / s) as f32,
                    ((ry as f64 + lv * rh as f64) / s) as f32,
                ]);
            }
            for &i in &prim.indices {
                merged.indices.push(base + i);
            }
        }
        weld_primitive(&mut merged);
        let occupancy = packed
            .rects
            .iter()
            .map(|r| r.2 as f64 * r.3 as f64)
            .sum::<f64>()
            / (s * s)
            * 100.0;
        log.push(format!(
            "atlas: class={} material={} size={} refs={} unique={} occupancy={:.1}% fallbacks={} scale={:.3} mime={}",
            class_tag(class),
            class_material_name(class),
            packed.canvas,
            bucket.refs,
            bucket.tiles.len(),
            occupancy,
            bucket.fallbacks,
            packed.scale,
            mime
        ));
        total_fallbacks += bucket.fallbacks;
        out.primitives.push(merged);
    }
    if out.primitives.is_empty() {
        bail!("atlas: no non-empty primitives");
    }
    log.push(format!(
        "atlas: classes={} prims_in={} prims_out={} images_in={} images_out={} fallbacks={} tris_in={} tris_out={} elapsed_ms={}",
        out.primitives.len(),
        model.primitives.len(),
        out.primitives.len(),
        model.images.len(),
        out.images.len(),
        total_fallbacks,
        model.total_tris(),
        out.primitives.iter().map(|p| p.indices.len() / 3).sum::<usize>(),
        {
            #[cfg(not(target_arch = "wasm32"))]
            let atlas_ms = t0.elapsed().as_millis();
            atlas_ms
        }
    ));
    out.log = log;
    Ok(out)
}
