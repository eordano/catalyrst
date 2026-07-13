use super::*;

#[test]
fn wgpu_bc7_u64ops_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_u64ops_golden") else {
        return;
    };
    let vals: Vec<u64> = vec![
        0,
        1,
        2,
        3,
        0x7fff,
        0x8000,
        0xffff,
        0x10000,
        0x7fffffff,
        0x80000000,
        0xfffffffe,
        0xffffffff,
        0x100000000,
        0x1ffffffff,
        8191,
        8192,
        8193,
        0x123456789abcdef0,
        0x8000000000000000,
        0xfffffffffffffffe,
        u64::MAX,
        (1u64 << 45) + 12345,
        1u64 << 33,
        1u64 << 31,
    ];
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    for &a in &vals {
        for &b in &vals {
            pairs.push((a, b));
        }
    }
    let mut st = 0x9e3779b97f4a7c15u64;
    for _ in 0..400 {
        pairs.push((xs64(&mut st), xs64(&mut st)));
    }
    let fs: Vec<f32> = vec![
        0.0,
        0.25,
        0.5,
        0.75,
        0.999,
        1.0,
        1.5,
        2.0,
        255.0,
        65535.75,
        8323200.0,
        30000000.5,
        123456789.0,
        2147483520.0,
        33554431.5,
    ];
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u32> = Vec::new();
    let push64 = |w: &mut Vec<u32>, v: u64| {
        w.push(v as u32);
        w.push((v >> 32) as u32);
    };
    for (i, &(a, b)) in pairs.iter().enumerate() {
        let f = fs[i % fs.len()];
        let sh = 1 + (i as u32) % 31;
        input.extend_from_slice(&[
            a as u32,
            (a >> 32) as u32,
            b as u32,
            (b >> 32) as u32,
            f.to_bits(),
            sh,
        ]);
        push64(&mut want, a.wrapping_add(b));
        push64(&mut want, a.saturating_add(b));
        push64(&mut want, a.saturating_mul(b));
        push64(&mut want, (a as u32 as u64) * (b as u32 as u64));
        want.push((a < b) as u32 | (((a <= b) as u32) << 1) | (((a == b) as u32) << 2));
        push64(&mut want, a >> sh);
        let conv = f as i64 as u64;
        assert!(conv < 1u64 << 31, "conv case {f} out of proven band");
        want.push(conv as u32);
    }
    let out = vec![0u8; pairs.len() * 12 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_u64ops",
        pairs.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "u64 ops");
}

