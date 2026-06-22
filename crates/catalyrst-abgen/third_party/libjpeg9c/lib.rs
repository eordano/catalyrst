//! Vendored IJG libjpeg 9c decoder — the JPEG codec FreeImage 3.18.0 bundles,
//! which is what Unity's editor `AssetImporter` uses to decode standalone
//! texture JPEGs. Linking this directly (instead of the system
//! libjpeg-turbo) closes the ±1-LSB chroma-edge tail on JPEG-sourced
//! standalone textures: the islow IDCT is byte-identical across all libjpeg
//! lineages, but the box-upsample / YCbCr->RGB merge rounds differently
//! between the IJG 9-series and the turbo lineage.
//!
//! See `docs/texture_jpeg_decoder.md`.

use std::os::raw::{c_int, c_uchar, c_ulong};

extern "C" {
    fn jpeg9c_decode_rgba(
        data: *const c_uchar,
        len: c_ulong,
        w: *mut c_int,
        h: *mut c_int,
        fancy_upsampling: c_int,
    ) -> *mut c_uchar;
    fn jpeg9c_free(p: *mut c_uchar);
}

/// Decode a JPEG to RGBA8 using IJG libjpeg 9c.
///
/// `fancy` selects chroma upsampling: `false` = box (the standalone /
/// editor-AssetImporter path), `true` = fancy (the glTFast path).
///
/// Returns `(rgba, width, height)` or `None` on decode failure.
pub fn decode_rgba(jpeg: &[u8], fancy: bool) -> Option<(Vec<u8>, u32, u32)> {
    let mut w: c_int = 0;
    let mut h: c_int = 0;
    let ptr = unsafe {
        jpeg9c_decode_rgba(
            jpeg.as_ptr(),
            jpeg.len() as c_ulong,
            &mut w,
            &mut h,
            if fancy { 1 } else { 0 },
        )
    };
    if ptr.is_null() || w <= 0 || h <= 0 {
        return None;
    }
    let len = (w as usize) * (h as usize) * 4;
    let rgba = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    unsafe { jpeg9c_free(ptr) };
    Some((rgba, w as u32, h as u32))
}
