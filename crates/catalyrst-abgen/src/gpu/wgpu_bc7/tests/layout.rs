use super::*;

const CONST_DUMP_WORDS: usize = 9490;

#[test]
fn params_layout() {
    assert_eq!(std::mem::size_of::<Params>(), 124);
    assert_eq!(std::mem::align_of::<Params>(), 4);
    assert_eq!(offset_of!(Params, max_partitions_mode), 0);
    assert_eq!(offset_of!(Params, weights), 32);
    assert_eq!(offset_of!(Params, uber_level), 48);
    assert_eq!(offset_of!(Params, refinement_passes), 52);
    assert_eq!(offset_of!(Params, mode4_rotation_mask), 56);
    assert_eq!(offset_of!(Params, mode4_index_mask), 60);
    assert_eq!(offset_of!(Params, mode5_rotation_mask), 64);
    assert_eq!(offset_of!(Params, uber1_mask), 68);
    assert_eq!(offset_of!(Params, perceptual), 72);
    assert_eq!(offset_of!(Params, pbit_search), 73);
    assert_eq!(offset_of!(Params, mode6_only), 74);
    assert_eq!(offset_of!(Params, op_max_mode13), 76);
    assert_eq!(offset_of!(Params, op_max_mode0), 80);
    assert_eq!(offset_of!(Params, op_max_mode2), 84);
    assert_eq!(offset_of!(Params, use_mode), 88);
    assert_eq!(offset_of!(Params, al_max_mode7), 96);
    assert_eq!(offset_of!(Params, mode67_weight_mul), 100);
    assert_eq!(offset_of!(Params, use_mode4), 116);
    assert_eq!(offset_of!(Params, use_mode5), 117);
    assert_eq!(offset_of!(Params, use_mode6), 118);
    assert_eq!(offset_of!(Params, use_mode7), 119);
    assert_eq!(offset_of!(Params, use_mode4_rotation), 120);
    assert_eq!(offset_of!(Params, use_mode5_rotation), 121);
}

#[test]
fn endpoint_err_layout_and_word() {
    assert_eq!(std::mem::size_of::<EndpointErr>(), 4);
    assert_eq!(offset_of!(EndpointErr, error), 0);
    assert_eq!(offset_of!(EndpointErr, lo), 2);
    assert_eq!(offset_of!(EndpointErr, hi), 3);
    let e = EndpointErr {
        error: 0x1234,
        lo: 0xab,
        hi: 0xcd,
    };
    assert_eq!(endpoint_err_word(e), 0xcdab1234);
    let raw = u32::from_le_bytes(unsafe { std::mem::transmute::<EndpointErr, [u8; 4]>(e) });
    assert_eq!(raw, endpoint_err_word(e));
}

#[test]
fn opt_tables_layout() {
    assert_eq!(std::mem::size_of::<OptTables>(), 17408);
    assert_eq!(offset_of!(OptTables, mode0), 0);
    assert_eq!(offset_of!(OptTables, mode1), 4096);
    assert_eq!(offset_of!(OptTables, mode6), 6144);
    assert_eq!(offset_of!(OptTables, mode7), 10240);
    assert_eq!(offset_of!(OptTables, mode5), 14336);
    assert_eq!(offset_of!(OptTables, mode4_3), 15360);
    assert_eq!(offset_of!(OptTables, mode4_2), 16384);
}

#[test]
fn opt_tables_words_pack() {
    let t = build_opt_tables();
    let w = opt_tables_words(&t);
    assert_eq!(w.len(), OPT_TABLES_WORDS);
    for c in 0..256 {
        for hp in 0..2 {
            for lp in 0..2 {
                assert_eq!(
                    w[c * 4 + hp * 2 + lp],
                    endpoint_err_word(t.mode0[c][hp][lp])
                );
                assert_eq!(
                    w[1536 + c * 4 + hp * 2 + lp],
                    endpoint_err_word(t.mode6[c][hp][lp])
                );
                assert_eq!(
                    w[2560 + c * 4 + hp * 2 + lp],
                    endpoint_err_word(t.mode7[c][hp][lp])
                );
            }
            assert_eq!(w[1024 + c * 2 + hp], endpoint_err_word(t.mode1[c][hp]));
        }
        assert_eq!(w[3584 + c], t.mode5[c]);
        assert_eq!(w[3840 + c], t.mode4_3[c]);
        assert_eq!(w[4096 + c], t.mode4_2[c]);
    }
}

