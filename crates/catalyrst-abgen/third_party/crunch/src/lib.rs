use std::os::raw::{c_int, c_uint};

#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum CrnFormat {
    DxnXy = 0,
    Dxt5 = 1,
}

pub const CRN_HEADER_FMT_DXT5: u32 = 2;
pub const CRN_HEADER_FMT_DXN_XY: u32 = 7;

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

    fn crn_ffi_decompress_rgba(
        crn_data: *const u8,
        crn_len: u32,
        out_rgba: *mut *mut u8,
        out_w: *mut u32,
        out_h: *mut u32,
        out_levels: *mut u32,
        out_format: *mut u32,
    ) -> c_int;

    fn crn_ffi_free_decoded(p: *mut u8);
}

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

    let bytes = unsafe { std::slice::from_raw_parts(out_data, out_size as usize).to_vec() };
    unsafe { crn_ffi_free(out_data) };
    Some(bytes)
}

pub fn crn_compress_bc5(
    mip_rgba: &[u8],
    mip_w: &[u32],
    mip_h: &[u32],
    quality_level: u32,
) -> Option<Vec<u8>> {
    crn_compress(CrnFormat::DxnXy, mip_rgba, mip_w, mip_h, quality_level, 0)
}

pub fn crn_compress_dxt5(
    mip_rgba: &[u8],
    mip_w: &[u32],
    mip_h: &[u32],
    quality_level: u32,
) -> Option<Vec<u8>> {
    crn_compress(CrnFormat::Dxt5, mip_rgba, mip_w, mip_h, quality_level, 0)
}

pub struct DecodedCrn {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub format: u32,
}

pub fn crn_decompress_level0(crn: &[u8]) -> Option<DecodedCrn> {
    let mut out_rgba: *mut u8 = std::ptr::null_mut();
    let mut w: u32 = 0;
    let mut h: u32 = 0;
    let mut levels: u32 = 0;
    let mut format: u32 = 0;

    let rc = unsafe {
        crn_ffi_decompress_rgba(
            crn.as_ptr(),
            u32::try_from(crn.len()).ok()?,
            &mut out_rgba,
            &mut w,
            &mut h,
            &mut levels,
            &mut format,
        )
    };
    if rc != 0 || out_rgba.is_null() {
        return None;
    }

    let nbytes = (w as usize) * (h as usize) * 4;
    let rgba = unsafe { std::slice::from_raw_parts(out_rgba, nbytes).to_vec() };
    unsafe { crn_ffi_free_decoded(out_rgba) };

    Some(DecodedCrn {
        rgba,
        width: w,
        height: h,
        levels,
        format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_smoke_test_emits_crn_magic() {
        let rgba: Vec<u8> = (0..16).flat_map(|_| [128u8, 128, 0, 255]).collect();
        let out = crn_compress_bc5(&rgba, &[4], &[4], 255).expect("crn_compress should succeed");
        assert!(out.len() >= 2, "output should be non-empty");

        assert_eq!(
            &out[0..2],
            b"Hx",
            "expected CRN magic 'Hx', got {:?}",
            &out[0..2]
        );
    }

    #[test]
    fn dxt5_compress_declares_dxt5_and_roundtrips() {
        let (w, h) = (8u32, 8u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 32) as u8, (y * 32) as u8, 200, 128 + (x * 8) as u8]);
            }
        }

        let crn = crn_compress_dxt5(&rgba, &[w], &[h], 255).expect("dxt5 crunch should succeed");
        assert_eq!(&crn[0..2], b"Hx");
        assert_eq!(u16::from_be_bytes([crn[12], crn[13]]) as u32, w);
        assert_eq!(u16::from_be_bytes([crn[14], crn[15]]) as u32, h);
        assert_eq!(crn[16], 1, "levels");
        assert_eq!(crn[17], 1, "faces");
        assert_eq!(crn[18] as u32, CRN_HEADER_FMT_DXT5);

        let dec = crn_decompress_level0(&crn).expect("decode should succeed");
        assert_eq!(dec.format, CRN_HEADER_FMT_DXT5);
        assert_eq!((dec.width, dec.height, dec.levels), (w, h, 1));
        assert_eq!(dec.rgba.len(), rgba.len());

        let mae: f64 = rgba
            .iter()
            .zip(dec.rgba.iter())
            .map(|(&a, &b)| (a as f64 - b as f64).abs())
            .sum::<f64>()
            / rgba.len() as f64;
        assert!(mae < 16.0, "round-trip mean abs error too high: {mae}");
    }

    #[test]
    fn dxn_payload_still_decodes_as_dxn() {
        let rgba: Vec<u8> = (0..16).flat_map(|_| [128u8, 128, 0, 255]).collect();
        let crn = crn_compress_bc5(&rgba, &[4], &[4], 255).expect("crn_compress should succeed");
        assert_eq!(crn[18] as u32, CRN_HEADER_FMT_DXN_XY);
        let dec = crn_decompress_level0(&crn).expect("decode should succeed");
        assert_eq!(dec.format, CRN_HEADER_FMT_DXN_XY);
        assert_eq!((dec.width, dec.height), (4, 4));
    }
}
