use crate::gpu::corelib::bc7::{build_opt_tables, Bc7Profile, EndpointErr, OptTables, Params};
use crate::gpu::corelib::mips::{box_halve_dims, compute_default_mip_count, level_block_dims};
use crate::gpu::wgpu::{gpu, Gpu, BLOCKIFY_WGSL};
use anyhow::{anyhow, ensure, Result};
use std::sync::OnceLock;

pub(crate) const BC7_WGSL: &str = include_str!("../shaders/bc7.wgsl");

pub(crate) const PARAMS_WORDS: usize = 42;
pub(crate) const OPT_TABLES_WORDS: usize = 4352;
pub(crate) const PLAN_STRIDE: usize = 110;

const _: () = assert!(std::mem::size_of::<Params>() == 124);
const _: () = assert!(std::mem::align_of::<Params>() == 4);
const _: () = assert!(std::mem::size_of::<EndpointErr>() == 4);
const _: () = assert!(std::mem::align_of::<EndpointErr>() == 2);
const _: () = assert!(std::mem::size_of::<OptTables>() == OPT_TABLES_WORDS * 4);
const _: () = assert!(std::mem::align_of::<OptTables>() == 4);

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn endpoint_err_word(e: EndpointErr) -> u32 {
    e.error as u32 | ((e.lo as u32) << 16) | ((e.hi as u32) << 24)
}

pub(crate) fn params_words(p: &Params) -> Vec<u32> {
    let mut w = Vec::with_capacity(PARAMS_WORDS);
    w.extend_from_slice(&p.max_partitions_mode);
    w.extend_from_slice(&p.weights);
    w.push(p.uber_level);
    w.push(p.refinement_passes);
    w.push(p.mode4_rotation_mask);
    w.push(p.mode4_index_mask);
    w.push(p.mode5_rotation_mask);
    w.push(p.uber1_mask);
    w.push(p.perceptual as u32);
    w.push(p.pbit_search as u32);
    w.push(p.mode6_only as u32);
    w.push(p.op_max_mode13);
    w.push(p.op_max_mode0);
    w.push(p.op_max_mode2);
    for b in p.use_mode {
        w.push(b as u32);
    }
    w.push(p.al_max_mode7);
    w.extend_from_slice(&p.mode67_weight_mul);
    w.push(p.use_mode4 as u32);
    w.push(p.use_mode5 as u32);
    w.push(p.use_mode6 as u32);
    w.push(p.use_mode7 as u32);
    w.push(p.use_mode4_rotation as u32);
    w.push(p.use_mode5_rotation as u32);
    assert_eq!(w.len(), PARAMS_WORDS);
    w
}

pub(crate) fn opt_tables_words(t: &OptTables) -> Vec<u32> {
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (t as *const OptTables).cast::<u8>(),
            std::mem::size_of::<OptTables>(),
        )
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub(crate) fn words_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

struct Engine {
    lin: ::wgpu::ComputePipeline,
    halve: ::wgpu::ComputePipeline,
    pack: ::wgpu::ComputePipeline,
    plan: [::wgpu::ComputePipeline; 3],
    enc: [::wgpu::ComputePipeline; 3],
    opt: ::wgpu::Buffer,
    params: [::wgpu::Buffer; 4],
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn make_pipeline(
    g: &Gpu,
    module: &::wgpu::ShaderModule,
    entry: &str,
    constants: &[(&str, f64)],
) -> ::wgpu::ComputePipeline {
    g.device
        .create_compute_pipeline(&::wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: None,
            module,
            entry_point: Some(entry),
            compilation_options: ::wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            cache: None,
        })
}

fn storage_init(g: &Gpu, label: &str, data: &[u8]) -> ::wgpu::Buffer {
    use ::wgpu::util::DeviceExt;
    g.device
        .create_buffer_init(&::wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: data,
            usage: ::wgpu::BufferUsages::STORAGE,
        })
}

fn storage_empty(g: &Gpu, label: &str, size: u64, extra: ::wgpu::BufferUsages) -> ::wgpu::Buffer {
    g.device.create_buffer(&::wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: ::wgpu::BufferUsages::STORAGE | extra,
        mapped_at_creation: false,
    })
}

