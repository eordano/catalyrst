use super::*;

#[test]
fn ccc_const_bits_pin() {
    assert_eq!((1.0f32 / 255.0).to_bits(), 0x3b808081);
    assert_eq!(0.213f32.to_bits(), 0x3e5a1cac);
    assert_eq!(0.715f32.to_bits(), 0x3f370a3d);
    assert_eq!(0.072f32.to_bits(), 0x3d9374bc);
    assert_eq!(0.9f32.to_bits(), 0x3f666666);
    assert_eq!(0.7f32.to_bits(), 0x3f333333);
    assert_eq!(1e-10f32.to_bits(), 0x2edbe6ff);
    assert_eq!(1e9f32.to_bits(), 0x4e6e6b28);
    assert_eq!(1e10f32.to_bits(), 0x501502f9);
}

#[test]
fn wgpu_bc7_div_exact_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_div_exact_golden") else {
        return;
    };
    let mut cases: Vec<(f32, f32)> = Vec::new();
    for a in -320i32..=320 {
        for b in 1i32..=40 {
            cases.push((a as f32, b as f32));
        }
    }
    for x in 0..=255i32 {
        cases.push((x as f32, 255.0));
        cases.push((-x as f32, 255.0));
    }
    cases.push((0.0, 3.0));
    cases.push((-0.0, 3.0));
    cases.push((0.0, -3.0));
    let mut st = 0xd1a1_50f2_55aa_1111u64;
    for _ in 0..24_000 {
        let gen = |st: &mut u64| -> f32 {
            let sign = (xs64(st) & 1) << 31;
            let exp = 117 + (xs64(st) % 31);
            let mant = xs64(st) & 0x7fffff;
            f32::from_bits(sign as u32 | ((exp as u32) << 23) | mant as u32)
        };
        cases.push((gen(&mut st), gen(&mut st)));
    }
    let mut input: Vec<u32> = Vec::with_capacity(cases.len() * 2);
    let mut want: Vec<u32> = Vec::with_capacity(cases.len());
    for &(a, b) in &cases {
        input.push(a.to_bits());
        input.push(b.to_bits());
        want.push((a / b).to_bits());
    }
    let out = vec![0u8; cases.len() * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_div",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_div");
}

fn push_ccp(input: &mut Vec<u32>, c: &probe::CCP) {
    input.push(c.nsw);
    input.push(c.tbl as u32);
    input.push(c.comp_bits);
    input.push(c.has_alpha as u32);
    input.push(c.has_pbits as u32);
    input.push(c.share_pbit as u32);
    input.push(c.perceptual as u32);
    input.extend_from_slice(&c.weights);
}

fn push_init(input: &mut Vec<u32>, init: &probe::CCInit) {
    input.push(init.err as u32);
    input.push((init.err >> 32) as u32);
    for q in init.low {
        input.push(q as u32);
    }
    for q in init.high {
        input.push(q as u32);
    }
    input.extend_from_slice(&init.pbits);
}

fn push_ccout(want: &mut Vec<u32>, out: &probe::CCOut, ctx: &str) {
    let (err, low, high, pbits, sel, seltmp) = out;
    assert!(
        *err < 1u64 << 31,
        "{ctx}: host err {err} out of proven band"
    );
    want.push(*err as u32);
    want.push((*err >> 32) as u32);
    for q in low.iter().chain(high.iter()) {
        want.push(*q as u32);
    }
    want.extend_from_slice(pbits);
    for q in sel.iter().chain(seltmp.iter()) {
        want.push(*q as u32);
    }
}

fn rand_ep(st: &mut u64, lim: u64) -> [i32; 4] {
    let mut e = [0i32; 4];
    for k in 0..4 {
        e[k] = (xs64(st) % lim) as i32;
    }
    e
}

fn e4w_errs(
    c: &probe::CCP,
    lo: &[[i32; 4]; 2],
    hi: &[[i32; 4]; 2],
    n: usize,
    px: &[[i32; 4]; 16],
) -> [u64; 4] {
    let mut out = [0u64; 4];
    for k in 0..4usize {
        let ec = probe::EvalCase {
            low: lo[k >> 1],
            high: hi[k & 1],
            pbits: [(k >> 1) as u32, (k & 1) as u32],
            nsw: c.nsw,
            tbl: c.tbl,
            comp_bits: c.comp_bits,
            weights: c.weights,
            has_alpha: c.has_alpha,
            has_pbits: c.has_pbits,
            share_pbit: c.share_pbit,
            perceptual: c.perceptual,
            init_err: u64::MAX,
            num_pixels: n,
        };
        out[k] = probe::eval_solution(&ec, px).0;
    }
    out
}