#[test]
fn wgpu_bc7_vecmath_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_vecmath_golden") else {
        return;
    };
    let mut cases: Vec<([f32; 4], [f32; 4], [i32; 4], u32, bool, f32, i32)> = vec![
        ([0.0; 4], [0.0; 4], [0, 0, 0, 0], 4, false, 0.0, 0),
        (
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 2.0, 3.0, 4.0],
            [15, 7, 3, 1],
            7,
            true,
            1.0,
            -5,
        ),
        (
            [3.0, -4.0, 12.0, 0.5],
            [-1.5, 2.5, 0.25, 8.0],
            [31, 0, 31, 16],
            5,
            false,
            -0.5,
            i32::MIN + 1,
        ),
        (
            [255.0, 255.0, 255.0, 255.0],
            [255.0; 4],
            [63, 63, 63, 63],
            6,
            true,
            1.5,
            i32::MAX,
        ),
    ];
    for f in [
        3.0f32,
        -3.0,
        0.1,
        7.0,
        10.0,
        1.0e-30,
        1.0e30,
        6.931_472,
        2.0,
        4.0,
        1.0 / 3.0,
        0.999_999_94,
        1.000_000_1,
    ] {
        cases.push((
            [1.0, 2.0, 3.0, 4.0],
            [4.0, 3.0, 2.0, 1.0],
            [5, 9, 2, 7],
            5,
            true,
            f,
            42,
        ));
    }
    let mut st = 0x5851f42d4c957f2du64;
    for _ in 0..500 {
        let rf = |st: &mut u64| ((xs64(st) % 400_000) as f32 - 200_000.0) / 128.0;
        let v = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
        let a = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
        let comp_bits = 4 + (xs64(&mut st) % 4) as u32;
        let has_pbits = xs64(&mut st) & 1 == 1;
        let nbits = comp_bits + has_pbits as u32;
        let mut c = [0i32; 4];
        for k in 0..4 {
            c[k] = (xs64(&mut st) & ((1u64 << nbits) - 1)) as i32;
        }
        let f = rf(&mut st);
        let mut x = xs64(&mut st) as i32;
        if x == i32::MIN {
            x = 0;
        }
        cases.push((v, a, c, comp_bits, has_pbits, f, x));
    }
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u32> = Vec::new();
    for &(v, a, c, comp_bits, has_pbits, f, x) in &cases {
        for q in v {
            input.push(q.to_bits());
        }
        for q in a {
            input.push(q.to_bits());
        }
        for q in c {
            input.push(q as u32);
        }
        input.push(comp_bits);
        input.push(has_pbits as u32);
        input.push(f.to_bits());
        input.push(x as u32);
        for q in probe::vec4f_normalize(v) {
            want.push(q.to_bits());
        }
        want.push(probe::vec4f_dot(v, a).to_bits());
        for q in probe::scale_color(c, comp_bits, has_pbits) {
            want.push(q as u32);
        }
        want.push(probe::saturate(f).to_bits());
        want.push(probe::itrunc(f) as u32);
        want.push(probe::iabs32(x) as u32);
        want.push(probe::sq(f).to_bits());
        let i = (has_pbits as usize) % 4;
        want.push(match comp_bits % 3 {
            0 => probe::weights2()[i],
            1 => probe::weights3()[i],
            _ => probe::weights4()[i],
        });
        if f != 0.0 {
            want.push((1.0f32 / f).to_bits());
            want.push(f.abs().sqrt().to_bits());
        } else {
            want.push(0);
            want.push(0);
        }
    }
    let out = vec![0u8; cases.len() * 16 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_vecmath",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "vecmath");
}

fn dist_cases() -> Vec<([i32; 4], [i32; 4], [u32; 4], bool)> {
    let sets = weight_sets();
    let corners = corner_colors();
    let mut cases = Vec::new();
    for &w in &sets {
        for perc in [false, true] {
            for e1 in &corners {
                for e2 in &corners {
                    cases.push((*e1, *e2, w, perc));
                }
            }
        }
    }
    let mut st = 0x2545f4914f6cdd1du64;
    for _ in 0..2000 {
        let mut e1 = [0i32; 4];
        let mut e2 = [0i32; 4];
        for k in 0..4 {
            e1[k] = (xs64(&mut st) % 256) as i32;
            e2[k] = (xs64(&mut st) % 256) as i32;
        }
        let w = sets[(xs64(&mut st) % sets.len() as u64) as usize];
        let perc = xs64(&mut st) & 1 == 1;
        cases.push((e1, e2, w, perc));
    }
    cases
}

fn check_dist_entry(
    g: &crate::gpu::wgpu::Gpu,
    entry: &str,
    host: impl Fn([i32; 4], [i32; 4], bool, [u32; 4]) -> u64,
) {
    let cases = dist_cases();
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u32> = Vec::new();
    for &(e1, e2, w, perc) in &cases {
        for q in e1 {
            input.push(q as u32);
        }
        for q in e2 {
            input.push(q as u32);
        }
        input.extend_from_slice(&w);
        input.push(perc as u32);
        input.extend_from_slice(&[0, 0, 0]);
        let d = host(e1, e2, perc, w);
        assert!(d < 1u64 << 31, "{entry}: host dist out of proven band");
        want.push(d as u32);
        want.push((d >> 32) as u32);
    }
    let out = vec![0u8; cases.len() * 2 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        entry,
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), entry);
}

