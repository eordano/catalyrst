use super::*;

pub fn compute_default_mip_count(width: u32, height: u32) -> i32 {
    let m = width.max(height).max(1);
    (31 - m.leading_zeros()) as i32 + 1
}

pub fn compute_mip_chain_size(width: u32, height: u32, mip_count: i32) -> usize {
    let mut total = 0usize;
    for m in 0..mip_count {
        let mw = (width >> m).max(1);
        let mh = (height >> m).max(1);
        let bx = (mw.div_ceil(4).max(1)) as usize;
        let by = (mh.div_ceil(4).max(1)) as usize;
        total += bx * by * 16;
    }
    total
}

pub fn encode_rgba32_mip_chain(
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

    let mip_count = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));

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
        parts.extend_from_slice(&level);
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}

const SRGB_TO_LINEAR_U8_BITS: [u32; 256] = [
    0x00000000, 0x399f22b4, 0x3a1f22b4, 0x3a6eb40e, 0x3a9f22b4, 0x3ac6eb61, 0x3aeeb40e, 0x3b0b3e5d,
    0x3b1f22b4, 0x3b33070b, 0x3b46eb61, 0x3b5b518d, 0x3b70f18d, 0x3b83e1c6, 0x3b8fe616, 0x3b9c87fc,
    0x3ba9c9b7, 0x3bb7ad6e, 0x3bc63549, 0x3bd56361, 0x3be539c1, 0x3bf5ba70, 0x3c0373b5, 0x3c0c6152,
    0x3c15a703, 0x3c1f45be, 0x3c293e6b, 0x3c3391f7, 0x3c3e4149, 0x3c494d43, 0x3c54b6c7, 0x3c607eb2,
    0x3c6ca5df, 0x3c792d22, 0x3c830aa8, 0x3c89af9e, 0x3c9085db, 0x3c978dc4, 0x3c9ec7c2, 0x3ca63434,
    0x3cadd37d, 0x3cb5a601, 0x3cbdac20, 0x3cc5e639, 0x3cce54ab, 0x3cd6f7d5, 0x3cdfd010, 0x3ce8ddb9,
    0x3cf2212c, 0x3cfb9ac1, 0x3d02a569, 0x3d0798dc, 0x3d0ca7e6, 0x3d11d2af, 0x3d171962, 0x3d1c7c2e,
    0x3d21fb3c, 0x3d2796b2, 0x3d2d4ebb, 0x3d332381, 0x3d39152b, 0x3d3f23e4, 0x3d454fd1, 0x3d4b991c,
    0x3d51ffee, 0x3d58846a, 0x3d5f26b8, 0x3d65e6fe, 0x3d6cc564, 0x3d73c20f, 0x3d7add29, 0x3d810b68,
    0x3d84b795, 0x3d887330, 0x3d8c3e4a, 0x3d9018f6, 0x3d940345, 0x3d97fd49, 0x3d9c0716, 0x3da020bb,
    0x3da44a4b, 0x3da883d7, 0x3daccd70, 0x3db12728, 0x3db59112, 0x3dba0b3a, 0x3dbe95b5, 0x3dc33092,
    0x3dc7dbe2, 0x3dcc97b6, 0x3dd1641f, 0x3dd6412c, 0x3ddb2eef, 0x3de02d78, 0x3de53cd5, 0x3dea5d19,
    0x3def8e52, 0x3df4d091, 0x3dfa23e8, 0x3dff8861, 0x3e027f07, 0x3e054280, 0x3e080ea3, 0x3e0ae377,
    0x3e0dc106, 0x3e10a754, 0x3e13966a, 0x3e168e51, 0x3e198f0f, 0x3e1c98ac, 0x3e1fab30, 0x3e22c6a3,
    0x3e25eb09, 0x3e29186c, 0x3e2c4ed1, 0x3e2f8e41, 0x3e32d6c5, 0x3e362861, 0x3e39831e, 0x3e3ce702,
    0x3e405416, 0x3e43ca5f, 0x3e4749e4, 0x3e4ad2ae, 0x3e4e64c2, 0x3e520027, 0x3e55a4e5, 0x3e595303,
    0x3e5d0a8b, 0x3e60cb7c, 0x3e6495e0, 0x3e6869bf, 0x3e6c4720, 0x3e702e0c, 0x3e741e84, 0x3e781890,
    0x3e7c1c38, 0x3e8014c2, 0x3e82203c, 0x3e84308d, 0x3e8645ba, 0x3e885fc5, 0x3e8a7eb1, 0x3e8ca283,
    0x3e8ecb3d, 0x3e90f8e1, 0x3e932b74, 0x3e9562f8, 0x3e979f71, 0x3e99e0e2, 0x3e9c274d, 0x3e9e72b7,
    0x3ea0c322, 0x3ea31891, 0x3ea57308, 0x3ea7d28a, 0x3eaa3718, 0x3eaca0b7, 0x3eaf0f69, 0x3eb18333,
    0x3eb3fc18, 0x3eb67a18, 0x3eb8fd37, 0x3ebb8579, 0x3ebe12e1, 0x3ec0a571, 0x3ec33d2d, 0x3ec5da17,
    0x3ec87c33, 0x3ecb2383, 0x3ecdd00b, 0x3ed081cd, 0x3ed338cc, 0x3ed5f50b, 0x3ed8b68d, 0x3edb7d55,
    0x3ede4965, 0x3ee11ac1, 0x3ee3f16b, 0x3ee6cd67, 0x3ee9aeb7, 0x3eec955d, 0x3eef815d, 0x3ef272ba,
    0x3ef56976, 0x3ef86594, 0x3efb6717, 0x3efe6e01, 0x3f00bd2d, 0x3f02460e, 0x3f03d1a7, 0x3f055ff9,
    0x3f06f106, 0x3f0884cf, 0x3f0a1b55, 0x3f0bb49b, 0x3f0d50a0, 0x3f0eef67, 0x3f1090f1, 0x3f12353e,
    0x3f13dc51, 0x3f15862b, 0x3f1732cd, 0x3f18e239, 0x3f1a946f, 0x3f1c4971, 0x3f1e0141, 0x3f1fbbdf,
    0x3f21794d, 0x3f23398d, 0x3f24fca0, 0x3f26c286, 0x3f288b42, 0x3f2a56d4, 0x3f2c253d, 0x3f2df680,
    0x3f2fca9f, 0x3f31a197, 0x3f337b6c, 0x3f355820, 0x3f3737b3, 0x3f391a26, 0x3f3aff7c, 0x3f3ce7b4,
    0x3f3ed2d2, 0x3f40c0d5, 0x3f42b1be, 0x3f44a590, 0x3f469c4b, 0x3f4895f1, 0x3f4a9282, 0x3f4c9201,
    0x3f4e946e, 0x3f5099cb, 0x3f52a218, 0x3f54ad57, 0x3f56bb8a, 0x3f58ccb1, 0x3f5ae0cd, 0x3f5cf7e0,
    0x3f5f11ec, 0x3f612eee, 0x3f634eef, 0x3f6571ea, 0x3f6797e3, 0x3f69c0d6, 0x3f6beccd, 0x3f6e1bc0,
    0x3f704db8, 0x3f7282af, 0x3f74baae, 0x3f76f5ae, 0x3f7933b8, 0x3f7b74c6, 0x3f7db8e0, 0x3f800000,
];

