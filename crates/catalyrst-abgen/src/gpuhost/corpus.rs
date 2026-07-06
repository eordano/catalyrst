use anyhow::{anyhow, bail, ensure, Result};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use crate::glbscan::{file_ext_lower, scan_entity, UriCache};
use crate::gpu::corelib::bc7::{Bc7Profile, OptTables, Params};
use crate::gpu::corelib::mips::{
    box_halve, linearize_pixel, pad_to_block_size, quantize_pixel, scanline_to_blocks,
};
use crate::gpu::tex_geometry;
use crate::local_store::LocalContentStore;

const DEFAULT_STORE: &str = "./contents";
const WINDOW: usize = 512;
const SCAN_AHEAD: usize = 4;
const MAX_INFLIGHT: u64 = 2048;
const MAX_BLOCKS_PER_LAUNCH: usize = 64_000_000;
const CPU_PAR_CHUNK_BLOCKS: usize = 4096;
const BUCKET_NAMES: [&str; 4] = ["slow", "slow_perceptual", "basic", "basic_perceptual"];

struct Flags {
    entities: String,
    limit: Option<usize>,
    slab_gb: f64,
    jobs: usize,
    cpu: bool,
    gpu_blockify: bool,
    store: String,
    queue: usize,
    timeline: Option<String>,
    scan_cache: Option<String>,
}

struct ScanCache {
    map: HashMap<String, Vec<(String, u8)>>,
    writer: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl ScanCache {
    fn open(path: &str) -> Result<ScanCache> {
        let mut map = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                let v: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let e = v.get("e").and_then(|x| x.as_str());
                let c = v.get("c").and_then(|x| x.as_array());
                let (e, c) = match (e, c) {
                    (Some(e), Some(c)) => (e, c),
                    _ => continue,
                };
                let mut list = Vec::with_capacity(c.len());
                let mut ok = true;
                for it in c {
                    match (
                        it.get(0).and_then(|x| x.as_str()),
                        it.get(1).and_then(|x| x.as_u64()),
                    ) {
                        (Some(h), Some(b)) => list.push((h.to_string(), b as u8)),
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    map.insert(e.to_string(), list);
                }
            }
        }
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| anyhow!("open scan cache {path}: {e}"))?;
        Ok(ScanCache {
            map,
            writer: std::sync::Mutex::new(std::io::BufWriter::new(f)),
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        })
    }
    fn append(&self, entity_id: &str, list: &[(String, u8)]) {
        use std::io::Write;
        let c: Vec<serde_json::Value> = list
            .iter()
            .map(|(h, b)| serde_json::json!([h, b]))
            .collect();
        let line = serde_json::json!({"e": entity_id, "c": c});
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "{line}");
        }
    }
    fn flush(&self) {
        use std::io::Write;
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.flush();
        }
    }
}

fn task_from_bits(hash: &str, bits: u8) -> TexTask {
    let model_ref = bits & 1 != 0;
    let is_normal = bits & 2 != 0;
    let linear = bits & 4 != 0;
    let profile = if model_ref {
        Bc7Profile::Slow
    } else {
        Bc7Profile::Basic
    };
    let color_space: i64 = if linear { 0 } else { 1 };
    let srgb = color_space == 1;
    let perceptual = srgb && !is_normal;
    TexTask {
        hash: hash.to_string(),
        bucket: bucket_index(profile, perceptual),
        srgb,
        is_normal,
        color_space,
    }
}

struct Timeline {
    t0: Instant,
    f: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
}

impl Timeline {
    fn create(path: &str) -> Result<std::sync::Arc<Timeline>> {
        use std::io::Write;
        let mut w = std::io::BufWriter::new(
            std::fs::File::create(path).map_err(|e| anyhow!("create {path}: {e}"))?,
        );
        writeln!(w, "t_s,event,a,b")?;
        Ok(std::sync::Arc::new(Timeline {
            t0: Instant::now(),
            f: std::sync::Mutex::new(w),
        }))
    }
    fn ev(&self, event: &str, a: u64, b: u64) {
        use std::io::Write;
        let t = self.t0.elapsed().as_secs_f64();
        if let Ok(mut w) = self.f.lock() {
            let _ = writeln!(w, "{t:.3},{event},{a},{b}");
        }
    }
    fn flush(&self) {
        use std::io::Write;
        if let Ok(mut w) = self.f.lock() {
            let _ = w.flush();
        }
    }
}

