// Plain-C FFI to cpp/decoder_api_c.cc — the wasm32 replacement for the cxx
// bridge in ffi.rs. Produces the same MeshDecodeResult layout.

use crate::utils::AttributeDataType;

const ATTRS_CAP: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DracoCAttr {
    dim: u32,
    data_type: i32,
    offset: u32,
    length: u32,
    unique_id: u32,
}

unsafe extern "C" {
    fn draco_c_decode_mesh(
        data: *const u8,
        len: usize,
        out_vertex_count: *mut u32,
        out_index_count: *mut u32,
        out_buffer_size: *mut usize,
        attrs_out: *mut DracoCAttr,
        attrs_cap: u32,
        out_attr_count: *mut u32,
        out_blob: *mut *mut u8,
        out_written: *mut usize,
    ) -> i32;
    fn draco_c_free(p: *mut u8);
    fn draco_c_position_quant_bounds(
        data: *const u8,
        len: usize,
        out_min3: *mut f32,
        out_max3: *mut f32,
    ) -> i32;
}

pub fn decode_mesh_with_config(data: &[u8]) -> Option<crate::MeshDecodeResult> {
    let mut vertex_count: u32 = 0;
    let mut index_count: u32 = 0;
    let mut buffer_size: usize = 0;
    let mut attrs = [DracoCAttr::default(); ATTRS_CAP];
    let mut attr_count: u32 = 0;
    let mut blob: *mut u8 = std::ptr::null_mut();
    let mut written: usize = 0;

    let rc = unsafe {
        draco_c_decode_mesh(
            data.as_ptr(),
            data.len(),
            &mut vertex_count,
            &mut index_count,
            &mut buffer_size,
            attrs.as_mut_ptr(),
            ATTRS_CAP as u32,
            &mut attr_count,
            &mut blob,
            &mut written,
        )
    };
    if rc != 0 || blob.is_null() {
        return None;
    }

    let buffer = unsafe { std::slice::from_raw_parts(blob, written).to_vec() };
    unsafe { draco_c_free(blob) };

    let mut config = crate::DracoDecodeConfig::new(vertex_count, index_count, buffer_size);
    for a in attrs.iter().take(attr_count as usize) {
        config.add_attribute(
            a.dim,
            AttributeDataType::from_draco_data_type(a.data_type),
            a.offset,
            a.length,
            a.unique_id,
        );
    }

    let mut mn = [0f32; 3];
    let mut mx = [0f32; 3];
    let valid = unsafe {
        draco_c_position_quant_bounds(data.as_ptr(), data.len(), mn.as_mut_ptr(), mx.as_mut_ptr())
    };
    if valid == 1 {
        config.set_position_quant_bounds(mn, mx);
    }

    Some(crate::MeshDecodeResult {
        data: buffer,
        config,
    })
}