fn build_engine(g: &Gpu) -> Engine {
    let blockify = g
        .device
        .create_shader_module(::wgpu::ShaderModuleDescriptor {
            label: Some("blockify"),
            source: ::wgpu::ShaderSource::Wgsl(BLOCKIFY_WGSL.into()),
        });
    let bc7 = g
        .device
        .create_shader_module(::wgpu::ShaderModuleDescriptor {
            label: Some("bc7"),
            source: ::wgpu::ShaderSource::Wgsl(BC7_WGSL.into()),
        });
    let t = build_opt_tables();
    let opt = storage_init(g, "bc7-opt-tables", &words_bytes(&opt_tables_words(&t)));
    let params = [
        Params::slow(false),
        Params::slow(true),
        Params::basic(false),
        Params::basic(true),
    ]
    .map(|p| storage_init(g, "bc7-params", &words_bytes(&params_words(&p))));
    Engine {
        lin: make_pipeline(g, &blockify, "blockify_linearize", &[]),
        halve: make_pipeline(g, &blockify, "blockify_halve", &[]),
        pack: make_pipeline(g, &blockify, "blockify_quantize_pack", &[]),
        plan: [
            make_pipeline(g, &bc7, "bc7_plan_alpha", &[]),
            make_pipeline(g, &bc7, "bc7_plan_opaque13", &[]),
            make_pipeline(g, &bc7, "bc7_plan_opaque02", &[]),
        ],
        enc: [0.0f64, 1.0, 2.0]
            .map(|c| make_pipeline(g, &bc7, "bc7_encode_blocks", &[("TRIAL_CLASS", c)])),
        opt,
        params,
    }
}

fn engine(g: &'static Gpu) -> &'static Engine {
    ENGINE.get_or_init(|| build_engine(g))
}

fn push_u64(b: &mut Vec<u8>, x: u64) {
    b.extend_from_slice(&x.to_le_bytes());
}

fn push_u32(b: &mut Vec<u8>, x: u32) {
    b.extend_from_slice(&x.to_le_bytes());
}

fn lin_item_bytes(base_px: u64, pyr_px: u64, srgb: bool) -> Vec<u8> {
    let mut b = Vec::with_capacity(24);
    push_u64(&mut b, base_px);
    push_u64(&mut b, pyr_px);
    push_u32(&mut b, srgb as u32);
    push_u32(&mut b, 0);
    b
}

fn halve_item_bytes(src_px: u64, dst_px: u64, w: u32, h: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(24);
    push_u64(&mut b, src_px);
    push_u64(&mut b, dst_px);
    push_u32(&mut b, w);
    push_u32(&mut b, h);
    b
}

fn pack_item_bytes(lvl_px: u64, blk_off: u64, w: u32, h: u32, srgb: bool) -> Vec<u8> {
    let mut b = Vec::with_capacity(32);
    push_u64(&mut b, lvl_px);
    push_u64(&mut b, blk_off);
    push_u32(&mut b, w);
    push_u32(&mut b, h);
    push_u32(&mut b, srgb as u32);
    push_u32(&mut b, 0);
    b
}

fn flip_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut flipped = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = &rgba[(h - 1 - y) * w * 4..(h - y) * w * 4];
        flipped[y * w * 4..(y + 1) * w * 4].copy_from_slice(src);
    }
    flipped
}

struct Stage<'a> {
    pipeline: &'a ::wgpu::ComputePipeline,
    wg: u32,
    total: u64,
    n_items: u32,
    fone: bool,
    bufs: &'a [(u32, &'a ::wgpu::Buffer)],
}