#[derive(Default)]
struct Metrics {
    entities_done: AtomicUsize,
    entities_failed: AtomicUsize,
    texture_refs: AtomicUsize,
    textures_unique: AtomicUsize,
    skipped_uncompressed: AtomicUsize,
    decode_failed: AtomicUsize,
    missing_content: AtomicUsize,
    src_pixels: AtomicU64,
    io_ns: AtomicU64,
    scan_ns: AtomicU64,
    decode_ns: AtomicU64,
    blockify_ns: AtomicU64,
}

#[derive(Default)]
struct EncodeState {
    fingerprint: u64,
    gpu_ns: u64,
    cpu_ns: u64,
    gpu_init_ns: u64,
    gpu_blockify_ns: u64,
    launches: u64,
    flushes: u64,
}

enum Slab {
    Blocks(Box<[Vec<u8>; 4]>),
    Texs(Vec<crate::gpu::BlockifyTex>),
}

#[derive(Default)]
struct EntityOut {
    blocks: [Vec<u8>; 4],
    texs: Vec<crate::gpu::BlockifyTex>,
    nblocks: [u64; 4],
    dev_bytes: u64,
}

fn parse_corpus_flags(args: &[String]) -> Result<Flags> {
    let mut entities: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut slab_gb = 20.0f64;
    let mut jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    let mut cpu = false;
    let mut gpu_blockify = false;
    let mut store = DEFAULT_STORE.to_string();
    let mut queue = 1usize;
    let mut timeline: Option<String> = None;
    let mut scan_cache: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--cpu" => {
                cpu = true;
            }
            "--gpu-blockify" => {
                gpu_blockify = true;
            }
            "--entities" | "--limit" | "--slab-gb" | "--jobs" | "--store" | "--queue"
            | "--timeline" | "--scan-cache" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| anyhow!("{flag} needs a value"))?;
                match flag {
                    "--entities" => entities = Some(v.clone()),
                    "--limit" => {
                        limit = Some(v.parse().map_err(|_| anyhow!("bad --limit value: {v}"))?)
                    }
                    "--slab-gb" => {
                        slab_gb = v.parse().map_err(|_| anyhow!("bad --slab-gb value: {v}"))?
                    }
                    "--jobs" => jobs = v.parse().map_err(|_| anyhow!("bad --jobs value: {v}"))?,
                    "--store" => store = v.clone(),
                    "--queue" => {
                        queue = v.parse().map_err(|_| anyhow!("bad --queue value: {v}"))?
                    }
                    "--timeline" => timeline = Some(v.clone()),
                    "--scan-cache" => scan_cache = Some(v.clone()),
                    _ => unreachable!(),
                }
            }
            other => bail!("unknown flag: {other}"),
        }
        i += 1;
    }
    ensure!(jobs > 0, "--jobs must be > 0");
    ensure!(slab_gb > 0.0, "--slab-gb must be > 0");
    ensure!(queue > 0, "--queue must be > 0");
    ensure!(
        !(cpu && gpu_blockify),
        "--gpu-blockify needs the gpu encode path (drop --cpu)"
    );
    Ok(Flags {
        entities: entities.ok_or_else(|| anyhow!("corpus requires --entities <file>"))?,
        limit,
        slab_gb,
        jobs,
        cpu,
        gpu_blockify,
        store,
        queue,
        timeline,
        scan_cache,
    })
}

fn bucket_index(profile: Bc7Profile, perceptual: bool) -> usize {
    let p = match profile {
        Bc7Profile::Slow => 0usize,
        Bc7Profile::Basic => 1usize,
    };
    p * 2 + perceptual as usize
}

fn bucket_params() -> [Params; 4] {
    [
        Params::slow(false),
        Params::slow(true),
        Params::basic(false),
        Params::basic(true),
    ]
}

fn detect_container(raw: &[u8]) -> String {
    if raw.len() >= 8 && &raw[0..8] == b"\x89PNG\r\n\x1a\n" {
        "PNG".to_string()
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        "JPEG".to_string()
    } else {
        String::new()
    }
}

fn pack_normal_map(rgba: &[u8]) -> Vec<u8> {
    let n = rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        out[i * 4] = 255;
        out[i * 4 + 1] = g;
        out[i * 4 + 2] = g;
        out[i * 4 + 3] = r;
    }
    out
}

fn decode_image(raw: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        if let Ok((rgba, w, h)) = crate::ffi::decode_jpeg_rgba_box(raw) {
            if rgba.len() == (w as usize) * (h as usize) * 4 {
                return Some((rgba, w, h));
            }
        }
    }
    let img = image::load_from_memory(raw).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

