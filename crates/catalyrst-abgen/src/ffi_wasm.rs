// wasm32 twin of ffi.rs: no dlopen, so every JPEG decode goes through the
// statically linked vendored libjpeg 9c. Native keeps turbojpeg as its
// default; the byte-parity gate runs the native side with ABGEN_JPEG_GLB_9C=1
// so both targets decode GLB JPEGs identically.

use anyhow::{anyhow, Result};

pub fn turbojpeg_available() -> bool {
    false
}

pub fn decode_jpeg_rgba(jpeg: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    libjpeg9c::decode_rgba(jpeg, true).ok_or_else(|| anyhow!("libjpeg9c decode failed"))
}

pub fn decode_jpeg_rgba_box(jpeg: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    libjpeg9c::decode_rgba(jpeg, false).ok_or_else(|| anyhow!("libjpeg9c decode failed"))
}