#[test]
fn params_words_shape() {
    for p in params4() {
        let w = params_words(&p);
        assert_eq!(w.len(), PARAMS_WORDS);
        assert_eq!(&w[0..8], &p.max_partitions_mode);
        assert_eq!(&w[8..12], &p.weights);
        assert_eq!(w[18], p.perceptual as u32);
        for i in 0..7 {
            assert_eq!(w[24 + i], p.use_mode[i] as u32);
        }
        assert_eq!(w[31], p.al_max_mode7);
        assert_eq!(&w[32..36], &p.mode67_weight_mul);
        assert_eq!(w[41], p.use_mode5_rotation as u32);
    }
    let slow = params_words(&Params::slow(true));
    let basic = params_words(&Params::basic(true));
    assert_eq!(slow[12], 0);
    assert_eq!(basic[12], 1);
    assert_eq!(slow[19], 1);
    assert_eq!(basic[19], 0);
    assert_eq!(slow[31], 2);
    assert_eq!(basic[31], 1);
}

fn push_i32s(v: &mut Vec<u32>, s: &[i32]) {
    for &x in s {
        v.push(x as u32);
    }
}

fn expected_const_words() -> Vec<u32> {
    let mut v = Vec::with_capacity(CONST_DUMP_WORDS);
    v.extend_from_slice(probe::weights2());
    v.extend_from_slice(probe::weights3());
    v.extend_from_slice(probe::weights4());
    for row in probe::weights2x() {
        for f in row {
            v.push(f.to_bits());
        }
    }
    for row in probe::weights3x() {
        for f in row {
            v.push(f.to_bits());
        }
    }
    for row in probe::weights4x() {
        for f in row {
            v.push(f.to_bits());
        }
    }
    for &b in probe::partition2().iter() {
        v.push(b as u32);
    }
    for &b in probe::partition3().iter() {
        v.push(b as u32);
    }
    push_i32s(&mut v, probe::anchor_2nd());
    push_i32s(&mut v, probe::anchor_3rd_1());
    push_i32s(&mut v, probe::anchor_3rd_2());
    for &n in probe::num_subsets().iter() {
        v.push(n as u32);
    }
    v.extend_from_slice(probe::partition_bits());
    v.extend_from_slice(probe::color_index_bitcount());
    push_i32s(&mut v, probe::alpha_index_bitcount());
    push_i32s(&mut v, probe::mode_has_p_bits());
    push_i32s(&mut v, probe::mode_has_shared_p_bits());
    v.extend_from_slice(probe::color_precision_table());
    v.extend_from_slice(probe::alpha_precision_table());
    v.push(probe::pr_weight().to_bits());
    v.push(probe::pb_weight().to_bits());
    v.extend_from_slice(&probe::mode_idx_words());
    v.push(probe::checkerboard_partition_index());
    for get in [
        &probe::subset_idx2 as &dyn Fn(usize) -> ([[i32; 16]; 3], [u32; 3]),
        &probe::subset_idx3,
    ] {
        for p in 0..64 {
            let (idx, _) = get(p);
            for s in 0..3 {
                push_i32s(&mut v, &idx[s]);
            }
        }
        for p in 0..64 {
            let (_, tot) = get(p);
            v.extend_from_slice(&tot);
        }
    }
    for n in TREE.iter() {
        v.push(n.feature as i32 as u32);
        v.push(n.threshold as u32);
        v.push(n.left as i32 as u32);
        v.push(n.right as i32 as u32);
    }
    assert_eq!(v.len(), CONST_DUMP_WORDS);
    v
}