struct E4Case {
    c: probe::CCP,
    lo: [[i32; 4]; 2],
    hi: [[i32; 4]; 2],
    init: probe::CCInit,
    n: usize,
    px: [[i32; 4]; 16],
}

#[test]
fn wgpu_bc7_eval4way_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_eval4way_golden") else {
        return;
    };
    let sets = weight_sets();
    let buckets: Vec<(u32, usize, u32, bool)> = vec![
        (16, 2, 7, true),
        (16, 2, 7, false),
        (8, 1, 6, false),
        (4, 0, 5, true),
    ];
    let mut st = 0xabad_1dea_0bad_c0deu64;
    let mut cases: Vec<E4Case> = Vec::new();
    for (bi, &(nsw, tbl, comp_bits, has_alpha)) in buckets.iter().enumerate() {
        for perceptual in [false, true] {
            for (wi, &weights) in sets.iter().enumerate() {
                for ci in 0..60usize {
                    let c = probe::CCP {
                        nsw,
                        tbl,
                        comp_bits,
                        weights,
                        has_alpha,
                        has_pbits: true,
                        share_pbit: false,
                        perceptual,
                    };
                    let lim = 1u64 << comp_bits;
                    let mut lo = [rand_ep(&mut st, lim), rand_ep(&mut st, lim)];
                    let mut hi = [rand_ep(&mut st, lim), rand_ep(&mut st, lim)];
                    match ci % 6 {
                        0 => {
                            lo[1] = lo[0];
                            hi[1] = hi[0];
                        }
                        1 => {
                            hi[0] = lo[0];
                            hi[1] = lo[1];
                        }
                        2 => {
                            lo = [[0; 4]; 2];
                            hi = [[(lim - 1) as i32; 4]; 2];
                        }
                        _ => {}
                    }
                    let mut blk = [0u8; 64];
                    gen_block(&mut st, bi * 5 + wi * 3 + ci, &mut blk);
                    let px = px_from_block(&blk);
                    let n = if ci % 4 == 0 {
                        1 + (xs64(&mut st) % 16) as usize
                    } else {
                        16
                    };
                    let err = match ci % 9 {
                        0 => 0u64,
                        1 => 42,
                        _ => u64::MAX,
                    };
                    let init = probe::CCInit {
                        err,
                        low: rand_ep(&mut st, lim),
                        high: rand_ep(&mut st, lim),
                        pbits: [0, 0],
                    };
                    cases.push(E4Case {
                        c,
                        lo,
                        hi,
                        init,
                        n,
                        px,
                    });
                }
            }
        }
    }
    let mine_c = probe::CCP {
        nsw: 16,
        tbl: 2,
        comp_bits: 7,
        weights: [1, 1, 1, 1],
        has_alpha: true,
        has_pbits: true,
        share_pbit: false,
        perceptual: false,
    };
    let mut ties_found = 0usize;
    for attempt in 0..60_000usize {
        if ties_found >= 120 {
            break;
        }
        let lim = 128u64;
        let base = rand_ep(&mut st, lim);
        let mut lo = [base, base];
        let mut hi = [rand_ep(&mut st, lim); 2];
        if attempt % 3 == 0 {
            lo = [rand_ep(&mut st, lim), rand_ep(&mut st, lim)];
            hi = [rand_ep(&mut st, lim), rand_ep(&mut st, lim)];
        }
        let mut blk = [0u8; 64];
        gen_block(
            &mut st,
            if attempt % 2 == 0 { 3 } else { attempt },
            &mut blk,
        );
        let px = px_from_block(&blk);
        let errs = e4w_errs(&mine_c, &lo, &hi, 16, &px);
        let mn = *errs.iter().min().unwrap();
        let band = mn.saturating_add(mn.saturating_mul(1) / 8192);
        let in_band = errs.iter().filter(|&&e| e <= band).count();
        let last_in = (0..4).filter(|&k| errs[k] <= band).max().unwrap();
        let first_min = (0..4).find(|&k| errs[k] == mn).unwrap();
        if in_band >= 2 && last_in != first_min {
            ties_found += 1;
            cases.push(E4Case {
                c: mine_c,
                lo,
                hi,
                init: probe::CCInit {
                    err: u64::MAX,
                    low: [0; 4],
                    high: [0; 4],
                    pbits: [0, 0],
                },
                n: 16,
                px,
            });
        }
    }
    assert!(
        ties_found >= 30,
        "tie mining found only {ties_found} tiebreak-decisive cases"
    );
    eprintln!(
        "wgpu_bc7_eval4way_golden: {} cases incl. {ties_found} mined tiebreak-decisive ties",
        cases.len()
    );
    let mut input: Vec<u32> = Vec::with_capacity(cases.len() * 104);
    let mut want: Vec<u32> = Vec::with_capacity(cases.len() * 44);
    for e in &cases {
        push_ccp(&mut input, &e.c);
        input.push(e.n as u32);
        push_init(&mut input, &e.init);
        for arr in [e.lo[0], e.lo[1], e.hi[0], e.hi[1]] {
            for q in arr {
                input.push(q as u32);
            }
        }
        push_pixels(&mut input, &e.px);
        let out = probe::eval_4way(&e.c, e.lo, e.hi, &e.init, e.n, &e.px);
        push_ccout(&mut want, &out, "eval4way");
    }
    assert_eq!(input.len(), cases.len() * 104);
    let out = vec![0u8; cases.len() * 44 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_eval4way",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_eval4way");
}

