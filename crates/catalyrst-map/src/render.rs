use std::collections::HashSet;

use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

use crate::map::{coords_to_id, MapData, Tile, TileType};

#[derive(Clone, Copy)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

struct Viewport {
    nw: Coord,
    se: Coord,
}

fn get_viewport(width: u32, height: u32, center: Coord, size: u32) -> Viewport {
    let padding = 1i32;
    let dim_w = ((width as f64 / size as f64).ceil() as i32) + padding;
    let dim_h = ((height as f64 / size as f64).ceil() as i32) + padding;
    let nw = Coord {
        x: center.x - div_ceil(dim_w, 2),
        y: center.y + div_ceil(dim_h, 2),
    };
    let se = Coord {
        x: center.x + div_ceil(dim_w, 2),
        y: center.y - div_ceil(dim_h, 2),
    };
    Viewport { nw, se }
}

#[inline]
fn div_ceil(a: i32, b: i32) -> i32 {
    (a as f64 / b as f64).ceil() as i32
}

struct RenderTile {
    color: Color,
    top: bool,
    left: bool,
    top_left: bool,
    scale: Option<f64>,
}

fn parse_hex(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    Color::from_rgba8(r, g, b, 255)
}

fn type_color(t: TileType) -> Color {
    match t {
        TileType::District => parse_hex("#5054D4"),
        TileType::Plaza => parse_hex("#70AC76"),
        TileType::Road => parse_hex("#716C7A"),
        TileType::Owned => parse_hex("#3D3A46"),
        TileType::Unowned => parse_hex("#09080A"),
    }
}

fn is_order_expired(tile: &Tile, now_secs: i64) -> bool {
    match tile.expires_at {
        Some(e) => e <= now_secs,
        None => false,
    }
}

fn is_rental_expired(rl: &crate::rentals::TileRentalListing, now_ms: i64) -> bool {
    rl.expiration < now_ms
}

pub fn render_png(
    data: &MapData,
    width: u32,
    height: u32,
    size: u32,
    center: Coord,
    selected: &[Coord],
    show_on_sale: bool,
    show_on_rent: bool,
) -> Result<Vec<u8>, String> {
    let mut pixmap = Pixmap::new(width, height).ok_or("invalid pixmap dimensions")?;
    pixmap.fill(parse_hex("#18141a"));

    let vp = get_viewport(width, height, center, size);
    let now_secs = chrono::Utc::now().timestamp();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let half_width = width as f64 / 2.0;
    let half_height = height as f64 / 2.0;
    let size_f = size as f64;
    let padding: f64 = if size < 7 {
        0.5
    } else if size < 12 {
        1.0
    } else if size < 18 {
        1.5
    } else {
        2.0
    };
    let offset: f64 = 1.0;

    let selection: HashSet<String> = selected.iter().map(|c| coords_to_id(c.x, c.y)).collect();

    let mut paint = Paint {
        anti_alias: false,
        ..Default::default()
    };

    for layer_idx in 0..(if selection.is_empty() { 1 } else { 3 }) {
        for x in vp.nw.x..vp.se.x {
            for y in vp.se.y..vp.nw.y {
                let offset_x = (center.x - x) as f64 * size_f;
                let offset_y = (y - center.y) as f64 * size_f;

                let rt: Option<RenderTile> = match layer_idx {
                    0 => {
                        let id = coords_to_id(x, y);
                        match data.tiles.get(&id) {
                            Some(tile) => {
                                let on_sale = show_on_sale
                                    && tile.price.is_some()
                                    && !is_order_expired(tile, now_secs);
                                let on_rent = show_on_rent
                                    && tile
                                        .rental_listing
                                        .as_ref()
                                        .map(|rl| !is_rental_expired(rl, now_ms))
                                        .unwrap_or(false);
                                let color = if on_sale || on_rent {
                                    parse_hex("#1FBCFF")
                                } else {
                                    type_color(tile.tile_type)
                                };
                                Some(RenderTile {
                                    color,
                                    top: tile.top,
                                    left: tile.left,
                                    top_left: tile.top_left,
                                    scale: None,
                                })
                            }
                            None => {
                                let color = if (x + y) % 2 == 0 {
                                    parse_hex("#110e13")
                                } else {
                                    parse_hex("#0d0b0e")
                                };
                                Some(RenderTile {
                                    color,
                                    top: false,
                                    left: false,
                                    top_left: false,
                                    scale: None,
                                })
                            }
                        }
                    }
                    1 => {
                        if selection.contains(&coords_to_id(x, y)) {
                            Some(RenderTile {
                                color: parse_hex("#ff0044"),
                                top: false,
                                left: false,
                                top_left: false,
                                scale: Some(1.4),
                            })
                        } else {
                            None
                        }
                    }
                    _ => {
                        if selection.contains(&coords_to_id(x, y)) {
                            Some(RenderTile {
                                color: parse_hex("#ff9990"),
                                top: false,
                                left: false,
                                top_left: false,
                                scale: Some(1.2),
                            })
                        } else {
                            None
                        }
                    }
                };

                let Some(rt) = rt else { continue };

                let half_size = match rt.scale {
                    Some(s) => (size_f * s) / 2.0,
                    None => size_f / 2.0,
                };
                let px = half_width - offset_x + half_size;
                let py = half_height - offset_y + half_size;

                paint.set_color(rt.color);
                draw_tile(&mut pixmap, &paint, px, py, size_f, padding, offset, &rt);
            }
        }
    }

    pixmap.encode_png().map_err(|e| e.to_string())
}

