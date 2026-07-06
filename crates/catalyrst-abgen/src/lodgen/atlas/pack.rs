use anyhow::{bail, Result};

use super::tile::Tile;
use super::{
    AtlasMode, GROW_TRIES, MAX_PACK_TRIES, MIN_TILE_DIM, NATIVE_MIN_CANVAS, NATIVE_SOLID_DIM,
    SHRINK_STEP, TARGET_OCCUPANCY,
};

struct SkyNode {
    x: u32,
    w: u32,
    y: u32,
}

pub(super) fn pack_skyline(
    sizes: &[(u32, u32)],
    order: &[usize],
    canvas: u32,
) -> Option<Vec<(u32, u32)>> {
    let mut nodes = vec![SkyNode {
        x: 0,
        w: canvas,
        y: 0,
    }];
    let mut out = vec![(0u32, 0u32); sizes.len()];
    for &ti in order {
        let (w, h) = sizes[ti];
        if w > canvas || h > canvas || w == 0 || h == 0 {
            return None;
        }
        let mut best: Option<(u32, u32)> = None;
        for i in 0..nodes.len() {
            let x = nodes[i].x;
            if x + w > canvas {
                continue;
            }
            let mut covered = 0u32;
            let mut y = 0u32;
            let mut j = i;
            while covered < w {
                y = y.max(nodes[j].y);
                covered += nodes[j].w;
                j += 1;
            }
            if y + h > canvas {
                continue;
            }
            if best.is_none_or(|(by, bx)| (y, x) < (by, bx)) {
                best = Some((y, x));
            }
        }
        let (y, x) = best?;
        place(&mut nodes, x, w, y + h);
        out[ti] = (x, y);
    }
    Some(out)
}

fn place(nodes: &mut Vec<SkyNode>, x: u32, w: u32, top: u32) {
    let end = x + w;
    let mut i = 0;
    while i < nodes.len() && nodes[i].x + nodes[i].w <= x {
        i += 1;
    }
    if i < nodes.len() && nodes[i].x < x {
        let keep = x - nodes[i].x;
        let split = SkyNode {
            x,
            w: nodes[i].w - keep,
            y: nodes[i].y,
        };
        nodes[i].w = keep;
        nodes.insert(i + 1, split);
        i += 1;
    }
    while i < nodes.len() && nodes[i].x + nodes[i].w <= end {
        nodes.remove(i);
    }
    if i < nodes.len() && nodes[i].x < end {
        let cut = end - nodes[i].x;
        nodes[i].x = end;
        nodes[i].w -= cut;
    }
    nodes.insert(i, SkyNode { x, w, y: top });
    let mut j = 0;
    while j + 1 < nodes.len() {
        if nodes[j].y == nodes[j + 1].y {
            nodes[j].w += nodes[j + 1].w;
            nodes.remove(j + 1);
        } else {
            j += 1;
        }
    }
}

pub(super) struct Packed {
    pub(super) rects: Vec<(u32, u32, u32, u32)>,
    pub(super) canvas: u32,
    pub(super) scale: f64,
}

fn pot_floor(v: u32, lo: u32, hi: u32) -> u32 {
    let mut c = lo;
    while c * 2 <= v.min(hi) {
        c *= 2;
    }
    c.min(hi)
}

fn pot_ceil(v: u32, lo: u32, hi: u32) -> u32 {
    let mut c = lo;
    while c < v && c < hi {
        c *= 2;
    }
    c.min(hi)
}

pub(super) fn expand_axis(a0: &mut u32, a1: &mut u32, full: u32, want: u32) {
    let want = want.min(full);
    if *a1 - *a0 < want {
        *a1 = (*a0 + want).min(full);
        *a0 = a1.saturating_sub(want);
    }
}

fn crop_dims(tile: &Tile, crop: &Option<[u32; 4]>) -> (u32, u32) {
    match crop {
        Some([_, _, w, h]) => (*w, *h),
        None => (tile.src_w, tile.src_h),
    }
}

fn pack_single(
    tiles: &mut [Tile],
    crops: &mut [Option<[u32; 4]>],
    mode: AtlasMode,
    max_pot: u32,
) -> Packed {
    let side = match mode {
        AtlasMode::FullBleed => {
            if tiles[0].is_solid() {
                max_pot
            } else {
                pot_floor(tiles[0].src_w.max(tiles[0].src_h), MIN_TILE_DIM, max_pot)
            }
        }
        AtlasMode::Native => {
            if tiles[0].is_solid() {
                NATIVE_SOLID_DIM
            } else {
                let (cw, ch) = crop_dims(&tiles[0], &crops[0]);
                pot_ceil(cw.max(ch), NATIVE_MIN_CANVAS, max_pot)
            }
        }
    };
    let (w, h) = match mode {
        AtlasMode::FullBleed => (side, side),
        AtlasMode::Native => {
            if tiles[0].is_solid() {
                (side, side)
            } else {
                let full_w = tiles[0].src_w;
                let full_h = tiles[0].src_h;
                let [mut x0, mut y0, cw, ch] = crops[0].unwrap_or([0, 0, full_w, full_h]);
                let mut x1 = x0 + cw;
                let mut y1 = y0 + ch;
                expand_axis(&mut x0, &mut x1, full_w, side);
                expand_axis(&mut y0, &mut y1, full_h, side);
                crops[0] = Some([x0, y0, x1 - x0, y1 - y0]);
                ((x1 - x0).min(side), (y1 - y0).min(side))
            }
        }
    };
    tiles[0].w = w;
    tiles[0].h = h;
    Packed {
        rects: vec![(0, 0, w, h)],
        canvas: side,
        scale: 1.0,
    }
}