fn flip_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut flipped = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = &rgba[(h - 1 - y) * w * 4..(h - 1 - y) * w * 4 + w * 4];
        flipped[y * w * 4..y * w * 4 + w * 4].copy_from_slice(src);
    }
    flipped
}

fn blockify_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: i32,
    srgb: bool,
    out: &mut Vec<u8>,
) -> usize {
    let w = width as usize;
    let h = height as usize;
    assert_eq!(rgba.len(), w * h * 4);
    let flipped = flip_rgba(rgba, width, height);

    let mut cur: Vec<f32> = vec![0f32; w * h * 4];
    for i in 0..(w * h) {
        linearize_pixel(&flipped[i * 4..i * 4 + 4], srgb, &mut cur[i * 4..i * 4 + 4]);
    }
    let mut cw = w;
    let mut ch = h;

    let mut total = 0usize;
    for m in 0..mip_count {
        let mut level = vec![0u8; cw * ch * 4];
        for i in 0..(cw * ch) {
            quantize_pixel(&cur[i * 4..i * 4 + 4], srgb, &mut level[i * 4..i * 4 + 4]);
        }
        let (padded, pw, ph) = pad_to_block_size(&level, cw, ch);
        let (blocks, n) = scanline_to_blocks(&padded, pw, ph);
        out.extend_from_slice(&blocks);
        total += n;
        if m < mip_count - 1 {
            let (next, nw, nh) = box_halve(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    total
}

struct TexTask {
    hash: String,
    bucket: usize,
    srgb: bool,
    is_normal: bool,
    color_space: i64,
}

fn scan_entity_tasks(
    store: &LocalContentStore,
    cache: &UriCache,
    m: &Metrics,
    entity_id: &str,
    scache: Option<&ScanCache>,
) -> Vec<TexTask> {
    let mut tasks: Vec<TexTask> = Vec::new();

    let t0 = Instant::now();
    let parsed: Option<serde_json::Value> = store
        .fetch_mmap(entity_id)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw[..]).ok());
    let v = match parsed {
        Some(v) => v,
        None => {
            m.io_ns
                .fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
            m.entities_failed.fetch_add(1, Ordering::Relaxed);
            return tasks;
        }
    };
    let content: Vec<(String, String)> = v
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let f = c.get("file")?.as_str()?.to_string();
                    let h = c.get("hash")?.as_str()?.to_string();
                    Some((f, h))
                })
                .collect()
        })
        .unwrap_or_default();
    let content_by_file: HashMap<String, String> = content
        .iter()
        .map(|(f, h)| (f.to_lowercase(), h.clone()))
        .collect();
    m.io_ns
        .fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let candidates: Vec<&(String, String)> = content
        .iter()
        .filter(|(f, _)| matches!(file_ext_lower(f).as_str(), ".png" | ".jpg" | ".jpeg"))
        .collect();
    m.texture_refs
        .fetch_add(candidates.len(), Ordering::Relaxed);
    if candidates.is_empty() {
        m.entities_done.fetch_add(1, Ordering::Relaxed);
        return tasks;
    }

    if let Some(sc) = scache {
        if let Some(list) = sc.map.get(entity_id) {
            sc.hits.fetch_add(1, Ordering::Relaxed);
            for (hash, bits) in list {
                tasks.push(task_from_bits(hash, *bits));
            }
            m.entities_done.fetch_add(1, Ordering::Relaxed);
            return tasks;
        }
        sc.misses.fetch_add(1, Ordering::Relaxed);
    }

    let t = Instant::now();
    let scan = scan_entity(store, &content_by_file, cache);
    m.scan_ns
        .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let mut list: Vec<(String, u8)> = Vec::with_capacity(candidates.len());
    for (_file, hash) in candidates {
        let mut bits = 0u8;
        if scan.model_refs.contains(hash) {
            bits |= 1;
        }
        if scan.normal_refs.contains(hash) {
            bits |= 2;
        }
        if scan.linear_refs.contains(hash) {
            bits |= 4;
        }
        list.push((hash.clone(), bits));
    }
    for (hash, bits) in &list {
        tasks.push(task_from_bits(hash, *bits));
    }
    if let Some(sc) = scache {
        sc.append(entity_id, &list);
    }
    m.entities_done.fetch_add(1, Ordering::Relaxed);
    tasks
}