#[test]
fn wgpu_bc7_dist_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_dist_golden") else {
        return;
    };
    check_dist_entry(g, "bc7_test_dist_rgb", |e1, e2, p, w| {
        probe::dist_rgb(e1, e2, p, w)
    });
    check_dist_entry(g, "bc7_test_dist_rgba", |e1, e2, p, w| {
        probe::dist_rgba(e1, e2, p, w)
    });
}

fn lsq_cases() -> Vec<(u32, u32, [i32; 16], [[i32; 4]; 16])> {
    let mut st = 0x94d049bb133111ebu64;
    let mut cases = Vec::new();
    for tbl in 0..3u32 {
        let tlen = probe::weightsx_table_len(tbl as usize) as u64;
        for ci in 0..140usize {
            let n = 1 + (xs64(&mut st) % 16) as u32;
            let mut sel = [0i32; 16];
            let mut colors = [[0i32; 4]; 16];
            for i in 0..16 {
                sel[i] = (xs64(&mut st) % tlen) as i32;
                for k in 0..4 {
                    colors[i][k] = (xs64(&mut st) % 256) as i32;
                }
            }
            if ci % 7 == 0 {
                sel = [(xs64(&mut st) % tlen) as i32; 16];
            }
            if ci % 11 == 0 {
                colors = [colors[0]; 16];
            }
            if ci % 13 == 0 {
                for (i, s) in sel.iter_mut().enumerate() {
                    *s = if i % 2 == 0 { 0 } else { (tlen - 1) as i32 };
                }
            }
            cases.push((n, tbl, sel, colors));
        }
    }
    cases
}

fn lsq_input_words(cases: &[(u32, u32, [i32; 16], [[i32; 4]; 16])]) -> Vec<u32> {
    let mut input = Vec::with_capacity(cases.len() * 84);
    for &(n, tbl, sel, colors) in cases {
        input.push(n);
        input.push(tbl);
        input.extend_from_slice(&[0, 0]);
        for s in sel {
            input.push(s as u32);
        }
        for c in colors {
            for q in c {
                input.push(q as u32);
            }
        }
    }
    input
}

#[test]
fn wgpu_bc7_lsq_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_lsq_golden") else {
        return;
    };
    let cases = lsq_cases();
    let input = words_bytes(&lsq_input_words(&cases));
    for (entry, width) in [
        ("bc7_test_lsq_rgba", 8usize),
        ("bc7_test_lsq_rgb", 8),
        ("bc7_test_lsq_a", 2),
    ] {
        let mut want: Vec<u32> = Vec::new();
        for &(n, tbl, sel, colors) in &cases {
            match entry {
                "bc7_test_lsq_rgba" => {
                    let (xl, xh) = probe::lsq_rgba(n as usize, &sel, tbl as usize, &colors);
                    for q in xl.iter().chain(xh.iter()) {
                        want.push(q.to_bits());
                    }
                }
                "bc7_test_lsq_rgb" => {
                    let (xl, xh) = probe::lsq_rgb(n as usize, &sel, tbl as usize, &colors);
                    for q in xl.iter().chain(xh.iter()) {
                        want.push(q.to_bits());
                    }
                }
                _ => {
                    let (xl, xh) = probe::lsq_a(n as usize, &sel, tbl as usize, &colors);
                    want.push(xl.to_bits());
                    want.push(xh.to_bits());
                }
            }
        }
        let out = vec![0u8; cases.len() * width * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            entry,
            cases.len() as u32,
            0,
            &[(4, &input), (3, &out)],
            3,
        );
        assert_bytes_eq(&got, &words_bytes(&want), entry);
    }
}

