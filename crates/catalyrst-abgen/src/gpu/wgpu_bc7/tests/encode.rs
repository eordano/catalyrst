use super::*;

fn alpha_variants() -> Vec<Params> {
    let mut v = params4().to_vec();
    let mut p = Params::slow(false);
    p.mode4_index_mask = 1;
    p.mode4_rotation_mask = 5;
    p.mode5_rotation_mask = 2;
    p.uber_level = 2;
    v.push(p);
    let mut p = Params::slow(true);
    p.mode4_index_mask = 2;
    p.uber_level = 4;
    p.pbit_search = false;
    v.push(p);
    let mut p = Params::basic(false);
    p.mode4_index_mask = 3;
    p.mode4_rotation_mask = 10;
    p.uber_level = 3;
    p.refinement_passes = 2;
    p.weights = [37, 5, 11, 3];
    v.push(p);
    let mut p = Params::basic(true);
    p.uber_level = 0;
    p.mode5_rotation_mask = 9;
    p.refinement_passes = 0;
    v.push(p);
    v
}

fn alpha_blocks(st: &mut u64) -> Vec<[[i32; 4]; 16]> {
    let mut out = Vec::new();
    for strat in [0usize, 1, 4, 8] {
        let mut blk = [0u8; 64];
        gen_block(st, strat, &mut blk);
        out.push(px_from_block(&blk));
    }
    let mut px = [[0i32; 4]; 16];
    for (i, row) in px.iter_mut().enumerate() {
        *row = [(i * 16) as i32, 255 - (i * 8) as i32, 40, (i * 17) as i32];
    }
    out.push(px);
    for (fill, lo, step) in [(6usize, 137i32, 0i32), (6, 100, 1), (6, 252, 1), (0, 0, 1)] {
        let mut blk = [0u8; 64];
        gen_block(st, fill, &mut blk);
        let mut px = px_from_block(&blk);
        for (i, row) in px.iter_mut().enumerate() {
            row[3] = lo + (i as i32 & 1) * step;
        }
        out.push(px);
    }
    let mut blk = [0u8; 64];
    gen_block(st, 0, &mut blk);
    let mut px = px_from_block(&blk);
    for (i, row) in px.iter_mut().enumerate() {
        row[3] = if i % 3 == 0 { 0 } else { 255 };
    }
    out.push(px);
    out
}

fn rotate_case(
    cp: &Params,
    rotation: usize,
    px: &[[i32; 4]; 16],
) -> ([u32; 4], [[i32; 4]; 16], i32, i32) {
    let mut weights = cp.weights;
    let mut rp = *px;
    if rotation != 0 {
        weights.swap(rotation - 1, 3);
        for row in rp.iter_mut() {
            row.swap(3, rotation - 1);
        }
    }
    let mut tlo = 255i32;
    let mut thi = 0i32;
    for row in &rp {
        tlo = tlo.min(row[3]);
        thi = thi.max(row[3]);
    }
    (weights, rp, tlo, thi)
}

fn push_alpha_out(want: &mut Vec<u32>, out: &probe::AlphaOut) {
    let (err, is, low, high, sel, asel) = out;
    want.push(*err as u32);
    want.push((*err >> 32) as u32);
    want.push(*is);
    for q in low.iter().chain(high.iter()) {
        want.push(*q as u32);
    }
    for q in sel.iter().chain(asel.iter()) {
        want.push(*q as u32);
    }
}