fn process_texture(
    store: &LocalContentStore,
    m: &Metrics,
    task: &TexTask,
    gpu_blockify: bool,
) -> EntityOut {
    let mut out = EntityOut::default();
    let bi = task.bucket;

    let t = Instant::now();
    let raw = match store.fetch_mmap(&task.hash) {
        Ok(r) => r,
        Err(_) => {
            m.decode_ns
                .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
            m.missing_content.fetch_add(1, Ordering::Relaxed);
            return out;
        }
    };
    let decoded = decode_image(&raw);
    let container = detect_container(&raw);
    m.decode_ns
        .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let (mut rgba, w, h) = match decoded {
        Some(d) => d,
        None => {
            m.decode_failed.fetch_add(1, Ordering::Relaxed);
            return out;
        }
    };

    let has_real_alpha = rgba.iter().skip(3).step_by(4).any(|&a| a < 255);
    let src = crate::texprofile::SourceImage {
        width: w,
        height: h,
        container,
        has_real_alpha,
    };
    let prof = crate::texprofile::bc7_profile(
        &src,
        task.color_space,
        task.is_normal,
        crate::texprofile::max_texture_size_for("mac"),
    );
    if !prof.compressed {
        m.skipped_uncompressed.fetch_add(1, Ordering::Relaxed);
        return out;
    }
    m.src_pixels
        .fetch_add((w as u64) * (h as u64), Ordering::Relaxed);

    let t = Instant::now();
    if has_real_alpha {
        crate::alpha_bleed::alpha_bleed_inplace(&mut rgba, w, h);
    }
    let (rgba, tw, th) = if (prof.target_w, prof.target_h) != (w, h) {
        (
            crate::resize::box_downscale_rgba(
                &rgba,
                w as usize,
                h as usize,
                prof.target_w as usize,
                prof.target_h as usize,
            ),
            prof.target_w,
            prof.target_h,
        )
    } else {
        (rgba, w, h)
    };
    let rgba = if task.is_normal {
        pack_normal_map(&rgba)
    } else {
        rgba
    };
    let (dev_bytes, nb) = tex_geometry(tw, th, prof.mip_count);
    out.dev_bytes += dev_bytes;
    if gpu_blockify {
        let flipped = flip_rgba(&rgba, tw, th);
        out.nblocks[bi] += nb;
        out.texs.push(crate::gpu::BlockifyTex {
            rgba: flipped,
            w: tw,
            h: th,
            mip_count: prof.mip_count,
            srgb: task.srgb,
            bucket: bi,
        });
    } else {
        let n = blockify_mip_chain(
            &rgba,
            tw,
            th,
            prof.mip_count,
            task.srgb,
            &mut out.blocks[bi],
        );
        out.nblocks[bi] += n as u64;
    }
    m.blockify_ns
        .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
    out
}

fn flush_pending_cpu(
    pending: &mut [Vec<u8>; 4],
    params: &[Params; 4],
    tables: &OptTables,
    pool: &rayon::ThreadPool,
    st: &mut EncodeState,
) -> Result<()> {
    let mut any = false;
    for i in 0..4 {
        let n_total = pending[i].len() / 64;
        if n_total == 0 {
            continue;
        }
        any = true;
        let mut off = 0usize;
        while off < n_total {
            let n = (n_total - off).min(MAX_BLOCKS_PER_LAUNCH);
            let slice = &pending[i][off * 64..(off + n) * 64];
            let t = Instant::now();
            let outs: Vec<Vec<u8>> = pool.install(|| {
                slice
                    .par_chunks(CPU_PAR_CHUNK_BLOCKS * 64)
                    .map(|c| {
                        crate::gpu::corelib::bc7::encode_blocks(c, c.len() / 64, &params[i], tables)
                    })
                    .collect()
            });
            st.cpu_ns += t.elapsed().as_nanos() as u64;
            let out_len: usize = outs.iter().map(|v| v.len()).sum();
            ensure!(
                out_len == n * 16,
                "cpu encode returned {out_len} bytes for {n} blocks (bucket {})",
                BUCKET_NAMES[i]
            );
            st.fingerprint ^= u64::from_le_bytes(outs[0][0..8].try_into().unwrap());
            st.launches += 1;
            off += n;
        }
        pending[i].clear();
    }
    if any {
        st.flushes += 1;
    }
    Ok(())
}

