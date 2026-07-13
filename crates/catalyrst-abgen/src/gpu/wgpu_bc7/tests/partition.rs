use super::*;

fn push_est_case(
    input: &mut Vec<u32>,
    op: u32,
    mode: u32,
    w: [u32; 4],
    perc: bool,
    num: u32,
    max_sol: i32,
    idxs: &[i32; 16],
    px: &[[i32; 4]; 16],
) {
    input.push(op);
    input.push(mode);
    input.extend_from_slice(&w);
    input.push(perc as u32);
    input.push(num);
    input.push(max_sol as u32);
    for &q in idxs {
        input.push(q as u32);
    }
    push_pixels(input, px);
}

fn push_u64_padded(want: &mut Vec<u32>, v: u64, pad_to: usize, ctx: &str) {
    assert!(v < 1u64 << 31, "{ctx}: host err {v} out of proven band");
    want.push(v as u32);
    want.push((v >> 32) as u32);
    for _ in 0..pad_to - 2 {
        want.push(0);
    }
}

fn push_sol_list(want: &mut Vec<u32>, l: &probe::SolList) {
    let (idx, errs, len) = l;
    for i in 0..8 {
        want.push(idx[i]);
        want.push(errs[i] as u32);
        want.push((errs[i] >> 32) as u32);
    }
    want.push(*len as u32);
}

fn est_idx_sets() -> Vec<([i32; 16], usize)> {
    let mut sets = Vec::new();
    let mut ident = [0i32; 16];
    for (i, v) in ident.iter_mut().enumerate() {
        *v = i as i32;
    }
    sets.push((ident, 16));
    sets.push((ident, 1));
    sets.push((ident, 7));
    let mut rev = [0i32; 16];
    for (i, v) in rev.iter_mut().enumerate() {
        *v = 15 - i as i32;
    }
    sets.push((rev, 16));
    for p in [0usize, 13, 34, 63] {
        let (idx, tot) = probe::subset_idx2(p);
        for s in 0..2 {
            sets.push((idx[s], tot[s] as usize));
        }
    }
    for p in [0usize, 21, 63] {
        let (idx, tot) = probe::subset_idx3(p);
        for s in 0..3 {
            sets.push((idx[s], tot[s] as usize));
        }
    }
    sets
}

fn est_blocks(st: &mut u64) -> Vec<[[i32; 4]; 16]> {
    let mut v = Vec::new();
    for strat in 0..9 {
        let mut blk = [0u8; 64];
        gen_block(st, strat, &mut blk);
        v.push(px_from_block(&blk));
    }
    v.push([[97, 4, 210, 33]; 16]);
    let mut alpha_only = [[120, 50, 200, 0]; 16];
    for (i, row) in alpha_only.iter_mut().enumerate() {
        row[3] = (i * 16) as i32;
    }
    v.push(alpha_only);
    let mut quads = [[0i32; 4]; 16];
    for (i, row) in quads.iter_mut().enumerate() {
        let q = ((i / 8) * 2 + (i % 4) / 2) as i32;
        *row = [q * 80, 255 - q * 60, q * q * 20, 255];
    }
    v.push(quads);
    v
}

fn est_variants() -> Vec<Params> {
    let mut v = params4().to_vec();
    let mut p = Params::slow(false);
    p.op_max_mode13 = 4;
    p.op_max_mode0 = 8;
    p.op_max_mode2 = 2;
    p.al_max_mode7 = 6;
    p.mode67_weight_mul = [2, 3, 1, 5];
    p.weights = [37, 5, 11, 3];
    v.push(p);
    let mut p = Params::slow(true);
    p.op_max_mode13 = 2;
    p.op_max_mode0 = 64;
    p.op_max_mode2 = 16;
    p.al_max_mode7 = 0;
    p.max_partitions_mode = [1, 16, 33, 64, 0, 0, 0, 3];
    v.push(p);
    let mut p = Params::basic(false);
    p.op_max_mode13 = 3;
    p.al_max_mode7 = 1;
    p.max_partitions_mode = [64, 35, 64, 64, 0, 0, 0, 64];
    v.push(p);
    v
}

