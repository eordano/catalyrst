use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use lru::LruCache;
use parking_lot::Mutex;
use tiny_skia::{IntSize, Pixmap};

const WORLD_MIN: f64 = -256.0;
const WORLD_MAX: f64 = 256.0;
const WORLD_PARCELS: f64 = WORLD_MAX - WORLD_MIN;

const TILE_PX: u32 = 256;

const REGION_PARCELS: f64 = 17.0;
const REGION_HALF: i32 = 8;

const MAX_ZOOM: i32 = 9;

const THUMB_ZOOM_MAX: i32 = 4;

const MAX_SAMPLES: i32 = 3;

struct Region {
    w: u32,
    h: u32,

    ppp: f64,

    data: Vec<u8>,
}

impl Region {
    fn bytes(&self) -> usize {
        self.data.len() + std::mem::size_of::<Region>()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SrcKey {
    cx: i32,
    cy: i32,
    thumb: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    z: i32,
    x: i32,
    y: i32,
}

#[derive(Clone)]
struct CachedTile {
    generation: u64,
    bytes: Arc<Vec<u8>>,
}

struct Index {
    tiles: HashMap<(i32, i32), i64>,
    generation: u64,
    signature: u64,
}

struct SourceCache {
    map: LruCache<SrcKey, Arc<Region>>,
    bytes: usize,
    budget: usize,
}

pub struct SatelliteState {
    base: PathBuf,
    index: Mutex<Index>,
    sources: Mutex<SourceCache>,
    output: Mutex<LruCache<TileKey, CachedTile>>,

    empty_png: Arc<Vec<u8>>,
}

impl SatelliteState {
    pub fn new(base: PathBuf, source_budget_bytes: usize, output_entries: usize) -> Arc<Self> {
        let empty = Pixmap::new(TILE_PX, TILE_PX)
            .and_then(|p| p.encode_png().ok())
            .unwrap_or_default();
        let st = Arc::new(SatelliteState {
            base,
            index: Mutex::new(Index {
                tiles: HashMap::new(),
                generation: 0,
                signature: 0,
            }),
            sources: Mutex::new(SourceCache {
                map: LruCache::new(NonZeroUsize::new(4096).unwrap()),
                bytes: 0,
                budget: source_budget_bytes,
            }),
            output: Mutex::new(LruCache::new(
                NonZeroUsize::new(output_entries.max(1)).unwrap(),
            )),
            empty_png: Arc::new(empty),
        });
        st.scan();
        st
    }

    pub fn scan(&self) {
        let mut tiles: HashMap<(i32, i32), i64> = HashMap::new();
        let mut sig: u64 = 0xcbf29ce484222325;
        let hash = |v: i64, s: &mut u64| {
            *s ^= v as u64;
            *s = s.wrapping_mul(0x100000001b3);
        };
        if let Ok(cols) = std::fs::read_dir(&self.base) {
            for col in cols.flatten() {
                let Some(cx) = col.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
                    continue;
                };
                let Ok(rows) = std::fs::read_dir(col.path()) else {
                    continue;
                };
                for f in rows.flatten() {
                    let name = f.file_name();
                    let Some(name) = name.to_str() else { continue };

                    let Some(stem) = name.strip_suffix(".png") else {
                        continue;
                    };
                    if stem.ends_with(".t") {
                        continue;
                    }
                    let Ok(cy) = stem.parse::<i32>() else {
                        continue;
                    };
                    let mtime = f
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    tiles.insert((cx, cy), mtime);
                    hash(cx as i64, &mut sig);
                    hash(cy as i64, &mut sig);
                    hash(mtime, &mut sig);
                }
            }
        }
        let mut idx = self.index.lock();
        if sig != idx.signature {
            tracing::info!(
                regions = tiles.len(),
                generation = idx.generation + 1,
                "satellite tile set changed; bumping generation"
            );
            idx.tiles = tiles;
            idx.signature = sig;
            idx.generation += 1;
        }
    }

    pub fn tile_png(&self, z: i32, x: i32, y: i32) -> Option<Arc<Vec<u8>>> {
        if !(0..=MAX_ZOOM).contains(&z) {
            return None;
        }
        let n = 1i32 << z;
        if x < 0 || y < 0 || x >= n || y >= n {
            return None;
        }
        let key = TileKey { z, x, y };

        let (generation, overlap) = {
            let idx = self.index.lock();
            let gen = idx.generation;
            let (wx0, wx1, wy0, wy1) = tile_world_bounds(z, x, y);
            let overlap: Vec<(i32, i32)> = idx
                .tiles
                .keys()
                .copied()
                .filter(|&(cx, cy)| {
                    let rx0 = (cx - REGION_HALF) as f64;
                    let rx1 = (cx + REGION_HALF + 1) as f64;
                    let ry0 = (cy - REGION_HALF) as f64;
                    let ry1 = (cy + REGION_HALF + 1) as f64;
                    rx0 < wx1 && rx1 > wx0 && ry0 < wy1 && ry1 > wy0
                })
                .collect();
            (gen, overlap)
        };

        if let Some(hit) = self.output.lock().get(&key) {
            if hit.generation == generation {
                return Some(hit.bytes.clone());
            }
        }

        let bytes = if overlap.is_empty() {
            self.empty_png.clone()
        } else {
            Arc::new(self.render_tile(z, x, y, &overlap))
        };

        self.output.lock().put(
            key,
            CachedTile {
                generation,
                bytes: bytes.clone(),
            },
        );
        Some(bytes)
    }

    fn render_tile(&self, z: i32, x: i32, y: i32, overlap: &[(i32, i32)]) -> Vec<u8> {
        let (wx0, _wx1, _wy0, wy1) = tile_world_bounds(z, x, y);

        let ppp_out = parcels_per_tile(z) / TILE_PX as f64;
        let want_thumb = z <= THUMB_ZOOM_MAX;

        let mut out = vec![0u8; (TILE_PX * TILE_PX * 4) as usize];

        for &(cx, cy) in overlap {
            let Some(region) = self.region(cx, cy, want_thumb) else {
                continue;
            };

            let rx0 = (cx - REGION_HALF) as f64;
            let ry_top = (cy + REGION_HALF + 1) as f64;
            let ry_bot = (cy - REGION_HALF) as f64;

            let ox_lo = (((cx - REGION_HALF) as f64 - wx0) / ppp_out)
                .floor()
                .max(0.0) as i32;
            let ox_hi = (((cx + REGION_HALF + 1) as f64 - wx0) / ppp_out).ceil() as i32;
            let ox_hi = ox_hi.min(TILE_PX as i32);
            let oy_lo = ((wy1 - ry_top) / ppp_out).floor().max(0.0) as i32;
            let oy_hi = ((wy1 - ry_bot) / ppp_out).ceil() as i32;
            let oy_hi = oy_hi.min(TILE_PX as i32);

            let fp = ppp_out * region.ppp;
            let samples = (fp.round() as i32).clamp(1, MAX_SAMPLES);

            for oy in oy_lo..oy_hi {
                let world_y = wy1 - (oy as f64 + 0.5) * ppp_out;
                for ox in ox_lo..ox_hi {
                    let world_x = wx0 + (ox as f64 + 0.5) * ppp_out;

                    let sc = (world_x - rx0) * region.ppp;
                    let sr = (ry_top - world_y) * region.ppp;

                    let (mut r, mut g, mut b, mut a, mut hit) = (0u32, 0u32, 0u32, 0u32, 0u32);
                    for sy in 0..samples {
                        let fy = sr + (sy as f64 + 0.5) / samples as f64 * fp - fp / 2.0;
                        let py = fy.floor() as i32;
                        if py < 0 || py >= region.h as i32 {
                            continue;
                        }
                        for sx in 0..samples {
                            let fx = sc + (sx as f64 + 0.5) / samples as f64 * fp - fp / 2.0;
                            let px = fx.floor() as i32;
                            if px < 0 || px >= region.w as i32 {
                                continue;
                            }
                            let i = ((py as u32 * region.w + px as u32) * 4) as usize;
                            r += region.data[i] as u32;
                            g += region.data[i + 1] as u32;
                            b += region.data[i + 2] as u32;
                            a += region.data[i + 3] as u32;
                            hit += 1;
                        }
                    }
                    if hit == 0 {
                        continue;
                    }
                    let o = ((oy as u32 * TILE_PX + ox as u32) * 4) as usize;
                    out[o] = (r / hit) as u8;
                    out[o + 1] = (g / hit) as u8;
                    out[o + 2] = (b / hit) as u8;
                    out[o + 3] = (a / hit) as u8;
                }
            }
        }

        match Pixmap::from_vec(out, IntSize::from_wh(TILE_PX, TILE_PX).unwrap()) {
            Some(pm) => pm
                .encode_png()
                .unwrap_or_else(|_| (*self.empty_png).clone()),
            None => (*self.empty_png).clone(),
        }
    }

    fn region(&self, cx: i32, cy: i32, thumb: bool) -> Option<Arc<Region>> {
        let key = SrcKey { cx, cy, thumb };
        if let Some(r) = self.sources.lock().map.get(&key) {
            return Some(r.clone());
        }

        let full = self.base.join(cx.to_string()).join(format!("{cy}.png"));
        let path = if thumb {
            let t = self.base.join(cx.to_string()).join(format!("{cy}.t.png"));
            if t.exists() {
                t
            } else {
                full
            }
        } else {
            full
        };
        let raw = std::fs::read(&path).ok()?;
        let pm = Pixmap::decode_png(&raw).ok()?;
        let w = pm.width();
        let h = pm.height();
        let region = Arc::new(Region {
            w,
            h,
            ppp: w as f64 / REGION_PARCELS,
            data: pm.data().to_vec(),
        });

        let mut sc = self.sources.lock();
        sc.bytes += region.bytes();
        sc.map.put(key, region.clone());
        while sc.bytes > sc.budget && sc.map.len() > 1 {
            if let Some((_, evicted)) = sc.map.pop_lru() {
                sc.bytes = sc.bytes.saturating_sub(evicted.bytes());
            } else {
                break;
            }
        }
        Some(region)
    }
}

#[inline]
fn parcels_per_tile(z: i32) -> f64 {
    WORLD_PARCELS / (1i32 << z) as f64
}

fn tile_world_bounds(z: i32, x: i32, y: i32) -> (f64, f64, f64, f64) {
    let span = parcels_per_tile(z);
    let x0 = WORLD_MIN + x as f64 * span;
    let x1 = x0 + span;
    let y1 = WORLD_MAX - y as f64 * span;
    let y0 = y1 - span;
    (x0, x1, y0, y1)
}