pub(super) fn pack_bucket(
    tiles: &mut [Tile],
    crops: &mut [Option<[u32; 4]>],
    mode: AtlasMode,
    max_pot: u32,
    padding: u32,
) -> Result<Packed> {
    if tiles.len() == 1 {
        return Ok(pack_single(tiles, crops, mode, max_pot));
    }
    let mut canvas = max_pot;
    let cap = canvas - 2 * padding;
    let natives: Vec<(u32, u32)> = tiles
        .iter()
        .zip(crops.iter())
        .map(|(t, c)| match mode {
            AtlasMode::FullBleed => {
                if t.is_solid() {
                    (cap, cap)
                } else {
                    (t.src_w.min(cap), t.src_h.min(cap))
                }
            }
            AtlasMode::Native => {
                if t.is_solid() {
                    (NATIVE_SOLID_DIM.min(cap), NATIVE_SOLID_DIM.min(cap))
                } else {
                    let (w, h) = crop_dims(t, c);
                    (w.min(cap), h.min(cap))
                }
            }
        })
        .collect();
    let dims_at = |scale: f64| -> Vec<(u32, u32)> {
        tiles
            .iter()
            .zip(natives.iter())
            .map(|(t, &(nw, nh))| {
                if mode == AtlasMode::Native && t.is_solid() {
                    return (nw, nh);
                }
                let (bw, bh) = match mode {
                    AtlasMode::FullBleed => (t.src_w, t.src_h),
                    AtlasMode::Native => (nw, nh),
                };
                (
                    ((bw as f64 * scale).round() as u32)
                        .max(MIN_TILE_DIM)
                        .min(nw),
                    ((bh as f64 * scale).round() as u32)
                        .max(MIN_TILE_DIM)
                        .min(nh),
                )
            })
            .collect()
    };
    let try_pack = |dims: &[(u32, u32)], canvas: u32| -> Option<Vec<(u32, u32)>> {
        let sizes: Vec<(u32, u32)> = dims
            .iter()
            .map(|&(w, h)| (w + 2 * padding, h + 2 * padding))
            .collect();
        let mut order: Vec<usize> = (0..dims.len()).collect();
        order.sort_by(|&a, &b| {
            let da = dims[a].0.max(dims[a].1);
            let db = dims[b].0.max(dims[b].1);
            db.cmp(&da).then_with(|| tiles[a].hash.cmp(&tiles[b].hash))
        });
        pack_skyline(&sizes, &order, canvas)
    };
    let occ_of = |dims: &[(u32, u32)]| -> f64 {
        dims.iter().map(|&(w, h)| w as f64 * h as f64).sum::<f64>()
            / (canvas as f64 * canvas as f64)
    };
    let padded_area: f64 = natives
        .iter()
        .map(|&(w, h)| (w + 2 * padding) as f64 * (h + 2 * padding) as f64)
        .sum();
    let mut scale = (TARGET_OCCUPANCY * canvas as f64 * canvas as f64 / padded_area)
        .sqrt()
        .min(1.0);
    let mut fitted: Option<(Vec<(u32, u32)>, Vec<(u32, u32)>, f64)> = None;
    for _ in 0..MAX_PACK_TRIES {
        let dims = dims_at(scale);
        if let Some(pos) = try_pack(&dims, canvas) {
            fitted = Some((pos, dims, scale));
            break;
        }
        if dims
            .iter()
            .all(|&(w, h)| w <= MIN_TILE_DIM && h <= MIN_TILE_DIM)
        {
            break;
        }
        scale *= SHRINK_STEP;
    }
    let Some((mut pos, mut dims, scale)) = fitted else {
        bail!(
            "atlas overflow: {} tiles cannot fit {}x{} even at minimum tile size",
            tiles.len(),
            canvas,
            canvas
        );
    };
    let occ = occ_of(&dims);
    if occ < TARGET_OCCUPANCY {
        let mut g = scale * (TARGET_OCCUPANCY / occ).sqrt();
        for _ in 0..GROW_TRIES {
            if g <= scale {
                break;
            }
            let gd = dims_at(g);
            if gd == dims {
                break;
            }
            if let Some(gp) = try_pack(&gd, canvas) {
                pos = gp;
                dims = gd;
                break;
            }
            g *= SHRINK_STEP;
        }
    }
    if mode == AtlasMode::Native {
        let extent = pos
            .iter()
            .zip(dims.iter())
            .map(|(&(x, y), &(w, h))| (x + w + 2 * padding).max(y + h + 2 * padding))
            .max()
            .unwrap_or(canvas);
        let mut cand = pot_ceil(extent, NATIVE_MIN_CANVAS, canvas);
        while cand < canvas {
            if let Some(p2) = try_pack(&dims, cand) {
                pos = p2;
                canvas = cand;
                break;
            }
            cand *= 2;
        }
    }
    for (t, &(w, h)) in tiles.iter_mut().zip(dims.iter()) {
        t.w = w;
        t.h = h;
    }
    let rects = pos
        .iter()
        .zip(dims.iter())
        .map(|(&(x, y), &(w, h))| (x + padding, y + padding, w, h))
        .collect();
    Ok(Packed {
        rects,
        canvas,
        scale,
    })
}