#[test]
fn wgpu_bc7_est_partition_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_est_partition_golden") else {
        return;
    };
    let variants = est_variants();
    let sets = est_idx_sets();
    let wsets = weight_sets();
    let mut st = 0xe57a_7e5e_ed06_0001u64;
    let blocks_px = est_blocks(&mut st);
    for (vi, cp) in variants.iter().enumerate() {
        let mut input: Vec<u32> = Vec::new();
        let mut want: Vec<u32> = Vec::new();
        let mut total = 0usize;
        for mode in [0usize, 1, 2, 3, 6, 7] {
            push_est_case(
                &mut input,
                4,
                mode as u32,
                [0; 4],
                false,
                0,
                0,
                &[0; 16],
                &blocks_px[0],
            );
            let (w, nsw, tbl, perc) = probe::est_params(mode, cp);
            want.extend_from_slice(&w);
            want.push(nsw);
            want.push(tbl);
            want.push(perc as u32);
            want.extend([0u32; 18]);
            total += 1;
        }
        for mode in [0usize, 1, 2, 3, 7] {
            for px in &blocks_px {
                push_est_case(
                    &mut input,
                    2,
                    mode as u32,
                    [0; 4],
                    false,
                    0,
                    0,
                    &[0; 16],
                    px,
                );
                want.push(probe::estimate_partition(mode, cp, px));
                want.extend([0u32; 24]);
                total += 1;
            }
        }
        for mode in [0usize, 1, 2, 3, 7] {
            for &ms in &[0i32, 1, 2, 3, 4, 6, 8, 16, 64] {
                if ms == 0 && (mode == 0 || mode == 2) && cp.max_partitions_mode[mode] > 1 {
                    continue;
                }
                for px in blocks_px.iter().step_by(3) {
                    push_est_case(
                        &mut input,
                        3,
                        mode as u32,
                        [0; 4],
                        false,
                        0,
                        ms,
                        &[0; 16],
                        px,
                    );
                    push_sol_list(&mut want, &probe::estimate_partition_list(mode, cp, ms, px));
                    total += 1;
                }
            }
        }
        if vi == 0 {
            for mode in [0usize, 1, 2, 3] {
                for (si, (idxs, num)) in sets.iter().enumerate() {
                    for (wi, &w) in wsets.iter().enumerate() {
                        let px = &blocks_px[(si + wi + mode) % blocks_px.len()];
                        push_est_case(
                            &mut input,
                            0,
                            mode as u32,
                            w,
                            false,
                            *num as u32,
                            0,
                            idxs,
                            px,
                        );
                        let e = probe::est_idx(mode, w, idxs, *num, px);
                        push_u64_padded(&mut want, e, 25, "est_idx");
                        total += 1;
                    }
                }
            }
            for perc in [false, true] {
                for (si, (idxs, num)) in sets.iter().enumerate() {
                    for (wi, &w) in wsets.iter().enumerate() {
                        let px = &blocks_px[(si + 2 * wi + 1) % blocks_px.len()];
                        push_est_case(&mut input, 1, 7, w, perc, *num as u32, 0, idxs, px);
                        let e = probe::est_mode7_idx(w, perc, idxs, *num, px);
                        push_u64_padded(&mut want, e, 25, "est_mode7_idx");
                        total += 1;
                    }
                }
            }
        }
        eprintln!("wgpu_bc7_est_partition_golden: variant {vi}: {total} cases");
        assert_eq!(input.len(), total * 89);
        assert_eq!(want.len(), total * 25);
        let pbytes = words_bytes(&params_words(cp));
        let out = vec![0u8; total * 25 * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_est_partition",
            total as u32,
            0,
            &[(1, &pbytes), (4, &words_bytes(&input)), (3, &out)],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&want),
            &format!("bc7_test_est_partition variant {vi}"),
        );
    }
}