fn draw_tile(
    pixmap: &mut Pixmap,
    paint: &Paint,
    x: f64,
    y: f64,
    size: f64,
    padding: f64,
    offset: f64,
    rt: &RenderTile,
) {
    let tile_size = match rt.scale {
        Some(s) => size * s,
        None => size,
    };

    if !rt.top && !rt.left {
        fill_rect(
            pixmap,
            paint,
            x - tile_size + padding,
            y - tile_size + padding,
            tile_size - padding,
            tile_size - padding,
        );
    } else if rt.top && rt.left && rt.top_left {
        fill_rect(
            pixmap,
            paint,
            x - tile_size - offset,
            y - tile_size - offset,
            tile_size + offset,
            tile_size + offset,
        );
    } else {
        if rt.left {
            fill_rect(
                pixmap,
                paint,
                x - tile_size - offset,
                y - tile_size + padding,
                tile_size + offset,
                tile_size - padding,
            );
        }
        if rt.top {
            fill_rect(
                pixmap,
                paint,
                x - tile_size + padding,
                y - tile_size - offset,
                tile_size - padding,
                tile_size + offset,
            );
        }
    }
}

fn fill_rect(pixmap: &mut Pixmap, paint: &Paint, x: f64, y: f64, w: f64, h: f64) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if let Some(rect) = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32) {
        pixmap.fill_rect(rect, paint, Transform::identity(), None);
    }
}

const MINIMAP_DIM: usize = 512;

fn minimap_pixmap(rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut pixmap =
        Pixmap::new(MINIMAP_DIM as u32, MINIMAP_DIM as u32).ok_or("invalid pixmap dimensions")?;
    pixmap.data_mut().copy_from_slice(rgba);
    pixmap.encode_png().map_err(|e| e.to_string())
}

fn minimap_pointer(x: i32, y: i32) -> Option<usize> {
    if !(-256..=255).contains(&x) || !(-256..=255).contains(&y) {
        return None;
    }
    let absolute_y = MINIMAP_DIM as i32 - (y + 256);
    if !(0..MINIMAP_DIM as i32).contains(&absolute_y) {
        return None;
    }
    Some(((x + 256 + absolute_y * MINIMAP_DIM as i32) * 4) as usize)
}

pub fn render_minimap(data: &MapData) -> Result<Vec<u8>, String> {
    let mut rgba = vec![0u8; MINIMAP_DIM * MINIMAP_DIM * 4];
    for tile in data.tiles.values() {
        let mut flags_r: u8 = 0;
        if tile.top {
            flags_r |= 8;
        }
        if tile.left {
            flags_r |= 16;
        }
        if tile.top_left {
            flags_r |= 32;
        }
        let flags_g: u8 = match tile.tile_type {
            TileType::District => 32,
            TileType::Road => 64,
            TileType::Owned => 128,
            _ => 0,
        };
        let Some(pointer) = minimap_pointer(tile.x, tile.y) else {
            continue;
        };
        rgba[pointer] = flags_r;
        rgba[pointer + 1] = flags_g;
        rgba[pointer + 2] = 0;
        rgba[pointer + 3] = 255;
    }
    minimap_pixmap(&rgba)
}

pub fn render_estate_minimap(data: &MapData) -> Result<Vec<u8>, String> {
    let mut rgba = vec![0u8; MINIMAP_DIM * MINIMAP_DIM * 4];
    let mut order: Vec<String> = Vec::new();
    for tile in data.tiles.values() {
        let mut index: u32 = 0;
        if let Some(estate_id) = &tile.estate_id {
            index = match order.iter().position(|e| e == estate_id) {
                Some(pos) => pos as u32,
                None => {
                    order.push(estate_id.clone());
                    order.len() as u32
                }
            };
        }
        let flags_b = (index & 0xff) as u8;
        let flags_g = ((index >> 8) & 0xff) as u8;
        let flags_r = ((index >> 16) & 0xff) as u8;
        let Some(pointer) = minimap_pointer(tile.x, tile.y) else {
            continue;
        };
        rgba[pointer] = flags_r;
        rgba[pointer + 1] = flags_g;
        rgba[pointer + 2] = flags_b;
        rgba[pointer + 3] = 255;
    }
    minimap_pixmap(&rgba)
}