fn findopt_mode_cfgs() -> Vec<(usize, u32, usize, u32, bool, bool, bool)> {
    vec![
        (0, 8, 1, 4, false, true, false),
        (1, 8, 1, 6, false, true, true),
        (2, 4, 0, 5, false, false, false),
        (3, 4, 0, 7, false, true, false),
        (4, 4, 0, 5, false, false, false),
        (4, 8, 1, 5, false, false, false),
        (5, 4, 0, 7, false, false, false),
        (6, 16, 2, 7, true, true, false),
        (6, 16, 2, 7, false, true, false),
        (7, 4, 0, 5, true, true, false),
    ]
}

#[derive(Clone)]
struct FOCase {
    mode: usize,
    c: probe::CCP,
    pbit_search: bool,
    xl: [f32; 4],
    xh: [f32; 4],
    init: probe::CCInit,
    n: usize,
    px: [[i32; 4]; 16],
}

#[test]
fn wgpu_bc7_findopt_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_findopt_golden") else {
        return;
    };
    let sets = weight_sets();
    let mut st = 0xf1d0_0713_5eed_2222u64;
    let mut cases: Vec<FOCase> = Vec::new();
    for (mi, &(mode, nsw, tbl, comp_bits, has_alpha, has_pbits, share)) in
        findopt_mode_cfgs().iter().enumerate()
    {
        for pbit_search in [false, true] {
            for perceptual in [false, true] {
                for (wi, &weights) in sets.iter().enumerate().take(3) {
                    for ci in 0..20usize {
                        let c = probe::CCP {
                            nsw,
                            tbl,
                            comp_bits,
                            weights,
                            has_alpha,
                            has_pbits,
                            share_pbit: share,
                            perceptual,
                        };
                        let mut blk = [0u8; 64];
                        gen_block(&mut st, mi * 7 + wi * 3 + ci, &mut blk);
                        let px = px_from_block(&blk);
                        let rf = |st: &mut u64| (xs64(st) % 1200) as f32 / 1024.0 - 0.05;
                        let mut xl = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
                        let mut xh = [rf(&mut st), rf(&mut st), rf(&mut st), rf(&mut st)];
                        match ci % 5 {
                            0 => {
                                for k in 0..4 {
                                    let mut lo = 255;
                                    let mut hi = 0;
                                    for row in &px {
                                        lo = lo.min(row[k]);
                                        hi = hi.max(row[k]);
                                    }
                                    xl[k] = lo as f32 / 255.0;
                                    xh[k] = hi as f32 / 255.0;
                                }
                            }
                            1 => {
                                xh = xl;
                            }
                            2 => {
                                std::mem::swap(&mut xl, &mut xh);
                            }
                            3 => {
                                xl = [-0.02, 0.0, 1.0, 1.04];
                                xh = [1.03, 1.0, 0.0, -0.01];
                            }
                            _ => {}
                        }
                        let n = if ci % 3 == 0 {
                            1 + (xs64(&mut st) % 16) as usize
                        } else {
                            16
                        };
                        cases.push(FOCase {
                            mode,
                            c,
                            pbit_search,
                            xl,
                            xh,
                            init: probe::CCInit {
                                err: u64::MAX,
                                low: [0; 4],
                                high: [0; 4],
                                pbits: [0, 0],
                            },
                            n,
                            px,
                        });
                    }
                }
            }
        }
    }
    let mut derived: Vec<FOCase> = Vec::new();
    for e in cases.iter().step_by(16) {
        let (_, out) =
            probe::find_optimal(e.mode, e.xl, e.xh, &e.c, e.pbit_search, &e.init, e.n, &e.px);
        let mut d = e.clone();
        d.init = probe::CCInit {
            err: out.0,
            low: out.1,
            high: out.2,
            pbits: out.3,
        };
        derived.push(d);
    }
    let n_derived = derived.len();
    cases.extend(derived);
    eprintln!(
        "wgpu_bc7_findopt_golden: {} cases incl. {n_derived} derived skip-path cases",
        cases.len()
    );
    let mut input: Vec<u32> = Vec::with_capacity(cases.len() * 98);
    let mut want: Vec<u32> = Vec::with_capacity(cases.len() * 46);
    for e in &cases {
        input.push(e.mode as u32);
        push_ccp(&mut input, &e.c);
        input.push(e.pbit_search as u32);
        input.push(e.n as u32);
        push_init(&mut input, &e.init);
        for q in e.xl.iter().chain(e.xh.iter()) {
            input.push(q.to_bits());
        }
        push_pixels(&mut input, &e.px);
        let (ret, out) =
            probe::find_optimal(e.mode, e.xl, e.xh, &e.c, e.pbit_search, &e.init, e.n, &e.px);
        want.push(ret as u32);
        want.push((ret >> 32) as u32);
        push_ccout(&mut want, &out, "findopt");
    }
    assert_eq!(input.len(), cases.len() * 98);
    let out = vec![0u8; cases.len() * 46 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_findopt",
        cases.len() as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_findopt");
}

