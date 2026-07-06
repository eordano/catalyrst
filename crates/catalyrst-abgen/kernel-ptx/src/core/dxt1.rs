pub const BLOCK_SIZE: usize = 8;
pub const PIXELS_PER_BLOCK: usize = 16;

pub fn pack_565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = ((r as u16) >> 3) & 0x1F;
    let g6 = ((g as u16) >> 2) & 0x3F;
    let b5 = ((b as u16) >> 3) & 0x1F;
    (r5 << 11) | (g6 << 5) | b5
}

pub fn unpack_565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1F) as u8;
    let g = ((c >> 5) & 0x3F) as u8;
    let b = (c & 0x1F) as u8;

    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

pub fn encode_block(rgba: &[u8; 64]) -> [u8; BLOCK_SIZE] {
    let mut pix = [[0u8; 3]; 16];
    for i in 0..16 {
        pix[i] = [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]];
    }

    let mut mean = [0f32; 3];
    for p in &pix {
        for k in 0..3 {
            mean[k] += p[k] as f32;
        }
    }
    for m in &mut mean {
        *m /= 16.0;
    }

    let mut cov = [[0f32; 3]; 3];
    for p in &pix {
        let d = [
            p[0] as f32 - mean[0],
            p[1] as f32 - mean[1],
            p[2] as f32 - mean[2],
        ];
        for a in 0..3 {
            for b in 0..3 {
                cov[a][b] += d[a] * d[b];
            }
        }
    }
    let mut axis = [1f32, 1f32, 1f32];
    for _ in 0..6 {
        let mut n = [0f32; 3];
        for a in 0..3 {
            for b in 0..3 {
                n[a] += cov[a][b] * axis[b];
            }
        }
        let mag = super::sqrtf(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
        if mag < 1e-6 {
            axis = [1.0, 1.0, 1.0];
            break;
        }
        axis = [n[0] / mag, n[1] / mag, n[2] / mag];
    }

    let mut min_dot = f32::INFINITY;
    let mut max_dot = f32::NEG_INFINITY;
    let mut min_i = 0usize;
    let mut max_i = 0usize;
    for (i, p) in pix.iter().enumerate() {
        let d = (p[0] as f32 - mean[0]) * axis[0]
            + (p[1] as f32 - mean[1]) * axis[1]
            + (p[2] as f32 - mean[2]) * axis[2];
        if d < min_dot {
            min_dot = d;
            min_i = i;
        }
        if d > max_dot {
            max_dot = d;
            max_i = i;
        }
    }
    let mut c0 = pack_565(pix[max_i][0], pix[max_i][1], pix[max_i][2]);
    let mut c1 = pack_565(pix[min_i][0], pix[min_i][1], pix[min_i][2]);

    if c0 == c1 {
        if c1 > 0 {
            c1 -= 1;
        } else {
            c0 += 1;
        }
    }
    if c0 < c1 {
        core::mem::swap(&mut c0, &mut c1);
    }

    let ep0 = unpack_565(c0);
    let ep1 = unpack_565(c1);
    let palette: [[u8; 3]; 4] = [
        ep0,
        ep1,
        [
            ((2u16 * ep0[0] as u16 + ep1[0] as u16) / 3) as u8,
            ((2u16 * ep0[1] as u16 + ep1[1] as u16) / 3) as u8,
            ((2u16 * ep0[2] as u16 + ep1[2] as u16) / 3) as u8,
        ],
        [
            ((ep0[0] as u16 + 2u16 * ep1[0] as u16) / 3) as u8,
            ((ep0[1] as u16 + 2u16 * ep1[1] as u16) / 3) as u8,
            ((ep0[2] as u16 + 2u16 * ep1[2] as u16) / 3) as u8,
        ],
    ];

    let mut bits = 0u32;
    for (i, p) in pix.iter().enumerate() {
        let mut best = 0u32;
        let mut best_err = i32::MAX;
        for (k, pc) in palette.iter().enumerate() {
            let dr = p[0] as i32 - pc[0] as i32;
            let dg = p[1] as i32 - pc[1] as i32;
            let db = p[2] as i32 - pc[2] as i32;
            let e = dr * dr + dg * dg + db * db;
            if e < best_err {
                best_err = e;
                best = k as u32;
            }
        }
        bits |= best << (2 * i);
    }

    let mut out = [0u8; BLOCK_SIZE];
    out[0] = (c0 & 0xFF) as u8;
    out[1] = ((c0 >> 8) & 0xFF) as u8;
    out[2] = (c1 & 0xFF) as u8;
    out[3] = ((c1 >> 8) & 0xFF) as u8;
    out[4] = (bits & 0xFF) as u8;
    out[5] = ((bits >> 8) & 0xFF) as u8;
    out[6] = ((bits >> 16) & 0xFF) as u8;
    out[7] = ((bits >> 24) & 0xFF) as u8;
    out
}

