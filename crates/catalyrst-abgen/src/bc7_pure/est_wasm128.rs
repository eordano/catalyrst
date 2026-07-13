use super::*;
use std::arch::wasm32::*;

// Bit-exactness with the scalar estimator is by construction (same op
// association, same pixel-order accumulation) and proven at runtime:
// qualified() compares both paths on a deterministic probe set and any
// mismatch permanently disqualifies the simd128 lane back to scalar.

fn gather4(src: &[f32; 16], idxs: &[i32; 16], base: usize, cnt: usize) -> v128 {
    let mut a = [0f32; 4];
    for k in 0..cnt {
        a[k] = src[idxs[base + k] as usize];
    }
    f32x4(a[0], a[1], a[2], a[3])
}

fn hmin4(v: v128) -> f32 {
    f32x4_extract_lane::<0>(v)
        .min(f32x4_extract_lane::<1>(v))
        .min(f32x4_extract_lane::<2>(v))
        .min(f32x4_extract_lane::<3>(v))
}

fn hmax4(v: v128) -> f32 {
    f32x4_extract_lane::<0>(v)
        .max(f32x4_extract_lane::<1>(v))
        .max(f32x4_extract_lane::<2>(v))
        .max(f32x4_extract_lane::<3>(v))
}

fn lanes4(v: v128) -> [f32; 4] {
    [
        f32x4_extract_lane::<0>(v),
        f32x4_extract_lane::<1>(v),
        f32x4_extract_lane::<2>(v),
        f32x4_extract_lane::<3>(v),
    ]
}

pub(super) fn est_idx_w128(
    mode: usize,
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(4);
    let mut rv = [f32x4_splat(0.0); 4];
    let mut gv = [f32x4_splat(0.0); 4];
    let mut bv = [f32x4_splat(0.0); 4];

    let v255 = f32x4_splat(255.0);
    let v0 = f32x4_splat(0.0);
    let (mut minr, mut ming, mut minb) = (v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb) = (v0, v0, v0);
    for c in 0..nchunks {
        let cnt = (num_pixels - c * 4).min(4);
        rv[c] = gather4(&lf.r, idxs, c * 4, cnt);
        gv[c] = gather4(&lf.g, idxs, c * 4, cnt);
        bv[c] = gather4(&lf.b, idxs, c * 4, cnt);
        let valid = i32x4_gt(i32x4_splat(cnt as i32), i32x4(0, 1, 2, 3));
        minr = f32x4_min(minr, v128_bitselect(rv[c], v255, valid));
        ming = f32x4_min(ming, v128_bitselect(gv[c], v255, valid));
        minb = f32x4_min(minb, v128_bitselect(bv[c], v255, valid));
        maxr = f32x4_max(maxr, v128_bitselect(rv[c], v0, valid));
        maxg = f32x4_max(maxg, v128_bitselect(gv[c], v0, valid));
        maxb = f32x4_max(maxb, v128_bitselect(bv[c], v0, valid));
    }
    let lr = hmin4(minr);
    let lg = hmin4(ming);
    let lb = hmin4(minb);
    let hr = hmax4(maxr);
    let hg = hmax4(maxg);
    let hb = hmax4(maxb);

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

    let farv = f32x4_splat(far);
    let fagv = f32x4_splat(fag);
    let fabv = f32x4_splat(fab);
    let lowv = f32x4_splat(low);
    let scalev = f32x4_splat(scale);
    let halfv = f32x4_splat(0.5);
    let invnv = f32x4_splat(inv_n);
    let onev = f32x4_splat(1.0);
    let srv = f32x4_splat(sr);
    let sgv = f32x4_splat(sg);
    let sbv = f32x4_splat(sb);
    let dirv = f32x4_splat(dir);
    let digv = f32x4_splat(dig);
    let dibv = f32x4_splat(dib);
    let wrv = f32x4_splat(wr);
    let wgv = f32x4_splat(wg);
    let wbv = f32x4_splat(wb);

    let mut total_errf = 0f32;
    for c in 0..nchunks {
        let d = f32x4_add(
            f32x4_add(f32x4_mul(farv, rv[c]), f32x4_mul(fagv, gv[c])),
            f32x4_mul(fabv, bv[c]),
        );
        let t1 = f32x4_add(f32x4_mul(f32x4_sub(d, lowv), scalev), halfv);
        let s0 = f32x4_mul(f32x4_floor(t1), invnv);
        let lt = f32x4_lt(s0, v0);
        let s1 = v128_bitselect(v0, s0, lt);
        let gt = f32x4_gt(s0, onev);
        let s = v128_bitselect(onev, s1, gt);
        let itr = f32x4_add(srv, f32x4_mul(dirv, s));
        let itg = f32x4_add(sgv, f32x4_mul(digv, s));
        let itb = f32x4_add(sbv, f32x4_mul(dibv, s));
        let dr = f32x4_sub(itr, rv[c]);
        let dg = f32x4_sub(itg, gv[c]);
        let db = f32x4_sub(itb, bv[c]);
        let term = f32x4_add(
            f32x4_add(
                f32x4_mul(f32x4_mul(wrv, dr), dr),
                f32x4_mul(f32x4_mul(wgv, dg), dg),
            ),
            f32x4_mul(f32x4_mul(wbv, db), db),
        );
        let t = lanes4(term);
        let cnt = (num_pixels - c * 4).min(4);
        for &tv in &t[..cnt] {
            total_errf += tv;
        }
    }
    total_errf as i64 as u64
}