#[test]
fn wgpu_bc7_mode4_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_mode4_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let mut st = 0xa1fa_40de_0000_5eedu64;
    let blocks_px = alpha_blocks(&mut st);
    for (vi, cp) in alpha_variants().iter().enumerate() {
        let mut input: Vec<u32> = Vec::new();
        let mut want: Vec<u32> = Vec::new();
        let mut total = 0usize;
        for px in &blocks_px {
            for rotation in 0..4usize {
                if rotation != 0 && (cp.perceptual || !cp.use_mode4_rotation) {
                    continue;
                }
                if cp.mode4_rotation_mask & (1 << rotation) == 0 {
                    continue;
                }
                let (weights, rp, tlo, thi) = rotate_case(cp, rotation, px);
                for init_err in [u64::MAX, 1_000u64] {
                    input.extend_from_slice(&weights);
                    input.push(tlo as u32);
                    input.push(thi as u32);
                    input.push(init_err as u32);
                    input.push((init_err >> 32) as u32);
                    push_pixels(&mut input, &rp);
                    let out = probe::alpha_mode4(weights, cp, tlo, thi, init_err, &rp, &t);
                    push_alpha_out(&mut want, &out);
                    total += 1;
                }
            }
        }
        eprintln!("wgpu_bc7_mode4_golden: variant {vi}: {total} cases");
        assert_eq!(input.len(), total * 72);
        assert_eq!(want.len(), total * 43);
        let pbytes = words_bytes(&params_words(cp));
        let out = vec![0u8; total * 43 * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_mode4",
            total as u32,
            0,
            &[
                (1, &pbytes),
                (2, &opt_bytes),
                (4, &words_bytes(&input)),
                (3, &out),
            ],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&want),
            &format!("bc7_test_mode4 variant {vi}"),
        );
    }
}

#[test]
fn wgpu_bc7_mode5_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_mode5_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let mut st = 0xa1fa_50de_0000_5eedu64;
    let blocks_px = alpha_blocks(&mut st);
    for (vi, cp) in alpha_variants().iter().enumerate() {
        let mut input: Vec<u32> = Vec::new();
        let mut want: Vec<u32> = Vec::new();
        let mut total = 0usize;
        for px in &blocks_px {
            for rotation in 0..4usize {
                if rotation != 0 && (cp.perceptual || !cp.use_mode5_rotation) {
                    continue;
                }
                if cp.mode5_rotation_mask & (1 << rotation) == 0 {
                    continue;
                }
                let (weights, rp, tlo, thi) = rotate_case(cp, rotation, px);
                input.extend_from_slice(&weights);
                input.push(tlo as u32);
                input.push(thi as u32);
                push_pixels(&mut input, &rp);
                let out = probe::alpha_mode5(weights, cp, tlo, thi, &rp, &t);
                push_alpha_out(&mut want, &out);
                total += 1;
            }
        }
        eprintln!("wgpu_bc7_mode5_golden: variant {vi}: {total} cases");
        assert_eq!(input.len(), total * 70);
        assert_eq!(want.len(), total * 43);
        let pbytes = words_bytes(&params_words(cp));
        let out = vec![0u8; total * 43 * 4];
        let got = run_kernel(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_test_mode5",
            total as u32,
            0,
            &[
                (1, &pbytes),
                (2, &opt_bytes),
                (4, &words_bytes(&input)),
                (3, &out),
            ],
            3,
        );
        assert_bytes_eq(
            &got,
            &words_bytes(&want),
            &format!("bc7_test_mode5 variant {vi}"),
        );
    }
}

fn encode_blocks_cpu(
    cp: &Params,
    t: &crate::gpu::corelib::bc7::OptTables,
    blocks: &[u8],
) -> Vec<u8> {
    use crate::gpu::corelib::bc7::GROUP_WIDTH;
    let num_blocks = blocks.len() / 64;
    let mut out = Vec::with_capacity(num_blocks * 16);
    for chunk in blocks.chunks(GROUP_WIDTH * 64) {
        let n = chunk.len() / 64;
        let mut grp = [[0u8; 16]; GROUP_WIDTH];
        encode_group(chunk, n, cp, t, &mut grp);
        for b in &grp[..n] {
            out.extend_from_slice(b);
        }
    }
    out
}

pub(crate) struct EncodePipes {
    plan_alpha: ::wgpu::ComputePipeline,
    plan_opaque13: ::wgpu::ComputePipeline,
    plan_opaque02: ::wgpu::ComputePipeline,
    enc_solid: ::wgpu::ComputePipeline,
    enc_alpha: ::wgpu::ComputePipeline,
    enc_opaque: ::wgpu::ComputePipeline,
}

