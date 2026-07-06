use super::*;

pub(super) struct LaneF32 {
    pub(super) r: [f32; 16],
    pub(super) g: [f32; 16],
    pub(super) b: [f32; 16],
    pub(super) a: [f32; 16],
}

impl LaneF32 {
    pub(super) fn new(pixels: &[ColorI; 16]) -> Self {
        let mut l = LaneF32 {
            r: [0.0; 16],
            g: [0.0; 16],
            b: [0.0; 16],
            a: [0.0; 16],
        };
        for i in 0..16 {
            l.r[i] = pixels[i].c[0] as f32;
            l.g[i] = pixels[i].c[1] as f32;
            l.b[i] = pixels[i].c[2] as f32;
            l.a[i] = pixels[i].c[3] as f32;
        }
        l
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn ccc_est_idx_vperm(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let r0 = _mm256_loadu_ps(lf.r.as_ptr());
    let r1 = _mm256_loadu_ps(lf.r.as_ptr().add(8));
    let g0 = _mm256_loadu_ps(lf.g.as_ptr());
    let g1 = _mm256_loadu_ps(lf.g.as_ptr().add(8));
    let b0 = _mm256_loadu_ps(lf.b.as_ptr());
    let b1 = _mm256_loadu_ps(lf.b.as_ptr().add(8));
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(r0, pix, r1);
        gv[c] = _mm256_permutex2var_ps(g0, pix, g1);
        bv[c] = _mm256_permutex2var_ps(b0, pix, b1);
        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));
        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);

    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = (n as f32 - 1.0) / (high - low);
    let inv_n = 1.0 / (n as f32 - 1.0);

    let (wr, wg, wb) = if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
        )
    } else {
        (1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
            _mm256_mul_ps(fabv, bv[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let itr = _mm256_add_ps(srv, _mm256_mul_ps(dirv, s));
        let itg = _mm256_add_ps(sgv, _mm256_mul_ps(digv, s));
        let itb = _mm256_add_ps(sbv, _mm256_mul_ps(dibv, s));
        let dr = _mm256_sub_ps(itr, rv[c]);
        let dg = _mm256_sub_ps(itg, gv[c]);
        let db = _mm256_sub_ps(itb, bv[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        if cnt == 8 {
            total_errf += t_arr[0];
            total_errf += t_arr[1];
            total_errf += t_arr[2];
            total_errf += t_arr[3];
            total_errf += t_arr[4];
            total_errf += t_arr[5];
            total_errf += t_arr[6];
            total_errf += t_arr[7];
        } else {
            for &t in &t_arr[..cnt] {
                total_errf += t;
            }
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn ccc_est_mode7_idx_vperm(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let r0 = _mm256_loadu_ps(lf.r.as_ptr());
    let r1 = _mm256_loadu_ps(lf.r.as_ptr().add(8));
    let g0 = _mm256_loadu_ps(lf.g.as_ptr());
    let g1 = _mm256_loadu_ps(lf.g.as_ptr().add(8));
    let b0 = _mm256_loadu_ps(lf.b.as_ptr());
    let b1 = _mm256_loadu_ps(lf.b.as_ptr().add(8));
    let a0 = _mm256_loadu_ps(lf.a.as_ptr());
    let a1 = _mm256_loadu_ps(lf.a.as_ptr().add(8));
    let lane = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);

    let nchunks = num_pixels.div_ceil(8);
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];
    let mut av = [_mm256_setzero_ps(); 2];

    let v255 = _mm256_set1_ps(255.0);
    let v0 = _mm256_setzero_ps();
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(r0, pix, r1);
        gv[c] = _mm256_permutex2var_ps(g0, pix, g1);
        bv[c] = _mm256_permutex2var_ps(b0, pix, b1);
        av[c] = _mm256_permutex2var_ps(a0, pix, a1);
        let valid = _mm256_castsi256_ps(_mm256_cmpgt_epi32(
            _mm256_set1_epi32((num_pixels - c * 8) as i32),
            lane,
        ));
        minr = _mm256_min_ps(minr, _mm256_blendv_ps(v255, rv[c], valid));
        ming = _mm256_min_ps(ming, _mm256_blendv_ps(v255, gv[c], valid));
        minb = _mm256_min_ps(minb, _mm256_blendv_ps(v255, bv[c], valid));
        mina = _mm256_min_ps(mina, _mm256_blendv_ps(v255, av[c], valid));
        maxr = _mm256_max_ps(maxr, _mm256_blendv_ps(v0, rv[c], valid));
        maxg = _mm256_max_ps(maxg, _mm256_blendv_ps(v0, gv[c], valid));
        maxb = _mm256_max_ps(maxb, _mm256_blendv_ps(v0, bv[c], valid));
        maxa = _mm256_max_ps(maxa, _mm256_blendv_ps(v0, av[c], valid));
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let la = hmin_ps256(mina);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);
    let ha = hmax_ps256(maxa);

    let n = 4f32;
    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = (n - 1.0) / (high - low);
    let inv_n = 1.0 / (n - 1.0);

    let (wr, wg, wb, wa) = if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
            p.weights[3] as f32,
        )
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let faav = _mm256_set1_ps(faa);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let halfv = _mm256_set1_ps(0.5);
    let invnv = _mm256_set1_ps(inv_n);
    let onev = _mm256_set1_ps(1.0);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let sav = _mm256_set1_ps(sa);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let diav = _mm256_set1_ps(dia);
    let wrv = _mm256_set1_ps(wr);
    let wgv = _mm256_set1_ps(wg);
    let wbv = _mm256_set1_ps(wb);
    let wav = _mm256_set1_ps(wa);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
                _mm256_mul_ps(fabv, bv[c]),
            ),
            _mm256_mul_ps(faav, av[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, onev);
        let s = _mm256_blendv_ps(s1, onev, gt);
        let dr = _mm256_sub_ps(_mm256_add_ps(srv, _mm256_mul_ps(dirv, s)), rv[c]);
        let dg = _mm256_sub_ps(_mm256_add_ps(sgv, _mm256_mul_ps(digv, s)), gv[c]);
        let db = _mm256_sub_ps(_mm256_add_ps(sbv, _mm256_mul_ps(dibv, s)), bv[c]);
        let da = _mm256_sub_ps(_mm256_add_ps(sav, _mm256_mul_ps(diav, s)), av[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(wrv, dr), dr),
                    _mm256_mul_ps(_mm256_mul_ps(wgv, dg), dg),
                ),
                _mm256_mul_ps(_mm256_mul_ps(wbv, db), db),
            ),
            _mm256_mul_ps(_mm256_mul_ps(wav, da), da),
        );
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        let cnt = (num_pixels - c * 8).min(8);
        if cnt == 8 {
            total_errf += t_arr[0];
            total_errf += t_arr[1];
            total_errf += t_arr[2];
            total_errf += t_arr[3];
            total_errf += t_arr[4];
            total_errf += t_arr[5];
            total_errf += t_arr[6];
            total_errf += t_arr[7];
        } else {
            for &t in &t_arr[..cnt] {
                total_errf += t;
            }
        }
    }
    total_errf as i64 as u64
}

