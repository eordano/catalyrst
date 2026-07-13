use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct ColorI {
    pub(super) c: [i32; 4],
}
#[derive(Clone, Copy, Default)]
pub(super) struct Vec4F {
    pub(super) c: [f32; 4],
}

#[inline]
pub(super) const fn saturate(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline]
pub(super) fn vec4f_dot(a: &Vec4F, b: &Vec4F) -> f32 {
    a.c[0] * b.c[0] + a.c[1] * b.c[1] + a.c[2] * b.c[2] + a.c[3] * b.c[3]
}
#[inline]
pub(super) fn vec4f_normalize(v: &mut Vec4F) {
    let mut s = v.c[0] * v.c[0] + v.c[1] * v.c[1] + v.c[2] * v.c[2] + v.c[3] * v.c[3];
    if s != 0.0 {
        s = 1.0 / super::sqrtf(s);
        v.c[0] *= s;
        v.c[1] *= s;
        v.c[2] *= s;
        v.c[3] *= s;
    }
}

#[inline]
pub(super) const fn iabs32(v: i32) -> i32 {
    v.abs()
}

#[inline]
pub(super) const fn itrunc(f: f32) -> i32 {
    f as i32
}

#[cfg(target_arch = "nvptx64")]
#[allow(dead_code)]
pub(super) trait F32Ext {
    fn floor(self) -> f32;
    fn abs(self) -> f32;
}
#[cfg(target_arch = "nvptx64")]
impl F32Ext for f32 {
    #[inline]
    fn floor(self) -> f32 {
        libm::floorf(self)
    }
    #[inline]
    fn abs(self) -> f32 {
        libm::fabsf(self)
    }
}

