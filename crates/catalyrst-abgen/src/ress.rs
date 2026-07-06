const RESS_ALIGNMENT: usize = 16;

pub const RESS_NODE_FLAGS: u32 = 0;

const ARCHIVE_PATH_FMT_PREFIX: &str = "archive:/";

#[derive(Clone, Debug)]
pub struct TextureBlob {
    pub path_id: i64,
    pub pixels: Vec<u8>,
    pub name: String,
}

impl TextureBlob {
    pub fn new(path_id: i64, pixels: Vec<u8>, name: impl Into<String>) -> Self {
        Self {
            path_id,
            pixels,
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamData {
    pub offset: usize,
    pub size: usize,
    pub path: String,
}

impl StreamData {
    pub const fn empty() -> Self {
        Self {
            offset: 0,
            size: 0,
            path: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuiltRess {
    pub payload: Vec<u8>,
    pub stream_data: Vec<(i64, StreamData)>,
    pub node_name: String,
    pub node_flags: u32,
}

#[inline]
const fn align_up(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

pub fn ress_node_name(cab_node_name: &str) -> String {
    format!("{cab_node_name}.resS")
}

fn stream_path(cab_node_name: &str) -> String {
    format!("{ARCHIVE_PATH_FMT_PREFIX}{cab_node_name}/{cab_node_name}.resS")
}

pub fn build_ress(
    textures: &[TextureBlob],
    cab_node_name: &str,
    inline_predicate: Option<&dyn Fn(&TextureBlob) -> bool>,
) -> BuiltRess {
    let path = stream_path(cab_node_name);

    let mut order: Vec<&TextureBlob> = textures.iter().collect();
    order.sort_by_key(|t| t.path_id);

    let streamed_bytes: usize = order
        .iter()
        .filter(|t| inline_predicate.is_none_or(|p| !p(t)))
        .map(|t| t.pixels.len())
        .sum();
    let mut payload = Vec::with_capacity(align_up(streamed_bytes, RESS_ALIGNMENT));
    let mut stream_data: Vec<(i64, StreamData)> = Vec::with_capacity(order.len());

    let mut offset = 0usize;
    for tex in order {
        if inline_predicate.is_some_and(|p| p(tex)) {
            stream_data.push((tex.path_id, StreamData::empty()));
            continue;
        }
        let pad = align_up(offset, RESS_ALIGNMENT) - offset;
        if pad != 0 {
            payload.resize(payload.len() + pad, 0);
            offset += pad;
        }
        let size = tex.pixels.len();
        stream_data.push((
            tex.path_id,
            StreamData {
                offset,
                size,
                path: path.clone(),
            },
        ));
        payload.extend_from_slice(&tex.pixels);
        offset += size;
    }
    let tail = align_up(offset, RESS_ALIGNMENT) - offset;
    if tail != 0 {
        payload.resize(payload.len() + tail, 0);
    }

    BuiltRess {
        payload,
        stream_data,
        node_name: ress_node_name(cab_node_name),
        node_flags: RESS_NODE_FLAGS,
    }
}