#[test]
fn wgpu_bc7_tables_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_tables_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt = opt_tables_words(&t);
    let opt_bytes = words_bytes(&opt);
    let total = (CONST_DUMP_WORDS + OPT_TABLES_WORDS) as u32;
    let out = vec![0u8; total as usize * 4];
    let mut got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_tables",
        total,
        0,
        &[(2, &opt_bytes), (3, &out)],
        3,
    );
    for entry in ["bc7_test_tables_priv1", "bc7_test_tables_priv2"] {
        let part = run_kernel(g, BC7_WGSL, "bc7", entry, total, 0, &[(3, &out)], 3);
        for (a, b) in got.iter_mut().zip(part.iter()) {
            *a |= *b;
        }
    }
    let mut want = expected_const_words();
    want.extend_from_slice(&opt);
    assert_bytes_eq(&got, &words_bytes(&want), "bc7 const tables + opt echo");
}

fn sigs_via_gpu(g: &crate::gpu::wgpu::Gpu, blocks: &[u8], num_blocks: usize) -> Vec<u32> {
    let num_groups = num_blocks.div_ceil(4);
    let out = vec![0u8; num_groups * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_group_sigs",
        num_groups as u32,
        num_blocks as u32,
        &[(4, blocks), (3, &out)],
        3,
    );
    got.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn check_sigs(g: &crate::gpu::wgpu::Gpu, blocks: &[u8], num_blocks: usize, ctx: &str) {
    let words = sigs_via_gpu(g, &blocks[..num_blocks * 64], num_blocks);
    let num_groups = num_blocks.div_ceil(4);
    assert_eq!(words.len(), num_groups, "{ctx}: group count");
    for gi in 0..num_groups {
        let start = gi * 4;
        let n = (num_blocks - start).min(4);
        let want = group_signature(&blocks[start * 64..(start + n) * 64], n);
        assert!(
            words[gi] <= 0xff,
            "{ctx}: group {gi} word {:#010x} exceeds u8 range",
            words[gi]
        );
        assert_eq!(
            words[gi] as u8, want,
            "{ctx}: group {gi} n={n} got {:#04x} want {want:#04x}",
            words[gi] as u8
        );
    }
}

#[test]
fn wgpu_bc7_group_sigs_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_group_sigs_golden") else {
        return;
    };
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for &(w, h) in &[(64u32, 64u32), (128, 32), (37, 53)] {
        for &seed in &[1u64, 7] {
            for srgb in [false, true] {
                cases.push((
                    format!("tex seed={seed} {w}x{h} srgb={srgb}"),
                    texture_blocks(&gen_texture(seed, w, h), w, h, srgb),
                ));
            }
        }
    }
    let solid_tex: Vec<u8> = std::iter::repeat_n([40u8, 80, 120, 200], 64 * 64)
        .flatten()
        .collect();
    cases.push((
        "all-solid 64x64".into(),
        texture_blocks(&solid_tex, 64, 64, false),
    ));
    let mut alpha_tex = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64u32 {
        for x in 0..64u32 {
            alpha_tex.extend_from_slice(&[
                (x * 4) as u8,
                (y * 4) as u8,
                (x + y) as u8,
                ((x * 255) / 63) as u8,
            ]);
        }
    }
    cases.push((
        "alpha-gradient 64x64".into(),
        texture_blocks(&alpha_tex, 64, 64, false),
    ));
    let (cls_blocks, _) = classify_cases();
    let mut mixed = Vec::new();
    for i in [0usize, 2, 4, 1, 3, 5, 0] {
        mixed.extend_from_slice(&cls_blocks[i]);
    }
    cases.push(("handcrafted mixed 7 blocks".into(), mixed));
    let mut tails_seen = [false; 4];
    for (name, blocks) in &cases {
        let total = blocks.len() / 64;
        for cut in 0..4usize {
            if total <= cut {
                continue;
            }
            let num_blocks = total - cut;
            tails_seen[num_blocks % 4] = true;
            check_sigs(g, blocks, num_blocks, &format!("{name} cut={cut}"));
        }
    }
    assert_eq!(tails_seen, [true; 4], "tail coverage n in 0..4");
}