#[derive(Clone)]
pub(super) struct CCParams {
    pub(super) num_selector_weights: u32,
    pub(super) psel_weights: &'static [u32],
    pub(super) psel_weightsx: &'static [[f32; 4]],
    pub(super) comp_bits: u32,
    pub(super) weights: [u32; 4],
    pub(super) has_alpha: bool,
    pub(super) has_pbits: bool,
    pub(super) endpoints_share_pbit: bool,
    pub(super) perceptual: bool,
}
impl CCParams {
    pub(super) const fn clear() -> Self {
        CCParams {
            num_selector_weights: 0,
            psel_weights: &G_WEIGHTS2,
            psel_weightsx: &G_WEIGHTS2X,
            comp_bits: 0,
            weights: [1, 1, 1, 1],
            has_alpha: false,
            has_pbits: false,
            endpoints_share_pbit: false,
            perceptual: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct CCResults {
    pub(super) best_overall_err: u64,
    pub(super) low: ColorI,
    pub(super) high: ColorI,
    pub(super) pbits: [u32; 2],
    pub(super) selectors: [i32; 16],
    pub(super) selectors_temp: [i32; 16],
}
impl CCResults {
    pub(super) fn new() -> Self {
        CCResults {
            best_overall_err: u64::MAX,
            low: ColorI::default(),
            high: ColorI::default(),
            pbits: [0, 0],
            selectors: [0; 16],
            selectors_temp: [0; 16],
        }
    }
}

#[inline]
pub(super) fn scale_color(c: &ColorI, p: &CCParams) -> ColorI {
    let n = p.comp_bits + if p.has_pbits { 1 } else { 0 };
    let mut r = ColorI::default();
    for i in 0..4 {
        let mut v = (c.c[i] as u32) << (8 - n);
        v |= v >> n;
        r.c[i] = v as i32;
    }
    r
}

#[inline]
fn compute_color_distance_rgb(e1: &ColorI, e2: &ColorI, perceptual: bool, w: &[u32; 4]) -> u64 {
    compute_color_distance_rgb_scalar(e1, e2, perceptual, w)
}

#[inline]
pub(super) fn compute_color_distance_rgb_scalar(
    e1: &ColorI,
    e2: &ColorI,
    perceptual: bool,
    w: &[u32; 4],
) -> u64 {
    if perceptual {
        let l1 = e1.c[0] as f32 * 0.2126 + e1.c[1] as f32 * 0.7152 + e1.c[2] as f32 * 0.0722;
        let cr1 = e1.c[0] as f32 - l1;
        let cb1 = e1.c[2] as f32 - l1;
        let l2 = e2.c[0] as f32 * 0.2126 + e2.c[1] as f32 * 0.7152 + e2.c[2] as f32 * 0.0722;
        let cr2 = e2.c[0] as f32 - l2;
        let cb2 = e2.c[2] as f32 - l2;
        let dl = l1 - l2;
        let dcr = cr1 - cr2;
        let dcb = cb1 - cb2;
        (w[0] as f32 * (dl * dl)
            + w[1] as f32 * PR_WEIGHT * (dcr * dcr)
            + w[2] as f32 * PB_WEIGHT * (dcb * dcb)) as i64 as u64
    } else {
        let dr = e1.c[0] as f32 - e2.c[0] as f32;
        let dg = e1.c[1] as f32 - e2.c[1] as f32;
        let db = e1.c[2] as f32 - e2.c[2] as f32;
        (w[0] as f32 * dr * dr + w[1] as f32 * dg * dg + w[2] as f32 * db * db) as i64 as u64
    }
}

#[inline]
fn compute_color_distance_rgba(e1: &ColorI, e2: &ColorI, perceptual: bool, w: &[u32; 4]) -> u64 {
    compute_color_distance_rgba_scalar(e1, e2, perceptual, w)
}

#[inline]
pub(super) fn compute_color_distance_rgba_scalar(
    e1: &ColorI,
    e2: &ColorI,
    perceptual: bool,
    w: &[u32; 4],
) -> u64 {
    let da = e1.c[3] as f32 - e2.c[3] as f32;
    let a_err = w[3] as f32 * (da * da);
    if perceptual {
        let l1 = e1.c[0] as f32 * 0.2126 + e1.c[1] as f32 * 0.7152 + e1.c[2] as f32 * 0.0722;
        let cr1 = e1.c[0] as f32 - l1;
        let cb1 = e1.c[2] as f32 - l1;
        let l2 = e2.c[0] as f32 * 0.2126 + e2.c[1] as f32 * 0.7152 + e2.c[2] as f32 * 0.0722;
        let cr2 = e2.c[0] as f32 - l2;
        let cb2 = e2.c[2] as f32 - l2;
        let dl = l1 - l2;
        let dcr = cr1 - cr2;
        let dcb = cb1 - cb2;
        (w[0] as f32 * (dl * dl)
            + w[1] as f32 * PR_WEIGHT * (dcr * dcr)
            + w[2] as f32 * PB_WEIGHT * (dcb * dcb)
            + a_err) as i64 as u64
    } else {
        let dr = e1.c[0] as f32 - e2.c[0] as f32;
        let dg = e1.c[1] as f32 - e2.c[1] as f32;
        let db = e1.c[2] as f32 - e2.c[2] as f32;
        (w[0] as f32 * dr * dr + w[1] as f32 * dg * dg + w[2] as f32 * db * db + a_err) as i64
            as u64
    }
}

pub(super) fn compute_lsq_endpoints_rgba(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_r, mut t_r) = (0f32, 0f32);
    let (mut q00_g, mut t_g) = (0f32, 0f32);
    let (mut q00_b, mut t_b) = (0f32, 0f32);
    let (mut q00_a, mut t_a) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_r += w * colors[i].c[0] as f32;
        t_r += colors[i].c[0] as f32;
        q00_g += w * colors[i].c[1] as f32;
        t_g += colors[i].c[1] as f32;
        q00_b += w * colors[i].c[2] as f32;
        t_b += colors[i].c[2] as f32;
        q00_a += w * colors[i].c[3] as f32;
        t_a += colors[i].c[3] as f32;
    }
    let q10_r = t_r - q00_r;
    let q10_g = t_g - q00_g;
    let q10_b = t_b - q00_b;
    let q10_a = t_a - q00_a;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    xl.c[0] = iz00 * q00_r + iz01 * q10_r;
    xh.c[0] = iz10 * q00_r + iz11 * q10_r;
    xl.c[1] = iz00 * q00_g + iz01 * q10_g;
    xh.c[1] = iz10 * q00_g + iz11 * q10_g;
    xl.c[2] = iz00 * q00_b + iz01 * q10_b;
    xh.c[2] = iz10 * q00_b + iz11 * q10_b;
    xl.c[3] = iz00 * q00_a + iz01 * q10_a;
    xh.c[3] = iz10 * q00_a + iz11 * q10_a;
}

pub(super) fn compute_lsq_endpoints_rgb(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    compute_lsq_endpoints_rgb_scalar(n, sel, sw, xl, xh, colors)
}

pub(super) fn compute_lsq_endpoints_rgb_scalar(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut Vec4F,
    xh: &mut Vec4F,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_r, mut t_r) = (0f32, 0f32);
    let (mut q00_g, mut t_g) = (0f32, 0f32);
    let (mut q00_b, mut t_b) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_r += w * colors[i].c[0] as f32;
        t_r += colors[i].c[0] as f32;
        q00_g += w * colors[i].c[1] as f32;
        t_g += colors[i].c[1] as f32;
        q00_b += w * colors[i].c[2] as f32;
        t_b += colors[i].c[2] as f32;
    }
    let q10_r = t_r - q00_r;
    let q10_g = t_g - q00_g;
    let q10_b = t_b - q00_b;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    xl.c[0] = iz00 * q00_r + iz01 * q10_r;
    xh.c[0] = iz10 * q00_r + iz11 * q10_r;
    xl.c[1] = iz00 * q00_g + iz01 * q10_g;
    xh.c[1] = iz10 * q00_g + iz11 * q10_g;
    xl.c[2] = iz00 * q00_b + iz01 * q10_b;
    xh.c[2] = iz10 * q00_b + iz11 * q10_b;
}

