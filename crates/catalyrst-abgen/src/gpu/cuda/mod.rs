use anyhow::{anyhow, bail, ensure, Result};
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

use crate::gpu::corelib::bc7::build_opt_tables;
use crate::gpu::corelib::bc7::Bc7Profile;
use crate::gpu::corelib::bc7::{OptTables, Params, GROUP_WIDTH};
use crate::gpu::corelib::mips::compute_default_mip_count;
use crate::gpu::corelib::mips::{box_halve_dims, level_block_dims, HalveItem, LinItem, PackItem};

type CuResult = i32;
type DevPtr = u64;

fn jit_cache_dir_ok(p: &std::path::Path) -> bool {
    if std::fs::create_dir_all(p).is_err() {
        return false;
    }
    let probe = p.join(format!(".abgen-gpu-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn encode_bdim() -> u32 {
    std::env::var("ABGEN_GPU_BDIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&b| b > 0 && b <= 1024)
        .unwrap_or(256)
}

fn batch_dev_cap() -> u64 {
    std::env::var("ABGEN_GPU_BATCH_DEV_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&b| b > 0)
        .unwrap_or(4_000_000_000)
}

fn encode_binning() -> bool {
    std::env::var("ABGEN_GPU_BINNING")
        .map(|v| v != "0")
        .unwrap_or(true)
}

fn sort_descs_by_sig(descs: &[u64], sigs: &[u8], out: &mut [u64]) {
    let mut hist = vec![0u32; 1024];
    for (i, &d) in descs.iter().enumerate() {
        let key = ((((d >> 4) & 0xf) as usize) << 8) | sigs[i] as usize;
        hist[key] += 1;
    }
    let mut acc = 0u32;
    for h in hist.iter_mut() {
        let c = *h;
        *h = acc;
        acc += c;
    }
    for (i, &d) in descs.iter().enumerate() {
        let key = ((((d >> 4) & 0xf) as usize) << 8) | sigs[i] as usize;
        out[hist[key] as usize] = d;
        hist[key] += 1;
    }
}

fn sort_perm_into(sigs: &[u8], out: &mut [u32]) {
    let mut hist = [0u32; 256];
    for &s in sigs {
        hist[s as usize] += 1;
    }
    let mut offs = [0u32; 256];
    let mut acc = 0u32;
    for i in 0..256 {
        offs[i] = acc;
        acc += hist[i];
    }
    for (i, &s) in sigs.iter().enumerate() {
        let o = offs[s as usize] as usize;
        out[o] = i as u32;
        offs[s as usize] += 1;
    }
}

type FnInit = unsafe extern "C" fn(u32) -> CuResult;
type FnDeviceGet = unsafe extern "C" fn(*mut i32, i32) -> CuResult;
type FnCtxCreate = unsafe extern "C" fn(*mut *mut c_void, u32, i32) -> CuResult;
type FnCtxSetCurrent = unsafe extern "C" fn(*mut c_void) -> CuResult;
type FnModuleLoadData = unsafe extern "C" fn(*mut *mut c_void, *const c_void) -> CuResult;
type FnModuleLoadDataEx = unsafe extern "C" fn(
    *mut *mut c_void,
    *const c_void,
    u32,
    *mut i32,
    *mut *mut c_void,
) -> CuResult;
type FnModuleGetFunction =
    unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const c_char) -> CuResult;
type FnMemAlloc = unsafe extern "C" fn(*mut DevPtr, usize) -> CuResult;
type FnMemcpyHtoD = unsafe extern "C" fn(DevPtr, *const c_void, usize) -> CuResult;
type FnMemcpyDtoH = unsafe extern "C" fn(*mut c_void, DevPtr, usize) -> CuResult;
type FnLaunchKernel = unsafe extern "C" fn(
    *mut c_void,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    *mut c_void,
    *mut *mut c_void,
    *mut *mut c_void,
) -> CuResult;
type FnCtxSynchronize = unsafe extern "C" fn() -> CuResult;
type FnMemFree = unsafe extern "C" fn(DevPtr) -> CuResult;
type FnGetErrorString = unsafe extern "C" fn(CuResult, *mut *const c_char) -> CuResult;
type FnStreamCreate = unsafe extern "C" fn(*mut *mut c_void, u32) -> CuResult;
type FnStreamSynchronize = unsafe extern "C" fn(*mut c_void) -> CuResult;
type FnStreamWaitEvent = unsafe extern "C" fn(*mut c_void, *mut c_void, u32) -> CuResult;
type FnMemcpyHtoDAsync =
    unsafe extern "C" fn(DevPtr, *const c_void, usize, *mut c_void) -> CuResult;
type FnMemcpyDtoHAsync = unsafe extern "C" fn(*mut c_void, DevPtr, usize, *mut c_void) -> CuResult;
type FnMemHostAlloc = unsafe extern "C" fn(*mut *mut c_void, usize, u32) -> CuResult;
type FnMemFreeHost = unsafe extern "C" fn(*mut c_void) -> CuResult;
type FnEventCreate = unsafe extern "C" fn(*mut *mut c_void, u32) -> CuResult;
type FnEventRecord = unsafe extern "C" fn(*mut c_void, *mut c_void) -> CuResult;
type FnEventSynchronize = unsafe extern "C" fn(*mut c_void) -> CuResult;
type FnEventElapsedTime = unsafe extern "C" fn(*mut f32, *mut c_void, *mut c_void) -> CuResult;

struct Gpu {
    ctx: *mut c_void,
    ctx_set_current: FnCtxSetCurrent,
    mem_alloc: FnMemAlloc,
    memcpy_htod: FnMemcpyHtoD,
    memcpy_dtoh: FnMemcpyDtoH,
    launch_kernel: FnLaunchKernel,
    ctx_synchronize: FnCtxSynchronize,
    mem_free: FnMemFree,
    get_error_string: FnGetErrorString,
    stream_create: FnStreamCreate,
    stream_synchronize: FnStreamSynchronize,
    stream_wait_event: FnStreamWaitEvent,
    memcpy_htod_async: FnMemcpyHtoDAsync,
    memcpy_dtoh_async: FnMemcpyDtoHAsync,
    mem_host_alloc: FnMemHostAlloc,
    mem_free_host: FnMemFreeHost,
    event_create: FnEventCreate,
    event_record: FnEventRecord,
    event_synchronize: FnEventSynchronize,
    event_elapsed: FnEventElapsedTime,
    func_encode: *mut c_void,
    func_encode_perm: *mut c_void,
    func_encode_desc: *mut c_void,
    func_sigs: *mut c_void,
    func_sigs_desc: *mut c_void,
    func_linearize: *mut c_void,
    func_quantize_pack: *mut c_void,
    func_halve: *mut c_void,
    has_blockify: bool,
}

unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

#[cfg(unix)]
unsafe fn load_driver_lib() -> *mut c_void {
    libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_NOW)
}

#[cfg(windows)]
unsafe extern "system" {
    fn LoadLibraryA(name: *const std::os::raw::c_char) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const std::os::raw::c_char) -> *mut c_void;
}

#[cfg(windows)]
unsafe fn load_driver_lib() -> *mut c_void {
    LoadLibraryA(c"nvcuda.dll".as_ptr())
}

#[cfg(unix)]
unsafe fn driver_sym(lib: *mut c_void, name: &CStr) -> *mut c_void {
    libc::dlsym(lib, name.as_ptr())
}

#[cfg(windows)]
unsafe fn driver_sym(lib: *mut c_void, name: &CStr) -> *mut c_void {
    GetProcAddress(lib, name.as_ptr())
}

unsafe fn sym<T: Copy>(lib: *mut c_void, name: &CStr) -> Result<T> {
    let p = driver_sym(lib, name);
    if p.is_null() {
        bail!("dlsym {:?} failed", name);
    }
    Ok(std::mem::transmute_copy::<*mut c_void, T>(&p))
}

unsafe fn struct_slice_bytes<T>(s: &[T]) -> &[u8] {
    std::slice::from_raw_parts(s.as_ptr().cast(), std::mem::size_of_val(s))
}

impl Gpu {
    unsafe fn err(&self, code: CuResult) -> anyhow::Error {
        let mut p: *const c_char = std::ptr::null();
        (self.get_error_string)(code, &mut p);
        if p.is_null() {
            anyhow!("CUDA error {code}")
        } else {
            anyhow!("CUDA error {code}: {}", CStr::from_ptr(p).to_string_lossy())
        }
    }

    unsafe fn check(&self, code: CuResult) -> Result<()> {
        if code == 0 {
            Ok(())
        } else {
            Err(self.err(code))
        }
    }

    unsafe fn new() -> Result<Gpu> {
        let cur_ok = std::env::var_os("CUDA_CACHE_PATH")
            .map(|p| jit_cache_dir_ok(std::path::Path::new(&p)))
            .unwrap_or(false);
        if !cur_ok {
            let base = std::env::var_os("HOME").or_else(|| std::env::var_os("LOCALAPPDATA"));
            if let Some(home) = base {
                let dir = std::path::Path::new(&home).join(".cache/abgen-gpu-jit");
                if jit_cache_dir_ok(&dir) {
                    std::env::set_var("CUDA_CACHE_PATH", &dir);
                }
            }
        }
        if std::env::var_os("CUDA_CACHE_MAXSIZE").is_none() {
            std::env::set_var("CUDA_CACHE_MAXSIZE", "1073741824");
        }
        let lib = load_driver_lib();
        if lib.is_null() {
            bail!("loading the CUDA driver library failed (no NVIDIA driver?)");
        }
        let init: FnInit = sym(lib, c"cuInit")?;
        let device_get: FnDeviceGet = sym(lib, c"cuDeviceGet")?;
        let ctx_create: FnCtxCreate = sym(lib, c"cuCtxCreate_v2")?;
        let module_load_data: FnModuleLoadData = sym(lib, c"cuModuleLoadData")?;
        let module_load_data_ex: FnModuleLoadDataEx = sym(lib, c"cuModuleLoadDataEx")?;
        let module_get_function: FnModuleGetFunction = sym(lib, c"cuModuleGetFunction")?;
        let mut g = Gpu {
            ctx: std::ptr::null_mut(),
            ctx_set_current: sym(lib, c"cuCtxSetCurrent")?,
            mem_alloc: sym(lib, c"cuMemAlloc_v2")?,
            memcpy_htod: sym(lib, c"cuMemcpyHtoD_v2")?,
            memcpy_dtoh: sym(lib, c"cuMemcpyDtoH_v2")?,
            launch_kernel: sym(lib, c"cuLaunchKernel")?,
            ctx_synchronize: sym(lib, c"cuCtxSynchronize")?,
            mem_free: sym(lib, c"cuMemFree_v2")?,
            get_error_string: sym(lib, c"cuGetErrorString")?,
            stream_create: sym(lib, c"cuStreamCreate")?,
            stream_synchronize: sym(lib, c"cuStreamSynchronize")?,
            stream_wait_event: sym(lib, c"cuStreamWaitEvent")?,
            memcpy_htod_async: sym(lib, c"cuMemcpyHtoDAsync_v2")?,
            memcpy_dtoh_async: sym(lib, c"cuMemcpyDtoHAsync_v2")?,
            mem_host_alloc: sym(lib, c"cuMemHostAlloc")?,
            mem_free_host: sym(lib, c"cuMemFreeHost")?,
            event_create: sym(lib, c"cuEventCreate")?,
            event_record: sym(lib, c"cuEventRecord")?,
            event_synchronize: sym(lib, c"cuEventSynchronize")?,
            event_elapsed: sym(lib, c"cuEventElapsedTime")?,
            func_encode: std::ptr::null_mut(),
            func_encode_perm: std::ptr::null_mut(),
            func_encode_desc: std::ptr::null_mut(),
            func_sigs: std::ptr::null_mut(),
            func_sigs_desc: std::ptr::null_mut(),
            func_linearize: std::ptr::null_mut(),
            func_quantize_pack: std::ptr::null_mut(),
            func_halve: std::ptr::null_mut(),
            has_blockify: false,
        };
        g.check(init(0))?;
        let mut dev: i32 = 0;
        g.check(device_get(&mut dev, 0))?;
        let mut ctx: *mut c_void = std::ptr::null_mut();
        g.check(ctx_create(&mut ctx, 0, dev))?;
        g.ctx = ctx;
        let ptx = match std::env::var("ABGEN_GPU_PTX") {
            Ok(ptx_path) => std::fs::read(&ptx_path)
                .map_err(|e| anyhow!("failed to read PTX at {ptx_path}: {e}"))?,
            Err(_) => include_bytes!("../kernel.ptx").to_vec(),
        };
        let ptx_c = CString::new(ptx).map_err(|_| anyhow!("PTX contains NUL byte"))?;
        let mut module: *mut c_void = std::ptr::null_mut();
        let maxreg: u32 = std::env::var("ABGEN_GPU_MAXREG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128);
        if maxreg > 0 {
            let mut opts = [0i32];
            let mut vals = [maxreg as usize as *mut c_void];
            g.check(module_load_data_ex(
                &mut module,
                ptx_c.as_ptr().cast(),
                1,
                opts.as_mut_ptr(),
                vals.as_mut_ptr(),
            ))
            .map_err(|e| anyhow!("PTX JIT load (maxreg={maxreg}) failed: {e}"))?;
        } else {
            g.check(module_load_data(&mut module, ptx_c.as_ptr().cast()))
                .map_err(|e| anyhow!("PTX JIT load failed: {e}"))?;
        }
        let mut f_encode: *mut c_void = std::ptr::null_mut();
        g.check(module_get_function(
            &mut f_encode,
            module,
            c"bc7_encode_groups".as_ptr(),
        ))?;
        g.func_encode = f_encode;
        let mut f_ep: *mut c_void = std::ptr::null_mut();
        if module_get_function(&mut f_ep, module, c"bc7_encode_groups_perm".as_ptr()) == 0 {
            g.func_encode_perm = f_ep;
        }
        let mut f_sig: *mut c_void = std::ptr::null_mut();
        if module_get_function(&mut f_sig, module, c"bc7_group_sigs".as_ptr()) == 0 {
            g.func_sigs = f_sig;
        }
        let mut f_desc: *mut c_void = std::ptr::null_mut();
        if module_get_function(&mut f_desc, module, c"bc7_encode_groups_desc".as_ptr()) == 0 {
            g.func_encode_desc = f_desc;
        }
        let mut f_sigd: *mut c_void = std::ptr::null_mut();
        if module_get_function(&mut f_sigd, module, c"bc7_group_sigs_desc".as_ptr()) == 0 {
            g.func_sigs_desc = f_sigd;
        }
        let mut f_lin: *mut c_void = std::ptr::null_mut();
        let mut f_pack: *mut c_void = std::ptr::null_mut();
        let mut f_halve: *mut c_void = std::ptr::null_mut();
        g.has_blockify = module_get_function(&mut f_lin, module, c"blockify_linearize".as_ptr())
            == 0
            && module_get_function(&mut f_pack, module, c"blockify_quantize_pack".as_ptr()) == 0
            && module_get_function(&mut f_halve, module, c"blockify_halve".as_ptr()) == 0;
        g.func_linearize = f_lin;
        g.func_quantize_pack = f_pack;
        g.func_halve = f_halve;
        Ok(g)
    }

    unsafe fn alloc_upload(&self, bytes: &[u8]) -> Result<DevPtr> {
        let mut d: DevPtr = 0;
        self.check((self.mem_alloc)(&mut d, bytes.len().max(1)))?;
        self.check((self.memcpy_htod)(d, bytes.as_ptr().cast(), bytes.len()))?;
        Ok(d)
    }

    unsafe fn launch_u64s(
        &self,
        func: *mut c_void,
        grid: u32,
        block: u32,
        args: &mut [u64],
    ) -> Result<()> {
        self.launch_u64s_on(func, grid, block, std::ptr::null_mut(), args)
    }

    unsafe fn launch_u64s_on(
        &self,
        func: *mut c_void,
        grid: u32,
        block: u32,
        stream: *mut c_void,
        args: &mut [u64],
    ) -> Result<()> {
        let mut argp: Vec<*mut c_void> = args
            .iter_mut()
            .map(|a| (a as *mut u64).cast::<c_void>())
            .collect();
        self.check((self.launch_kernel)(
            func,
            grid,
            1,
            1,
            block,
            1,
            1,
            0,
            stream,
            argp.as_mut_ptr(),
            std::ptr::null_mut(),
        ))
    }

    unsafe fn encode_dev(
        &self,
        d_blocks: DevPtr,
        num_blocks: usize,
        params: &Params,
        tables: &OptTables,
    ) -> Result<Vec<u8>> {
        let params_bytes = std::slice::from_raw_parts(
            (params as *const Params).cast::<u8>(),
            std::mem::size_of::<Params>(),
        );
        let tables_bytes = std::slice::from_raw_parts(
            (tables as *const OptTables).cast::<u8>(),
            std::mem::size_of::<OptTables>(),
        );
        let d_params = self.alloc_upload(params_bytes)?;
        let d_tables = self.alloc_upload(tables_bytes)?;
        let out_len = num_blocks * 16;
        let mut d_out: DevPtr = 0;
        self.check((self.mem_alloc)(&mut d_out, out_len))?;

        let num_groups = num_blocks.div_ceil(GROUP_WIDTH) as u32;
        let block_dim: u32 = encode_bdim();
        let grid_dim: u32 = num_groups.div_ceil(block_dim);

        let mut args = [d_blocks, num_blocks as u64, d_params, d_tables, d_out];
        let launch = self.launch_u64s(self.func_encode, grid_dim, block_dim, &mut args);
        let sync = launch.and_then(|_| self.check((self.ctx_synchronize)()));

        let mut out = vec![0u8; out_len];
        let copy = sync
            .and_then(|_| self.check((self.memcpy_dtoh)(out.as_mut_ptr().cast(), d_out, out_len)));
        for d in [d_params, d_tables, d_out] {
            let _ = (self.mem_free)(d);
        }
        copy?;
        Ok(out)
    }
}

fn gpu() -> Result<&'static Gpu> {
    static G: OnceLock<std::result::Result<Gpu, String>> = OnceLock::new();
    match G.get_or_init(|| unsafe { Gpu::new().map_err(|e| format!("{e:#}")) }) {
        Ok(g) => Ok(g),
        Err(e) => bail!("gpu init: {e}"),
    }
}

pub fn encode_blocks_gpu(
    rgba_block_major: &[u8],
    num_blocks: usize,
    params: &Params,
    tables: &OptTables,
) -> Result<Vec<u8>> {
    ensure!(
        rgba_block_major.len() >= num_blocks * 64,
        "input has {} bytes, need {} for {} blocks",
        rgba_block_major.len(),
        num_blocks * 64,
        num_blocks
    );
    if num_blocks == 0 {
        return Ok(Vec::new());
    }
    let g = gpu()?;
    unsafe {
        let d_blocks = g.alloc_upload(&rgba_block_major[..num_blocks * 64])?;
        let r = g.encode_dev(d_blocks, num_blocks, params, tables);
        let _ = (g.mem_free)(d_blocks);
        r
    }
}

pub fn cmd_probe() {
    let g = match gpu() {
        Ok(g) => g,
        Err(e) => {
            println!("probe: gpu init failed: {e:#}");
            return;
        }
    };
    unsafe {
        for gb in [24.0f64, 20.0, 18.0, 16.0, 14.0, 12.0, 10.0, 8.0] {
            let bytes = (gb * 1e9) as usize;
            let mut p: DevPtr = 0;
            let code = (g.mem_alloc)(&mut p, bytes);
            if code == 0 {
                println!("probe: {gb} GB single alloc OK");
                let _ = (g.mem_free)(p);
            } else {
                println!("probe: {gb} GB single alloc FAILED (code {code})");
            }
        }
        let mut hp: *mut c_void = std::ptr::null_mut();
        let hcode = (g.mem_host_alloc)(&mut hp, 3_000_000_000, 0);
        println!("probe: 3 GB pinned host alloc -> code {hcode}");
        let mut p: DevPtr = 0;
        let code = (g.mem_alloc)(&mut p, 20_400_000_000);
        println!("probe: 20.4 GB dev alloc after pinned -> code {code}");
        if code == 0 {
            let _ = (g.mem_free)(p);
        }
        if hcode == 0 {
            let _ = (g.mem_free_host)(hp);
        }
    }
}

pub struct BlockifyTex {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub mip_count: i32,
    pub srgb: bool,
    pub bucket: usize,
}

#[derive(Default)]
pub struct BlockifyStats {
    pub fingerprint: u64,
    pub launches: u64,
    pub blockify_ns: u64,
    pub encode_ns: u64,
    pub blocks_by_bucket: [u64; 4],
}

struct SlabLayout {
    lin_items: Vec<LinItem>,
    lin_prefix: Vec<u64>,
    pack_items: Vec<Vec<PackItem>>,
    pack_prefix: Vec<Vec<u64>>,
    halve_items: Vec<Vec<HalveItem>>,
    halve_prefix: Vec<Vec<u64>>,
    base_px_total: u64,
    pyr_px_total: u64,
    bucket_nb: [u64; 4],
    bucket_base: [u64; 4],
    total_blocks: u64,
}

struct DevArena {
    ptr: DevPtr,
    cap: usize,
}

impl DevArena {
    const fn new() -> DevArena {
        DevArena { ptr: 0, cap: 0 }
    }
    unsafe fn ensure(&mut self, g: &Gpu, bytes: usize, tag: &str) -> Result<DevPtr> {
        if bytes > self.cap {
            if self.ptr != 0 {
                let _ = (g.mem_free)(self.ptr);
                self.ptr = 0;
                self.cap = 0;
            }
            let want = (bytes + bytes / 16).max(1);
            let mut p: DevPtr = 0;
            let mut code = 0;
            for attempt in 0..8 {
                code = (g.mem_alloc)(&mut p, want);
                if code != 2 {
                    break;
                }
                eprintln!(
                    "warn: dev alloc {tag} ({want} bytes) OOM, retry {}",
                    attempt + 1
                );
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
            g.check(code)
                .map_err(|e| anyhow!("dev alloc {tag} ({want} bytes): {e}"))?;
            self.ptr = p;
            self.cap = want;
        }
        Ok(self.ptr)
    }
}

struct PinnedBuf {
    ptr: *mut u8,
    cap: usize,
    pinned: bool,
}

impl PinnedBuf {
    const fn new() -> PinnedBuf {
        PinnedBuf {
            ptr: std::ptr::null_mut(),
            cap: 0,
            pinned: false,
        }
    }
    unsafe fn free(&mut self, g: &Gpu) {
        if !self.ptr.is_null() {
            if self.pinned {
                let _ = (g.mem_free_host)(self.ptr.cast());
            } else {
                std::alloc::dealloc(
                    self.ptr,
                    std::alloc::Layout::from_size_align_unchecked(self.cap, 64),
                );
            }
            self.ptr = std::ptr::null_mut();
            self.cap = 0;
        }
    }
    unsafe fn ensure(&mut self, g: &Gpu, bytes: usize, tag: &str) -> Result<*mut u8> {
        if bytes > self.cap {
            self.free(g);
            let want = (bytes + bytes / 16).max(1);
            let mut p: *mut c_void = std::ptr::null_mut();
            if (g.mem_host_alloc)(&mut p, want, 0) == 0 {
                self.pinned = true;
            } else {
                p = std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(want, 64))
                    .cast();
                if p.is_null() {
                    bail!("host alloc {tag} ({want} bytes) failed (pinned and malloc)");
                }
                self.pinned = false;
                eprintln!("warn: pinned alloc {tag} ({want} bytes) failed; using pageable memory");
            }
            self.ptr = p.cast();
            self.cap = want;
        }
        Ok(self.ptr)
    }
}

struct DescPending {
    slot: usize,
    plan: Vec<(u64, usize, usize)>,
    tex_bytes: Vec<usize>,
    total_bytes: usize,
    out_len: usize,
    o_blocks: usize,
    ntexs: usize,
    ndescs: usize,
    nblocks: u64,
    base_bytes: usize,
    layout_ms: f64,
    stage_ms: f64,
    uwait_ms: f64,
    descbuild_ms: f64,
    sig_ms: f64,
    submit_ms: f64,
    t0: std::time::Instant,
    finalized: bool,
    bdim: u32,
}

struct Inflight {
    stats: BlockifyStats,
    pending: [bool; 2],
    pending_len: [usize; 2],
    launch_idx: usize,
    lin_total: u64,
    parity: usize,
    o_blocks: usize,
    layout_ms: f64,
    stage_ms: f64,
    overlapped: bool,
    base_bytes: usize,
    t0: std::time::Instant,
}

pub struct SlabEngine {
    g: &'static Gpu,
    compute: *mut c_void,
    copy: *mut c_void,
    upstream: *mut c_void,
    d_all: DevArena,
    d_out: [DevArena; 2],
    d_sig: [DevArena; 2],
    d_perm: [DevArena; 2],
    d_desc: [DevArena; 2],
    d_out_all: [DevArena; 2],
    h_base: [PinnedBuf; 2],
    h_meta: [PinnedBuf; 2],
    h_out: [PinnedBuf; 2],
    h_sig: [PinnedBuf; 2],
    h_perm: [PinnedBuf; 2],
    h_out_all: [PinnedBuf; 2],
    h_desc: [PinnedBuf; 2],
    ev_kdone: [*mut c_void; 2],
    ev_out: [*mut c_void; 2],
    ev_b0: *mut c_void,
    ev_b1: *mut c_void,
    ev_e0: *mut c_void,
    ev_e1: *mut c_void,
    ev_u0: [*mut c_void; 2],
    ev_u1: [*mut c_void; 2],
    ev_sig: [*mut c_void; 2],
    ev_d0: [*mut c_void; 2],
    ev_d1: [*mut c_void; 2],
    ev_db0: [*mut c_void; 2],
    ev_db1: [*mut c_void; 2],
    d_params: [DevPtr; 4],
    d_params4: DevPtr,
    d_tables: DevPtr,
    slab_ctr: u64,
    parity: usize,
    uploaded: [bool; 2],
    inflight: Option<Inflight>,
    collect: bool,
    collected: Vec<u8>,
    launch_plan: Option<Vec<(u64, usize, usize)>>,
    desc_pending: VecDeque<DescPending>,
    desc_done: VecDeque<Vec<Vec<u8>>>,
    desc_texmeta: Option<(Vec<usize>, usize)>,
}

mod chain;
mod slab;

pub use chain::tex_geometry;
pub(crate) use chain::{encode_bc7_mip_chain_gpu, gpu_ready};
