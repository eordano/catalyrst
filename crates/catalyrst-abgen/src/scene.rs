use image::RgbaImage;

#[derive(Clone, Debug, Default)]
pub struct MorphTarget {
    pub positions: Vec<[f64; 3]>,
    pub normals: Option<Vec<[f64; 3]>>,
    pub tangents: Option<Vec<[f64; 3]>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AttrSig {
    pub position: Option<i64>,
    pub normal: Option<i64>,
    pub tangent: Option<i64>,
    pub texcoords: Vec<i64>,
    pub color: Option<i64>,
    pub joints: Option<i64>,
    pub weights: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MorphSig {
    pub position: Option<i64>,
    pub normal: Option<i64>,
    pub tangent: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Primitive {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,

    pub has_source_normals: bool,
    pub uvs: Option<Vec<[f64; 2]>>,
    pub tangents: Option<Vec<[f64; 4]>>,
    pub indices: Vec<u32>,
    pub material_index: Option<usize>,
    pub name: String,
    pub colors: Option<Vec<[f64; 4]>>,
    pub uv_sets: Vec<Vec<[f64; 2]>>,
    pub weights: Option<Vec<[f64; 4]>>,
    pub joints: Option<Vec<[u32; 4]>>,
    pub skin_index: Option<usize>,
    pub go_name: String,
    pub morph_targets: Vec<MorphTarget>,
    pub morph_weights: Vec<f32>,
    pub morph_target_names: Vec<String>,
    pub gltf_mesh_index: Option<usize>,
    pub gltf_prim_index: usize,
    pub gltf_attr_sig: Option<AttrSig>,

    pub gltf_morph_sig: Vec<MorphSig>,
    pub position_min_decl: Option<[f64; 3]>,
    pub position_max_decl: Option<[f64; 3]>,
    pub from_draco: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub translation: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    pub primitives: Vec<Primitive>,
    pub children: Vec<usize>,
    pub is_collider: bool,

    pub name_is_collider: bool,

    pub extra_colliders: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TexRef {
    pub image: usize,
    pub sampler: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sampler {
    pub mag_filter: Option<i64>,
    pub min_filter: Option<i64>,
    pub wrap_s: Option<i64>,
    pub wrap_t: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexTransform {
    pub scale: [f64; 2],
    pub offset: [f64; 2],
}

impl TexTransform {
    pub fn is_identity(&self) -> bool {
        self.scale == [1.0, 1.0] && self.offset == [0.0, 0.0]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub name: String,
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    pub alpha_mode: String,
    pub alpha_cutoff: f64,
    pub emissive: [f64; 3],
    pub base_color_image: Option<TexRef>,

    pub base_color_emit_image: Option<TexRef>,
    pub emissive_image: Option<TexRef>,
    pub normal_image: Option<TexRef>,
    pub metallic_roughness_image: Option<TexRef>,

    pub metal_rough_emit_image: Option<TexRef>,
    pub occlusion_image: Option<TexRef>,
    pub normal_scale: f64,
    pub occlusion_strength: f64,
    pub double_sided: bool,
    pub tex_transforms: std::collections::BTreeMap<String, TexTransform>,
    pub uses_uv_channel_select: bool,
    pub uses_spec_gloss: bool,
    pub spec_gloss_image: Option<TexRef>,
    pub specular_factor: [f64; 3],
    pub glossiness_factor: f64,
    pub specular_color_image: Option<TexRef>,
    pub uses_emissive_strength: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub skeleton: Option<usize>,
    pub bind_poses: Vec<[f64; 16]>,
}

#[derive(Clone, Default)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub root_nodes: Vec<usize>,
    pub name: Option<String>,
    pub materials: Vec<Material>,
    pub images: Vec<Option<RgbaImage>>,
    pub image_embedded: Vec<bool>,
    pub image_bytes: Vec<Option<Vec<u8>>>,
    pub image_sampler: Vec<(Option<i64>, Option<i64>)>,
    pub image_wrap: Vec<(Option<i64>, Option<i64>)>,
    pub samplers: Vec<Sampler>,
    pub image_uri: Vec<Option<String>>,

    pub texture_refs: Vec<TexRef>,
    pub normal_images: std::collections::HashSet<usize>,
    pub skins: Vec<Skin>,
    pub extra_scenes: Vec<(Option<String>, Vec<usize>)>,

    pub unique_node_names: Vec<String>,
}