struct SinkCtx<'a> {
    gpu_blockify: bool,
    slab_limit_dev_bytes: f64,
    params: &'a [Params; 4],
    pool: &'a rayon::ThreadPool,
    cpu_tables: &'a Option<Box<OptTables>>,
    tx: Option<&'a mpsc::SyncSender<Slab>>,
    tl: &'a Option<std::sync::Arc<Timeline>>,
    pending_blocks: [Vec<u8>; 4],
    pending_texs: Vec<crate::gpu::BlockifyTex>,
    pending_dev: u64,
    blocks_by_bucket: [u64; 4],
    st: EncodeState,
    slab_seq: u64,
    send_failed: bool,
    cpu_err: Option<anyhow::Error>,
}

impl SinkCtx<'_> {
    fn push(&mut self, out: EntityOut) {
        for i in 0..4 {
            self.blocks_by_bucket[i] += out.nblocks[i];
        }
        self.pending_dev += out.dev_bytes;
        if self.gpu_blockify {
            self.pending_texs.extend(out.texs);
        } else {
            for (i, v) in out.blocks.into_iter().enumerate() {
                self.pending_blocks[i].extend_from_slice(&v);
            }
        }
        if self.pending_dev as f64 >= self.slab_limit_dev_bytes {
            self.pending_dev = 0;
            self.flush(false);
        }
    }
    fn flush(&mut self, fin: bool) {
        if let Some(tables) = self.cpu_tables {
            if let Err(e) = flush_pending_cpu(
                &mut self.pending_blocks,
                self.params,
                tables,
                self.pool,
                &mut self.st,
            ) {
                self.cpu_err = Some(e);
            }
        } else {
            let slab = if self.gpu_blockify {
                Slab::Texs(std::mem::take(&mut self.pending_texs))
            } else {
                Slab::Blocks(Box::new(std::mem::take(&mut self.pending_blocks)))
            };
            let t_send = Instant::now();
            if self.tx.unwrap().send(slab).is_err() {
                if !fin {
                    self.send_failed = true;
                }
                return;
            }
            if let Some(t) = self.tl {
                t.ev(
                    "slab_send",
                    self.slab_seq,
                    t_send.elapsed().as_millis() as u64,
                );
            }
            self.slab_seq += 1;
        }
    }
}

fn gpu_worker(
    rx: mpsc::Receiver<Slab>,
    tl: Option<std::sync::Arc<Timeline>>,
) -> Result<EncodeState> {
    let tables = crate::gpu::corelib::bc7::build_opt_tables();
    let params = bucket_params();
    let mut st = EncodeState::default();
    let t = Instant::now();
    let warm = vec![0u8; 64 * 64];
    let out = crate::gpu::encode_blocks_gpu(&warm, 64, &params[0], &tables)?;
    ensure!(
        out.len() == 64 * 16,
        "gpu warmup returned {} bytes",
        out.len()
    );
    st.gpu_init_ns = t.elapsed().as_nanos() as u64;
    let mut engine: Option<crate::gpu::SlabEngine> = None;
    let mut slab_idx = 0u64;
    while let Ok(slab) = rx.recv() {
        if let Some(t) = &tl {
            t.ev("gpu_slab_start", slab_idx, 0);
        }
        match slab {
            Slab::Blocks(pending) => {
                let mut any = false;
                for i in 0..4 {
                    let n_total = pending[i].len() / 64;
                    if n_total == 0 {
                        continue;
                    }
                    any = true;
                    let mut off = 0usize;
                    while off < n_total {
                        let n = (n_total - off).min(MAX_BLOCKS_PER_LAUNCH);
                        let slice = &pending[i][off * 64..(off + n) * 64];
                        let t = Instant::now();
                        let out = crate::gpu::encode_blocks_gpu(slice, n, &params[i], &tables)?;
                        st.gpu_ns += t.elapsed().as_nanos() as u64;
                        ensure!(
                            out.len() == n * 16,
                            "gpu encode returned {} bytes for {n} blocks (bucket {})",
                            out.len(),
                            BUCKET_NAMES[i]
                        );
                        st.fingerprint ^= u64::from_le_bytes(out[0..8].try_into().unwrap());
                        st.launches += 1;
                        off += n;
                    }
                }
                if any {
                    st.flushes += 1;
                }
            }
            Slab::Texs(texs) => {
                if texs.is_empty() {
                    continue;
                }
                if engine.is_none() {
                    engine = Some(crate::gpu::SlabEngine::new(&params, &tables)?);
                }
                if let Some(stats) = engine
                    .as_mut()
                    .unwrap()
                    .submit_slab(&texs, MAX_BLOCKS_PER_LAUNCH)?
                {
                    st.fingerprint ^= stats.fingerprint;
                    st.launches += stats.launches;
                    st.gpu_ns += stats.encode_ns;
                    st.gpu_blockify_ns += stats.blockify_ns;
                    if stats.launches > 0 {
                        st.flushes += 1;
                    }
                }
            }
        }
        if let Some(t) = &tl {
            t.ev("gpu_slab_end", slab_idx, st.launches);
        }
        slab_idx += 1;
    }
    if let Some(eng) = engine.as_mut() {
        if let Some(stats) = eng.finish()? {
            st.fingerprint ^= stats.fingerprint;
            st.launches += stats.launches;
            st.gpu_ns += stats.encode_ns;
            st.gpu_blockify_ns += stats.blockify_ns;
            if stats.launches > 0 {
                st.flushes += 1;
            }
        }
    }
    Ok(st)
}

