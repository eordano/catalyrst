//! Magenta "broken-asset" placeholder textures with the failure reason baked in
//! as text. Used when `--magenta-missing` is set: instead of failing an entity
//! (missing dependency) or silently dropping an undecodable texture, abgen
//! substitutes one of these so the asset is renderable AND obviously broken —
//! the universal missing-texture magenta, with a yellow/black-outlined label
//! that reads e.g. `MISSING: scifipack_tx.png`.
//!
//! Dependency-free + deterministic: a hand-authored 5x7 bitmap font (verifiable
//! in source, no font crate, no anti-aliasing) rendered onto a solid canvas.
//! This path is gated behind a flag and excluded from parity corpora, so it
//! never affects byte-exact reference output.

use image::{Rgba, RgbaImage};

const MAGENTA: Rgba<u8> = Rgba([255, 0, 255, 255]);
const YELLOW: Rgba<u8> = Rgba([255, 255, 0, 255]);
const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// 5x7 bitmap font. `#` = pixel on, anything else = off. Uppercase only —
/// `draw_text` upper-cases input. Covers A-Z, 0-9 and the punctuation that
/// shows up in content paths / error strings. Unknown chars render as a box.
#[rustfmt::skip]
fn glyph(c: char) -> [&'static str; GLYPH_H] {
    match c {
        'A' => [" ### ","#   #","#   #","#####","#   #","#   #","#   #"],
        'B' => ["#### ","#   #","#   #","#### ","#   #","#   #","#### "],
        'C' => [" ####","#    ","#    ","#    ","#    ","#    "," ####"],
        'D' => ["#### ","#   #","#   #","#   #","#   #","#   #","#### "],
        'E' => ["#####","#    ","#    ","#### ","#    ","#    ","#####"],
        'F' => ["#####","#    ","#    ","#### ","#    ","#    ","#    "],
        'G' => [" ####","#    ","#    ","#  ##","#   #","#   #"," ####"],
        'H' => ["#   #","#   #","#   #","#####","#   #","#   #","#   #"],
        'I' => ["#####","  #  ","  #  ","  #  ","  #  ","  #  ","#####"],
        'J' => ["#####","   # ","   # ","   # ","   # ","#  # "," ##  "],
        'K' => ["#   #","#  # ","# #  ","##   ","# #  ","#  # ","#   #"],
        'L' => ["#    ","#    ","#    ","#    ","#    ","#    ","#####"],
        'M' => ["#   #","## ##","# # #","#   #","#   #","#   #","#   #"],
        'N' => ["#   #","##  #","# # #","#  ##","#   #","#   #","#   #"],
        'O' => [" ### ","#   #","#   #","#   #","#   #","#   #"," ### "],
        'P' => ["#### ","#   #","#   #","#### ","#    ","#    ","#    "],
        'Q' => [" ### ","#   #","#   #","#   #","# # #","#  # "," ## #"],
        'R' => ["#### ","#   #","#   #","#### ","# #  ","#  # ","#   #"],
        'S' => [" ####","#    ","#    "," ### ","    #","    #","#### "],
        'T' => ["#####","  #  ","  #  ","  #  ","  #  ","  #  ","  #  "],
        'U' => ["#   #","#   #","#   #","#   #","#   #","#   #"," ### "],
        'V' => ["#   #","#   #","#   #","#   #","#   #"," # # ","  #  "],
        'W' => ["#   #","#   #","#   #","#   #","# # #","## ##","#   #"],
        'X' => ["#   #","#   #"," # # ","  #  "," # # ","#   #","#   #"],
        'Y' => ["#   #","#   #"," # # ","  #  ","  #  ","  #  ","  #  "],
        'Z' => ["#####","    #","   # ","  #  "," #   ","#    ","#####"],
        '0' => [" ### ","#   #","#  ##","# # #","##  #","#   #"," ### "],
        '1' => ["  #  "," ##  ","  #  ","  #  ","  #  ","  #  "," ### "],
        '2' => [" ### ","#   #","    #","   # ","  #  "," #   ","#####"],
        '3' => ["#####","   # ","  #  ","   # ","    #","#   #"," ### "],
        '4' => ["   # ","  ## "," # # ","#  # ","#####","   # ","   # "],
        '5' => ["#####","#    ","#### ","    #","    #","#   #"," ### "],
        '6' => [" ### ","#    ","#    ","#### ","#   #","#   #"," ### "],
        '7' => ["#####","    #","   # ","  #  "," #   "," #   "," #   "],
        '8' => [" ### ","#   #","#   #"," ### ","#   #","#   #"," ### "],
        '9' => [" ### ","#   #","#   #"," ####","    #","    #"," ### "],
        '.' => ["     ","     ","     ","     ","     "," ##  "," ##  "],
        ',' => ["     ","     ","     ","     "," ##  "," ##  ","#    "],
        ':' => ["     "," ##  "," ##  ","     "," ##  "," ##  ","     "],
        '-' => ["     ","     ","     ","#####","     ","     ","     "],
        '_' => ["     ","     ","     ","     ","     ","     ","#####"],
        '/' => ["    #","    #","   # ","  #  "," #   ","#    ","#    "],
        '!' => ["  #  ","  #  ","  #  ","  #  ","  #  ","     ","  #  "],
        '(' => ["   # ","  #  "," #   "," #   "," #   ","  #  ","   # "],
        ')' => [" #   ","  #  ","   # ","   # ","   # ","  #  "," #   "],
        '\'' => ["  #  ","  #  "," #   ","     ","     ","     ","     "],
        '"' => [" # # "," # # "," # # ","     ","     ","     ","     "],
        ' ' => ["     ","     ","     ","     ","     ","     ","     "],
        _   => ["#####","#   #","#   #","#   #","#   #","#   #","#####"],
    }
}