#[doc(hidden)]
pub const fn srgb_to_linear_u8(c: u8) -> f32 {
    f32::from_bits(SRGB_TO_LINEAR_U8_BITS[c as usize])
}

const SRGB_U8_LIN_THRESHOLD_BITS: [u32; 255] = [
    0x391f22b3, 0x39eeb40e, 0x3a46eb61, 0x3a8b3e5d, 0x3ab3070b, 0x3adacfb7, 0x3b014c32, 0x3b153089,
    0x3b2914df, 0x3b3cf936, 0x3b50f2d1, 0x3b65fb99, 0x3b7c3404, 0x3b89d060, 0x3b962333, 0x3ba314bd,
    0x3bb0a731, 0x3bbedcb6, 0x3bcdb76c, 0x3bdd3966, 0x3bed64ae, 0x3bfe3b46, 0x3c07df91, 0x3c10f918,
    0x3c1a6b33, 0x3c2436c8, 0x3c2e5cc8, 0x3c38de1a, 0x3c43bba4, 0x3c4ef647, 0x3c5a8ee1, 0x3c668653,
    0x3c72dd73, 0x3c7f9511, 0x3c865703, 0x3c8d1490, 0x3c940395, 0x3c9b247b, 0x3ca277a8, 0x3ca9fd79,
    0x3cb1b654, 0x3cb9a29a, 0x3cc1c2aa, 0x3cca16e3, 0x3cd29fa4, 0x3cdb5d4d, 0x3ce45033, 0x3ced78b6,
    0x3cf6d72f, 0x3d0035fc, 0x3d051bb6, 0x3d0a1cee, 0x3d0f39d1, 0x3d14728a, 0x3d19c745, 0x3d1f382b,
    0x3d24c56b, 0x3d2a6f25, 0x3d303587, 0x3d3618b9, 0x3d3c18e5, 0x3d423633, 0x3d4870cb, 0x3d4ec8d5,
    0x3d553e75, 0x3d5bd1d5, 0x3d62831a, 0x3d69526b, 0x3d703fef, 0x3d774bcf, 0x3d7e7628, 0x3d82df93,
    0x3d869374, 0x3d8a56cb, 0x3d8e29ac, 0x3d920c27, 0x3d95fe4f, 0x3d9a0036, 0x3d9e11ec, 0x3da23384,
    0x3da66510, 0x3daaa6a1, 0x3daef847, 0x3db35a17, 0x3db7cc1d, 0x3dbc4e6b, 0x3dc0e115, 0x3dc58429,
    0x3dca37b9, 0x3dcefbd6, 0x3dd3d090, 0x3dd8b5f6, 0x3dddac19, 0x3de2b30a, 0x3de7cad9, 0x3decf396,
    0x3df22d50, 0x3df7781a, 0x3dfcd3fe, 0x3e012088, 0x3e03dfaf, 0x3e06a77c, 0x3e0977f7, 0x3e0c5127,
    0x3e0f3314, 0x3e121dc6, 0x3e151144, 0x3e180d95, 0x3e1b12c2, 0x3e1e20d1, 0x3e2137ca, 0x3e2457b7,
    0x3e27809a, 0x3e2ab27c, 0x3e2ded67, 0x3e313160, 0x3e347e6f, 0x3e37d49d, 0x3e3b33ed, 0x3e3e9c68,
    0x3e420e14, 0x3e4588fa, 0x3e490d21, 0x3e4c9a8f, 0x3e50314b, 0x3e53d15c, 0x3e577aca, 0x3e5b2d9a,
    0x3e5ee9d4, 0x3e62af7d, 0x3e667e9f, 0x3e6a5742, 0x3e6e3965, 0x3e722512, 0x3e761a53, 0x3e7a192c,
    0x3e7e21a5, 0x3e8119e2, 0x3e8327c7, 0x3e853a88, 0x3e875223, 0x3e896e9f, 0x3e8b8ffd, 0x3e8db642,
    0x3e8fe171, 0x3e92118c, 0x3e944697, 0x3e968095, 0x3e98bf89, 0x3e9b0377, 0x3e9d4c62, 0x3e9f9a4c,
    0x3ea1ed38, 0x3ea4452b, 0x3ea6a226, 0x3ea9042e, 0x3eab6b44, 0x3eadd76c, 0x3eb048aa, 0x3eb2bf02,
    0x3eb53a71, 0x3eb7bb00, 0x3eba40b1, 0x3ebccb85, 0x3ebf5b81, 0x3ec1f0a6, 0x3ec48af9, 0x3ec72a7c,
    0x3ec9cf32, 0x3ecc791d, 0x3ecf2842, 0x3ed1dca2, 0x3ed49641, 0x3ed75521, 0x3eda1945, 0x3edce2b1,
    0x3edfb168, 0x3ee2856a, 0x3ee55ebd, 0x3ee83d62, 0x3eeb215d, 0x3eee0ab0, 0x3ef0f95e, 0x3ef3ed6a,
    0x3ef6e6d7, 0x3ef9e5a5, 0x3efce9df, 0x3efff37e, 0x3f018145, 0x3f030b82, 0x3f049877, 0x3f062827,
    0x3f07ba91, 0x3f094fba, 0x3f0ae79f, 0x3f0c8245, 0x3f0e1faa, 0x3f0fbfd2, 0x3f1162be, 0x3f13086e,
    0x3f14b0e4, 0x3f165c22, 0x3f180a29, 0x3f19baf9, 0x3f1b6e96, 0x3f1d24fe, 0x3f1ede35, 0x3f209a3c,
    0x3f225912, 0x3f241abb, 0x3f25df37, 0x3f27a688, 0x3f2970ae, 0x3f2b3dab, 0x3f2d0d83, 0x3f2ee033,
    0x3f30b5bd, 0x3f328e24, 0x3f346968, 0x3f36478c, 0x3f38288f, 0x3f3a0c73, 0x3f3bf33a, 0x3f3ddce5,
    0x3f3fc974, 0x3f41b8ea, 0x3f43ab48, 0x3f45a08e, 0x3f4798bf, 0x3f4993da, 0x3f4b91e2, 0x3f4d92d8,
    0x3f4f96bd, 0x3f519d91, 0x3f53a757, 0x3f55b410, 0x3f57c3bd, 0x3f59d65e, 0x3f5bebf6, 0x3f5e0485,
    0x3f60200d, 0x3f623e90, 0x3f64600b, 0x3f668486, 0x3f68abfa, 0x3f6ad671, 0x3f6d03e3, 0x3f6f345b,
    0x3f7167d0, 0x3f739e4d, 0x3f75d7cb, 0x3f781452, 0x3f7a53db, 0x3f7c9671, 0x3f7edc0e,
];