fn run_stage(g: &Gpu, enc: &mut ::wgpu::CommandEncoder, st: &Stage) {
    use ::wgpu::util::DeviceExt;
    let max_wg = g.device.limits().max_compute_workgroups_per_dimension as u64;
    let chunk = max_wg * st.wg as u64;
    let layout = st.pipeline.get_bind_group_layout(0);
    let mut base = 0u64;
    while base < st.total {
        let n = (st.total - base).min(chunk);
        let fone = if st.fone { 1.0f32.to_bits() } else { 0 };
        let mut meta = Vec::with_capacity(16);
        for v in [st.n_items, st.total as u32, base as u32, fone] {
            meta.extend_from_slice(&v.to_le_bytes());
        }
        let meta_buf = g
            .device
            .create_buffer_init(&::wgpu::util::BufferInitDescriptor {
                label: Some("meta"),
                contents: &meta,
                usage: ::wgpu::BufferUsages::UNIFORM,
            });
        let mut entries = vec![::wgpu::BindGroupEntry {
            binding: 0,
            resource: meta_buf.as_entire_binding(),
        }];
        for (binding, buf) in st.bufs {
            entries.push(::wgpu::BindGroupEntry {
                binding: *binding,
                resource: buf.as_entire_binding(),
            });
        }
        let bg = g.device.create_bind_group(&::wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &entries,
        });
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(st.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(n.div_ceil(st.wg as u64) as u32, 1, 1);
        base += n;
    }
}

struct Level {
    w: usize,
    h: usize,
    px_off: u64,
    blk_off: u64,
    nb: u64,
}