fn plans_variants() -> Vec<Params> {
    let mut v = params4().to_vec();
    let mut p = Params::slow(false);
    p.op_max_mode13 = 4;
    p.op_max_mode0 = 2;
    p.op_max_mode2 = 8;
    p.al_max_mode7 = 6;
    v.push(p);
    let mut p = Params::slow(true);
    p.use_mode = [true, false, true, false, true, true, true];
    p.use_mode7 = false;
    p.op_max_mode0 = 3;
    v.push(p);
    let mut p = Params::basic(false);
    p.use_mode = [false, true, false, true, true, true, true];
    p.op_max_mode13 = 2;
    p.max_partitions_mode = [16, 1, 64, 64, 0, 0, 0, 64];
    v.push(p);
    let mut p = Params::slow(false);
    p.al_max_mode7 = 1;
    p.op_max_mode2 = 6;
    p.mode67_weight_mul = [3, 1, 2, 4];
    v.push(p);
    v
}

#[test]
fn wgpu_bc7_plans_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_plans_golden") else {
        return;
    };
    let mut st = 0x91a5_0000_0bad_5eedu64;
    let mut blocks_px = est_blocks(&mut st);
    for strat in 0..12 {
        let mut blk = [0u8; 64];
        gen_block(&mut st, strat, &mut blk);
        blocks_px.push(px_from_block(&blk));
    }
    for (vi, cp) in plans_variants().iter().enumerate() {
        let mut input: Vec<u32> = Vec::new();
        let mut want: Vec<u32> = Vec::new();
        for px in &blocks_px {
            push_pixels(&mut input, px);
            let plan = probe::build_plans(cp, px);
            want.push(plan.part0);
            want.push(plan.part13);
            want.push(plan.part2);
            want.push(plan.use_list13 as u32);
            want.push(plan.use_list2 as u32);
            want.push(plan.use_list0 as u32);
            push_sol_list(&mut want, &plan.list13);
            push_sol_list(&mut want, &plan.list2);
            push_sol_list(&mut want, &plan.list0);
            push_sol_list(&mut want, &plan.list7);
        }
        let total = blocks_px.len();
        assert_eq!(input.len(), total * 64);
        assert_eq!(want.len(), total * 106);
        let pbytes = words_bytes(&params_words(cp));
        let out = vec![0u8; total * 106 * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_plans",
            total as u32,
            0,
            &[(1, &pbytes), (4, &words_bytes(&input)), (3, &out)],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&want),
            &format!("bc7_test_plans variant {vi}"),
        );
    }
}

fn tree_walk_leaves(
    idx: usize,
    lo: &mut [i32; 10],
    hi: &mut [i32; 10],
    out: &mut Vec<([i32; 10], (u8, u16))>,
) {
    let n = &TREE[idx];
    if n.feature < 0 {
        let mut feat = [0i32; 10];
        for k in 0..10 {
            feat[k] = 0i32.clamp(lo[k], hi[k]);
        }
        out.push((feat, (n.left as u8, n.right as u16)));
        return;
    }
    let f = n.feature as usize;
    let t = n.threshold;
    let saved_hi = hi[f];
    if t < hi[f] {
        hi[f] = t;
    }
    if lo[f] <= hi[f] {
        tree_walk_leaves(n.left as usize, lo, hi, out);
    }
    hi[f] = saved_hi;
    let saved_lo = lo[f];
    if t + 1 > lo[f] {
        lo[f] = t + 1;
    }
    if lo[f] <= hi[f] {
        tree_walk_leaves(n.right as usize, lo, hi, out);
    }
    lo[f] = saved_lo;
}