fn pack_colors() -> Vec<[i32; 4]> {
    let mut v = corner_colors();
    for x in 0..256i32 {
        v.push([x, x, x, 255]);
        v.push([x, x, x, x]);
        v.push([x, 255 - x, x / 2, 255]);
        v.push([255 - x, 0, x, 128]);
    }
    let mut st = 0x1234_5678_9abc_def0u64;
    while v.len() < 4300 {
        let mut c = [0i32; 4];
        for k in 0..4 {
            c[k] = (xs64(&mut st) % 256) as i32;
        }
        v.push(c);
    }
    v
}

struct PackCase {
    color: [i32; 4],
    nsw: u32,
    perceptual: bool,
    weights: [u32; 4],
    num_pixels: usize,
    pixels: [[i32; 4]; 16],
}

fn pack_cases(nsw_options: &[u32]) -> Vec<PackCase> {
    let colors = pack_colors();
    let sets = weight_sets();
    let mut st = 0x0123_4567_89ab_cdefu64;
    let mut cases = Vec::new();
    for &color in &colors {
        for &weights in &sets {
            for perceptual in [false, true] {
                for &nsw in nsw_options {
                    let num_pixels = 1 + (xs64(&mut st) % 16) as usize;
                    let mut pixels = [color; 16];
                    if xs64(&mut st).is_multiple_of(16) {
                        for px in pixels.iter_mut() {
                            for k in 0..4 {
                                px[k] = (xs64(&mut st) % 256) as i32;
                            }
                        }
                    }
                    cases.push(PackCase {
                        color,
                        nsw,
                        perceptual,
                        weights,
                        num_pixels,
                        pixels,
                    });
                }
            }
        }
    }
    cases
}

fn check_pack_entry(
    g: &crate::gpu::wgpu::Gpu,
    entry: &str,
    nsw_options: &[u32],
    host: impl Fn(u32, bool, [u32; 4], [usize; 4], usize, &[[i32; 4]; 16]) -> probe::PackOut,
) {
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let cases = pack_cases(nsw_options);
    let mut input: Vec<u32> = Vec::with_capacity(cases.len() * 75);
    let mut want: Vec<u32> = Vec::with_capacity(cases.len() * 28);
    for c in &cases {
        input.push(c.num_pixels as u32);
        for k in 0..4 {
            input.push(c.color[k] as u32);
        }
        input.push(c.nsw);
        input.push(c.perceptual as u32);
        input.extend_from_slice(&c.weights);
        for px in &c.pixels {
            for &q in px {
                input.push(q as u32);
            }
        }
        let rgba = [
            c.color[0] as usize,
            c.color[1] as usize,
            c.color[2] as usize,
            c.color[3] as usize,
        ];
        let (low, high, pbits, sel, err) = host(
            c.nsw,
            c.perceptual,
            c.weights,
            rgba,
            c.num_pixels,
            &c.pixels,
        );
        for q in low {
            want.push(q as u32);
        }
        for q in high {
            want.push(q as u32);
        }
        want.extend_from_slice(&pbits);
        for q in sel {
            want.push(q as u32);
        }
        want.push(err as u32);
        want.push((err >> 32) as u32);
    }
    let out = vec![0u8; cases.len() * 28 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        entry,
        cases.len() as u32,
        0,
        &[(2, &opt_bytes), (4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), entry);
}

#[test]
fn wgpu_bc7_pack_one_color_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_pack_one_color_golden") else {
        return;
    };
    let t0 = build_opt_tables();
    check_pack_entry(g, "bc7_test_pack_mode0", &[8], |_, p, w, rgba, n, px| {
        probe::pack_mode0_one_color(p, w, rgba, n, px, &t0)
    });
    let t1 = build_opt_tables();
    check_pack_entry(g, "bc7_test_pack_mode1", &[8], |_, p, w, rgba, n, px| {
        probe::pack_mode1_one_color(p, w, rgba, n, px, &t1)
    });
    let t24 = build_opt_tables();
    check_pack_entry(
        g,
        "bc7_test_pack_mode24",
        &[4, 8],
        |nsw, p, w, rgba, n, px| probe::pack_mode24_one_color(nsw, p, w, rgba, n, px, &t24),
    );
    let t6 = build_opt_tables();
    check_pack_entry(g, "bc7_test_pack_mode6", &[16], |_, p, w, rgba, n, px| {
        probe::pack_mode6_one_color(p, w, rgba, n, px, &t6)
    });
    let t7 = build_opt_tables();
    check_pack_entry(g, "bc7_test_pack_mode7", &[4], |_, p, w, rgba, n, px| {
        probe::pack_mode7_one_color(p, w, rgba, n, px, &t7)
    });
}

