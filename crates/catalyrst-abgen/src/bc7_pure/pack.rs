use super::*;

pub(super) fn pack_mode1_to_one_color(
    p: &CCParams,
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
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
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
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
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
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
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
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
    res: &mut CCResults,
    r: usize,
    g: usize,
    b: usize,
    a: usize,
    num_pixels: usize,
    pixels: &[ColorI],
) -> u64 {
    let t = opt();
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