fn tree_leaf_feats() -> Vec<([i32; 10], (u8, u16))> {
    let mut out = Vec::new();
    let mut lo = [-1_000_000i32; 10];
    let mut hi = [1_000_000i32; 10];
    tree_walk_leaves(0, &mut lo, &mut hi, &mut out);
    out
}

fn modetree_blocks(st: &mut u64) -> Vec<[[i32; 4]; 16]> {
    let cp = Params::slow(false);
    let mut blocks: Vec<[[i32; 4]; 16]> = Vec::new();
    for strat in 0..27 {
        let mut blk = [0u8; 64];
        gen_block(st, strat, &mut blk);
        let mut px = px_from_block(&blk);
        if strat % 3 == 1 {
            for (i, row) in px.iter_mut().enumerate() {
                row[3] = ((i * 15) % 256) as i32;
            }
        }
        blocks.push(px);
    }
    blocks.push([[10, 20, 30, 100]; 16]);
    blocks.push([[200, 180, 190, 255]; 16]);
    blocks.push([[255, 255, 255, 255]; 16]);
    blocks.push([[0, 0, 0, 0]; 16]);
    blocks.push([[255, 170, 120, 60]; 16]);
    for _ in 0..8 {
        let mut px = [[0i32; 4]; 16];
        for row in px.iter_mut() {
            for k in 0..3 {
                row[k] = (xs64(st) % 256) as i32;
            }
            row[3] = 1 + (xs64(st) % 254) as i32;
        }
        let alpha = px[0][3];
        for row in px.iter_mut() {
            row[3] = alpha;
        }
        blocks.push(px);
    }
    for _ in 0..4 {
        let base = [
            (xs64(st) % 200) as i32,
            (xs64(st) % 200) as i32,
            (xs64(st) % 200) as i32,
        ];
        let mut px = [[base[0], base[1], base[2], 77]; 16];
        px[3][0] += 2;
        blocks.push(px);
    }
    let mut counts = [0usize; 3];
    for px in &blocks {
        counts[hint_code(&cp, px).0 as usize] += 1;
    }
    assert!(
        counts[1] >= 1 && counts[2] >= 1,
        "crafted hint coverage failed: counts none/mode5/mode6 = {counts:?}"
    );
    eprintln!(
        "modetree_blocks: {} blocks, hint counts none/mode5/mode6 = {counts:?}",
        blocks.len()
    );
    blocks
}

#[test]
fn wgpu_bc7_modetree_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_modetree_golden") else {
        return;
    };
    let leaves = tree_leaf_feats();
    let n_leaf_nodes = TREE.iter().filter(|n| n.feature < 0).count();
    eprintln!(
        "wgpu_bc7_modetree_golden: {} feasible leaf paths ({} leaf nodes in TREE)",
        leaves.len(),
        n_leaf_nodes
    );
    assert!(leaves.len() >= n_leaf_nodes);
    for (feat, (mode, conf)) in &leaves {
        assert_eq!(
            mode_tree::predict(feat),
            (*mode, *conf),
            "host predict disagrees with DFS leaf for {feat:?}"
        );
    }
    let mut st = 0x0dec_15e0_7ee5_0001u64;
    let blocks = modetree_blocks(&mut st);
    let variants = [Params::slow(false), Params::basic(true)];
    for (vi, cp) in variants.iter().enumerate() {
        let mut input: Vec<u32> = Vec::new();
        let mut want: Vec<u32> = Vec::new();
        let mut total = 0usize;
        for (feat, (mode, conf)) in &leaves {
            input.push(1);
            for &f in feat {
                input.push(f as u32);
            }
            input.extend([0u32; 54]);
            want.push(*mode as u32);
            want.push(*conf as u32);
            want.extend([0u32; 53]);
            total += 1;
        }
        for px in &blocks {
            input.push(0);
            push_pixels(&mut input, px);
            let feat = mode_tree::block_features(px);
            let (mode, conf) = mode_tree::predict(&feat);
            let (code, gated) = hint_code(cp, px);
            for f in feat {
                want.push(f as u32);
            }
            want.push(mode as u32);
            want.push(conf as u32);
            want.push(code);
            want.extend_from_slice(&params_words(&gated));
            total += 1;
        }
        assert_eq!(input.len(), total * 65);
        assert_eq!(want.len(), total * 55);
        let pbytes = words_bytes(&params_words(cp));
        let out = vec![0u8; total * 55 * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_modetree",
            total as u32,
            0,
            &[(1, &pbytes), (4, &words_bytes(&input)), (3, &out)],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&want),
            &format!("bc7_test_modetree variant {vi}"),
        );
    }
}