#[test]
fn wgpu_bc7_fixdeg_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_fixdeg_golden") else {
        return;
    };
    let mut st = 0xfeed_face_dead_beefu64;
    let mut cases: Vec<(u32, i32, [i32; 4], [i32; 4], [f32; 4], [f32; 4])> = Vec::new();
    let iscales = [3i32, 7, 15, 31, 63, 127];
    for mode in [0u32, 1, 2, 4, 6, 7] {
        for &iscale in &iscales {
            for ci in 0..120usize {
                let mut tmin = [0i32; 4];
                let mut tmax = [0i32; 4];
                for k in 0..4 {
                    tmin[k] = (xs64(&mut st) % (iscale as u64 + 1)) as i32;
                    tmax[k] = (xs64(&mut st) % (iscale as u64 + 1)) as i32;
                }
                match ci % 5 {
                    0 => {
                        tmax = tmin;
                    }
                    1 => {
                        for k in 0..3 {
                            if xs64(&mut st) & 1 == 1 {
                                tmax[k] = tmin[k];
                            }
                        }
                    }
                    2 => {
                        tmin = [0, iscale, iscale >> 1, 0];
                        tmax = tmin;
                    }
                    3 => {
                        tmin = [iscale, 0, (iscale >> 1) + 1, iscale];
                        tmax = tmin;
                    }
                    _ => {}
                }
                let rf = |st: &mut u64| (xs64(st) % 2560) as f32 / 2559.0;
                let mut xl = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
                let xh = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
                if ci % 3 == 0 {
                    xl = xh;
                }
                if ci % 7 == 0 {
                    xl[1] = xh[1];
                }
                cases.push((mode, iscale, tmin, tmax, xl, xh));
            }
        }
    }
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u32> = Vec::new();
    for &(mode, iscale, tmin, tmax, xl, xh) in &cases {
        input.push(mode);
        input.push(iscale as u32);
        for q in tmin {
            input.push(q as u32);
        }
        for q in tmax {
            input.push(q as u32);
        }
        for q in xl {
            input.push(q.to_bits());
        }
        for q in xh {
            input.push(q.to_bits());
        }
        let (a, b) = probe::fix_degenerate(mode as usize, tmin, tmax, xl, xh, iscale);
        for q in a {
            want.push(q as u32);
        }
        for q in b {
            want.push(q as u32);
        }
    }
    let out = vec![0u8; cases.len() * 8 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_fixdeg",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_fixdeg");
}

fn eval_buckets() -> Vec<(u32, u32, u32, bool, bool, bool)> {
    vec![
        (8, 1, 4, false, true, false),
        (8, 1, 6, false, true, true),
        (4, 0, 5, false, false, false),
        (4, 0, 7, false, true, false),
        (8, 1, 5, false, false, false),
        (4, 0, 7, false, false, false),
        (16, 2, 7, false, true, false),
        (16, 2, 7, true, true, false),
        (4, 0, 5, true, true, false),
        (16, 2, 7, true, false, false),
    ]
}

