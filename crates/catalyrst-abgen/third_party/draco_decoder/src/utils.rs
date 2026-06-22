/// Data types for mesh attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeDataType {
    /// Signed 8-bit integer
    Int8,
    /// Unsigned 8-bit integer
    UInt8,
    /// Signed 16-bit integer
    Int16,
    /// Unsigned 16-bit integer
    UInt16,
    /// Signed 32-bit integer
    Int32,
    /// Unsigned 32-bit integer
    UInt32,
    /// 32-bit floating point
    Float32,
}

impl AttributeDataType {
    /// Returns the size in bytes of this data type.
    pub fn size_in_bytes(&self) -> usize {
        match self {
            AttributeDataType::Int8 | AttributeDataType::UInt8 => 1,
            AttributeDataType::Int16 | AttributeDataType::UInt16 => 2,
            AttributeDataType::Int32 | AttributeDataType::UInt32 | AttributeDataType::Float32 => 4,
        }
    }

    /// Converts Draco native DataType enum value to AttributeDataType.
    ///
    /// Draco DataType enum values:
    /// - 1: DT_INT8
    /// - 2: DT_UINT8
    /// - 3: DT_INT16
    /// - 4: DT_UINT16
    /// - 5: DT_INT32
    /// - 6: DT_UINT32
    /// - 9: DT_FLOAT32
    pub fn from_draco_data_type(value: i32) -> Self {
        match value {
            1 => AttributeDataType::Int8,
            2 => AttributeDataType::UInt8,
            3 => AttributeDataType::Int16,
            4 => AttributeDataType::UInt16,
            5 => AttributeDataType::Int32,
            6 => AttributeDataType::UInt32,
            9 => AttributeDataType::Float32,
            _ => AttributeDataType::Float32,
        }
    }
}

/// Describes a single attribute in a decoded mesh.
///
/// An attribute represents per-vertex data such as positions, normals, or texture coordinates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MeshAttribute {
    dim: u32,
    data_type: AttributeDataType,
    offset: u32,
    lenght: u32,
    unique_id: u32,
}

impl MeshAttribute {
    pub fn new(
        dim: u32,
        data_type: AttributeDataType,
        offset: u32,
        lenght: u32,
        unique_id: u32,
    ) -> Self {
        Self { dim, data_type, offset, lenght, unique_id }
    }

    pub fn offset(&self) -> u32 { self.offset }
    pub fn lenght(&self) -> u32 { self.lenght }
    pub fn data_type(&self) -> AttributeDataType { self.data_type }
    pub fn dim(&self) -> u32 { self.dim }

    /// Draco attribute `unique_id`. glTF's `KHR_draco_mesh_compression`
    /// references attributes by this id.
    pub fn unique_id(&self) -> u32 { self.unique_id }
}

/// Configuration and metadata for a decoded Draco mesh.
///
/// This struct contains all the information needed to interpret the decoded
/// mesh buffer, including vertex count, index count, and attribute layouts.
#[derive(Debug, PartialEq)]
pub struct DracoDecodeConfig {
    vertex_count: u32,
    index_count: u32,
    index_length: u32,
    buffer_size: usize,
    attributes: Vec<MeshAttribute>,
    /// Pre-quantization bounds for the POSITION attribute, recovered from
    /// the draco header's `AttributeQuantizationTransform`. `None` if the
    /// POSITION attribute wasn't quantized (the post-decode stream scan
    /// already gives exact bounds in that case).
    ///
    /// Stored in **glTF coordinate space** (pre-x-flip) — same as a glTF
    /// POSITION accessor's `min`/`max` arrays. Downstream consumers apply
    /// the basis flip themselves.
    position_quant_bounds: Option<([f32; 3], [f32; 3])>,
}

impl DracoDecodeConfig {
    /// Creates a new config with a pre-computed buffer size.
    ///
    /// Used internally when decoding from C++ FFI.
    pub(crate) fn new(vertex_count: u32, index_count: u32, buffer_size: usize) -> Self {
        let index_length = if index_count <= u16::MAX as u32 {
            index_count as usize * 2
        } else {
            index_count as usize * 4
        } as u32;

        Self {
            vertex_count,
            index_count,
            index_length,
            buffer_size,
            attributes: Vec::new(),
            position_quant_bounds: None,
        }
    }

    /// Records the pre-quantization POSITION bounds recovered from the
    /// draco header. Used internally when receiving bounds from C++ FFI.
    pub(crate) fn set_position_quant_bounds(&mut self, min: [f32; 3], max: [f32; 3]) {
        self.position_quant_bounds = Some((min, max));
    }

    /// Returns the pre-quantization POSITION bounds in glTF coordinate
    /// space, when the encoder used `AttributeQuantizationTransform` on
    /// the POSITION attribute. `None` for unquantized inputs.
    pub fn position_quant_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.position_quant_bounds
    }

    /// Returns the total byte length of the index data.
    pub fn index_length(&self) -> u32 {
        self.index_length
    }

    /// Adds an attribute with specified offset and length.
    ///
    /// Used internally when receiving attribute data from C++ FFI.
    pub(crate) fn add_attribute(
        &mut self,
        dim: u32,
        data_type: AttributeDataType,
        offset: u32,
        length: u32,
        unique_id: u32,
    ) {
        let attribute = MeshAttribute {
            dim,
            data_type,
            offset,
            lenght: length,
            unique_id,
        };
        self.attributes.push(attribute);
    }

    /// Returns the attribute at the given index, if it exists.
    pub fn get_attribute(&self, index: usize) -> Option<&MeshAttribute> {
        self.attributes.get(index)
    }

    /// Returns a vector of all attributes.
    pub fn attributes(&self) -> Vec<MeshAttribute> {
        self.attributes.clone()
    }

    /// Returns the number of vertices in the mesh.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// Returns the number of indices in the mesh.
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Returns the total buffer size required for the decoded mesh.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl DracoDecodeConfig {
    /// Returns the estimated buffer size for the decoded mesh.
    ///
    /// This is an alias for `buffer_size()`.
    pub fn estimate_buffer_size(&self) -> usize {
        self.buffer_size
    }
}

/// Typed values for a decoded mesh attribute.
#[derive(Debug)]
pub enum AttributeValues {
    /// Signed 8-bit integer values
    Int8(Vec<i8>),
    /// Unsigned 8-bit integer values
    UInt8(Vec<u8>),
    /// Signed 16-bit integer values
    Int16(Vec<i16>),
    /// Unsigned 16-bit integer values
    UInt16(Vec<u16>),
    /// Signed 32-bit integer values
    Int32(Vec<i32>),
    /// Unsigned 32-bit integer values
    UInt32(Vec<u32>),
    /// 32-bit floating point values
    Float32(Vec<f32>),
}

/// Result of decoding a Draco mesh.
///
/// Contains the decoded mesh buffer and metadata describing its layout.
#[derive(Debug)]
pub struct MeshDecodeResult {
    /// The decoded mesh buffer containing indices and attribute data.
    pub data: Vec<u8>,
    /// Metadata describing the mesh structure and attribute layouts.
    pub config: DracoDecodeConfig,
}
