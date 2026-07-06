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
