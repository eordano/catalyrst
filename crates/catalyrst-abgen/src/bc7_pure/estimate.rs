use super::*;

#[derive(Clone, Copy)]
pub(super) struct SubsetIdx {
    pub(super) idx: [[i32; 16]; 3],
    pub(super) total: [usize; 3],
}

pub(super) fn subset_idx_tables(total_subsets: usize) -> &'static [SubsetIdx; 64] {
    static T2: OnceLock<Box<[SubsetIdx; 64]>> = OnceLock::new();
    static T3: OnceLock<Box<[SubsetIdx; 64]>> = OnceLock::new();
    let build = |table: &'static [u8; 64 * 16]| -> Box<[SubsetIdx; 64]> {
        let mut out = Box::new(
            [SubsetIdx {
                idx: [[0i32; 16]; 3],
                total: [0usize; 3],
            }; 64],
        );
        for partition in 0..64 {
            let part = &table[partition * 16..partition * 16 + 16];
            let e = &mut out[partition];
            for (index, &pp) in part.iter().enumerate() {
                let pp = pp as usize;
                e.idx[pp][e.total[pp]] = index as i32;
                e.total[pp] += 1;
            }
        }
        out
    };
    if total_subsets == 3 {
        T3.get_or_init(|| build(&G_PARTITION3))
    } else {
        T2.get_or_init(|| build(&G_PARTITION2))
    }
}

pub(super) fn ccc_est_idx(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2() {
            unsafe {
                return ccc_est_idx_avx2(mode, p, idxs, num_pixels, pixels);
            }
        }
    }
    ccc_est_idx_scalar(mode, p, idxs, num_pixels, pixels)
}

pub(super) fn ccc_est_idx_scalar(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb) = (255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb) = (0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
    }
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
    let mut total_errf = 0f32;
    if p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32 + fag * px.c[1] as f32 + fab * px.c[2] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let itr = sr + dir * s;
            let itg = sg + dig * s;
            let itb = sb + dib * s;
            let dr = itr - px.c[0] as f32;
            let dg = itg - px.c[1] as f32;
            let db = itb - px.c[2] as f32;
            total_errf += dr * dr + dg * dg + db * db;
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub(super) unsafe fn hmin_ps256(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let lo = _mm256_castps256_ps128(v);
    let hi = _mm256_extractf128_ps::<1>(v);
    let m = _mm_min_ps(lo, hi);
    let m = _mm_min_ps(m, _mm_movehl_ps(m, m));
    let m = _mm_min_ss(m, _mm_shuffle_ps(m, m, 1));
    _mm_cvtss_f32(m)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub(super) unsafe fn hmax_ps256(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let lo = _mm256_castps256_ps128(v);
    let hi = _mm256_extractf128_ps::<1>(v);
    let m = _mm_max_ps(lo, hi);
    let m = _mm_max_ps(m, _mm_movehl_ps(m, m));
    let m = _mm_max_ss(m, _mm_shuffle_ps(m, m, 1));
    _mm_cvtss_f32(m)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn ccc_est_idx_avx2(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return ccc_est_idx_scalar(mode, p, idxs, num_pixels, pixels);
    }
    let base = pixels.as_ptr() as *const i32;
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
        let idx = _mm256_slli_epi32::<2>(pix);
        rv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base, idx));
        gv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(1), idx));
        bv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(2), idx));

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
        for &t in &t_arr[..cnt] {
            total_errf += t;
        }
    }
    total_errf as i64 as u64
}

pub(super) fn ccc_est_mode7_idx(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if has_avx2() {
            unsafe {
                return ccc_est_mode7_idx_avx2(p, idxs, num_pixels, pixels);
            }
        }
    }
    ccc_est_mode7_idx_scalar(p, idxs, num_pixels, pixels)
}

pub(super) fn ccc_est_mode7_idx_scalar(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    let (mut lr, mut lg, mut lb, mut la) = (255f32, 255f32, 255f32, 255f32);
    let (mut hr, mut hg, mut hb, mut ha) = (0f32, 0f32, 0f32, 0f32);
    for k in 0..num_pixels {
        let px = &pixels[idxs[k] as usize];
        let r = px.c[0] as f32;
        let g = px.c[1] as f32;
        let b = px.c[2] as f32;
        let a = px.c[3] as f32;
        lr = lr.min(r);
        lg = lg.min(g);
        lb = lb.min(b);
        la = la.min(a);
        hr = hr.max(r);
        hg = hg.max(g);
        hb = hb.max(b);
        ha = ha.max(a);
    }
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
    let mut total_errf = 0f32;
    if !p.perceptual
        && (p.weights[0] != 1 || p.weights[1] != 1 || p.weights[2] != 1 || p.weights[3] != 1)
    {
        let wr = p.weights[0] as f32;
        let wg = p.weights[1] as f32;
        let wb = p.weights[2] as f32;
        let wa = p.weights[3] as f32;
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += wr * dr * dr + wg * dg * dg + wb * db * db + wa * da * da;
        }
    } else {
        for k in 0..num_pixels {
            let px = &pixels[idxs[k] as usize];
            let d = far * px.c[0] as f32
                + fag * px.c[1] as f32
                + fab * px.c[2] as f32
                + faa * px.c[3] as f32;
            let s = (((d - low) * scale + 0.5).floor() * inv_n).clamp(0.0, 1.0);
            let dr = sr + dir * s - px.c[0] as f32;
            let dg = sg + dig * s - px.c[1] as f32;
            let db = sb + dib * s - px.c[2] as f32;
            let da = sa + dia * s - px.c[3] as f32;
            total_errf += dr * dr + dg * dg + db * db + da * da;
        }
    }
    total_errf as i64 as u64
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn ccc_est_mode7_idx_avx2(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    pixels: &[ColorI; 16],
) -> u64 {
    use std::arch::x86_64::*;
    if num_pixels == 0 {
        return ccc_est_mode7_idx_scalar(p, idxs, num_pixels, pixels);
    }
    let base = pixels.as_ptr() as *const i32;
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
        let idx = _mm256_slli_epi32::<2>(pix);
        rv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base, idx));
        gv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(1), idx));
        bv[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(2), idx));
        av[c] = _mm256_cvtepi32_ps(_mm256_i32gather_epi32::<4>(base.add(3), idx));
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
        for &t in &t_arr[..cnt] {
            total_errf += t;
        }
    }
    total_errf as i64 as u64
}