fn prepare_encode_pipes(g: &crate::gpu::wgpu::Gpu) -> EncodePipes {
    let enc = |class: f64| {
        prepare_kernel_const(
            g,
            BC7_WGSL,
            "bc7",
            "bc7_encode_blocks",
            &[("TRIAL_CLASS", class)],
        )
    };
    EncodePipes {
        plan_alpha: prepare_kernel(g, BC7_WGSL, "bc7", "bc7_plan_alpha"),
        plan_opaque13: prepare_kernel(g, BC7_WGSL, "bc7", "bc7_plan_opaque13"),
        plan_opaque02: prepare_kernel(g, BC7_WGSL, "bc7", "bc7_plan_opaque02"),
        enc_solid: enc(0.0),
        enc_alpha: enc(1.0),
        enc_opaque: enc(2.0),
    }
}

fn encode_blocks_gpu(
    g: &crate::gpu::wgpu::Gpu,
    pipes: &EncodePipes,
    cp: &Params,
    opt_bytes: &[u8],
    blocks: &[u8],
) -> Vec<u8> {
    let num_blocks = (blocks.len() / 64) as u32;
    let pbytes = words_bytes(&params_words(cp));
    let num_groups = num_blocks.div_ceil(4);
    let mut scratch = vec![0u8; num_blocks as usize * PLAN_STRIDE * 4];
    for pipe in [
        &pipes.plan_alpha,
        &pipes.plan_opaque13,
        &pipes.plan_opaque02,
    ] {
        scratch = dispatch_prepared_wg(
            g,
            pipe,
            num_groups,
            num_blocks,
            &[(1, &pbytes), (4, blocks), (3, &scratch)],
            3,
            64,
        );
    }
    let mut out = vec![0u8; num_blocks as usize * 16];
    for pipe in [&pipes.enc_solid, &pipes.enc_alpha, &pipes.enc_opaque] {
        out = dispatch_prepared_wg(
            g,
            pipe,
            num_blocks,
            num_blocks,
            &[
                (1, &pbytes),
                (2, opt_bytes),
                (4, blocks),
                (5, &scratch),
                (3, &out),
            ],
            3,
            64,
        );
    }
    out
}

fn extreme_blocks() -> Vec<u8> {
    let mut out = Vec::new();
    let (cls, _) = classify_cases();
    for b in &cls {
        out.extend_from_slice(b);
    }
    for px in [
        [0u8, 0, 0, 0],
        [255, 255, 255, 255],
        [0, 0, 0, 255],
        [255, 255, 255, 0],
        [1, 1, 1, 254],
        [128, 128, 128, 127],
    ] {
        out.extend_from_slice(&solid_block(px));
    }
    out.extend_from_slice(&block_with(|i| {
        [
            (i * 17) as u8,
            255 - (i * 16) as u8,
            (i * i) as u8,
            if i < 8 { 0 } else { 255 },
        ]
    }));
    out.extend_from_slice(&block_with(|i| [200, 10, 30, (i * 17) as u8]));
    out.extend_from_slice(&block_with(|i| {
        let v = (i * 16) as u8;
        [v, v, v, 254]
    }));
    out.extend_from_slice(&block_with(|i| {
        [255 - i as u8, i as u8, 128, 255 - (i as u8 & 1)]
    }));
    out.extend_from_slice(&block_with(|i| {
        [
            10 + i as u8,
            20,
            250 - i as u8,
            if i == 5 { 3 } else { 200 },
        ]
    }));
    out.extend_from_slice(&block_with(|i| [(i * 4) as u8, 0, 255, 1]));
    out
}

fn encode_buckets() -> Vec<(String, Params)> {
    let mut v: Vec<(String, Params)> = params4()
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("bucket{i}"), p.clone()))
        .collect();
    let mut p = Params::basic(false);
    p.mode6_only = true;
    v.push(("mode6_only".into(), p));
    v
}

