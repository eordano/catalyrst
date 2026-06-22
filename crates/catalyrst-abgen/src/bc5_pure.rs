const BC5_BLOCK_SIZE: usize = 16;

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

    let params = texpresso::Params {
        algorithm: texpresso::Algorithm::RangeFit,
        weights: [1.0, 1.0, 1.0],
        weigh_colour_by_alpha: false,
    };

    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mip_count {
        let repacked = repack_for_bc5(&cur);
        let (padded, pw, ph) = pad_to_block_size(&repacked, cw, ch);
        let block_count = (pw / 4) * (ph / 4);
        let mut level_out = vec![0u8; block_count * BC5_BLOCK_SIZE];
        texpresso::Format::Bc5.compress(&padded, pw, ph, params, &mut level_out);
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

pub fn encode_bc5_normal_crn_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    quality_level: u32,
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
    let mut mip_w_vec: Vec<u32> = Vec::with_capacity(mip_count as usize);
    let mut mip_h_vec: Vec<u32> = Vec::with_capacity(mip_count as usize);
    let mut mip_rgba: Vec<u8> = Vec::new();
    for m in 0..mip_count {
        let repacked = repack_for_bc5(&cur);
        mip_rgba.extend_from_slice(&repacked);
        mip_w_vec.push(cw as u32);
        mip_h_vec.push(ch as u32);
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve_rgba_u8(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }

    if let Some(crn_bytes) =
        crunch_ffi::crn_compress_bc5(&mip_rgba, &mip_w_vec, &mip_h_vec, quality_level)
    {
        return (crn_bytes, mip_count);
    }

    encode_bc5_mip_chain(rgba, width, height, Some(mip_count), flip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_4x4_block_is_16_bytes() {
        let mut rgba = vec![0u8; 16 * 4];
        for i in 0..16 {
            rgba[i * 4] = 255;
            rgba[i * 4 + 1] = 128;
            rgba[i * 4 + 2] = 127;
            rgba[i * 4 + 3] = 200;
        }
        let (data, mips) = encode_bc5_mip_chain(&rgba, 4, 4, Some(1), false);
        assert_eq!(data.len(), 16);
        assert_eq!(mips, 1);

        assert!(data[0..8].iter().any(|&b| b != 0));
        assert!(data[8..16].iter().any(|&b| b != 0));
    }

    #[test]
    fn mip_chain_1024_matches_prod_raw_byte_count() {
        let mut rgba = vec![0u8; 1024 * 1024 * 4];
        for i in 0..(1024 * 1024) {
            rgba[i * 4] = 255;
            rgba[i * 4 + 1] = 100;
            rgba[i * 4 + 2] = 127;
            rgba[i * 4 + 3] = 200;
        }
        let (data, mips) = encode_bc5_mip_chain(&rgba, 1024, 1024, None, false);
        assert_eq!(mips, 11);

        let bc_blocks =
            256 * 256 + 128 * 128 + 64 * 64 + 32 * 32 + 16 * 16 + 8 * 8 + 4 * 4 + 2 * 2 + 1 + 1 + 1;
        assert_eq!(data.len(), bc_blocks * 16);

        assert_eq!(data.len(), 1_398_128);
    }

    #[test]
    fn repack_maps_alpha_to_red() {
        let rgba = vec![255u8, 200, 127, 99];
        let out = repack_for_bc5(&rgba);
        assert_eq!(out, vec![99u8, 200, 0, 255]);
    }

    #[test]
    fn mip_chain_8x8_byte_count() {
        let rgba = vec![128u8; 8 * 8 * 4];
        let (data, mips) = encode_bc5_mip_chain(&rgba, 8, 8, None, false);
        assert_eq!(mips, 4);
        assert_eq!(data.len(), 7 * 16);
    }
}