pub(super) fn est_mode7_idx_w128(
    p: &CCParams,
    idxs: &[i32; 16],
    num_pixels: usize,
    lf: &LaneF32,
) -> u64 {
    if num_pixels == 0 {
        return 0;
    }
    let nchunks = num_pixels.div_ceil(4);
    let mut rv = [f32x4_splat(0.0); 4];
    let mut gv = [f32x4_splat(0.0); 4];
    let mut bv = [f32x4_splat(0.0); 4];
    let mut av = [f32x4_splat(0.0); 4];

    let v255 = f32x4_splat(255.0);
    let v0 = f32x4_splat(0.0);
    let (mut minr, mut ming, mut minb, mut mina) = (v255, v255, v255, v255);
    let (mut maxr, mut maxg, mut maxb, mut maxa) = (v0, v0, v0, v0);
    for c in 0..nchunks {
        let cnt = (num_pixels - c * 4).min(4);
        rv[c] = gather4(&lf.r, idxs, c * 4, cnt);
        gv[c] = gather4(&lf.g, idxs, c * 4, cnt);
        bv[c] = gather4(&lf.b, idxs, c * 4, cnt);
        av[c] = gather4(&lf.a, idxs, c * 4, cnt);
        let valid = i32x4_gt(i32x4_splat(cnt as i32), i32x4(0, 1, 2, 3));
        minr = f32x4_min(minr, v128_bitselect(rv[c], v255, valid));
        ming = f32x4_min(ming, v128_bitselect(gv[c], v255, valid));
        minb = f32x4_min(minb, v128_bitselect(bv[c], v255, valid));
        mina = f32x4_min(mina, v128_bitselect(av[c], v255, valid));
        maxr = f32x4_max(maxr, v128_bitselect(rv[c], v0, valid));
        maxg = f32x4_max(maxg, v128_bitselect(gv[c], v0, valid));
        maxb = f32x4_max(maxb, v128_bitselect(bv[c], v0, valid));
        maxa = f32x4_max(maxa, v128_bitselect(av[c], v0, valid));
    }
    let lr = hmin4(minr);
    let lg = hmin4(ming);
    let lb = hmin4(minb);
    let la = hmin4(mina);
    let hr = hmax4(maxr);
    let hg = hmax4(maxg);
    let hb = hmax4(maxb);
    let ha = hmax4(maxa);

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

    let farv = f32x4_splat(far);
    let fagv = f32x4_splat(fag);
    let fabv = f32x4_splat(fab);
    let faav = f32x4_splat(faa);
    let lowv = f32x4_splat(low);
    let scalev = f32x4_splat(scale);
    let halfv = f32x4_splat(0.5);
    let invnv = f32x4_splat(inv_n);
    let onev = f32x4_splat(1.0);
    let srv = f32x4_splat(sr);
    let sgv = f32x4_splat(sg);
    let sbv = f32x4_splat(sb);
    let sav = f32x4_splat(sa);
    let dirv = f32x4_splat(dir);
    let digv = f32x4_splat(dig);
    let dibv = f32x4_splat(dib);
    let diav = f32x4_splat(dia);
    let wrv = f32x4_splat(wr);
    let wgv = f32x4_splat(wg);
    let wbv = f32x4_splat(wb);
    let wav = f32x4_splat(wa);

    let mut total_errf = 0f32;
    for c in 0..nchunks {
        let d = f32x4_add(
            f32x4_add(
                f32x4_add(f32x4_mul(farv, rv[c]), f32x4_mul(fagv, gv[c])),
                f32x4_mul(fabv, bv[c]),
            ),
            f32x4_mul(faav, av[c]),
        );
        let t1 = f32x4_add(f32x4_mul(f32x4_sub(d, lowv), scalev), halfv);
        let s0 = f32x4_mul(f32x4_floor(t1), invnv);
        let lt = f32x4_lt(s0, v0);
        let s1 = v128_bitselect(v0, s0, lt);
        let gt = f32x4_gt(s0, onev);
        let s = v128_bitselect(onev, s1, gt);
        let dr = f32x4_sub(f32x4_add(srv, f32x4_mul(dirv, s)), rv[c]);
        let dg = f32x4_sub(f32x4_add(sgv, f32x4_mul(digv, s)), gv[c]);
        let db = f32x4_sub(f32x4_add(sbv, f32x4_mul(dibv, s)), bv[c]);
        let da = f32x4_sub(f32x4_add(sav, f32x4_mul(diav, s)), av[c]);
        let term = f32x4_add(
            f32x4_add(
                f32x4_add(
                    f32x4_mul(f32x4_mul(wrv, dr), dr),
                    f32x4_mul(f32x4_mul(wgv, dg), dg),
                ),
                f32x4_mul(f32x4_mul(wbv, db), db),
            ),
            f32x4_mul(f32x4_mul(wav, da), da),
        );
        let t = lanes4(term);
        let cnt = (num_pixels - c * 4).min(4);
        for &tv in &t[..cnt] {
            total_errf += tv;
        }
    }
    total_errf as i64 as u64
}