/// Set a pixel if in bounds.
fn put(img: &mut RgbaImage, x: i64, y: i64, col: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, col);
    }
}

/// Draw one glyph at (ox, oy) scaled by `scale`, with an 8-neighbour black
/// outline under a yellow fill (legible over magenta).
fn draw_glyph(img: &mut RgbaImage, c: char, ox: i64, oy: i64, scale: i64) {
    let g = glyph(c.to_ascii_uppercase());
    for (ry, row) in g.iter().enumerate() {
        for (cx, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            // each font pixel becomes a scale x scale block
            for sy in 0..scale {
                for sx in 0..scale {
                    let px = ox + cx as i64 * scale + sx;
                    let py = oy + ry as i64 * scale + sy;
                    // outline first (8 neighbours), then fill on top
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            put(img, px + dx, py + dy, BLACK);
                        }
                    }
                }
            }
        }
    }
    for (ry, row) in g.iter().enumerate() {
        for (cx, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    put(img, ox + cx as i64 * scale + sx, oy + ry as i64 * scale + sy, YELLOW);
                }
            }
        }
    }
}

/// Greedy word-wrap to `cols` characters per line.
fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line = word.to_string();
        } else if line.len() + 1 + word.len() <= cols {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        }
        // hard-break a single word longer than the line width
        while line.len() > cols {
            out.push(line[..cols].to_string());
            line = line[cols..].to_string();
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// Build a square magenta placeholder texture (`size` x `size`) with `lines` of
/// label text rendered yellow-on-black-outline, centred vertically. The first
/// line is treated as a heading (drawn at a larger scale).
pub fn error_texture(lines: &[&str], size: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(size, size, MAGENTA);

    // Scale the font so a heading line is comfortably readable at this size.
    let head_scale = (size as i64 / 64).max(2);
    let body_scale = (head_scale - 1).max(1);
    let pad = (size as i64 / 16).max(2);

    // wrap each provided line to the body width, tagging its scale
    let cols_for = |scale: i64| -> usize {
        let cell = (GLYPH_W as i64 + 1) * scale;
        (((size as i64 - 2 * pad) / cell).max(1)) as usize
    };
    let mut rendered: Vec<(String, i64)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let scale = if i == 0 { head_scale } else { body_scale };
        for w in wrap(l, cols_for(scale)) {
            rendered.push((w, scale));
        }
    }

    let line_h = |scale: i64| (GLYPH_H as i64 + 2) * scale;
    let total_h: i64 = rendered.iter().map(|(_, s)| line_h(*s)).sum();
    let mut y = ((size as i64 - total_h) / 2).max(pad);

    for (text, scale) in &rendered {
        let text_w = text.chars().count() as i64 * (GLYPH_W as i64 + 1) * scale;
        let mut x = ((size as i64 - text_w) / 2).max(pad);
        for c in text.chars() {
            draw_glyph(&mut img, c, x, y, *scale);
            x += (GLYPH_W as i64 + 1) * scale;
        }
        y += line_h(*scale);
    }
    img
}

/// Convenience: a `MISSING:` / `BAD TEXTURE:` placeholder naming the asset.
pub fn missing_texture(kind: &str, name: &str, size: u32) -> RgbaImage {
    // strip directories for the heading; keep it short and legible
    let base = name.rsplit('/').next().unwrap_or(name);
    error_texture(&[kind, base], size)
}
