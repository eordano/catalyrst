#[cfg(not(target_arch = "nvptx64"))]
const BC5_BLOCK_SIZE: usize = 16;

#[cfg(not(target_arch = "nvptx64"))]
pub(crate) fn repack_for_bc5(rgba: &[u8]) -> Vec<u8> {
    debug_assert!(rgba.len().is_multiple_of(4));
    let mut out = vec![0u8; rgba.len()];
    for i in 0..(rgba.len() / 4) {
        let y = rgba[i * 4 + 1];
        let x = rgba[i * 4 + 3];
        out[i * 4] = x;
        out[i * 4 + 1] = y;
        out[i * 4 + 2] = 0;
        out[i * 4 + 3] = 255;
    }
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
pub(crate) fn box_halve_rgba_u8(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let c = 4usize;
    let nh = (h / 2).max(1);
    let nw = (w / 2).max(1);
    let fh = if h > 1 { 2 } else { 1 };
    let fw = if w > 1 { 2 } else { 1 };
    let denom = (fh * fw) as u32;
    let mut out = vec![0u8; nh * nw * c];
    let row_stride = w * c;
    for ny in 0..nh {
        for nx in 0..nw {
            for ch in 0..c {
                let mut acc: u32 = 0;
                for dy in 0..fh {
                    for dx in 0..fw {
                        let y = ny * fh + dy;
                        let x = nx * fw + dx;
                        acc += rgba[y * row_stride + x * c + ch] as u32;
                    }
                }
                out[(ny * nw + nx) * c + ch] = ((acc + denom / 2) / denom) as u8;
            }
        }
    }
    (out, nw, nh)
}

pub fn fix_range(min: &mut u8, max: &mut u8, steps: u8) {
    if (*max - *min) < steps {
        *max = (i32::from(*min) + i32::from(steps)).min(i32::from(u8::MAX)) as u8;
    }
    if (*max - *min) < steps {
        *min = (i32::from(*max) - i32::from(steps)).max(0) as u8;
    }
}

pub fn fit_codes(
    rgba: &[[u8; 4]; 16],
    channel: usize,
    mask: u32,
    codes: [u8; 8],
    indices: &mut [u8; 16],
) -> u32 {
    let mut err = 0;

    for i in 0..16 {
        let bit = 1 << i;
        if (mask & bit) == 0 {
            indices[i] = 0;
            continue;
        }

        let value = rgba[i][channel];
        let mut least = u32::MAX;
        let mut index = 0;
        for (j, &code) in codes.iter().enumerate().take(8) {
            let dist = i32::from(value) - i32::from(code);
            let dist = (dist * dist) as u32;

            if dist < least {
                least = dist;
                index = j as u8;
            }
        }

        indices[i] = index;
        err += least;
    }

    err
}

pub fn write_alpha_block(alpha0: u8, alpha1: u8, indices: &[u8; 16], block: &mut [u8]) {
    let mut buf = [0u8; 8];

    buf[0] = alpha0;
    buf[1] = alpha1;

    for i in 0..2 {
        let mut value = 0u32;
        for j in 0..8 {
            let index = u32::from(indices[8 * i + j]);
            value |= index << (3 * j);
        }

        let tmp = &mut buf[2 + i * 3..5 + i * 3];
        for (j, t) in tmp.iter_mut().enumerate() {
            *t = ((value >> (8 * j)) & 0xFF) as u8;
        }
    }
    block.copy_from_slice(&buf);
}

pub fn write_alpha_block5(alpha0: u8, alpha1: u8, indices: &[u8; 16], block: &mut [u8]) {
    if alpha0 > alpha1 {
        let mut swapped = *indices;
        for index in &mut swapped[..] {
            *index = match *index {
                0 => 1,
                1 => 0,
                x @ 2..=5 => 7 - x,
                x => x,
            }
        }

        write_alpha_block(alpha1, alpha0, &swapped, block);
    } else {
        write_alpha_block(alpha0, alpha1, indices, block);
    }
}

pub fn write_alpha_block7(alpha0: u8, alpha1: u8, indices: &[u8; 16], block: &mut [u8]) {
    if alpha0 < alpha1 {
        let mut swapped = *indices;
        for index in &mut swapped[..] {
            *index = match *index {
                0 => 1,
                1 => 0,
                x => 9 - x,
            }
        }

        write_alpha_block(alpha1, alpha0, &swapped, block);
    } else {
        write_alpha_block(alpha0, alpha1, indices, block);
    }
}

pub fn compress_bc3(rgba: &[[u8; 4]; 16], channel: usize, mask: u32, block: &mut [u8]) {
    let mut min5 = u8::MAX;
    let mut max5 = 0u8;
    let mut min7 = u8::MAX;
    let mut max7 = 0u8;

    for (i, pixel) in rgba.iter().enumerate() {
        let bit = 1 << i;
        if (mask & bit) == 0 {
            continue;
        }

        let value = pixel[channel];
        min7 = min7.min(value);
        max7 = max7.max(value);

        if value != 0 {
            min5 = min5.min(value);
        }
        if value != u8::MAX {
            max5 = max5.max(value);
        }
    }

    if min5 > max5 {
        min5 = max5;
    }
    if min7 > max7 {
        min7 = max7;
    }

    fix_range(&mut min5, &mut max5, 5);
    fix_range(&mut min7, &mut max7, 7);

    let mut codes5 = [0u8; 8];
    codes5[0] = min5;
    codes5[1] = max5;
    for i in 1..5i32 {
        codes5[1 + i as usize] = (((5 - i) * i32::from(min5) + i * i32::from(max5)) / 5) as u8;
    }
    codes5[6] = 0;
    codes5[7] = u8::MAX;

    let mut codes7 = [0u8; 8];
    codes7[0] = min5;
    codes7[1] = max5;
    for i in 1..7i32 {
        codes7[1 + i as usize] = (((7 - i) * i32::from(min7) + i * i32::from(max7)) / 7) as u8;
    }

    let mut indices5 = [0u8; 16];
    let mut indices7 = [0u8; 16];
    let err5 = fit_codes(rgba, channel, mask, codes5, &mut indices5);
    let err7 = fit_codes(rgba, channel, mask, codes7, &mut indices7);

    if err5 <= err7 {
        write_alpha_block5(min5, max5, &indices5, block);
    } else {
        write_alpha_block7(min7, max7, &indices7, block);
    }
}

pub fn compress_bc5_block(rgba: &[[u8; 4]; 16], mask: u32, output: &mut [u8]) {
    compress_bc3(rgba, 0, mask, &mut output[0..8]);
    compress_bc3(rgba, 1, mask, &mut output[8..16]);
}

#[cfg(not(target_arch = "nvptx64"))]
fn num_blocks(size: usize) -> usize {
    size.div_ceil(4)
}

#[cfg(not(target_arch = "nvptx64"))]
fn compress_bc5_texture(rgba: &[u8], width: usize, height: usize, output: &mut [u8]) {
    assert!(output.len() >= num_blocks(width) * num_blocks(height) * BC5_BLOCK_SIZE);

    let blocks_wide = num_blocks(width);

    let output_rows = output.chunks_mut(blocks_wide * BC5_BLOCK_SIZE);

    output_rows.enumerate().for_each(|(y, output_row)| {
        let mut source_rgba = [[0u8; 4]; 16];
        let output_blocks = output_row.chunks_mut(BC5_BLOCK_SIZE);

        output_blocks.enumerate().for_each(|(x, output_block)| {
            let mut mask = 0u32;
            for py in 0..4 {
                for px in 0..4 {
                    let index = 4 * py + px;

                    let sx = 4 * x + px;
                    let sy = 4 * y + py;

                    if sx < width && sy < height {
                        let src_index = 4 * (width * sy + sx);
                        source_rgba[index].copy_from_slice(&rgba[src_index..src_index + 4]);

                        mask |= 1 << index;
                    }
                }
            }

            compress_bc5_block(&source_rgba, mask, output_block);
        });
    });
}

#[cfg(not(target_arch = "nvptx64"))]
pub fn encode_bc5_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
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

    let mut cur = flipped;
    let mut cw = w;
    let mut ch = h;

    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mip_count {
        let repacked = repack_for_bc5(&cur);
        let (padded, pw, ph) = pad_to_block_size(&repacked, cw, ch);
        let block_count = (pw / 4) * (ph / 4);
        let mut level_out = vec![0u8; block_count * BC5_BLOCK_SIZE];
        compress_bc5_texture(&padded, pw, ph, &mut level_out);
        parts.extend_from_slice(&level_out);

        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve_rgba_u8(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}

#[cfg(not(target_arch = "nvptx64"))]
pub fn pack_normal_map_linearized(rgba: &[u8]) -> Vec<u8> {
    use super::mips::{round_half_up_u8, srgb_to_linear_u8};
    let n = rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let lin_r = round_half_up_u8(srgb_to_linear_u8(rgba[i * 4]) * 255.0);
        let lin_g = round_half_up_u8(srgb_to_linear_u8(rgba[i * 4 + 1]) * 255.0);
        out[i * 4] = 255;
        out[i * 4 + 1] = lin_g;
        out[i * 4 + 2] = lin_g;
        out[i * 4 + 3] = lin_r;
    }
    out
}