#[test]
fn wgpu_bc7_dbg_compile() {
    let Ok(entry) = std::env::var("ABGEN_GPU_DBG_ENTRY") else {
        return;
    };
    let Some(g) = gpu_or_skip("wgpu_bc7_dbg_compile") else {
        return;
    };
    g.device.set_device_lost_callback(|reason, msg| {
        eprintln!("wgpu_bc7_dbg_compile: DEVICE LOST ({reason:?}): {msg}");
    });
    for e in entry.split(',') {
        let t = std::time::Instant::now();
        let _p = prepare_kernel(g, BC7_WGSL, "bc7", e);
        g.device.poll(::wgpu::PollType::Poll).ok();
        eprintln!(
            "wgpu_bc7_dbg_compile: {e} compiled in {:.1}s",
            t.elapsed().as_secs_f64()
        );
    }
}

#[test]
fn wgpu_bc7_dbg_block() {
    if std::env::var("ABGEN_GPU_DBG_BLOCK").is_err() {
        return;
    }
    let Some(g) = gpu_or_skip("wgpu_bc7_dbg_block") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let bi: usize = std::env::var("ABGEN_GPU_DBG_BLOCK")
        .unwrap()
        .parse()
        .unwrap_or(0);
    let cp = Params::slow(false);
    let tex = gen_texture(1, 64, 64);
    let blocks = texture_blocks(&tex, 64, 64, false);
    let blk = &blocks[bi * 64..bi * 64 + 64];
    let px = px_from_block(blk.try_into().unwrap());
    let sig = group_signature(blk, 1);
    let (hint, _) = hint_code(&cp, &px);
    let plan = probe::build_plans(&cp, &px);
    eprintln!("CPU: class={sig} hint={hint}");
    eprintln!(
        "CPU plan: part0={} part13={} part2={} use13={} use2={} use0={}",
        plan.part0, plan.part13, plan.part2, plan.use_list13, plan.use_list2, plan.use_list0
    );
    eprintln!("CPU list13: {:?}", plan.list13);
    eprintln!("CPU list2: {:?}", plan.list2);
    eprintln!("CPU list0: {:?}", plan.list0);
    eprintln!("CPU list7: {:?}", plan.list7);
    let mut grp = [[0u8; 16]; 4];
    encode_group(blk, 1, &cp, &t, &mut grp);
    eprintln!("CPU block: {:?}", grp[0]);
    let pipes = prepare_encode_pipes(g);
    let pbytes = words_bytes(&params_words(&cp));
    let mut plans_out = vec![0u8; PLAN_STRIDE * 4];
    for pipe in [
        &pipes.plan_alpha,
        &pipes.plan_opaque13,
        &pipes.plan_opaque02,
    ] {
        plans_out = dispatch_prepared_wg(
            g,
            pipe,
            1,
            1,
            &[(1, &pbytes), (4, blk), (3, &plans_out)],
            3,
            64,
        );
    }
    let words: Vec<u32> = plans_out
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    eprintln!("GPU pass1 words: {words:?}");
    let got = encode_blocks_gpu(g, &pipes, &cp, &opt_bytes, blk);
    eprintln!("GPU block: {got:?}");
}

