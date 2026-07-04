#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeDataType {
    Int8,

    UInt8,

    Int16,

    UInt16,

    Int32,

    UInt32,

    Float32,
}

impl AttributeDataType {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            AttributeDataType::Int8 | AttributeDataType::UInt8 => 1,
            AttributeDataType::Int16 | AttributeDataType::UInt16 => 2,
            AttributeDataType::Int32 | AttributeDataType::UInt32 | AttributeDataType::Float32 => 4,
        }
    }

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
        Self {
            dim,
            data_type,
            offset,
            lenght,
            unique_id,
        }
    }

    pub fn offset(&self) -> u32 {
        self.offset
    }
    pub fn lenght(&self) -> u32 {
        self.lenght
    }
    pub fn data_type(&self) -> AttributeDataType {
        self.data_type
    }
    pub fn dim(&self) -> u32 {
        self.dim
    }

    pub fn unique_id(&self) -> u32 {
        self.unique_id
    }
}

#[derive(Debug, PartialEq)]
pub struct DracoDecodeConfig {
    vertex_count: u32,
    index_count: u32,
    index_length: u32,
    buffer_size: usize,
    attributes: Vec<MeshAttribute>,

    position_quant_bounds: Option<([f32; 3], [f32; 3])>,
}

impl DracoDecodeConfig {
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

    pub(crate) fn set_position_quant_bounds(&mut self, min: [f32; 3], max: [f32; 3]) {
        self.position_quant_bounds = Some((min, max));
    }

    pub fn position_quant_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.position_quant_bounds
    }

    pub fn index_length(&self) -> u32 {
        self.index_length
    }

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

    pub fn get_attribute(&self, index: usize) -> Option<&MeshAttribute> {
        self.attributes.get(index)
    }

    pub fn attributes(&self) -> Vec<MeshAttribute> {
        self.attributes.clone()
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl DracoDecodeConfig {
    pub fn estimate_buffer_size(&self) -> usize {
        self.buffer_size
    }
}

#[derive(Debug)]
pub enum AttributeValues {
    Int8(Vec<i8>),

    UInt8(Vec<u8>),

    Int16(Vec<i16>),

    UInt16(Vec<u16>),

    Int32(Vec<i32>),

    UInt32(Vec<u32>),

    Float32(Vec<f32>),
}

#[derive(Debug)]
pub struct MeshDecodeResult {
    pub data: Vec<u8>,

    pub config: DracoDecodeConfig,
}