#[test]
fn wgpu_bc7_classify_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_classify_golden") else {
        return;
    };
    let (blocks, want) = classify_cases();
    for (i, b) in blocks.iter().enumerate() {
        assert_eq!(
            group_signature(b, 1),
            want[i] as u8,
            "corelib class of handcrafted block {i}"
        );
    }
    let buf: Vec<u8> = blocks.concat();
    let out = vec![0u8; blocks.len() * 4];
    let got = run_kernel(
        g,
        BC7_WGSL,
        "bc7",
        "bc7_test_classify",
        blocks.len() as u32,
        0,
        &[(4, &buf), (3, &out)],
        3,
    );
    let got_words: Vec<u32> = got
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert_eq!(got_words, want, "classify codes");
}

#[test]
fn wgpu_bc7_params_echo_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_params_echo_golden") else {
        return;
    };
    for (k, p) in params4().iter().enumerate() {
        let words = params_words(p);
        let bytes = words_bytes(&words);
        let out = vec![0u8; PARAMS_WORDS * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_params_echo",
            PARAMS_WORDS as u32,
            0,
            &[(1, &bytes), (3, &out)],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&words),
            &format!("params echo variant {k}"),
        );
    }
}

#[test]
fn luma_bits_pin() {
    let bits: Vec<u32> = probe::luma_weights().iter().map(|f| f.to_bits()).collect();
    assert_eq!(bits, vec![0x3e59b3d0, 0x3f371759, 0x3d93dd98]);
}

#[test]
fn bc7_dist_bound_probe() {
    let sets = weight_sets();
    let corners = corner_colors();
    let mut max_d = 0u64;
    let mut check = |d: u64| {
        if d > max_d {
            max_d = d;
        }
    };
    for &w in &sets {
        for perc in [false, true] {
            for e1 in &corners {
                for e2 in &corners {
                    check(probe::dist_rgb(*e1, *e2, perc, w));
                    check(probe::dist_rgba(*e1, *e2, perc, w));
                }
            }
        }
    }
    let mut st = 0xdeadbeefcafef00du64;
    for _ in 0..200_000 {
        let mut e1 = [0i32; 4];
        let mut e2 = [0i32; 4];
        for k in 0..4 {
            e1[k] = (xs64(&mut st) % 256) as i32;
            e2[k] = (xs64(&mut st) % 256) as i32;
        }
        let w = sets[(xs64(&mut st) % sets.len() as u64) as usize];
        let perc = xs64(&mut st) & 1 == 1;
        check(probe::dist_rgb(e1, e2, perc, w));
        check(probe::dist_rgba(e1, e2, perc, w));
    }
    assert!(
        max_d < (1u64 << 31),
        "distance-fn total {max_d} reaches 2^31; f32_to_u64 needs a 2-word split"
    );
    eprintln!(
        "bc7_dist_bound_probe: max per-call distance total {max_d} (~2^{:.1}) < 2^31",
        (max_d as f64).log2()
    );
}

#[test]
fn bc7_lane_independence_property() {
    let tables = build_opt_tables();
    let params = params4();
    let mut st = 0x243f6a8885a308d3u64;
    let groups = 2000usize;
    for gi in 0..groups {
        let mut blocks = [0u8; 256];
        for k in 0..4 {
            gen_block(&mut st, gi * 4 + k, &mut blocks[k * 64..(k + 1) * 64]);
        }
        for (pi, p) in params.iter().enumerate() {
            let mut out4 = [[0u8; 16]; 4];
            encode_group(&blocks, 4, p, &tables, &mut out4);
            for k in 0..4 {
                let mut out1 = [[0u8; 16]; 1];
                encode_group(&blocks[k * 64..(k + 1) * 64], 1, p, &tables, &mut out1);
                assert_eq!(
                    out1[0], out4[k],
                    "lane dependence: group {gi} bucket {pi} lane {k}"
                );
            }
        }
    }
    eprintln!(
        "bc7_lane_independence_property: HOLDS over {groups} seeded groups x 4 param buckets; one thread per BLOCK is safe for S5-S8"
    );
}