#[test]
fn mode4_scale_bits_pin() {
    assert_eq!((63.0f32 / 255.0f32).to_bits(), 0x3e7cfcfd);
}

fn zero_optin() -> probe::OptIn {
    probe::OptIn {
        mode: 0,
        partition: 0,
        selectors: [0; 16],
        alpha_selectors: [0; 16],
        low: [[0; 4]; 3],
        high: [[0; 4]; 3],
        pbits: [[0; 2]; 3],
        rotation: 0,
        index_selector: 0,
    }
}

fn push_optin(input: &mut Vec<u32>, op: u32, o: &probe::OptIn) {
    input.push(op);
    input.push(o.mode);
    input.push(o.partition);
    input.push(o.rotation);
    input.push(o.index_selector);
    for s in o.selectors {
        input.push(s as u32);
    }
    for s in o.alpha_selectors {
        input.push(s as u32);
    }
    for k in 0..3 {
        for q in o.low[k] {
            input.push(q as u32);
        }
    }
    for k in 0..3 {
        for q in o.high[k] {
            input.push(q as u32);
        }
    }
    for k in 0..3 {
        input.extend_from_slice(&o.pbits[k]);
    }
}

fn anchor_pos(subsets: usize, k: usize, partition: u32) -> usize {
    if k == 0 {
        0
    } else if subsets == 3 && k == 1 {
        probe::anchor_3rd_1()[partition as usize] as usize
    } else if subsets == 3 && k == 2 {
        probe::anchor_3rd_2()[partition as usize] as usize
    } else {
        probe::anchor_2nd()[partition as usize] as usize
    }
}

fn rand_optin(st: &mut u64, mode: usize, partition: u32, anchor_mask: u32) -> probe::OptIn {
    let subsets = probe::num_subsets()[mode];
    let index_selector = if mode == 4 { (xs64(st) & 1) as u32 } else { 0 };
    let rotation = if mode == 4 || mode == 5 {
        (xs64(st) & 3) as u32
    } else {
        0
    };
    let cbits = probe::color_index_bitcount()[mode] + index_selector;
    let abits = if mode == 4 || mode == 5 {
        (probe::alpha_index_bitcount()[mode] as u32) - index_selector
    } else {
        0
    };
    let cprec = probe::color_precision_table()[mode];
    let aprec = probe::alpha_precision_table()[mode];
    let mut o = zero_optin();
    o.mode = mode as u32;
    o.partition = partition;
    o.rotation = rotation;
    o.index_selector = index_selector;
    for i in 0..16 {
        o.selectors[i] = (xs64(st) % (1u64 << cbits)) as i32;
        if abits > 0 {
            o.alpha_selectors[i] = (xs64(st) % (1u64 << abits)) as i32;
        }
    }
    for k in 0..subsets {
        for c in 0..4 {
            let prec = if c == 3 {
                if mode >= 4 {
                    aprec
                } else {
                    0
                }
            } else {
                cprec
            };
            if prec > 0 {
                o.low[k][c] = (xs64(st) % (1u64 << prec)) as i32;
                o.high[k][c] = (xs64(st) % (1u64 << prec)) as i32;
            }
        }
        o.pbits[k] = [(xs64(st) & 1) as u32, (xs64(st) & 1) as u32];
    }
    for k in 0..subsets {
        let anchor = anchor_pos(subsets, k, partition);
        if anchor_mask & (1 << k) != 0 {
            o.selectors[anchor] |= 1 << (cbits - 1);
        } else {
            o.selectors[anchor] &= !(1 << (cbits - 1));
        }
        if abits > 0 {
            if anchor_mask & (1 << (k + 3)) != 0 {
                o.alpha_selectors[anchor] |= 1 << (abits - 1);
            } else {
                o.alpha_selectors[anchor] &= !(1 << (abits - 1));
            }
        }
    }
    o
}