#[test]
fn wgpu_bc7_encode_blocks_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_encode_blocks_golden") else {
        return;
    };
    let t = build_opt_tables();
    let opt_bytes = words_bytes(&opt_tables_words(&t));
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    let sizes: &[(u32, u32, &[u64])] = &[
        (64, 64, &[1, 7, 11]),
        (128, 32, &[1, 7]),
        (37, 53, &[1, 7]),
        (256, 256, &[1]),
    ];
    for &(w, h, seeds) in sizes {
        for &seed in seeds {
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
    cases.push(("handcrafted extremes".into(), extreme_blocks()));
    g.device.on_uncaptured_error(std::sync::Arc::new(|e| {
        eprintln!("wgpu_bc7_encode_blocks_golden: UNCAPTURED ERROR: {e}");
    }));
    g.device.set_device_lost_callback(|reason, msg| {
        eprintln!("wgpu_bc7_encode_blocks_golden: DEVICE LOST ({reason:?}): {msg}");
    });
    let t_pipe = std::time::Instant::now();
    let pipes = prepare_encode_pipes(g);
    eprintln!(
        "wgpu_bc7_encode_blocks_golden: pipelines compiled in {:.1}s",
        t_pipe.elapsed().as_secs_f64()
    );
    let mut total_blocks = 0usize;
    for (bname, cp) in encode_buckets() {
        for (name, blocks) in &cases {
            let num_blocks = blocks.len() / 64;
            let got = encode_blocks_gpu(g, &pipes, &cp, &opt_bytes, blocks);
            let want = encode_blocks_cpu(&cp, &t, blocks);
            assert_eq!(
                got.len(),
                num_blocks * 16,
                "bc7_encode_blocks {bname} {name}: output length"
            );
            for b in 0..num_blocks {
                assert_eq!(
                    &got[b * 16..b * 16 + 16],
                    &want[b * 16..b * 16 + 16],
                    "bc7_encode_blocks {bname} case {name} block {b}"
                );
            }
            total_blocks += num_blocks;
            eprintln!(
                "wgpu_bc7_encode_blocks_golden: {bname} {name}: {num_blocks} blocks bit-exact"
            );
        }
    }
    eprintln!("wgpu_bc7_encode_blocks_golden: TOTAL {total_blocks} blocks compared, zero diffs");
}

#[test]
fn wgpu_bc7_mip_chain_qualification() {
    if gpu_or_skip("wgpu_bc7_mip_chain_qualification").is_none() {
        return;
    }
    let st = crate::gpu::qualify::qualify_backend_with(
        "wgpu",
        &|rgba, w, h, mc, flip, srgb, perc, prof| {
            super::encode_bc7_mip_chain(rgba, w, h, mc, flip, srgb, perc, prof)
        },
        false,
    );
    assert!(st.qualified, "wgpu qualification failed: {:?}", st.reason);
    eprintln!(
        "wgpu_bc7_mip_chain_qualification: FULL battery green \
         (64x64/128x32/37x53 x srgb x perceptual x Slow/Basic, flip=true, vs bc7_pure)"
    );
}

#[test]
fn wgpu_bc7_mip_chain_explicit_args_golden() {
    if gpu_or_skip("wgpu_bc7_mip_chain_explicit_args_golden").is_none() {
        return;
    }
    for &(w, h) in &[(37u32, 53u32), (64, 64)] {
        let tex = gen_texture(11, w, h);
        for (mc, flip) in [(Some(1), false), (Some(3), false), (None, true)] {
            let (want, want_mips) = crate::bc7_pure::encode_bc7_mip_chain_with_profile(
                &tex,
                w,
                h,
                mc,
                flip,
                true,
                false,
                crate::bc7_pure::Bc7Profile::Basic,
            );
            let (got, got_mips) = super::encode_bc7_mip_chain(
                &tex,
                w,
                h,
                mc,
                flip,
                true,
                false,
                crate::gpu::corelib::bc7::Bc7Profile::Basic,
            )
            .expect("wgpu mip chain encode");
            assert_eq!(
                got_mips, want_mips,
                "mip-chain {w}x{h} mc={mc:?} flip={flip}: mip count"
            );
            assert_bytes_eq(
                &got,
                &want,
                &format!("mip-chain {w}x{h} mc={mc:?} flip={flip}"),
            );
        }
    }
    let err = super::encode_bc7_mip_chain(
        &[0u8; 12],
        2,
        2,
        None,
        false,
        false,
        false,
        crate::gpu::corelib::bc7::Bc7Profile::Basic,
    )
    .expect_err("length validation");
    assert!(err.to_string().contains("rgba len"), "{err}");
}

#[test]
fn buffer_demand_default_limits_pin() {
    let defaults = ::wgpu::Limits::default();
    for &(w, h) in &[(64u64, 64u64), (128, 32), (37, 53)] {
        let px = w * h;
        let nb = w.div_ceil(4) * h.div_ceil(4);
        let (bind, buf) = buffer_demand(px * 4, px, nb, nb);
        assert!(
            bind <= defaults.max_storage_buffer_binding_size,
            "qualification-size demand {bind} must fit default binding limit"
        );
        assert!(buf <= defaults.max_buffer_size);
    }
    let px = 4096u64 * 4096;
    let nb = 1024u64 * 1024;
    let (bind, buf) = buffer_demand(px * 4, px, nb, nb);
    assert!(
        bind > defaults.max_storage_buffer_binding_size,
        "4096x4096 mips=1 demand {bind} must exceed default binding limit {} (the panic class)",
        defaults.max_storage_buffer_binding_size
    );
    assert!(buf >= bind);
}

#[test]
fn wgpu_bc7_mip_chain_large_texture_golden() {
    let Some(g) = gpu_or_skip("wgpu_bc7_mip_chain_large_texture_golden") else {
        return;
    };
    let (w, h) = (4096u32, 4096u32);
    let px = (w as u64) * (h as u64);
    let (bw, bh) = mips::level_block_dims(w as usize, h as usize);
    let nb = (bw * bh) as u64;
    let (bind, buf) = buffer_demand(px * 4, px, nb, nb);
    assert!(
        bind > ::wgpu::Limits::default().max_storage_buffer_binding_size,
        "case must exceed wgpu default limits to cover the class"
    );
    let lim = g.device.limits();
    if bind > lim.max_storage_buffer_binding_size || buf > lim.max_buffer_size {
        eprintln!(
            "wgpu_bc7_mip_chain_large_texture_golden: SKIP adapter binding limit {} / buffer limit {} below demand {bind}/{buf}; asserting graceful Err instead",
            lim.max_storage_buffer_binding_size, lim.max_buffer_size
        );
        let tex = vec![0u8; (px * 4) as usize];
        let err = super::encode_bc7_mip_chain(
            &tex,
            w,
            h,
            Some(1),
            false,
            true,
            true,
            crate::gpu::corelib::bc7::Bc7Profile::Basic,
        )
        .expect_err("must refuse, not panic, past device limits");
        assert!(
            err.to_string().contains("exceeds wgpu device limits"),
            "{err}"
        );
        return;
    }
    let tex = gen_texture(1, w, h);
    let (got, got_mips) = super::encode_bc7_mip_chain(
        &tex,
        w,
        h,
        Some(1),
        false,
        true,
        true,
        crate::gpu::corelib::bc7::Bc7Profile::Basic,
    )
    .expect("large-texture wgpu encode past default limits");
    assert_eq!(got_mips, 1);
    assert_eq!(got.len() as u64, nb * 16);
    let lin = lin_cpu(&tex, true);
    let p = Params::basic(true);
    let t = build_opt_tables();
    let num_groups = nb / 4;
    let mut sampled = vec![0u64, num_groups / 2, num_groups - 1];
    let mut st = 0x9e3779b97f4a7c15u64;
    for _ in 0..61 {
        sampled.push(xs64(&mut st) % num_groups);
    }
    sampled.sort_unstable();
    sampled.dedup();
    for &gi in &sampled {
        let mut blocks = [0u8; 256];
        for k in 0..4usize {
            let bi = gi * 4 + k as u64;
            let bx = (bi % bw as u64) as usize;
            let by = (bi / bw as u64) as usize;
            let mut blk = [0u8; 64];
            mips::quantize_pack_block(&lin, w as usize, h as usize, true, bx, by, &mut blk);
            blocks[k * 64..(k + 1) * 64].copy_from_slice(&blk);
        }
        let mut want = [[0u8; 16]; 4];
        encode_group(&blocks, 4, &p, &t, &mut want);
        for k in 0..4usize {
            let off = ((gi * 4 + k as u64) * 16) as usize;
            assert_eq!(
                &got[off..off + 16],
                &want[k],
                "large-texture group {gi} lane {k}"
            );
        }
    }
    eprintln!(
        "wgpu_bc7_mip_chain_large_texture_golden: 4096x4096 mips=1 encoded on-GPU past default limits; {} sampled groups bit-exact",
        sampled.len()
    );
}
