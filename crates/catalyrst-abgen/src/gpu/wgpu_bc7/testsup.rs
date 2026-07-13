use crate::gpu::wgpu::{adapter_summary, gpu, Gpu};

pub(crate) const WG: u64 = 256;

pub(crate) fn require() -> bool {
    std::env::var("ABGEN_GPU_REQUIRE_WGPU").as_deref() == Ok("1")
}

pub(crate) fn gpu_or_skip(test: &str) -> Option<&'static Gpu> {
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

pub(crate) fn prepare_kernel(
    g: &Gpu,
    wgsl: &str,
    label: &str,
    entry: &str,
) -> ::wgpu::ComputePipeline {
    prepare_kernel_const(g, wgsl, label, entry, &[])
}

pub(crate) fn prepare_kernel_const(
    g: &Gpu,
    wgsl: &str,
    label: &str,
    entry: &str,
    constants: &[(&str, f64)],
) -> ::wgpu::ComputePipeline {
    let module = g
        .device
        .create_shader_module(::wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: ::wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
    g.device
        .create_compute_pipeline(&::wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: ::wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            cache: None,
        })
}

pub(crate) fn run_kernel(
    g: &Gpu,
    wgsl: &str,
    label: &str,
    entry: &str,
    total: u32,
    n_items: u32,
    storages: &[(u32, &[u8])],
    readback: u32,
) -> Vec<u8> {
    let pipeline = prepare_kernel(g, wgsl, label, entry);
    dispatch_prepared(g, &pipeline, total, n_items, storages, readback)
}

pub(crate) fn dispatch_prepared(
    g: &Gpu,
    pipeline: &::wgpu::ComputePipeline,
    total: u32,
    n_items: u32,
    storages: &[(u32, &[u8])],
    readback: u32,
) -> Vec<u8> {
    dispatch_prepared_wg(g, pipeline, total, n_items, storages, readback, WG)
}

pub(crate) fn dispatch_prepared_wg(
    g: &Gpu,
    pipeline: &::wgpu::ComputePipeline,
    total: u32,
    n_items: u32,
    storages: &[(u32, &[u8])],
    readback: u32,
    wg: u64,
) -> Vec<u8> {
    use ::wgpu::util::DeviceExt;
    let mut meta = Vec::with_capacity(16);
    for v in [n_items, total, 0u32, 1.0f32.to_bits()] {
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((total as u64).div_ceil(wg) as u32, 1, 1);
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

pub(crate) fn assert_bytes_eq(got: &[u8], want: &[u8], ctx: &str) {
    assert_eq!(got.len(), want.len(), "{ctx}: length mismatch");
    if let Some(i) = got.iter().zip(want.iter()).position(|(a, b)| a != b) {
        panic!(
            "{ctx}: first divergence at byte {i}: got {:#04x} want {:#04x}",
            got[i], want[i]
        );
    }
}