pub fn cmd_corpus(args: &[String]) -> Result<i32> {
    let flags = parse_corpus_flags(args)?;
    let text = std::fs::read_to_string(&flags.entities)
        .map_err(|e| anyhow!("read {}: {e}", flags.entities))?;
    let mut ids: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let id = l.split('\t').next().unwrap_or("").trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect();
    if let Some(n) = flags.limit {
        ids.truncate(n);
    }
    println!(
        "corpus: entities={} store={} jobs={} slab_gb={} mode={} gpu_blockify={}",
        ids.len(),
        flags.store,
        flags.jobs,
        flags.slab_gb,
        if flags.cpu { "cpu" } else { "gpu" },
        flags.gpu_blockify
    );

    let store = LocalContentStore::new(flags.store.clone());
    let cache = UriCache::new();
    let params = bucket_params();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(flags.jobs)
        .build()
        .map_err(|e| anyhow!("rayon pool: {e}"))?;
    let metrics = Metrics::default();
    let scache: Option<ScanCache> = match &flags.scan_cache {
        Some(p) => Some(ScanCache::open(p)?),
        None => None,
    };
    let mut seen: HashSet<(String, u8)> = HashSet::new();

    let tl: Option<std::sync::Arc<Timeline>> = match &flags.timeline {
        Some(p) => Some(Timeline::create(p)?),
        None => None,
    };

    let wall = Instant::now();
    let mut cpu_tables: Option<Box<OptTables>> = None;
    let mut tx_opt: Option<mpsc::SyncSender<Slab>> = None;
    let mut worker: Option<std::thread::JoinHandle<Result<EncodeState>>> = None;
    if flags.cpu {
        cpu_tables = Some(crate::gpu::corelib::bc7::build_opt_tables());
    } else {
        let (tx, rx) = mpsc::sync_channel::<Slab>(flags.queue);
        tx_opt = Some(tx);
        let wtl = tl.clone();
        worker = Some(std::thread::spawn(move || gpu_worker(rx, wtl)));
    }

    let slab_limit_dev_bytes = flags.slab_gb * 1e9;
    let mut last_progress = 0usize;
    let gpu_blockify = flags.gpu_blockify;

    let (scan_tx, scan_rx) = mpsc::sync_channel::<(usize, Vec<Vec<TexTask>>)>(SCAN_AHEAD);
    let (done_tx, done_rx) = mpsc::channel::<(u64, EntityOut)>();
    let ids_ref = &ids;
    let store_ref = &store;
    let cache_ref = &cache;
    let metrics_ref = &metrics;
    let pool_ref = &pool;
    let scache_ref = scache.as_ref();
    let tl_scan = tl.clone();

    let (mut st, blocks_by_bucket, cpu_err) = std::thread::scope(|sc| {
        sc.spawn(move || {
            for (widx, window) in ids_ref.chunks(WINDOW).enumerate() {
                if let Some(t) = &tl_scan {
                    t.ev("scan_start", widx as u64, window.len() as u64);
                }
                let tasklists: Vec<Vec<TexTask>> = pool_ref.install(|| {
                    window
                        .par_iter()
                        .map(|id| {
                            scan_entity_tasks(store_ref, cache_ref, metrics_ref, id, scache_ref)
                        })
                        .collect()
                });
                if let Some(t) = &tl_scan {
                    t.ev("scan_end", widx as u64, 0);
                }
                if scan_tx.send((widx, tasklists)).is_err() {
                    return;
                }
            }
        });

        let mut ctx = SinkCtx {
            gpu_blockify,
            slab_limit_dev_bytes,
            params: &params,
            pool: &pool,
            cpu_tables: &cpu_tables,
            tx: tx_opt.as_ref(),
            tl: &tl,
            pending_blocks: Default::default(),
            pending_texs: Vec::new(),
            pending_dev: 0,
            blocks_by_bucket: [0u64; 4],
            st: EncodeState::default(),
            slab_seq: 0,
            send_failed: false,
            cpu_err: None,
        };
        let mut reorder: BTreeMap<u64, EntityOut> = BTreeMap::new();
        let mut dispatched = 0u64;
        let mut released = 0u64;

        pool.in_place_scope(|s| {
            for (widx, tasklists) in scan_rx.iter() {
                if ctx.send_failed || ctx.cpu_err.is_some() {
                    break;
                }
                let mut claimed: Vec<TexTask> = Vec::new();
                for tasks in tasklists {
                    for task in tasks {
                        if seen.insert((task.hash.clone(), task.bucket as u8)) {
                            metrics.textures_unique.fetch_add(1, Ordering::Relaxed);
                            claimed.push(task);
                        }
                    }
                }
                if let Some(t) = &tl {
                    t.ev("claimed", widx as u64, claimed.len() as u64);
                }
                for task in claimed {
                    while dispatched - released >= MAX_INFLIGHT {
                        match done_rx.recv() {
                            Ok((sq, out)) => {
                                reorder.insert(sq, out);
                                while let Some(o) = reorder.remove(&released) {
                                    ctx.push(o);
                                    released += 1;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let seq = dispatched;
                    dispatched += 1;
                    let dtx = done_tx.clone();
                    s.spawn(move |_| {
                        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            process_texture(store_ref, metrics_ref, &task, gpu_blockify)
                        }));
                        match res {
                            Ok(out) => {
                                let _ = dtx.send((seq, out));
                            }
                            Err(p) => {
                                let _ = dtx.send((seq, EntityOut::default()));
                                std::panic::resume_unwind(p);
                            }
                        }
                    });
                }
                while let Ok((sq, out)) = done_rx.try_recv() {
                    reorder.insert(sq, out);
                }
                while let Some(o) = reorder.remove(&released) {
                    ctx.push(o);
                    released += 1;
                }
                let done = metrics.entities_done.load(Ordering::Relaxed)
                    + metrics.entities_failed.load(Ordering::Relaxed);
                if done / 2000 > last_progress {
                    last_progress = done / 2000;
                    println!(
                        "progress: entities={done} textures={} blocks={} elapsed_s={:.1}",
                        metrics.textures_unique.load(Ordering::Relaxed),
                        ctx.blocks_by_bucket.iter().sum::<u64>(),
                        wall.elapsed().as_secs_f64()
                    );
                }
            }
            drop(scan_rx);
            while released < dispatched && !ctx.send_failed && ctx.cpu_err.is_none() {
                match done_rx.recv() {
                    Ok((sq, out)) => {
                        reorder.insert(sq, out);
                        while let Some(o) = reorder.remove(&released) {
                            ctx.push(o);
                            released += 1;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        drop(done_tx);
        if !ctx.send_failed && ctx.cpu_err.is_none() {
            ctx.flush(true);
        }
        (ctx.st, ctx.blocks_by_bucket, ctx.cpu_err)
    });
    if let Some(sc) = &scache {
        sc.flush();
    }
    if let Some(e) = cpu_err {
        return Err(e);
    }
    drop(tx_opt);
    if let Some(h) = worker {
        match h.join() {
            Ok(Ok(ws)) => st = ws,
            Ok(Err(e)) => bail!("gpu worker: {e:#}"),
            Err(_) => bail!("gpu worker panicked"),
        }
    }

    let wall_s = wall.elapsed().as_secs_f64();
    if let Some(t) = &tl {
        t.ev("done", 0, 0);
        t.flush();
    }
    let blocks_total: u64 = blocks_by_bucket.iter().sum();
    let encode_ns = if flags.cpu { st.cpu_ns } else { st.gpu_ns };
    let encode_s = encode_ns as f64 / 1e9;
    let rate = if encode_s > 0.0 {
        blocks_total as f64 / encode_s
    } else {
        0.0
    };
    let sec = |ns: &AtomicU64| ns.load(Ordering::Relaxed) as f64 / 1e9;

    println!();
    println!("== corpus report ==");
    println!(
        "entities: processed={} failed={}",
        metrics.entities_done.load(Ordering::Relaxed),
        metrics.entities_failed.load(Ordering::Relaxed)
    );
    println!(
        "textures: unique={} referenced={} skipped_uncompressed={} decode_failed={} missing_content={}",
        metrics.textures_unique.load(Ordering::Relaxed),
        metrics.texture_refs.load(Ordering::Relaxed),
        metrics.skipped_uncompressed.load(Ordering::Relaxed),
        metrics.decode_failed.load(Ordering::Relaxed),
        metrics.missing_content.load(Ordering::Relaxed)
    );
    println!(
        "source_pixels: {}",
        metrics.src_pixels.load(Ordering::Relaxed)
    );
    println!(
        "blocks: total={} slow={} slow_perceptual={} basic={} basic_perceptual={}",
        blocks_total,
        blocks_by_bucket[0],
        blocks_by_bucket[1],
        blocks_by_bucket[2],
        blocks_by_bucket[3]
    );
    println!(
        "slabs: flushes={} encode_launches={}",
        st.flushes, st.launches
    );
    if let Some(sc) = &scache {
        println!(
            "scan_cache: hits={} misses={}",
            sc.hits.load(Ordering::Relaxed),
            sc.misses.load(Ordering::Relaxed)
        );
    }
    println!(
        "phase_seconds(thread-summed): entity_io={:.3} glb_scan={:.3} image_decode={:.3} blockify={:.3}",
        sec(&metrics.io_ns),
        sec(&metrics.scan_ns),
        sec(&metrics.decode_ns),
        sec(&metrics.blockify_ns)
    );
    println!(
        "encode_seconds: gpu={:.3} cpu={:.3} gpu_init={:.3} gpu_blockify={:.3}",
        st.gpu_ns as f64 / 1e9,
        st.cpu_ns as f64 / 1e9,
        st.gpu_init_ns as f64 / 1e9,
        st.gpu_blockify_ns as f64 / 1e9
    );
    println!("wall_seconds: {wall_s:.3}");
    println!(
        "encode_blocks_per_s: {rate:.0} ({})",
        if flags.cpu { "cpu" } else { "gpu" }
    );
    println!("fingerprint: {:#018x}", st.fingerprint);

    let j = serde_json::json!({
        "mode": if flags.cpu { "cpu" } else { "gpu" },
        "gpu_blockify": flags.gpu_blockify,
        "jobs": flags.jobs,
        "slab_gb": flags.slab_gb,
        "queue": flags.queue,
        "limit": flags.limit,
        "entities_processed": metrics.entities_done.load(Ordering::Relaxed),
        "entities_failed": metrics.entities_failed.load(Ordering::Relaxed),
        "textures_unique": metrics.textures_unique.load(Ordering::Relaxed),
        "texture_refs": metrics.texture_refs.load(Ordering::Relaxed),
        "skipped_uncompressed": metrics.skipped_uncompressed.load(Ordering::Relaxed),
        "decode_failed": metrics.decode_failed.load(Ordering::Relaxed),
        "missing_content": metrics.missing_content.load(Ordering::Relaxed),
        "source_pixels": metrics.src_pixels.load(Ordering::Relaxed),
        "blocks_total": blocks_total,
        "blocks_by_bucket": {
            BUCKET_NAMES[0]: blocks_by_bucket[0],
            BUCKET_NAMES[1]: blocks_by_bucket[1],
            BUCKET_NAMES[2]: blocks_by_bucket[2],
            BUCKET_NAMES[3]: blocks_by_bucket[3],
        },
        "slab_flushes": st.flushes,
        "encode_launches": st.launches,
        "scan_cache": scache.as_ref().map(|sc| serde_json::json!({
            "hits": sc.hits.load(Ordering::Relaxed),
            "misses": sc.misses.load(Ordering::Relaxed),
        })),
        "phase_s": {
            "entity_io": sec(&metrics.io_ns),
            "glb_scan": sec(&metrics.scan_ns),
            "image_decode": sec(&metrics.decode_ns),
            "blockify": sec(&metrics.blockify_ns),
            "gpu_encode": st.gpu_ns as f64 / 1e9,
            "cpu_encode": st.cpu_ns as f64 / 1e9,
            "gpu_init": st.gpu_init_ns as f64 / 1e9,
            "gpu_blockify": st.gpu_blockify_ns as f64 / 1e9,
        },
        "wall_s": wall_s,
        "encode_blocks_per_s": rate,
        "fingerprint": format!("{:#018x}", st.fingerprint),
    });
    println!("{j}");
    Ok(0)
}
