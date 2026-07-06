use std::future::Future;
use std::sync::OnceLock;
use std::task::{Context, Poll};

pub(crate) const BLOCKIFY_WGSL: &str = include_str!("shaders/blockify.wgsl");

pub(crate) struct Gpu {
    pub(crate) device: ::wgpu::Device,
    pub(crate) queue: ::wgpu::Queue,
    pub(crate) info: ::wgpu::AdapterInfo,
}

static GPU: OnceLock<Result<Gpu, String>> = OnceLock::new();

pub(crate) fn block_on_now<F: Future>(fut: F) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn adapter_score(info: &::wgpu::AdapterInfo) -> i32 {
    let mut s = match info.device_type {
        ::wgpu::DeviceType::DiscreteGpu => 8,
        ::wgpu::DeviceType::IntegratedGpu => 4,
        ::wgpu::DeviceType::VirtualGpu => 2,
        ::wgpu::DeviceType::Other => 1,
        ::wgpu::DeviceType::Cpu => 0,
    };
    if info.name.to_ascii_uppercase().contains("NVIDIA") {
        s += 1;
    }
    s
}

fn init() -> Result<Gpu, String> {
    let instance = ::wgpu::Instance::new(::wgpu::InstanceDescriptor::new_without_display_handle());
    let adapters = block_on_now(instance.enumerate_adapters(::wgpu::Backends::all()));
    let mut best: Option<(i32, ::wgpu::Adapter)> = None;
    for a in adapters {
        let s = adapter_score(&a.get_info());
        if best.as_ref().map(|(bs, _)| s > *bs).unwrap_or(true) {
            best = Some((s, a));
        }
    }
    let adapter = best
        .map(|(_, a)| a)
        .ok_or_else(|| String::from("no wgpu adapter"))?;
    let info = adapter.get_info();
    let (device, queue) = block_on_now(adapter.request_device(&::wgpu::DeviceDescriptor {
        label: Some("abgen-gpu-wgpu"),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .map_err(|e| format!("wgpu request_device failed on {}: {e}", info.name))?;
    Ok(Gpu {
        device,
        queue,
        info,
    })
}

pub(crate) fn gpu() -> Result<&'static Gpu, String> {
    match GPU.get_or_init(init) {
        Ok(g) => Ok(g),
        Err(e) => Err(e.clone()),
    }
}

pub(crate) fn adapter_summary() -> Result<String, String> {
    let g = gpu()?;
    Ok(format!(
        "{} [{:?} {:?}] driver: {} {}",
        g.info.name, g.info.backend, g.info.device_type, g.info.driver, g.info.driver_info
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::corelib::mips;
    use crate::gpuhost::oracle::gen_texture;

    const SIZES: [(u32, u32); 3] = [(64, 64), (128, 32), (37, 53)];
    const SEEDS: [u64; 2] = [1, 7];
    const WG: u64 = 256;

    fn require() -> bool {
        std::env::var("ABGEN_GPU_REQUIRE_WGPU").as_deref() == Ok("1")
    }

    fn gpu_or_skip(test: &str) -> Option<&'static Gpu> {
        match gpu() {
            Ok(g) => {
                eprintln!(
                    "{test}: wgpu adapter: {}",
                    adapter_summary().expect("summary after init")
                );
                Some(g)
            }
            Err(e) => {
                if require() {
                    panic!("no wgpu adapter (ABGEN_GPU_REQUIRE_WGPU=1): {e}");
                }
                eprintln!("{test}: SKIP no wgpu adapter: {e}");
                None
            }
        }
    }

    fn run_kernel(
        g: &Gpu,
        entry: &str,
        total: u32,
        n_items: u32,
        storages: &[(u32, &[u8])],
        readback: u32,
    ) -> Vec<u8> {
        use ::wgpu::util::DeviceExt;
        let module = g
            .device
            .create_shader_module(::wgpu::ShaderModuleDescriptor {
                label: Some("blockify"),
                source: ::wgpu::ShaderSource::Wgsl(BLOCKIFY_WGSL.into()),
            });
        let pipeline = g
            .device
            .create_compute_pipeline(&::wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
        let mut meta = Vec::with_capacity(16);
        for v in [n_items, total, 0u32, 0u32] {
            meta.extend_from_slice(&v.to_le_bytes());
        }
        let meta_buf = g
            .device
            .create_buffer_init(&::wgpu::util::BufferInitDescriptor {
                label: Some("meta"),
                contents: &meta,
                usage: ::wgpu::BufferUsages::UNIFORM,
            });
        let bufs: Vec<(u32, ::wgpu::Buffer)> = storages
            .iter()
            .map(|(binding, data)| {
                (
                    *binding,
                    g.device
                        .create_buffer_init(&::wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: data,
                            usage: ::wgpu::BufferUsages::STORAGE | ::wgpu::BufferUsages::COPY_SRC,
                        }),
                )
            })
            .collect();
        let layout = pipeline.get_bind_group_layout(0);
        let mut entries = vec![::wgpu::BindGroupEntry {
            binding: 0,
            resource: meta_buf.as_entire_binding(),
        }];
        for (binding, buf) in &bufs {
            entries.push(::wgpu::BindGroupEntry {
                binding: *binding,
                resource: buf.as_entire_binding(),
            });
        }
        let bind_group = g.device.create_bind_group(&::wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &entries,
        });
        let (_, rb_buf) = bufs
            .iter()
            .find(|(b, _)| *b == readback)
            .expect("readback binding present");
        let rb_size = storages
            .iter()
            .find(|(b, _)| *b == readback)
            .expect("readback binding present")
            .1
            .len() as u64;
        let staging = g.device.create_buffer(&::wgpu::BufferDescriptor {
            label: Some("staging"),
            size: rb_size,
            usage: ::wgpu::BufferUsages::MAP_READ | ::wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((total as u64).div_ceil(WG) as u32, 1, 1);
        }
        enc.copy_buffer_to_buffer(rb_buf, 0, &staging, 0, rb_size);
        g.queue.submit([enc.finish()]);
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(::wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        g.device
            .poll(::wgpu::PollType::wait_indefinitely())
            .expect("device poll wait");
        rx.recv().expect("map_async callback").expect("map ok");
        let out = slice.get_mapped_range().expect("mapped range").to_vec();
        staging.unmap();
        out
    }

    fn assert_bytes_eq(got: &[u8], want: &[u8], ctx: &str) {
        assert_eq!(got.len(), want.len(), "{ctx}: length mismatch");
        if let Some(i) = got.iter().zip(want.iter()).position(|(a, b)| a != b) {
            panic!(
                "{ctx}: first divergence at byte {i}: got {:#04x} want {:#04x}",
                got[i], want[i]
            );
        }
    }

    fn lin_cpu(tex: &[u8], srgb: bool) -> Vec<f32> {
        let n = tex.len() / 4;
        let mut out = vec![0f32; n * 4];
        for i in 0..n {
            mips::linearize_pixel(&tex[i * 4..i * 4 + 4], srgb, &mut out[i * 4..i * 4 + 4]);
        }
        out
    }

    fn f32s_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn u64s_bytes(v: &[u64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn lin_item(base_px: u64, pyr_px: u64, srgb: bool) -> Vec<u8> {
        let mut b = Vec::with_capacity(24);
        b.extend_from_slice(&base_px.to_le_bytes());
        b.extend_from_slice(&pyr_px.to_le_bytes());
        b.extend_from_slice(&(srgb as u32).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    fn halve_item(src_px: u64, dst_px: u64, w: u32, h: u32) -> Vec<u8> {
        let mut b = Vec::with_capacity(24);
        b.extend_from_slice(&src_px.to_le_bytes());
        b.extend_from_slice(&dst_px.to_le_bytes());
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b
    }

    fn pack_item(lvl_px: u64, blk_off: u64, w: u32, h: u32, srgb: bool) -> Vec<u8> {
        let mut b = Vec::with_capacity(32);
        b.extend_from_slice(&lvl_px.to_le_bytes());
        b.extend_from_slice(&blk_off.to_le_bytes());
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(&(srgb as u32).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    fn pyramid(tex: &[u8], w: u32, h: u32, srgb: bool) -> Vec<(Vec<f32>, usize, usize)> {
        let mut levels = vec![(lin_cpu(tex, srgb), w as usize, h as usize)];
        loop {
            let (cur, w, h) = levels.last().unwrap();
            if *w == 1 && *h == 1 {
                break;
            }
            let (next, nw, nh) = mips::box_halve(cur, *w, *h);
            levels.push((next, nw, nh));
        }
        levels
    }

    #[test]
    fn wgpu_adapter_reported() {
        let Some(g) = gpu_or_skip("wgpu_adapter_reported") else {
            return;
        };
        if require() {
            assert!(
                g.info.name.to_ascii_uppercase().contains("NVIDIA"),
                "expected NVIDIA adapter, got {} [{:?}]",
                g.info.name,
                g.info.backend
            );
        }
    }

    #[test]
    fn wgpu_blockify_linearize_golden() {
        let Some(g) = gpu_or_skip("wgpu_blockify_linearize_golden") else {
            return;
        };
        for &(w, h) in &SIZES {
            for srgb in [false, true] {
                let texs: Vec<Vec<u8>> = SEEDS.iter().map(|&s| gen_texture(s, w, h)).collect();
                let npx = (w as u64) * (h as u64);
                let mut items = Vec::new();
                let mut prefixes = Vec::new();
                let mut base = Vec::new();
                let mut want = Vec::new();
                for (i, tex) in texs.iter().enumerate() {
                    items.extend(lin_item(i as u64 * npx, i as u64 * npx, srgb));
                    prefixes.push(i as u64 * npx);
                    base.extend_from_slice(tex);
                    want.extend(f32s_bytes(&lin_cpu(tex, srgb)));
                }
                let total = (npx as u32) * texs.len() as u32;
                let pyr = vec![0u8; total as usize * 16];
                let got = run_kernel(
                    g,
                    "blockify_linearize",
                    total,
                    texs.len() as u32,
                    &[
                        (1, &items),
                        (4, &u64s_bytes(&prefixes)),
                        (5, &base),
                        (6, &pyr),
                    ],
                    6,
                );
                assert_bytes_eq(&got, &want, &format!("linearize {w}x{h} srgb={srgb}"));
            }
        }
    }

    #[test]
    fn wgpu_blockify_halve_golden() {
        let Some(g) = gpu_or_skip("wgpu_blockify_halve_golden") else {
            return;
        };
        for &(w0, h0) in &SIZES {
            for srgb in [false, true] {
                let mut curs: Vec<Vec<f32>> = SEEDS
                    .iter()
                    .map(|&s| lin_cpu(&gen_texture(s, w0, h0), srgb))
                    .collect();
                let mut w = w0 as usize;
                let mut h = h0 as usize;
                let mut level = 0usize;
                while w > 1 || h > 1 {
                    let wants: Vec<(Vec<f32>, usize, usize)> =
                        curs.iter().map(|c| mips::box_halve(c, w, h)).collect();
                    let (nw, nh) = (wants[0].1, wants[0].2);
                    let px = (w * h) as u64;
                    let np = (nw * nh) as u64;
                    let nsrc = curs.len() as u64;
                    let mut pyr_f: Vec<f32> = Vec::new();
                    let mut items = Vec::new();
                    let mut prefixes = Vec::new();
                    for (i, cur) in curs.iter().enumerate() {
                        pyr_f.extend_from_slice(cur);
                        items.extend(halve_item(
                            i as u64 * px,
                            nsrc * px + i as u64 * np,
                            w as u32,
                            h as u32,
                        ));
                        prefixes.push(i as u64 * np);
                    }
                    pyr_f.extend(std::iter::repeat_n(0f32, (nsrc * np) as usize * 4));
                    let total = (np * nsrc) as u32;
                    let got = run_kernel(
                        g,
                        "blockify_halve",
                        total,
                        curs.len() as u32,
                        &[
                            (3, &items),
                            (4, &u64s_bytes(&prefixes)),
                            (6, &f32s_bytes(&pyr_f)),
                        ],
                        6,
                    );
                    for (i, (want, _, _)) in wants.iter().enumerate() {
                        let off = ((nsrc * px + i as u64 * np) * 4 * 4) as usize;
                        assert_bytes_eq(
                            &got[off..off + want.len() * 4],
                            &f32s_bytes(want),
                            &format!(
                                "halve {w0}x{h0} srgb={srgb} level={level} seed={}",
                                SEEDS[i]
                            ),
                        );
                    }
                    curs = wants.into_iter().map(|(v, _, _)| v).collect();
                    w = nw;
                    h = nh;
                    level += 1;
                }
            }
        }
    }

    #[test]
    fn wgpu_blockify_quantize_pack_golden() {
        let Some(g) = gpu_or_skip("wgpu_blockify_quantize_pack_golden") else {
            return;
        };
        for &(w0, h0) in &SIZES {
            for srgb in [false, true] {
                let mut pyr_f: Vec<f32> = Vec::new();
                let mut items = Vec::new();
                let mut prefixes = Vec::new();
                let mut want = Vec::new();
                let mut lvl_px = 0u64;
                let mut blk_off = 0u64;
                let mut total_blocks = 0u64;
                let mut n_items = 0u32;
                for &seed in &SEEDS {
                    let tex = gen_texture(seed, w0, h0);
                    for (level, w, h) in pyramid(&tex, w0, h0, srgb) {
                        let (bw, bh) = mips::level_block_dims(w, h);
                        items.extend(pack_item(lvl_px, blk_off, w as u32, h as u32, srgb));
                        prefixes.push(total_blocks);
                        for by in 0..bh {
                            for bx in 0..bw {
                                let mut blk = [0u8; 64];
                                mips::quantize_pack_block(&level, w, h, srgb, bx, by, &mut blk);
                                want.extend_from_slice(&blk);
                            }
                        }
                        pyr_f.extend_from_slice(&level);
                        lvl_px += (w * h) as u64;
                        blk_off += (bw * bh) as u64;
                        total_blocks += (bw * bh) as u64;
                        n_items += 1;
                    }
                }
                let blocks = vec![0u8; total_blocks as usize * 64];
                let got = run_kernel(
                    g,
                    "blockify_quantize_pack",
                    total_blocks as u32,
                    n_items,
                    &[
                        (2, &items),
                        (4, &u64s_bytes(&prefixes)),
                        (6, &f32s_bytes(&pyr_f)),
                        (7, &blocks),
                    ],
                    7,
                );
                assert_bytes_eq(&got, &want, &format!("quantize_pack {w0}x{h0} srgb={srgb}"));
            }
        }
    }
}