#[test]
fn wgpu_bc7_encode_bits_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_encode_bits_golden") else {
        return;
    };
    let mut st = 0xb17b_ac4e_d51d_5eedu64;
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u8> = Vec::new();
    let mut total = 0usize;
    for mode in 0..8usize {
        let parts = if probe::num_subsets()[mode] == 1 {
            1u32
        } else {
            1u32 << probe::partition_bits()[mode]
        };
        let rolls = if parts == 1 { 128 } else { 16 };
        for partition in 0..parts {
            for roll in 0..rolls {
                let o = rand_optin(&mut st, mode, partition, (roll as u32) & 63);
                push_optin(&mut input, 0, &o);
                want.extend_from_slice(&probe::encode_block_bits(&o));
                total += 1;
            }
        }
    }
    for roll in 0..512usize {
        let o = rand_optin(&mut st, 6, 0, (roll as u32) & 63);
        push_optin(&mut input, 1, &o);
        want.extend_from_slice(&probe::encode_block_mode6(&o));
        total += 1;
    }
    eprintln!("wgpu_bc7_encode_bits_golden: {total} cases");
    assert!(total >= 4096);
    assert_eq!(input.len(), total * 67);
    let out = vec![0u8; total * 16];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_encode_bits",
        total as u32,
        0,
        &[(4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &want, "bc7_test_encode_bits");
}

#[test]
fn wgpu_bc7_solid_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_solid_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let axis = [0usize, 1, 2, 3, 31, 32, 63, 64, 127, 128, 191, 254, 255];
    let cas = [0i32, 1, 127, 254, 255];
    let mut st = 0x5011_dca5_e5ee_d001u64;
    let mut input: Vec<u32> = Vec::new();
    let mut want: Vec<u8> = Vec::new();
    let mut total = 0usize;
    let push_case =
        |input: &mut Vec<u32>, want: &mut Vec<u8>, cr: usize, cg: usize, cb: usize, ca: i32| {
            input.extend_from_slice(&[cr as u32, cg as u32, cb as u32, ca as u32]);
            want.extend_from_slice(&probe::block_solid(cr, cg, cb, ca, &t));
        };
    for (i, &cr) in axis.iter().enumerate() {
        for (j, &cg) in axis.iter().enumerate() {
            for (k, &cb) in axis.iter().enumerate() {
                let ca = cas[(i + j + k) % cas.len()];
                push_case(&mut input, &mut want, cr, cg, cb, ca);
                total += 1;
            }
        }
    }
    for _ in 0..1024 {
        let cr = (xs64(&mut st) % 256) as usize;
        let cg = (xs64(&mut st) % 256) as usize;
        let cb = (xs64(&mut st) % 256) as usize;
        let ca = (xs64(&mut st) % 256) as i32;
        push_case(&mut input, &mut want, cr, cg, cb, ca);
        total += 1;
    }
    eprintln!("wgpu_bc7_solid_golden: {total} cases");
    assert_eq!(input.len(), total * 4);
    let out = vec![0u8; total * 16];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_solid",
        total as u32,
        0,
        &[(2, &opt_bytes), (4, &words_bytes(&input)), (3, &out)],
        3,
    );
    assert_bytes_eq(&got, &want, "bc7_test_solid");
}