#[test]
fn wgpu_bc7_ccc_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_ccc_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let sets = weight_sets();
    let mode_cfgs: Vec<(usize, u32, usize, u32, bool, bool, bool)> = vec![
        (0, 8, 1, 4, false, true, false),
        (1, 8, 1, 6, false, true, true),
        (2, 4, 0, 5, false, false, false),
        (3, 4, 0, 7, false, true, false),
        (4, 8, 1, 5, false, false, false),
        (5, 4, 0, 7, false, false, false),
        (6, 16, 2, 7, true, true, false),
        (7, 4, 0, 5, true, true, false),
    ];
    let cp_buckets: Vec<(bool, u32, u32, u32)> = vec![
        (true, 1, 0, 7),
        (false, 1, 1, 7),
        (true, 1, 1, 7),
        (true, 1, 2, 7),
        (true, 2, 4, 7),
        (false, 0, 2, 5),
        (true, 0, 4, 1),
        (false, 3, 3, 2),
    ];
    let mut st = 0xcccc_0ddb_a11a_5eedu64;
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u32> = Vec::new();
    let mut total = 0usize;
    for (mi, &(mode, nsw, tbl, comp_bits, has_alpha, has_pbits, share)) in
        mode_cfgs.iter().enumerate()
    {
        for (cpi, &(pbit_search, refinement_passes, uber_level, uber1_mask)) in
            cp_buckets.iter().enumerate()
        {
            for perceptual in [false, true] {
                for refinement in [false, true] {
                    for ci in 0..16usize {
                        let weights = sets[(mi + cpi + ci) % sets.len()];
                        let c = probe::CCP {
                            nsw,
                            tbl,
                            comp_bits,
                            weights,
                            has_alpha,
                            has_pbits,
                            share_pbit: share,
                            perceptual,
                        };
                        let mut blk = [0u8; 64];
                        gen_block(&mut st, ci, &mut blk);
                        let mut px = px_from_block(&blk);
                        match ci {
                            9 => {
                                for (i, row) in px.iter_mut().enumerate() {
                                    *row = if i % 2 == 0 {
                                        [0, 0, 0, 255]
                                    } else {
                                        [255, 255, 255, 255]
                                    };
                                }
                            }
                            10 => {
                                px = [[137, 42, 250, 9]; 16];
                            }
                            11 => {
                                let base = px[0];
                                for (i, row) in px.iter_mut().enumerate() {
                                    *row = base;
                                    row[3] = (i * 17) as i32;
                                }
                            }
                            _ => {}
                        }
                        let n = if ci % 3 == 0 {
                            1 + (xs64(&mut st) % 16) as usize
                        } else {
                            16
                        };
                        input.push(mode as u32);
                        push_ccp(&mut input, &c);
                        input.push(pbit_search as u32);
                        input.push(refinement_passes);
                        input.push(uber_level);
                        input.push(uber1_mask);
                        input.push(refinement as u32);
                        input.push(n as u32);
                        push_pixels(&mut input, &px);
                        let out = probe::ccc(
                            mode,
                            &c,
                            pbit_search,
                            refinement_passes,
                            uber_level,
                            uber1_mask,
                            refinement,
                            n,
                            &px,
                            &t,
                        );
                        push_ccout(&mut want, &out, "ccc");
                        total += 1;
                    }
                }
            }
        }
    }
    eprintln!("wgpu_bc7_ccc_golden: {total} cases");
    assert_eq!(input.len(), total * 82);
    let out = vec![0u8; total * 44 * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_ccc",
        total as u32,
        0,
        &[(2, &opt_bytes), (4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &words_bytes(&want), "bc7_test_ccc");
}

#[test]
fn bc7_evalsol_n16_consistency() {
    let mut st = 0x0000_600d_cafe_f00du64;
    for ci in 0..200usize {
        let mut low = [0i32; 4];
        let mut high = [0i32; 4];
        for k in 0..4 {
            low[k] = (xs64(&mut st) % 128) as i32;
            high[k] = (xs64(&mut st) % 128) as i32;
        }
        let pbits = [(xs64(&mut st) & 1) as u32, (xs64(&mut st) & 1) as u32];
        let mut blk = [0u8; 64];
        gen_block(&mut st, ci, &mut blk);
        let mut pixels = [[0i32; 4]; 16];
        for i in 0..16 {
            for k in 0..4 {
                pixels[i][k] = blk[i * 4 + k] as i32;
            }
        }
        let case = probe::EvalCase {
            low,
            high,
            pbits,
            nsw: 16,
            tbl: 2,
            comp_bits: 7,
            weights: [1, 1, 1, 1],
            has_alpha: false,
            has_pbits: true,
            share_pbit: false,
            perceptual: false,
            init_err: u64::MAX,
            num_pixels: 16,
        };
        let (ret, _, _, _, _, sel, seltmp) = probe::eval_solution(&case, &pixels);
        let mut qmin = [0i32; 4];
        let mut qmax = [0i32; 4];
        for k in 0..4 {
            qmin[k] = (low[k] << 1) | pbits[0] as i32;
            qmax[k] = (high[k] << 1) | pbits[1] as i32;
        }
        let amin = probe::scale_color(qmin, 7, true);
        let amax = probe::scale_color(qmax, 7, true);
        let mut wc = [[0f32; 4]; 16];
        for j in 0..4 {
            wc[0][j] = amin[j] as f32;
            wc[15][j] = amax[j] as f32;
        }
        for i in 1..15 {
            let wf = probe::weights4()[i] as f32;
            for j in 0..3 {
                wc[i][j] =
                    ((wc[0][j] * (64.0 - wf) + wc[15][j] * wf + 32.0) * (1.0 / 64.0)).floor();
            }
        }
        let lr = amin[0] as f32;
        let lg = amin[1] as f32;
        let lb = amin[2] as f32;
        let dr = amax[0] as f32 - lr;
        let dg = amax[1] as f32 - lg;
        let db = amax[2] as f32 - lb;
        let f = 16.0 / (dr * dr + dg * dg + db * db);
        let (total, sel2) = probe::eval_n16_rgb(
            16,
            &pixels,
            &wc,
            [1.0, 1.0, 1.0],
            [dr, dg, db],
            [lr * -dr, lg * -dg, lb * -db],
            f,
            16,
        );
        assert_eq!(ret, total as i64 as u64, "n16 total mismatch case {ci}");
        assert_eq!(seltmp, sel2, "n16 selectors_temp mismatch case {ci}");
        assert_eq!(sel, sel2, "n16 selectors mismatch case {ci}");
    }
    eprintln!(
        "bc7_evalsol_n16_consistency: evaluate_solution n16 rgb path == eval_solution_n16_rgb_scalar over 200 seeded cases"
    );
}
