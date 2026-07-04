#[cxx::bridge]
mod cpp {
    struct MeshAttribute {
        dim: u32,
        data_type: u32,
        offset: u32,
        length: u32,
        unique_id: u32,
    }

    struct MeshConfig {
        vertex_count: u32,
        index_count: u32,
        index_length: u32,
        buffer_size: usize,
        attributes: Vec<MeshAttribute>,
    }

    struct PositionQuantBounds {
        valid: bool,
        min: [f32; 3],
        max: [f32; 3],
    }

    unsafe extern "C++" {
        include!("decoder_api.h");

        type DracoMesh;

        pub fn decode_point_cloud(data: &[u8]) -> Vec<u8>;

        pub fn create_mesh(data: &[u8]) -> UniquePtr<DracoMesh>;

        pub fn compute_mesh_config(mesh: &DracoMesh, config: &mut MeshConfig) -> bool;

        pub unsafe fn decode_mesh_to_buffer(
            mesh: &DracoMesh,
            out_ptr: *mut u8,
            out_len: usize,
        ) -> usize;

        pub fn decode_position_quant_bounds(data: &[u8], out: &mut PositionQuantBounds) -> bool;
    }
}

#[allow(dead_code)]
pub fn decode_point_cloud_native(data: &[u8]) -> Vec<u8> {
    cpp::decode_point_cloud(data)
}

fn convert_config(cpp_config: cpp::MeshConfig) -> crate::DracoDecodeConfig {
    let mut config = crate::DracoDecodeConfig::new(
        cpp_config.vertex_count,
        cpp_config.index_count,
        cpp_config.buffer_size,
    );

    for attr in cpp_config.attributes {
        let data_type = crate::AttributeDataType::from_draco_data_type(attr.data_type as i32);
        config.add_attribute(
            attr.dim,
            data_type,
            attr.offset,
            attr.length,
            attr.unique_id,
        );
    }

    config
}

pub fn decode_mesh_with_config(data: &[u8]) -> Option<crate::MeshDecodeResult> {
    let mesh = cpp::create_mesh(data);
    if mesh.is_null() {
        panic!("Failed to create mesh from data");
    }

    let mut cpp_config = cpp::MeshConfig {
        vertex_count: 0,
        index_count: 0,
        index_length: 0,
        buffer_size: 0,
        attributes: Vec::new(),
    };

    if !cpp::compute_mesh_config(&mesh, &mut cpp_config) {
        panic!("Failed to compute mesh config");
    }

    let buffer_size = cpp_config.buffer_size;
    let mut config = convert_config(cpp_config);
    let mut buffer = vec![0u8; buffer_size];

    let written = unsafe { cpp::decode_mesh_to_buffer(&mesh, buffer.as_mut_ptr(), buffer.len()) };

    if written == 0 {
        panic!("Failed to decode mesh to buffer");
    }

    buffer.truncate(written);

    let mut qbounds = cpp::PositionQuantBounds {
        valid: false,
        min: [0.0; 3],
        max: [0.0; 3],
    };
    if cpp::decode_position_quant_bounds(data, &mut qbounds) && qbounds.valid {
        config.set_position_quant_bounds(qbounds.min, qbounds.max);
    }

    Some(crate::MeshDecodeResult {
        data: buffer,
        config,
    })
}
