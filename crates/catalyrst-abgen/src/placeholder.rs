use image::{Rgba, RgbaImage};

const MAGENTA: Rgba<u8> = Rgba([255, 0, 255, 255]);
const YELLOW: Rgba<u8> = Rgba([255, 255, 0, 255]);
const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

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

fn put(img: &mut RgbaImage, x: i64, y: i64, col: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, col);
    }
}

fn draw_glyph(img: &mut RgbaImage, c: char, ox: i64, oy: i64, scale: i64) {
    let g = glyph(c.to_ascii_uppercase());
    for (ry, row) in g.iter().enumerate() {
        for (cx, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }

            for sy in 0..scale {
                for sx in 0..scale {
                    let px = ox + cx as i64 * scale + sx;
                    let py = oy + ry as i64 * scale + sy;

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
                    put(
                        img,
                        ox + cx as i64 * scale + sx,
                        oy + ry as i64 * scale + sy,
                        YELLOW,
                    );
                }
            }
        }
    }
}

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

pub fn error_texture(lines: &[&str], size: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(size, size, MAGENTA);

    let head_scale = (size as i64 / 64).max(2);
    let body_scale = (head_scale - 1).max(1);
    let pad = (size as i64 / 16).max(2);

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

pub fn missing_texture(kind: &str, name: &str, size: u32) -> RgbaImage {
    let base = name.rsplit('/').next().unwrap_or(name);
    error_texture(&[kind, base], size)
}