pub(crate) fn buffer_demand(
    base_bytes: u64,
    total_px: u64,
    nb0: u64,
    num_blocks: u64,
) -> (u64, u64) {
    let binding = base_bytes
        .max(total_px * 16)
        .max(nb0 * 64)
        .max(nb0 * PLAN_STRIDE as u64 * 4);
    (binding, binding.max(num_blocks * 16))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_bc7_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
    perceptual: bool,
    profile: Bc7Profile,
) -> Result<(Vec<u8>, i32)> {
    let w = width as usize;
    let h = height as usize;
    ensure!(width > 0 && height > 0, "empty texture {width}x{height}");
    ensure!(
        rgba.len() == w * h * 4,
        "rgba len {} != {}x{}x4",
        rgba.len(),
        w,
        h
    );
    let mips = mip_count.unwrap_or_else(|| compute_default_mip_count(width, height));
    ensure!(mips >= 1, "mip_count {mips} < 1");
    let g = gpu().map_err(|e| anyhow!("wgpu unavailable: {e}"))?;
    let eng = engine(g);
    let bucket = match (profile, perceptual) {
        (Bc7Profile::Slow, false) => 0usize,
        (Bc7Profile::Slow, true) => 1,
        (Bc7Profile::Basic, false) => 2,
        (Bc7Profile::Basic, true) => 3,
    };
    let data = if flip {
        flip_rgba(rgba, width, height)
    } else {
        rgba.to_vec()
    };
    let mut levels = Vec::with_capacity(mips as usize);
    let (mut cw, mut ch) = (w, h);
    let (mut px_off, mut blk_off) = (0u64, 0u64);
    for _ in 0..mips {
        let (bw, bh) = level_block_dims(cw, ch);
        levels.push(Level {
            w: cw,
            h: ch,
            px_off,
            blk_off,
            nb: (bw * bh) as u64,
        });
        px_off += (cw * ch) as u64;
        blk_off += (bw * bh) as u64;
        let (nw, nh) = box_halve_dims(cw, ch);
        cw = nw;
        ch = nh;
    }
    let total_px = px_off;
    let num_blocks = blk_off;
    let limits = g.device.limits();
    let (need_binding, need_buffer) =
        buffer_demand(data.len() as u64, total_px, levels[0].nb, num_blocks);
    ensure!(
        need_binding <= limits.max_storage_buffer_binding_size
            && need_buffer <= limits.max_buffer_size,
        "texture {width}x{height} mips={mips} exceeds wgpu device limits: needs storage binding {need_binding} B (max {}) and buffer {need_buffer} B (max {})",
        limits.max_storage_buffer_binding_size,
        limits.max_buffer_size
    );
    let base_buf = storage_init(g, "base-rgba", &data);
    let pyr_buf = storage_empty(g, "pyr", total_px * 16, ::wgpu::BufferUsages::empty());
    let staging = g.device.create_buffer(&::wgpu::BufferDescriptor {
        label: Some("bc7-staging"),
        size: num_blocks * 16,
        usage: ::wgpu::BufferUsages::MAP_READ | ::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let zero_prefix = storage_init(g, "prefix0", &0u64.to_le_bytes());
    let lin_items = storage_init(g, "lin-items", &lin_item_bytes(0, 0, srgb));
    let params_buf = &eng.params[bucket];
    let mut cmd = g.device.create_command_encoder(&Default::default());
    run_stage(
        g,
        &mut cmd,
        &Stage {
            pipeline: &eng.lin,
            wg: 256,
            total: (levels[0].w * levels[0].h) as u64,
            n_items: 1,
            fone: false,
            bufs: &[
                (1, &lin_items),
                (4, &zero_prefix),
                (5, &base_buf),
                (6, &pyr_buf),
            ],
        },
    );
    for i in 1..levels.len() {
        let src = &levels[i - 1];
        let dst = &levels[i];
        let item = storage_init(
            g,
            "halve-item",
            &halve_item_bytes(src.px_off, dst.px_off, src.w as u32, src.h as u32),
        );
        run_stage(
            g,
            &mut cmd,
            &Stage {
                pipeline: &eng.halve,
                wg: 256,
                total: (dst.w * dst.h) as u64,
                n_items: 1,
                fone: false,
                bufs: &[(3, &item), (4, &zero_prefix), (6, &pyr_buf)],
            },
        );
    }
    for l in &levels {
        let blocks_l = storage_empty(g, "blocks-level", l.nb * 64, ::wgpu::BufferUsages::empty());
        let scratch_l = storage_empty(
            g,
            "plan-scratch-level",
            l.nb * PLAN_STRIDE as u64 * 4,
            ::wgpu::BufferUsages::empty(),
        );
        let out_l = storage_empty(
            g,
            "bc7-out-level",
            l.nb * 16,
            ::wgpu::BufferUsages::COPY_SRC,
        );
        let pack_item = storage_init(
            g,
            "pack-item",
            &pack_item_bytes(l.px_off, 0, l.w as u32, l.h as u32, srgb),
        );
        run_stage(
            g,
            &mut cmd,
            &Stage {
                pipeline: &eng.pack,
                wg: 256,
                total: l.nb,
                n_items: 1,
                fone: false,
                bufs: &[
                    (2, &pack_item),
                    (4, &zero_prefix),
                    (6, &pyr_buf),
                    (7, &blocks_l),
                ],
            },
        );
        for pipe in &eng.plan {
            run_stage(
                g,
                &mut cmd,
                &Stage {
                    pipeline: pipe,
                    wg: 64,
                    total: l.nb.div_ceil(4),
                    n_items: l.nb as u32,
                    fone: true,
                    bufs: &[(1, params_buf), (4, &blocks_l), (3, &scratch_l)],
                },
            );
        }
        for pipe in &eng.enc {
            run_stage(
                g,
                &mut cmd,
                &Stage {
                    pipeline: pipe,
                    wg: 64,
                    total: l.nb,
                    n_items: l.nb as u32,
                    fone: true,
                    bufs: &[
                        (1, params_buf),
                        (2, &eng.opt),
                        (4, &blocks_l),
                        (5, &scratch_l),
                        (3, &out_l),
                    ],
                },
            );
        }
        cmd.copy_buffer_to_buffer(&out_l, 0, &staging, l.blk_off * 16, l.nb * 16);
    }
    g.queue.submit([cmd.finish()]);
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(::wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    g.device
        .poll(::wgpu::PollType::wait_indefinitely())
        .map_err(|e| anyhow!("wgpu device poll failed: {e:?}"))?;
    rx.recv()
        .map_err(|_| anyhow!("wgpu map_async callback dropped"))?
        .map_err(|e| anyhow!("wgpu readback map failed: {e:?}"))?;
    let out = slice
        .get_mapped_range()
        .map_err(|e| anyhow!("wgpu mapped range failed: {e:?}"))?
        .to_vec();
    staging.unmap();
    Ok((out, mips))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod testsup;