fn eval_cases() -> Vec<(probe::EvalCase, [[i32; 4]; 16])> {
    let sets = weight_sets();
    let mut st = 0x00c0_ffee_1234_5678u64;
    let mut cases = Vec::new();
    for (bi, &(nsw, tbl, comp_bits, has_alpha, has_pbits, share)) in
        eval_buckets().iter().enumerate()
    {
        for perceptual in [false, true] {
            for (wi, &weights) in sets.iter().enumerate() {
                for ci in 0..40usize {
                    let lim = 1u64 << comp_bits;
                    let mut low = [0i32; 4];
                    let mut high = [0i32; 4];
                    for k in 0..4 {
                        low[k] = (xs64(&mut st) % lim) as i32;
                        high[k] = (xs64(&mut st) % lim) as i32;
                    }
                    let mut pbits = [(xs64(&mut st) & 1) as u32, (xs64(&mut st) & 1) as u32];
                    match ci % 8 {
                        0 => {
                            high = low;
                        }
                        1 => {
                            high = low;
                            pbits = [pbits[0], pbits[0]];
                        }
                        2 => {
                            std::mem::swap(&mut low, &mut high);
                        }
                        3 => {
                            low = [0; 4];
                            high = [(lim - 1) as i32; 4];
                        }
                        4 => {
                            low = [(lim - 1) as i32; 4];
                            high = [0; 4];
                        }
                        _ => {}
                    }
                    let mut blk = [0u8; 64];
                    gen_block(&mut st, bi * 7 + wi * 3 + ci, &mut blk);
                    let mut pixels = [[0i32; 4]; 16];
                    for i in 0..16 {
                        for k in 0..4 {
                            pixels[i][k] = blk[i * 4 + k] as i32;
                        }
                    }
                    let num_pixels = if ci % 5 == 0 {
                        1 + (xs64(&mut st) % 16) as usize
                    } else {
                        16
                    };
                    let init_err = match ci % 9 {
                        0 => 0,
                        1 => 42,
                        _ => u64::MAX,
                    };
                    cases.push((
                        probe::EvalCase {
                            low,
                            high,
                            pbits,
                            nsw,
                            tbl: tbl as usize,
                            comp_bits,
                            weights,
                            has_alpha,
                            has_pbits,
                            share_pbit: share,
                            perceptual,
                            init_err,
                            num_pixels,
                        },
                        pixels,
                    ));
                }
            }
        }
    }
    cases
}

#[test]
fn wgpu_bc7_evalsol_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_evalsol_golden") else {
        return;
    };
    let cases = eval_cases();
    let mut input: Vec<u32> = Vec::with_capacity(cases.len() * 88);
    let mut want: Vec<u32> = Vec::with_capacity(cases.len() * 46);
    for (c, pixels) in &cases {
        input.push(c.nsw);
        input.push(c.tbl as u32);
        input.push(c.comp_bits);
        input.push(c.has_alpha as u32);
        input.push(c.has_pbits as u32);
        input.push(c.share_pbit as u32);
        input.push(c.perceptual as u32);
        input.push(c.num_pixels as u32);
        input.push(c.init_err as u32);
        input.push((c.init_err >> 32) as u32);
        input.extend_from_slice(&c.weights);
        for q in c.low {
            input.push(q as u32);
        }
        for q in c.high {
            input.push(q as u32);
        }
        input.extend_from_slice(&c.pbits);
        for px in pixels {
            for &q in px {
                input.push(q as u32);
            }
        }
        let (ret, best, low, high, pbits, sel, seltmp) = probe::eval_solution(c, pixels);
        assert!(ret < 1u64 << 31, "evalsol: host total out of proven band");
        want.push(ret as u32);
        want.push((ret >> 32) as u32);
        want.push(best as u32);
        want.push((best >> 32) as u32);
        for q in low {
            want.push(q as u32);
        }
        for q in high {
            want.push(q as u32);
        }
        want.extend_from_slice(&pbits);
        for q in sel {
            want.push(q as u32);
        }
        for q in seltmp {
            want.push(q as u32);
        }
    }
    let out = vec![0u8; cases.len() * 46 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_evalsol",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_evalsol");
}