#[cfg(not(target_arch = "nvptx64"))]
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

#[cfg(not(target_arch = "nvptx64"))]
fn srgb_to_linear_u8(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(not(target_arch = "nvptx64"))]
fn linear_to_srgb_u8(lin: f32) -> u8 {
    let lin = lin.clamp(0.0, 1.0);
    let s = if lin <= 0.0031308 {
        12.92 * lin
    } else {
        1.055 * lin.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

#[cfg(not(target_arch = "nvptx64"))]
fn box_halve_rgba(arr: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
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

#[cfg(not(target_arch = "nvptx64"))]
fn round_half_up_u8(v: f32) -> u8 {
    let r = (v + 0.5).floor();
    if r <= 0.0 {
        0
    } else if r >= 255.0 {
        255
    } else {
        r as u8
    }
}

#[cfg(not(target_arch = "nvptx64"))]
pub fn encode_dxt1_mip_chain(
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

    let mip_count = mip_count.unwrap_or_else(|| {
        let m = width.max(height).max(1) as f64;
        (m.log2().floor() as i32) + 1
    });

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
        let bw = pw / 4;
        let bh = ph / 4;
        let row_bytes = pw * 4;
        for by in 0..bh {
            for bx in 0..bw {
                let mut block = [0u8; PIXELS_PER_BLOCK * 4];
                let base = by * 4 * row_bytes + bx * 16;
                for r in 0..4 {
                    let start = base + r * row_bytes;
                    block[r * 16..r * 16 + 16].copy_from_slice(&padded[start..start + 16]);
                }
                let enc = encode_block(&block);
                parts.extend_from_slice(&enc);
            }
        }
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve_rgba(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}

#[cfg(all(test, not(target_arch = "nvptx64")))]
mod tests {
    use super::*;

    #[test]
    fn solid_block_is_8_bytes() {
        let mut rgba = vec![0u8; 16 * 4];
        for i in 0..16 {
            rgba[i * 4] = 0xAA;
            rgba[i * 4 + 1] = 0x55;
            rgba[i * 4 + 2] = 0x33;
            rgba[i * 4 + 3] = 0xFF;
        }
        let (data, mips) = encode_dxt1_mip_chain(&rgba, 4, 4, Some(1), false, false);
        assert_eq!(data.len(), 8);
        assert_eq!(mips, 1);

        assert!(data.iter().any(|&b| b != 0));
    }

    #[test]
    fn mip_chain_byte_count_matches_block_math() {
        let rgba = vec![0xFFu8; 8 * 8 * 4];
        let (data, mips) = encode_dxt1_mip_chain(&rgba, 8, 8, None, false, false);
        assert_eq!(mips, 4);
        assert_eq!(data.len(), 7 * 8);
    }

    #[test]
    fn mip_chain_512_matches_prod_byte_count() {
        let rgba = vec![0x80u8; 512 * 512 * 4];
        let (data, mips) = encode_dxt1_mip_chain(&rgba, 512, 512, None, false, false);
        assert_eq!(mips, 10);
        assert_eq!(data.len(), 174_776);
    }

    #[test]
    fn always_4_color_mode() {
        let mut rgba = vec![0u8; 16 * 4];
        for i in 0..16 {
            rgba[i * 4] = (i * 16) as u8;
            rgba[i * 4 + 1] = (i * 16) as u8;
            rgba[i * 4 + 2] = (i * 16) as u8;
            rgba[i * 4 + 3] = 0xFF;
        }
        let (data, _) = encode_dxt1_mip_chain(&rgba, 4, 4, Some(1), false, false);
        let c0 = u16::from_le_bytes([data[0], data[1]]);
        let c1 = u16::from_le_bytes([data[2], data[3]]);
        assert!(c0 >= c1, "block must be in 4-color mode (c0 >= c1)");
    }
}
