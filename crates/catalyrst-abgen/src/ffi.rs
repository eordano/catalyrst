use anyhow::{anyhow, Result};
#[cfg(not(abgen_static_turbojpeg))]
use std::ffi::CString;
use std::os::raw::{c_int, c_ulong, c_void};
use std::sync::OnceLock;

#[cfg(all(unix, not(abgen_static_turbojpeg)))]
const RTLD_NOW: c_int = 2;

#[cfg(all(windows, not(abgen_static_turbojpeg)))]
unsafe extern "system" {
    fn LoadLibraryA(name: *const std::os::raw::c_char) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const std::os::raw::c_char) -> *mut c_void;
}

#[cfg(all(unix, not(abgen_static_turbojpeg)))]
unsafe fn dlopen(path: &str) -> Result<*mut c_void> {
    let c = CString::new(path)?;
    let h = libc::dlopen(c.as_ptr(), RTLD_NOW);
    if h.is_null() {
        let err = libc::dlerror();
        let msg = if err.is_null() {
            "unknown dlopen error".to_string()
        } else {
            std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned()
        };
        return Err(anyhow!("dlopen({}) failed: {}", path, msg));
    }
    Ok(h)
}

#[cfg(all(windows, not(abgen_static_turbojpeg)))]
unsafe fn dlopen(path: &str) -> Result<*mut c_void> {
    let c = CString::new(path)?;
    let h = LoadLibraryA(c.as_ptr());
    if h.is_null() {
        return Err(anyhow!("LoadLibraryA({}) failed", path));
    }
    Ok(h)
}

#[cfg(all(unix, not(abgen_static_turbojpeg)))]
unsafe fn dlsym(handle: *mut c_void, name: &str) -> Result<*mut c_void> {
    let c = CString::new(name)?;
    let s = libc::dlsym(handle, c.as_ptr());
    if s.is_null() {
        return Err(anyhow!("dlsym({}) not found", name));
    }
    Ok(s)
}

#[cfg(all(windows, not(abgen_static_turbojpeg)))]
unsafe fn dlsym(handle: *mut c_void, name: &str) -> Result<*mut c_void> {
    let c = CString::new(name)?;
    let s = GetProcAddress(handle, c.as_ptr());
    if s.is_null() {
        return Err(anyhow!("GetProcAddress({}) not found", name));
    }
    Ok(s)
}

const TJPF_RGBA: c_int = 7;

const TJFLAG_FASTUPSAMPLE: c_int = 256;

const MAX_JPEG_DIM: c_int = 16384;

type FnTjInitDecompress = unsafe extern "C" fn() -> *mut c_void;
type FnTjDecompressHeader3 = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    c_ulong,
    *mut c_int,
    *mut c_int,
    *mut c_int,
    *mut c_int,
) -> c_int;
type FnTjDecompress2 = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    c_ulong,
    *mut u8,
    c_int,
    c_int,
    c_int,
    c_int,
    c_int,
) -> c_int;
type FnTjDestroy = unsafe extern "C" fn(*mut c_void) -> c_int;

struct TurboJpeg {
    init_decompress: FnTjInitDecompress,
    decompress_header3: FnTjDecompressHeader3,
    decompress2: FnTjDecompress2,
    destroy: FnTjDestroy,
}

unsafe impl Send for TurboJpeg {}
unsafe impl Sync for TurboJpeg {}

#[cfg(abgen_static_turbojpeg)]
unsafe extern "C" {
    fn tjInitDecompress() -> *mut c_void;
    fn tjDecompressHeader3(
        handle: *mut c_void,
        jpeg_buf: *const u8,
        jpeg_size: c_ulong,
        width: *mut c_int,
        height: *mut c_int,
        subsamp: *mut c_int,
        colorspace: *mut c_int,
    ) -> c_int;
    fn tjDecompress2(
        handle: *mut c_void,
        jpeg_buf: *const u8,
        jpeg_size: c_ulong,
        dst_buf: *mut u8,
        width: c_int,
        pitch: c_int,
        height: c_int,
        pixel_format: c_int,
        flags: c_int,
    ) -> c_int;
    fn tjDestroy(handle: *mut c_void) -> c_int;
}

