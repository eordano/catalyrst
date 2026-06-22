//! Thin Rust wrapper over the vendored BinomialLLC/crunch CRN encoder.
//!
//! The C ABI in `cpp/crn_wrapper.h` exposes one entry point —
//! `crn_ffi_compress_rgba` — that takes an RGBA32 mip chain (mip 0
//! first) plus per-mip dimensions and returns a `.CRN` byte stream.
//! crnlib internally runs its DXN/3DC encoder against the source
//! pixels and then RDO-compresses the resulting blocks into the
//! .CRN container (magic bytes `Hx`).
//!
//! Public domain — see `license.txt`.

use std::os::raw::{c_int, c_uint};

#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum CrnFormat {
    DxnXy = 0,
}

unsafe extern "C" {
    fn crn_ffi_compress_rgba(
        fmt: u32,
        mip_rgba: *const u8,
        mip_rgba_len: usize,
        mip_w: *const u32,
        mip_h: *const u32,
        mip_count: u32,
        quality_level: u32,
        num_helper_threads: u32,
        out_data: *mut *mut u8,
        out_size: *mut u32,
    ) -> c_int;

    fn crn_ffi_free(p: *mut u8);
}

/// Compress an RGBA32 mip chain into a .CRN byte stream for the given
/// format. `mip_rgba` is the concatenation of all mip-level pixel
/// buffers (mip 0 first, each `mip_w[i] * mip_h[i] * 4` bytes,
/// row-major, top-down).
///
/// `quality_level` runs 0..=255 (crnlib's `cCRNMinQualityLevel` ..
/// `cCRNMaxQualityLevel`); pass 255 for upstream default.
/// `num_helper_threads` is forwarded; pass 0 to keep crnlib
/// single-threaded.
///
/// Returns the .CRN payload on success, `None` if crnlib refused the
/// inputs.
pub fn crn_compress(
    fmt: CrnFormat,
    mip_rgba: &[u8],
    mip_w: &[u32],
    mip_h: &[u32],
    quality_level: u32,
    num_helper_threads: u32,
) -> Option<Vec<u8>> {
    assert_eq!(
        mip_w.len(),
        mip_h.len(),
        "mip_w and mip_h must have the same length"
    );
    let mip_count = mip_w.len();
    assert!(mip_count > 0, "mip_count must be > 0");
    assert!(mip_count <= 16, "mip_count must be <= cCRNMaxLevels (16)");

    // Sanity-check the flat buffer length matches the per-mip sum.
    let expected: usize = mip_w
        .iter()
        .zip(mip_h.iter())
        .map(|(&w, &h)| (w as usize) * (h as usize) * 4)
        .sum();
    assert_eq!(
        mip_rgba.len(),
        expected,
        "mip_rgba length mismatch: got {}, expected {}",
        mip_rgba.len(),
        expected
    );

    let mut out_data: *mut u8 = std::ptr::null_mut();
    let mut out_size: u32 = 0;

    let rc = unsafe {
        crn_ffi_compress_rgba(
            fmt as u32,
            mip_rgba.as_ptr(),
            mip_rgba.len(),
            mip_w.as_ptr(),
            mip_h.as_ptr(),
            mip_count as u32,
            quality_level as c_uint,
            num_helper_threads as c_uint,
            &mut out_data,
            &mut out_size,
        )
    };

    if rc != 0 || out_data.is_null() {
        return None;
    }

    let bytes =
        unsafe { std::slice::from_raw_parts(out_data, out_size as usize).to_vec() };
    unsafe { crn_ffi_free(out_data) };
    Some(bytes)
}

/// Convenience wrapper for BC5 (DXN_XY) normal-map compression. Input
/// must already be in the (R=X, G=Y, B=*, A=*) layout that
/// `bc5_pure::repack_for_bc5` produces.
pub fn crn_compress_bc5(
    mip_rgba: &[u8],
    mip_w: &[u32],
    mip_h: &[u32],
    quality_level: u32,
) -> Option<Vec<u8>> {
    crn_compress(CrnFormat::DxnXy, mip_rgba, mip_w, mip_h, quality_level, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: compress a tiny flat block, expect CRN magic bytes
    /// `Hx` at the start. This is the minimum proof that crnlib's
    /// linker chain — pthreads, LZMA, miniz, every static — resolved.
    #[test]
    fn tiny_smoke_test_emits_crn_magic() {
        // 4x4 single mip in (R=X, G=Y) layout — X=128, Y=128 ≈ flat
        // normal pointing straight at the camera.
        let rgba: Vec<u8> = (0..16)
            .flat_map(|_| [128u8, 128, 0, 255])
            .collect();
        let out = crn_compress_bc5(&rgba, &[4], &[4], 255).expect("crn_compress should succeed");
        assert!(out.len() >= 2, "output should be non-empty");
        // CRN file magic: 'H' 'x' (per crn_decomp.h crnd::crn_header::cCRNSigValue = 0x4878).
        assert_eq!(&out[0..2], b"Hx", "expected CRN magic 'Hx', got {:?}", &out[0..2]);
    }
}