pub(super) fn compute_lsq_endpoints_a(
    n: usize,
    sel: &[i32],
    sw: &[[f32; 4]],
    xl: &mut f32,
    xh: &mut f32,
    colors: &[ColorI],
) {
    let (mut z00, mut z10, mut z11) = (0f32, 0f32, 0f32);
    let (mut q00_a, mut t_a) = (0f32, 0f32);
    for i in 0..n {
        let s = sel[i] as usize;
        z00 += sw[s][0];
        z10 += sw[s][1];
        z11 += sw[s][2];
        let w = sw[s][3];
        q00_a += w * colors[i].c[3] as f32;
        t_a += colors[i].c[3] as f32;
    }
    let q10_a = t_a - q00_a;
    let z01 = z10;
    let mut det = z00 * z11 - z01 * z10;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let iz00 = z11 * det;
    let iz01 = -z01 * det;
    let iz10 = -z10 * det;
    let iz11 = z00 * det;
    *xl = iz00 * q00_a + iz01 * q10_a;
    *xh = iz10 * q00_a + iz11 * q10_a;
}

pub(super) fn pack_mode1_to_one_color(
    p: &CCParams,
    t: &OptTables,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..2 {
        let err =
            t.mode1[r][pp].error as u32 + t.mode1[g][pp].error as u32 + t.mode1[b][pp].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let er = &t.mode1[r][best_p];
    let eg = &t.mode1[g][best_p];
    let eb = &t.mode1[b][best_p];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, 0];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, 0];
    res.pbits = [best_p as u32, 0];
    for i in 0..num_pixels {
        res.selectors[i] = MODE1_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = ((res.low.c[i] as u32) << 1 | res.pbits[0]) << 1;
        low |= low >> 7;
        let mut high = ((res.high.c[i] as u32) << 1 | res.pbits[0]) << 1;
        high |= high >> 7;
        pc.c[i] =
            ((low * (64 - G_WEIGHTS3[MODE1_IDX]) + high * G_WEIGHTS3[MODE1_IDX] + 32) >> 6) as i32;
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

pub(super) fn pack_mode24_to_one_color(
    p: &CCParams,
    t: &OptTables,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let (er, eg, eb) = if p.num_selector_weights == 8 {
        (t.mode4_3[r], t.mode4_3[g], t.mode4_3[b])
    } else {
        (t.mode4_2[r], t.mode4_2[g], t.mode4_2[b])
    };
    res.low.c = [
        (er & 0xFF) as i32,
        (eg & 0xFF) as i32,
        (eb & 0xFF) as i32,
        0,
    ];
    res.high.c = [(er >> 8) as i32, (eg >> 8) as i32, (eb >> 8) as i32, 0];
    let idx = if p.num_selector_weights == 8 {
        MODE4_IDX3
    } else {
        MODE4_IDX2
    };
    for i in 0..num_pixels {
        res.selectors[i] = idx as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = (res.low.c[i] as u32) << 3;
        low |= low >> 5;
        let mut high = (res.high.c[i] as u32) << 3;
        high |= high >> 5;
        pc.c[i] = if p.num_selector_weights == 8 {
            ((low * (64 - G_WEIGHTS3[MODE4_IDX3]) + high * G_WEIGHTS3[MODE4_IDX3] + 32) >> 6) as i32
        } else {
            ((low * (64 - G_WEIGHTS2[MODE4_IDX2]) + high * G_WEIGHTS2[MODE4_IDX2] + 32) >> 6) as i32
        };
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

pub(super) fn pack_mode0_to_one_color(
    p: &CCParams,
    t: &OptTables,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let err = t.mode0[r][pp >> 1][pp & 1].error as u32
            + t.mode0[g][pp >> 1][pp & 1].error as u32
            + t.mode0[b][pp >> 1][pp & 1].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let er = &t.mode0[r][best_p >> 1][best_p & 1];
    let eg = &t.mode0[g][best_p >> 1][best_p & 1];
    let eb = &t.mode0[b][best_p >> 1][best_p & 1];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, 0];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, 0];
    res.pbits = [(best_p & 1) as u32, (best_p >> 1) as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE0_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..3 {
        let mut low = ((res.low.c[i] as u32) << 1 | res.pbits[0]) << 3;
        low |= low >> 5;
        let mut high = ((res.high.c[i] as u32) << 1 | res.pbits[1]) << 3;
        high |= high >> 5;
        pc.c[i] =
            ((low * (64 - G_WEIGHTS3[MODE0_IDX]) + high * G_WEIGHTS3[MODE0_IDX] + 32) >> 6) as i32;
    }
    pc.c[3] = 255;
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgb(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

pub(super) fn pack_mode6_to_one_color(
    p: &CCParams,
    t: &OptTables,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let hi_p = pp >> 1;
        let lo_p = pp & 1;
        let err = t.mode6[r][hi_p][lo_p].error as u32
            + t.mode6[g][hi_p][lo_p].error as u32
            + t.mode6[b][hi_p][lo_p].error as u32
            + t.mode6[a][hi_p][lo_p].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let best_hi = best_p >> 1;
    let best_lo = best_p & 1;
    let er = &t.mode6[r][best_hi][best_lo];
    let eg = &t.mode6[g][best_hi][best_lo];
    let eb = &t.mode6[b][best_hi][best_lo];
    let ea = &t.mode6[a][best_hi][best_lo];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, ea.lo as i32];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, ea.hi as i32];
    res.pbits = [best_lo as u32, best_hi as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE6_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..4 {
        let low = (res.low.c[i] as u32) << 1 | res.pbits[0];
        let high = (res.high.c[i] as u32) << 1 | res.pbits[1];
        pc.c[i] =
            ((low * (64 - G_WEIGHTS4[MODE6_IDX]) + high * G_WEIGHTS4[MODE6_IDX] + 32) >> 6) as i32;
    }
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgba(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}

pub(super) fn pack_mode7_to_one_color(
    p: &CCParams,
    t: &OptTables,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let mut best_err = u32::MAX;
    let mut best_p = 0usize;
    for pp in 0..4usize {
        let hi_p = pp >> 1;
        let lo_p = pp & 1;
        let err = t.mode7[r][hi_p][lo_p].error as u32
            + t.mode7[g][hi_p][lo_p].error as u32
            + t.mode7[b][hi_p][lo_p].error as u32
            + t.mode7[a][hi_p][lo_p].error as u32;
        if err < best_err {
            best_err = err;
            best_p = pp;
        }
    }
    let best_hi = best_p >> 1;
    let best_lo = best_p & 1;
    let er = &t.mode7[r][best_hi][best_lo];
    let eg = &t.mode7[g][best_hi][best_lo];
    let eb = &t.mode7[b][best_hi][best_lo];
    let ea = &t.mode7[a][best_hi][best_lo];
    res.low.c = [er.lo as i32, eg.lo as i32, eb.lo as i32, ea.lo as i32];
    res.high.c = [er.hi as i32, eg.hi as i32, eb.hi as i32, ea.hi as i32];
    res.pbits = [best_lo as u32, best_hi as u32];
    for i in 0..num_pixels {
        res.selectors[i] = MODE7_IDX as i32;
    }
    let mut pc = ColorI::default();
    for i in 0..4 {
        let low = (res.low.c[i] as u32) << 1 | res.pbits[0];
        let high = (res.high.c[i] as u32) << 1 | res.pbits[1];
        pc.c[i] =
            ((low * (64 - G_WEIGHTS2[MODE7_IDX]) + high * G_WEIGHTS2[MODE7_IDX] + 32) >> 6) as i32;
    }
    let mut total = 0u64;
    for i in 0..num_pixels {
        total += compute_color_distance_rgba(&pc, &pixels[i], p.perceptual, &p.weights);
    }
    res.best_overall_err = total;
    total
}