#[doc(hidden)]
pub fn linear_to_srgb_u8(lin: f32) -> u8 {
    if lin <= 0.0 || lin.is_nan() {
        return 0;
    }
    if lin >= 1.0 {
        return 255;
    }

    let bits = lin.to_bits();

    SRGB_U8_LIN_THRESHOLD_BITS.partition_point(|&t| t <= bits) as u8
}

#[doc(hidden)]
pub fn round_half_up_u8(v: f32) -> u8 {
    let r = (v + 0.5).floor();
    if r <= 0.0 {
        0
    } else if r >= 255.0 {
        255
    } else {
        r as u8
    }
}

fn scanline_to_blocks(rgba: &[u8], width: usize, height: usize) -> (Vec<u8>, usize) {
    let bw = width / 4;
    let bh = height / 4;
    let row_bytes = width * 4;
    let mut out = vec![0u8; bw * bh * 64];
    let mut o = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let base = by * 4 * row_bytes + bx * 16;
            for r in 0..4 {
                let start = base + r * row_bytes;
                out[o..o + 16].copy_from_slice(&rgba[start..start + 16]);
                o += 16;
            }
        }
    }
    (out, bw * bh)
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

#[doc(hidden)]
pub fn box_halve(arr: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
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

pub fn encode_bc7_mip_chain_with_profile(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: Bc7Profile,
) -> (Vec<u8>, i32) {
    #[cfg(feature = "gpu")]
    if crate::gpu_dispatch::enabled() {
        if let Some(r) = crate::gpu_dispatch::encode_bc7_mip_chain(
            rgba, width, height, mip_count, flip, srgb, perceptual, profile,
        ) {
            return r;
        }
    }
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

    let mip_count = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));
    let params = match profile {
        Bc7Profile::Slow => Params::slow(perceptual),
        Bc7Profile::Basic => Params::basic(perceptual),
    };

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
        let (blocks, n) = scanline_to_blocks(&padded, pw, ph);
        let comp = encode_blocks(&blocks, n, &params);
        parts.extend_from_slice(&comp);
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mip_count)
}