#[inline]
pub(super) fn est_subset_err(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
    lf: Option<&LaneF32>,
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    if let Some(lf) = lf {
        unsafe {
            return if mode == 7 {
                ccc_est_mode7_idx_vperm(p, idxs, num_pixels, lf)
            } else {
                ccc_est_idx_vperm(mode, p, idxs, num_pixels, lf)
            };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    if let Some(lf) = lf {
        return if mode == 7 {
            super::est_wasm128::est_mode7_idx_w128(p, idxs, num_pixels, lf)
        } else {
            super::est_wasm128::est_idx_w128(mode, p, idxs, num_pixels, lf)
        };
    }
    let _ = lf;
    if mode == 7 {
        ccc_est_mode7_idx(p, idxs, num_pixels, pixels)
    } else {
        ccc_est_idx(mode, p, idxs, num_pixels, pixels)
    }
}

pub(super) fn lanes_f32_if_supported(lanes: &[&[ColorI; 16]]) -> Option<Vec<LaneF32>> {
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    if super::est_wasm128::qualified() {
        return Some(lanes.iter().map(|p| LaneF32::new(p)).collect());
    }
    if has_avx512vl() && has_avx2() {
        Some(lanes.iter().map(|p| LaneF32::new(p)).collect())
    } else {
        None
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct EstPreRgb {
    r0: std::arch::x86_64::__m256,
    r1: std::arch::x86_64::__m256,
    g0: std::arch::x86_64::__m256,
    g1: std::arch::x86_64::__m256,
    b0: std::arch::x86_64::__m256,
    b1: std::arch::x86_64::__m256,
    v255: std::arch::x86_64::__m256,
    v0: std::arch::x86_64::__m256,
    wrv: std::arch::x86_64::__m256,
    wgv: std::arch::x86_64::__m256,
    wbv: std::arch::x86_64::__m256,
    invnv: std::arch::x86_64::__m256,
    halfv: std::arch::x86_64::__m256,
    onev: std::arch::x86_64::__m256,
    nm1: f32,
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_pre_rgb(mode: usize, p: &CCParams, lf: &LaneF32) -> EstPreRgb {
    use std::arch::x86_64::*;
    let n = 1u32 << G_COLOR_INDEX_BITCOUNT[mode];
    let nm1 = n as f32 - 1.0;
    let inv_n = 1.0 / nm1;
    let (wr, wg, wb) = if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
        )
    } else {
        (1.0, 1.0, 1.0)
    };
    EstPreRgb {
        r0: _mm256_loadu_ps(lf.r.as_ptr()),
        r1: _mm256_loadu_ps(lf.r.as_ptr().add(8)),
        g0: _mm256_loadu_ps(lf.g.as_ptr()),
        g1: _mm256_loadu_ps(lf.g.as_ptr().add(8)),
        b0: _mm256_loadu_ps(lf.b.as_ptr()),
        b1: _mm256_loadu_ps(lf.b.as_ptr().add(8)),
        v255: _mm256_set1_ps(255.0),
        v0: _mm256_setzero_ps(),
        wrv: _mm256_set1_ps(wr),
        wgv: _mm256_set1_ps(wg),
        wbv: _mm256_set1_ps(wb),
        invnv: _mm256_set1_ps(inv_n),
        halfv: _mm256_set1_ps(0.5),
        onev: _mm256_set1_ps(1.0),
        nm1,
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct EstPreRgba {
    r0: std::arch::x86_64::__m256,
    r1: std::arch::x86_64::__m256,
    g0: std::arch::x86_64::__m256,
    g1: std::arch::x86_64::__m256,
    b0: std::arch::x86_64::__m256,
    b1: std::arch::x86_64::__m256,
    a0: std::arch::x86_64::__m256,
    a1: std::arch::x86_64::__m256,
    v255: std::arch::x86_64::__m256,
    v0: std::arch::x86_64::__m256,
    wrv: std::arch::x86_64::__m256,
    wgv: std::arch::x86_64::__m256,
    wbv: std::arch::x86_64::__m256,
    wav: std::arch::x86_64::__m256,
    invnv: std::arch::x86_64::__m256,
    halfv: std::arch::x86_64::__m256,
    onev: std::arch::x86_64::__m256,
    nm1: f32,
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
unsafe fn est_pre_rgba(p: &CCParams, lf: &LaneF32) -> EstPreRgba {
    use std::arch::x86_64::*;
    let n = 4f32;
    let nm1 = n - 1.0;
    let inv_n = 1.0 / nm1;
    let (wr, wg, wb, wa) = if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        (
            p.weights[0] as f32,
            p.weights[1] as f32,
            p.weights[2] as f32,
            p.weights[3] as f32,
        )
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };
    EstPreRgba {
        r0: _mm256_loadu_ps(lf.r.as_ptr()),
        r1: _mm256_loadu_ps(lf.r.as_ptr().add(8)),
        g0: _mm256_loadu_ps(lf.g.as_ptr()),
        g1: _mm256_loadu_ps(lf.g.as_ptr().add(8)),
        b0: _mm256_loadu_ps(lf.b.as_ptr()),
        b1: _mm256_loadu_ps(lf.b.as_ptr().add(8)),
        a0: _mm256_loadu_ps(lf.a.as_ptr()),
        a1: _mm256_loadu_ps(lf.a.as_ptr().add(8)),
        v255: _mm256_set1_ps(255.0),
        v0: _mm256_setzero_ps(),
        wrv: _mm256_set1_ps(wr),
        wgv: _mm256_set1_ps(wg),
        wbv: _mm256_set1_ps(wb),
        wav: _mm256_set1_ps(wa),
        invnv: _mm256_set1_ps(inv_n),
        halfv: _mm256_set1_ps(0.5),
        onev: _mm256_set1_ps(1.0),
        nm1,
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
#[inline]
unsafe fn subset_err_rgb_pre(pre: &EstPreRgb, idxs: &[i32; 16], num_pixels: usize) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(8);
    let last = nchunks - 1;

    let tailk: __mmask8 = ((1u32 << (num_pixels - last * 8)) - 1) as __mmask8;
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];

    let v255 = pre.v255;
    let v0 = pre.v0;
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(pre.r0, pix, pre.r1);
        gv[c] = _mm256_permutex2var_ps(pre.g0, pix, pre.g1);
        bv[c] = _mm256_permutex2var_ps(pre.b0, pix, pre.b1);
        let k = if c == last { tailk } else { 0xff };
        minr = _mm256_mask_min_ps(minr, k, minr, rv[c]);
        ming = _mm256_mask_min_ps(ming, k, ming, gv[c]);
        minb = _mm256_mask_min_ps(minb, k, minb, bv[c]);
        maxr = _mm256_mask_max_ps(maxr, k, maxr, rv[c]);
        maxg = _mm256_mask_max_ps(maxg, k, maxg, gv[c]);
        maxb = _mm256_mask_max_ps(maxb, k, maxb, bv[c]);
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);

    let sr = lr;
    let sg = lg;
    let sb = lb;
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let far = dir;
    let fag = dig;
    let fab = dib;
    let low = far * sr + fag * sg + fab * sb;
    let high = far * hr + fag * hg + fab * hb;
    let scale = pre.nm1 / (high - low);

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
            _mm256_mul_ps(fabv, bv[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), pre.halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), pre.invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, pre.onev);
        let s = _mm256_blendv_ps(s1, pre.onev, gt);
        let itr = _mm256_add_ps(srv, _mm256_mul_ps(dirv, s));
        let itg = _mm256_add_ps(sgv, _mm256_mul_ps(digv, s));
        let itb = _mm256_add_ps(sbv, _mm256_mul_ps(dibv, s));
        let dr = _mm256_sub_ps(itr, rv[c]);
        let dg = _mm256_sub_ps(itg, gv[c]);
        let db = _mm256_sub_ps(itb, bv[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_mul_ps(pre.wrv, dr), dr),
                _mm256_mul_ps(_mm256_mul_ps(pre.wgv, dg), dg),
            ),
            _mm256_mul_ps(_mm256_mul_ps(pre.wbv, db), db),
        );

        let term = if c == last {
            _mm256_maskz_mov_ps(tailk, term)
        } else {
            term
        };
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        total_errf += t_arr[0];
        total_errf += t_arr[1];
        total_errf += t_arr[2];
        total_errf += t_arr[3];
        total_errf += t_arr[4];
        total_errf += t_arr[5];
        total_errf += t_arr[6];
        total_errf += t_arr[7];
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
#[inline]
unsafe fn subset_err_rgba_pre(pre: &EstPreRgba, idxs: &[i32; 16], num_pixels: usize) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(8);
    let last = nchunks - 1;

    let tailk: __mmask8 = ((1u32 << (num_pixels - last * 8)) - 1) as __mmask8;
    let mut rv = [_mm256_setzero_ps(); 2];
    let mut gv = [_mm256_setzero_ps(); 2];
    let mut bv = [_mm256_setzero_ps(); 2];
    let mut av = [_mm256_setzero_ps(); 2];

    let v255 = pre.v255;
    let v0 = pre.v0;
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let pix = _mm256_loadu_si256(idxs.as_ptr().add(c * 8) as *const __m256i);
        rv[c] = _mm256_permutex2var_ps(pre.r0, pix, pre.r1);
        gv[c] = _mm256_permutex2var_ps(pre.g0, pix, pre.g1);
        bv[c] = _mm256_permutex2var_ps(pre.b0, pix, pre.b1);
        av[c] = _mm256_permutex2var_ps(pre.a0, pix, pre.a1);
        let k = if c == last { tailk } else { 0xff };
        minr = _mm256_mask_min_ps(minr, k, minr, rv[c]);
        ming = _mm256_mask_min_ps(ming, k, ming, gv[c]);
        minb = _mm256_mask_min_ps(minb, k, minb, bv[c]);
        mina = _mm256_mask_min_ps(mina, k, mina, av[c]);
        maxr = _mm256_mask_max_ps(maxr, k, maxr, rv[c]);
        maxg = _mm256_mask_max_ps(maxg, k, maxg, gv[c]);
        maxb = _mm256_mask_max_ps(maxb, k, maxb, bv[c]);
        maxa = _mm256_mask_max_ps(maxa, k, maxa, av[c]);
    }
    let lr = hmin_ps256(minr);
    let lg = hmin_ps256(ming);
    let lb = hmin_ps256(minb);
    let la = hmin_ps256(mina);
    let hr = hmax_ps256(maxr);
    let hg = hmax_ps256(maxg);
    let hb = hmax_ps256(maxb);
    let ha = hmax_ps256(maxa);

    let (sr, sg, sb, sa) = (lr, lg, lb, la);
    let dir = hr - lr;
    let dig = hg - lg;
    let dib = hb - lb;
    let dia = ha - la;
    let (far, fag, fab, faa) = (dir, dig, dib, dia);
    let low = far * sr + fag * sg + fab * sb + faa * sa;
    let high = far * hr + fag * hg + fab * hb + faa * ha;
    let scale = pre.nm1 / (high - low);

    let farv = _mm256_set1_ps(far);
    let fagv = _mm256_set1_ps(fag);
    let fabv = _mm256_set1_ps(fab);
    let faav = _mm256_set1_ps(faa);
    let lowv = _mm256_set1_ps(low);
    let scalev = _mm256_set1_ps(scale);
    let srv = _mm256_set1_ps(sr);
    let sgv = _mm256_set1_ps(sg);
    let sbv = _mm256_set1_ps(sb);
    let sav = _mm256_set1_ps(sa);
    let dirv = _mm256_set1_ps(dir);
    let digv = _mm256_set1_ps(dig);
    let dibv = _mm256_set1_ps(dib);
    let diav = _mm256_set1_ps(dia);

    let mut total_errf = 0f32;
    let mut t_arr = [0f32; 8];
    for c in 0..nchunks {
        let d = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(_mm256_mul_ps(farv, rv[c]), _mm256_mul_ps(fagv, gv[c])),
                _mm256_mul_ps(fabv, bv[c]),
            ),
            _mm256_mul_ps(faav, av[c]),
        );
        let t1 = _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(d, lowv), scalev), pre.halfv);
        let s0 = _mm256_mul_ps(_mm256_floor_ps(t1), pre.invnv);
        let lt = _mm256_cmp_ps::<_CMP_LT_OQ>(s0, v0);
        let s1 = _mm256_blendv_ps(s0, v0, lt);
        let gt = _mm256_cmp_ps::<_CMP_GT_OQ>(s0, pre.onev);
        let s = _mm256_blendv_ps(s1, pre.onev, gt);
        let dr = _mm256_sub_ps(_mm256_add_ps(srv, _mm256_mul_ps(dirv, s)), rv[c]);
        let dg = _mm256_sub_ps(_mm256_add_ps(sgv, _mm256_mul_ps(digv, s)), gv[c]);
        let db = _mm256_sub_ps(_mm256_add_ps(sbv, _mm256_mul_ps(dibv, s)), bv[c]);
        let da = _mm256_sub_ps(_mm256_add_ps(sav, _mm256_mul_ps(diav, s)), av[c]);
        let term = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_add_ps(
                    _mm256_mul_ps(_mm256_mul_ps(pre.wrv, dr), dr),
                    _mm256_mul_ps(_mm256_mul_ps(pre.wgv, dg), dg),
                ),
                _mm256_mul_ps(_mm256_mul_ps(pre.wbv, db), db),
            ),
            _mm256_mul_ps(_mm256_mul_ps(pre.wav, da), da),
        );

        let term = if c == last {
            _mm256_maskz_mov_ps(tailk, term)
        } else {
            term
        };
        _mm256_storeu_ps(t_arr.as_mut_ptr(), term);
        total_errf += t_arr[0];
        total_errf += t_arr[1];
        total_errf += t_arr[2];
        total_errf += t_arr[3];
        total_errf += t_arr[4];
        total_errf += t_arr[5];
        total_errf += t_arr[6];
        total_errf += t_arr[7];
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
pub(super) unsafe fn est_partition_lane_vperm(
    mode: usize,
    p: &CCParams,
    lf: &LaneF32,
    tab: &[SubsetIdx; 64],
    total_partitions: u32,
    total_subsets: usize,
) -> u32 {
    debug_assert!(mode != 7);
    let pre = est_pre_rgb(mode, p, lf);
    let mut best_err = u64::MAX;
    let mut best_partition = 0u32;
    for partition in 0..total_partitions {
        let si = &tab[partition as usize];
        let mut total_subset_err = 0u64;
        for subset in 0..total_subsets {
            let err = subset_err_rgb_pre(&pre, &si.idx[subset], si.total[subset]);
            total_subset_err += err;
            if total_subset_err >= best_err {
                break;
            }
        }
        if total_subset_err < best_err {
            best_err = total_subset_err;
            best_partition = partition;
            if best_err == 0 {
                break;
            }
        }
        if total_subsets == 2
            && partition as usize == BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
            && best_partition as usize != BC7E_2SUBSET_CHECKERBOARD_PARTITION_INDEX
        {
            break;
        }
    }
    best_partition
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,avx512f,avx512vl")]
pub(super) unsafe fn est_partition_list_lane_vperm(
    mode: usize,
    p: &CCParams,
    lf: &LaneF32,
    tab: &[SubsetIdx; 64],
    part_lo: u32,
    part_hi: u32,
    total_subsets: usize,
    solutions: &mut [Solution],
    num_solutions: &mut i32,
    max_solutions: i32,
) -> i32 {
    let pre_rgb = if mode != 7 {
        Some(est_pre_rgb(mode, p, lf))
    } else {
        None
    };
    let pre_rgba = if mode == 7 {
        Some(est_pre_rgba(p, lf))
    } else {
        None
    };
    let mut i_at = 0i32;
    for partition in part_lo..part_hi {
        let si = &tab[partition as usize];
        let full = *num_solutions == max_solutions;
        let thresh = if full {
            solutions[(max_solutions - 1) as usize].err
        } else {
            u64::MAX
        };
        let mut total_subset_err = 0u64;
        let mut pruned = false;
        for subset in 0..total_subsets {
            let err = if let Some(pre) = &pre_rgba {
                subset_err_rgba_pre(pre, &si.idx[subset], si.total[subset])
            } else {
                subset_err_rgb_pre(
                    pre_rgb.as_ref().unwrap_unchecked(),
                    &si.idx[subset],
                    si.total[subset],
                )
            };
            total_subset_err += err;
            if total_subset_err >= thresh {
                pruned = true;
                break;
            }
        }
        if pruned {
            i_at = *num_solutions;
            continue;
        }
        let mut i = 0i32;
        while i < *num_solutions {
            if total_subset_err < solutions[i as usize].err {
                break;
            }
            i += 1;
        }
        if i < *num_solutions {
            let mut solutions_to_move = (max_solutions - 1) - i;
            let num_elements_at_i = *num_solutions - i;
            if solutions_to_move > num_elements_at_i {
                solutions_to_move = num_elements_at_i;
            }
            let mut j = solutions_to_move - 1;
            while j >= 0 {
                solutions[(i + j + 1) as usize] = solutions[(i + j) as usize];
                j -= 1;
            }
        }
        if *num_solutions < max_solutions {
            *num_solutions += 1;
        }
        if i < *num_solutions {
            solutions[i as usize].err = total_subset_err;
            solutions[i as usize].index = partition;
        }
        i_at = i;
    }
    i_at
}