pub(super) fn qualified() -> bool {
    static Q: OnceLock<bool> = OnceLock::new();
    *Q.get_or_init(probe_matches_scalar)
}

fn xs(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn probe_matches_scalar() -> bool {
    if std::env::var_os("ABGEN_BC7_SCALAR").is_some() {
        return false;
    }
    let mut st = 0x9e3779b97f4a7c15u64;
    let weight_sets: [([u32; 4], bool); 4] = [
        ([1, 1, 1, 1], false),
        ([128, 64, 16, 256], true),
        ([128, 64, 16, 256], false),
        ([2, 3, 5, 7], false),
    ];
    let modes = [0usize, 1, 2, 3, 7];
    for case in 0..128usize {
        let mode = modes[case % modes.len()];
        let num_pixels = case % 16 + 1;
        let (weights, perceptual) = weight_sets[case % weight_sets.len()];
        let mut pixels = [ColorI::default(); 16];
        if case % 8 == 7 {
            let c = [
                (xs(&mut st) & 0xff) as i32,
                (xs(&mut st) & 0xff) as i32,
                (xs(&mut st) & 0xff) as i32,
                (xs(&mut st) & 0xff) as i32,
            ];
            pixels = [ColorI { c }; 16];
        } else {
            for px in pixels.iter_mut() {
                px.c = [
                    (xs(&mut st) & 0xff) as i32,
                    (xs(&mut st) & 0xff) as i32,
                    (xs(&mut st) & 0xff) as i32,
                    (xs(&mut st) & 0xff) as i32,
                ];
            }
        }
        let mut idxs = [0i32; 16];
        for (i, v) in idxs.iter_mut().enumerate() {
            *v = i as i32;
        }
        for i in (1..16usize).rev() {
            let j = (xs(&mut st) % (i as u64 + 1)) as usize;
            idxs.swap(i, j);
        }
        let mut p = CCParams::clear();
        p.weights = weights;
        p.perceptual = perceptual;
        let lf = LaneF32::new(&pixels);
        let ok = if mode == 7 {
            est_mode7_idx_w128(&p, &idxs, num_pixels, &lf)
                == ccc_est_mode7_idx_scalar(&p, &idxs, num_pixels, &pixels)
        } else {
            est_idx_w128(mode, &p, &idxs, num_pixels, &lf)
                == ccc_est_idx_scalar(mode, &p, &idxs, num_pixels, &pixels)
        };
        if !ok {
            return false;
        }
    }
    true
}