impl TurboJpeg {
    #[cfg(not(abgen_static_turbojpeg))]
    fn load() -> Result<Self> {
        let mut candidates: Vec<String> = Vec::new();
        if let Ok(v) = std::env::var("TURBOJPEG_LIB") {
            if !v.is_empty() {
                candidates.push(v);
            }
        }
        if let Some(v) = option_env!("ABGEN_TURBOJPEG_LIB") {
            if !v.is_empty() {
                candidates.push(v.to_string());
            }
        }
        #[cfg(target_os = "macos")]
        {
            candidates.push("libturbojpeg.dylib".to_string());
            candidates.push("libturbojpeg.0.dylib".to_string());

            candidates.push("/opt/homebrew/lib/libturbojpeg.dylib".to_string());
            candidates.push("/usr/local/lib/libturbojpeg.dylib".to_string());
        }
        #[cfg(not(target_os = "macos"))]
        {
            if cfg!(windows) {
                candidates.push("turbojpeg.dll".to_string());
            } else {
                candidates.push("libturbojpeg.so.0".to_string());
                candidates.push("libturbojpeg.so".to_string());
            }
        }
        let mut last_err = anyhow!("no candidate");
        for cand in candidates {
            match unsafe { dlopen(&cand) } {
                Ok(h) => {
                    let resolved = unsafe {
                        let i = dlsym(h, "tjInitDecompress")?;
                        let dh = dlsym(h, "tjDecompressHeader3")?;
                        let d2 = dlsym(h, "tjDecompress2")?;
                        let de = dlsym(h, "tjDestroy")?;
                        TurboJpeg {
                            init_decompress: std::mem::transmute::<*mut c_void, FnTjInitDecompress>(
                                i,
                            ),
                            decompress_header3: std::mem::transmute::<
                                *mut c_void,
                                FnTjDecompressHeader3,
                            >(dh),
                            decompress2: std::mem::transmute::<*mut c_void, FnTjDecompress2>(d2),
                            destroy: std::mem::transmute::<*mut c_void, FnTjDestroy>(de),
                        }
                    };
                    return Ok(resolved);
                }
                Err(e) => last_err = e,
            }
        }
        Err(anyhow!("libturbojpeg not loadable: {last_err}"))
    }

    #[cfg(abgen_static_turbojpeg)]
    fn load() -> Result<Self> {
        Ok(TurboJpeg {
            init_decompress: tjInitDecompress,
            decompress_header3: tjDecompressHeader3,
            decompress2: tjDecompress2,
            destroy: tjDestroy,
        })
    }
}

static TURBOJPEG: OnceLock<Result<TurboJpeg, String>> = OnceLock::new();

pub fn turbojpeg_available() -> bool {
    turbojpeg().is_ok()
}

fn turbojpeg() -> Result<&'static TurboJpeg> {
    TURBOJPEG
        .get_or_init(|| TurboJpeg::load().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow!("{}", e))
}

pub fn decode_jpeg_rgba(jpeg: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    decode_jpeg_rgba_flags(jpeg, 0)
}

pub fn decode_jpeg_rgba_box(jpeg: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    if std::env::var_os("ABGEN_JPEG_TURBO_BOX").is_none() {
        if let Some(out) = libjpeg9c::decode_rgba(jpeg, false) {
            return Ok(out);
        }
    }
    decode_jpeg_rgba_flags(jpeg, TJFLAG_FASTUPSAMPLE)
}

fn decode_jpeg_rgba_flags(jpeg: &[u8], flags: c_int) -> Result<(Vec<u8>, u32, u32)> {
    let lib = turbojpeg()?;
    unsafe {
        let handle = (lib.init_decompress)();
        if handle.is_null() {
            return Err(anyhow!("tjInitDecompress failed"));
        }
        let mut w: c_int = 0;
        let mut h: c_int = 0;
        let mut subsamp: c_int = 0;
        let mut colorspace: c_int = 0;
        let rc = (lib.decompress_header3)(
            handle,
            jpeg.as_ptr(),
            jpeg.len() as c_ulong,
            &mut w,
            &mut h,
            &mut subsamp,
            &mut colorspace,
        );
        if rc != 0 || w <= 0 || h <= 0 {
            (lib.destroy)(handle);
            return Err(anyhow!("tjDecompressHeader3 failed (rc={rc})"));
        }
        if w > MAX_JPEG_DIM || h > MAX_JPEG_DIM {
            (lib.destroy)(handle);
            return Err(anyhow!(
                "jpeg dimensions {}x{} exceed MAX_JPEG_DIM ({})",
                w,
                h,
                MAX_JPEG_DIM
            ));
        }

        let pitch: c_int = w * 4;
        let buf_len: usize = (w as usize)
            .checked_mul(4)
            .and_then(|n| n.checked_mul(h as usize))
            .ok_or_else(|| anyhow!("jpeg output size overflow"))?;
        let mut out = vec![0u8; buf_len];

        let rc = (lib.decompress2)(
            handle,
            jpeg.as_ptr(),
            jpeg.len() as c_ulong,
            out.as_mut_ptr(),
            w,
            pitch,
            h,
            TJPF_RGBA,
            flags,
        );
        (lib.destroy)(handle);
        if rc != 0 {
            return Err(anyhow!("tjDecompress2 failed (rc={rc})"));
        }
        Ok((out, w as u32, h as u32))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn turbojpeg_decodes_fixture_when_available() {
        if !super::turbojpeg_available() {
            eprintln!(
                "libturbojpeg not loadable; skipping (vendored libjpeg9c remains the fallback)"
            );
            return;
        }
        let jpeg = include_bytes!("../testdata/gradient-16x16.jpg");
        let (rgba, w, h) = super::decode_jpeg_rgba(jpeg).expect("turbojpeg decode");
        assert_eq!((w, h), (16, 16));
        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert!(rgba.chunks(4).all(|p| p[3] == 255), "alpha must be opaque");
        assert!(rgba.iter().any(|&b| b > 0), "decoded pixels all zero");
    }
}
