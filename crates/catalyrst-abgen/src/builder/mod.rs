mod finalize;
mod material;
mod mesh;
mod nodes;
mod standalone;
mod templates;
#[cfg(test)]
mod tests;
mod texture;

pub use templates::template_available;
pub use templates::template_dir;
pub use templates::templates_missing;
pub use templates::templates_missing_in;
pub use templates::REQUIRED_TEMPLATES;
pub use texture::source_image_decodes;

use material::shader_pptr;
use standalone::StandaloneTextureBuilder;
use templates::load_template;

use crate::bc7_pure;
use crate::sbp_order::{CrossBundlePosition, ExternalsPosition};
use crate::scene::{Primitive, Scene, TexRef};
use crate::unity::serialized_file::SerializedType;
use crate::unity::{self, Bundle};
use crate::value::Value;
use crate::{
    animation, animation_mecanim, bundle as bundle_io, cabname, dxt_unity, gltf, hashes, materials,
    mesh_layout, pathids, ress, sbp_order, skeleton, texprofile,
};
use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

pub type ResolveHash<'a> = &'a dyn Fn(&str) -> Option<String>;

use crate::shader::{SHADER_FILE_ID, SHADER_PATH_ID};

const GLB_FILE_TYPE: i32 = pathids::FILE_TYPE_META_ASSET;
const MAT_FILE_TYPE: i32 = pathids::FILE_TYPE_SERIALIZED_ASSET;
const TEX_FILE_TYPE: i32 = pathids::FILE_TYPE_META_ASSET;
const META_FILE_TYPE: i32 = pathids::FILE_TYPE_META_ASSET;

const TEXTURE_CLASS_ID: i64 = 28;
const TEXTURE_LOCAL_ID: i64 = TEXTURE_CLASS_ID * 100000;

fn target_platform_for(target: &str) -> Option<i32> {
    match (if target.is_empty() { "linux" } else { target })
        .to_lowercase()
        .as_str()
    {
        "linux" => Some(24),
        "windows" => Some(19),
        "mac" | "osx" => Some(2),
        "webgl" => Some(20),
        _ => None,
    }
}

fn target_from_bundle_name(bundle_name: &str) -> &'static str {
    let lower = bundle_name.to_lowercase();
    for plat in ["linux", "windows", "mac", "webgl", "osx"] {
        if lower.ends_with(&format!("_{plat}")) {
            return match plat {
                "linux" => "linux",
                "windows" => "windows",
                "mac" => "mac",
                "webgl" => "webgl",
                "osx" => "osx",
                _ => unreachable!(),
            };
        }
    }
    "linux"
}

#[derive(Clone, Copy, Default)]
struct Toggles {
    collection_mode: bool,
    real_textures: bool,
    v38_compat: bool,
    v38_timestamp: i64,
}

impl Toggles {
    fn from_opts(opts: &BuildOpts<'_>) -> Self {
        Toggles {
            collection_mode: opts.collection_mode,
            real_textures: opts.real_textures,
            v38_compat: opts.v38_compat,
            v38_timestamp: opts.v38_timestamp,
        }
    }
}

fn emits_metadata_textasset(root_hash: &str, v38_compat: bool) -> bool {
    if v38_compat {
        return true;
    }
    !root_hash.starts_with("Qm")
}

fn metadata_timestamp(t: Toggles) -> i64 {
    if !t.v38_compat {
        return 0;
    }
    t.v38_timestamp
}

fn metadata_version_for_target(target: &str, v38_compat: bool) -> &'static str {
    if v38_compat {
        return "7.0";
    }
    match target {
        "mac" | "osx" | "windows" | "linux" | "webgl" => "8.0",
        _ => "7.0",
    }
}

fn natural_bundle_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a = a.as_bytes();
    let b = b.as_bytes();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        let (ca, cb) = (a[i], b[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let (mut ie, mut je) = (i, j);
            while ie < a.len() && a[ie].is_ascii_digit() {
                ie += 1;
            }
            while je < b.len() && b[je].is_ascii_digit() {
                je += 1;
            }

            let (mut is_, mut js_) = (i, j);
            while is_ + 1 < ie && a[is_] == b'0' {
                is_ += 1;
            }
            while js_ + 1 < je && b[js_] == b'0' {
                js_ += 1;
            }
            let ord = (ie - is_)
                .cmp(&(je - js_))
                .then_with(|| a[is_..ie].cmp(&b[js_..je]))
                .then_with(|| (ie - i).cmp(&(je - j)));
            if ord != Ordering::Equal {
                return ord;
            }
            i = ie;
            j = je;
        } else {
            if ca != cb {
                return ca.cmp(&cb);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

#[derive(Clone, Debug)]
enum Role {
    Bundle,
    Glb(String, String),

    GlbIdx(String, String, u32),
    Mat(String),
    Tex(String),
    Meta,
    AnimController,
    AnimControllerSubClip(usize),
}

struct Builder<'a> {
    proto: &'a HashMap<String, SerializedType>,
    base: &'a HashMap<String, Value>,
    root_hash: String,
    bundle_name: String,
    keep_forward_plus: bool,
    is_emote: bool,
    is_wearable: bool,
    is_gltf: bool,
    target: &'static str,
    glb_guid: String,
    pid: i64,
    objects: BTreeMap<i64, (String, Value)>,
    order: Vec<i64>,
    roles: HashMap<i64, Role>,

    glb_role_keys: HashMap<(String, String), Vec<i64>>,
    colorspaces: HashMap<usize, i64>,
    dxt1_images: HashSet<usize>,
    bc5_normal_images: HashSet<usize>,
    spec_color_only_images: HashSet<usize>,
    unbound_images: HashSet<usize>,
    tex_pid: HashMap<(usize, Option<usize>), i64>,
    tex_name: HashMap<(usize, Option<usize>), String>,
    tex_first_sampler: HashMap<usize, Option<usize>>,
    force_inline_tex: HashSet<i64>,
    image_distinct_samplers: HashMap<usize, HashSet<Option<usize>>>,
    sampler_canon: HashMap<(usize, Option<usize>), Option<usize>>,
    mat_pid: HashMap<usize, i64>,
    default_mat: Option<i64>,
    node_tr: HashMap<usize, i64>,

    visits_left: HashMap<usize, usize>,
    orphan_assets_emitted: bool,
    pending_smr: Vec<(i64, i64, i64, Vec<i64>, Option<usize>, Vec<f32>)>,
    scene_object_pids: Vec<i64>,
    material_entries: Vec<(String, i64, Vec<i64>)>,
    texture_entries: Vec<(String, i64)>,

    animator_controller_entry: Option<(i64, Vec<i64>)>,
    glb_referenced_mats: Vec<i64>,
    recycle_seen: HashMap<String, i64>,
    mesh_pid_by_gltf: HashMap<(usize, usize, i64, Option<usize>), i64>,

    collidable_mesh_keys: HashSet<(usize, usize, Option<usize>)>,
    component_pids: Vec<i64>,
    component_roles: Vec<(i64, Role)>,
    root_go_pid: i64,
    bundle_root_assigned: bool,

    anim_target_go: i64,
    anim_target_recycle: String,
    anim_clip_name_pids: Vec<(String, i64)>,
    meta_pid: i64,
    ab_pid: i64,
    gltf_json: serde_json::Value,
    gltf_buffers: Vec<Vec<u8>>,
    resolve_hash: Option<&'a dyn Fn(&str) -> Option<String>>,
    ext_tex_pptr: HashMap<usize, (i64, i64)>,
    mat_external_pptrs: HashMap<i64, Vec<(i64, i64)>>,
    ext_bundle_files: Vec<String>,
    ext_bundle_fileid: HashMap<String, i64>,
    metadata_dependencies: Vec<String>,
    externals_position: Option<ExternalsPosition>,
    cross_bundle_position: Option<CrossBundlePosition>,
    material_externals_overrides: Option<Vec<(ExternalsPosition, CrossBundlePosition)>>,
    force_default_material: bool,
    toggles: Toggles,
    lod: Option<LodBuildParams>,
    lod_mesh_entries: Vec<(String, i64)>,
    lod_mesh_names: HashMap<(usize, usize), String>,
}

impl<'a> Builder<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        proto: &'a HashMap<String, SerializedType>,
        base: &'a HashMap<String, Value>,
        root_hash: String,
        bundle_name: String,
        keep_forward_plus: bool,
        is_emote: bool,
        is_wearable: bool,
        is_gltf: bool,
        gltf_json: serde_json::Value,
        gltf_buffers: Vec<Vec<u8>>,
        resolve_hash: Option<&'a dyn Fn(&str) -> Option<String>>,
        metadata_dependencies: Vec<String>,
        externals_position: Option<ExternalsPosition>,
        cross_bundle_position: Option<CrossBundlePosition>,
        material_externals_overrides: Option<Vec<(ExternalsPosition, CrossBundlePosition)>>,
        force_default_material: bool,
        toggles: Toggles,
        lod: Option<LodBuildParams>,
    ) -> Self {
        let target = target_from_bundle_name(&bundle_name);
        let glb_guid = pathids::asset_guid(&root_hash);
        Builder {
            proto,
            base,
            root_hash,
            bundle_name,
            keep_forward_plus,
            is_emote,
            is_wearable,
            is_gltf,
            target,
            glb_guid,
            pid: 1,
            objects: BTreeMap::new(),
            order: Vec::new(),
            roles: HashMap::new(),
            glb_role_keys: HashMap::new(),
            colorspaces: HashMap::new(),
            dxt1_images: HashSet::new(),
            bc5_normal_images: HashSet::new(),
            spec_color_only_images: HashSet::new(),
            unbound_images: HashSet::new(),
            tex_pid: HashMap::new(),
            tex_name: HashMap::new(),
            tex_first_sampler: HashMap::new(),
            force_inline_tex: HashSet::new(),
            image_distinct_samplers: HashMap::new(),
            sampler_canon: HashMap::new(),
            mat_pid: HashMap::new(),
            default_mat: None,
            node_tr: HashMap::new(),
            visits_left: HashMap::new(),
            orphan_assets_emitted: false,
            pending_smr: Vec::new(),
            scene_object_pids: Vec::new(),
            material_entries: Vec::new(),
            texture_entries: Vec::new(),
            animator_controller_entry: None,
            glb_referenced_mats: Vec::new(),
            recycle_seen: HashMap::new(),
            mesh_pid_by_gltf: HashMap::new(),
            collidable_mesh_keys: HashSet::new(),
            component_pids: Vec::new(),
            component_roles: Vec::new(),
            root_go_pid: 0,
            bundle_root_assigned: false,
            anim_target_go: 0,
            anim_target_recycle: String::new(),
            anim_clip_name_pids: Vec::new(),
            meta_pid: 0,
            ab_pid: 0,
            gltf_json,
            gltf_buffers,
            resolve_hash,
            ext_tex_pptr: HashMap::new(),
            mat_external_pptrs: HashMap::new(),
            ext_bundle_files: Vec::new(),
            ext_bundle_fileid: HashMap::new(),
            metadata_dependencies,
            externals_position,
            cross_bundle_position,
            material_externals_overrides,
            force_default_material,
            toggles,
            lod,
            lod_mesh_entries: Vec::new(),
            lod_mesh_names: HashMap::new(),
        }
    }

    fn assign_lod_mesh_names(&mut self, scene: &Scene) {
        if self.lod.is_none() {
            return;
        }
        let mut k = 0usize;
        for node in &scene.nodes {
            let n = node.primitives.len();
            if n == 0 {
                continue;
            }
            let mut order: Vec<usize> = (0..n).collect();
            if n >= 2 {
                order.swap(0, 1);
            }
            for pi in order {
                let p = &node.primitives[pi];
                if !p.name.is_empty() {
                    continue;
                }
                let Some(mi) = p.gltf_mesh_index else {
                    continue;
                };
                let key = (mi, p.gltf_prim_index);
                if self.lod_mesh_names.contains_key(&key) {
                    continue;
                }
                self.lod_mesh_names.insert(key, format!("mesh_{k}_{k}"));
                k += 1;
            }
        }
    }

    fn active_shader_pptr(&self) -> Value {
        if self.lod.is_some() {
            crate::value::pptr(SHADER_FILE_ID, crate::shader::TEXARRAY_SHADER_PATH_ID)
        } else {
            shader_pptr()
        }
    }

    fn shader_cab_name(&self) -> String {
        if self.lod.is_some() {
            cabname::cab_name(&crate::shader::texarray_bundle_name(self.target))
        } else {
            cabname::shader_bundle_cab(self.target).to_string()
        }
    }

    fn shader_dep_path_id(&self) -> i64 {
        if self.lod.is_some() {
            crate::shader::TEXARRAY_SHADER_PATH_ID
        } else {
            SHADER_PATH_ID
        }
    }

    const fn npid(&mut self) -> i64 {
        self.pid += 1;
        self.pid
    }

    fn add(&mut self, type_name: &str, tree: Value, role: Role) -> i64 {
        let pid = self.npid();
        self.set_obj(pid, type_name, tree, role);
        pid
    }

    fn set_obj(&mut self, pid: i64, type_name: &str, tree: Value, role: Role) {
        if !self.objects.contains_key(&pid) {
            self.order.push(pid);
        }
        self.objects.insert(pid, (type_name.to_string(), tree));
        let role = self.dedup_glb_role(pid, role);
        self.roles.insert(pid, role);
    }

    fn insert_role(&mut self, pid: i64, role: Role) {
        let role = self.dedup_glb_role(pid, role);
        self.roles.insert(pid, role);
    }

    fn dedup_glb_role(&mut self, pid: i64, role: Role) -> Role {
        let (short_type, recycle) = match role {
            Role::Glb(t, r) | Role::GlbIdx(t, r, _) => (t, r),
            other => return other,
        };
        let owners = self
            .glb_role_keys
            .entry((short_type.clone(), recycle.clone()))
            .or_default();
        let idx = match owners.iter().position(|&p| p == pid) {
            Some(pos) => pos,
            None => {
                owners.push(pid);
                owners.len() - 1
            }
        } as u32;
        if idx == 0 {
            Role::Glb(short_type, recycle)
        } else {
            Role::GlbIdx(short_type, recycle, idx)
        }
    }

    fn base_clone(&self, key: &str) -> Value {
        self.base.get(key).cloned().unwrap_or(Value::Null)
    }

    fn set_muscle_clip_size(&self, clip: &mut Value) {
        const STRIPPED_FIXED_FIELDS_SIZE: i64 = 388;
        let Some(node) = self
            .proto
            .get("AnimationClip")
            .and_then(|st| st.node.as_ref())
        else {
            return;
        };
        let Some(mc_node) = node.m_Children.iter().find(|c| c.m_Name == "m_MuscleClip") else {
            return;
        };
        let Some(mc_val) = clip.get("m_MuscleClip") else {
            return;
        };

        let len = unity::write_typetree(mc_val, mc_node, false).len() as i64;
        clip.insert("m_MuscleClipSize", len + STRIPPED_FIXED_FIELDS_SIZE);
    }

    fn resolve_pathid(&self, role: &Role) -> i64 {
        match role {
            Role::Bundle => 1,
            Role::Glb(short_type, recycle) => {
                let lid = pathids::local_id_for_recycle_name(short_type, recycle);
                pathids::prefab_packed_path_id(&self.glb_guid, lid, GLB_FILE_TYPE)
            }
            Role::GlbIdx(short_type, recycle, idx) => {
                let lid = pathids::local_id_for_recycle_name_indexed(short_type, recycle, *idx);
                pathids::prefab_packed_path_id(&self.glb_guid, lid, GLB_FILE_TYPE)
            }
            Role::Mat(key) => {
                let guid = pathids::asset_guid(&format!("{}/material/{}", self.root_hash, key));
                pathids::prefab_packed_path_id(&guid, 2100000, MAT_FILE_TYPE)
            }
            Role::Tex(key) => {
                let guid = pathids::asset_guid(&format!("{}/texture/{}", self.root_hash, key));
                pathids::prefab_packed_path_id(&guid, 2800000, TEX_FILE_TYPE)
            }
            Role::Meta => {
                let guid = pathids::asset_guid(&format!("{}/metadata", self.root_hash));
                pathids::prefab_packed_path_id(&guid, 4900000, META_FILE_TYPE)
            }
            Role::AnimController => {
                let guid = pathids::asset_guid(&format!("{}/animatorController", self.root_hash));
                pathids::prefab_packed_path_id(&guid, 9100000, MAT_FILE_TYPE)
            }
            Role::AnimControllerSubClip(idx) => {
                let seed = format!("{}/animatorController", self.root_hash);
                let guid = pathids::asset_guid(&seed);
                let fid = pathids::deterministic_sub_asset_path_id(&seed, *idx);
                pathids::prefab_packed_path_id(&guid, fid, MAT_FILE_TYPE)
            }
        }
    }
}

fn is_glb_or_gltf(data: &[u8], ext: &str) -> bool {
    if ext.to_lowercase() == ".gltf" {
        return true;
    }
    data.len() >= 4 && &data[0..4] == b"glTF"
}

#[derive(Debug, Clone)]
pub struct BundleArtifact {
    pub data: Vec<u8>,
    pub image_uri: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct LodBuildParams {
    pub level: u32,
    pub plane_clipping: [f64; 4],
    pub vertical_clipping: [f64; 4],
    pub root_position: [f64; 3],
    pub main_asset: String,
    pub timestamp: Option<i64>,
}

pub struct BuildOpts<'a> {
    pub keep_forward_plus: bool,
    pub source_file: Option<&'a str>,
    pub entity_type: Option<&'a str>,
    pub resolve: gltf::Resolve<'a>,
    pub model_referenced: bool,
    pub resolve_hash: Option<&'a dyn Fn(&str) -> Option<String>>,
    pub metadata_dependencies: &'a [String],
    pub expect_hash: Option<&'a str>,
    pub standalone_color_space: Option<i64>,

    pub standalone_normal: bool,

    pub force_default_material: bool,

    pub magenta_missing: bool,

    pub collection_mode: bool,

    pub real_textures: bool,

    pub v38_compat: bool,

    pub v38_timestamp: i64,

    pub lod: Option<&'a LodBuildParams>,
}

impl<'a> BuildOpts<'a> {
    pub const COLLECTION_MODE_ENV: &'static str = "ABGEN_COLLECTION_MODE";

    pub const REAL_TEXTURES_ENV: &'static str = "ABGEN_REAL_TEXTURES";

    pub const V38_COMPAT_ENV: &'static str = "ABGEN_V38_COMPAT";

    pub const V38_TIMESTAMP_ENV: &'static str = "ABGEN_V38_TIMESTAMP";

    pub const MAGENTA_MISSING_ENV: &'static str = "ABGEN_MAGENTA_MISSING";

    pub fn env_collection_mode() -> bool {
        crate::clihelp::env_bool(Self::COLLECTION_MODE_ENV, false)
    }

    pub fn env_real_textures() -> bool {
        crate::clihelp::env_bool(Self::REAL_TEXTURES_ENV, false)
    }

    pub fn env_v38_compat() -> bool {
        crate::clihelp::env_bool(Self::V38_COMPAT_ENV, false)
    }

    pub fn env_v38_timestamp() -> i64 {
        std::env::var(Self::V38_TIMESTAMP_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0)
    }

    pub fn env_magenta_missing() -> bool {
        crate::clihelp::env_bool(Self::MAGENTA_MISSING_ENV, false)
    }
}

impl<'a> Default for BuildOpts<'a> {
    fn default() -> Self {
        Self {
            keep_forward_plus: true,
            source_file: None,
            entity_type: None,
            resolve: None,
            model_referenced: false,
            resolve_hash: None,
            metadata_dependencies: &[],
            expect_hash: None,
            standalone_color_space: None,
            standalone_normal: false,
            force_default_material: false,
            magenta_missing: false,
            collection_mode: false,
            real_textures: false,
            v38_compat: false,
            v38_timestamp: 0,
            lod: None,
        }
    }
}

pub fn build_bundle(
    bytes: &[u8],
    bundle_name: &str,
    root_hash: &str,
    opts: &BuildOpts<'_>,
) -> Result<BundleArtifact> {
    let src = opts.source_file.unwrap_or("");

    let collection_mode = opts.collection_mode;
    let is_emote = (!collection_mode
        && matches!(opts.entity_type, Some(t) if t.eq_ignore_ascii_case("emote")))
        || src.to_lowercase().ends_with("_emote.glb");
    let ext = if src.to_lowercase().ends_with(".gltf") {
        ".gltf"
    } else {
        ".glb"
    };

    if !is_glb_or_gltf(bytes, ext) {
        let (mut bundle, proto, base) = load_template()?;
        let mut b = StandaloneTextureBuilder::new(
            proto,
            base,
            root_hash.to_string(),
            bundle_name.to_string(),
            opts.source_file.map(|s| s.to_string()),
            opts.model_referenced,
            opts.standalone_color_space,
            opts.standalone_normal,
            Toggles::from_opts(opts),
        );
        let data = b.build(bytes, &mut bundle, None)?;
        return Ok(BundleArtifact {
            data,
            image_uri: Vec::new(),
        });
    }

    let target = target_from_bundle_name(bundle_name);
    let default_pos = ExternalsPosition::for_target(target);
    let default_cb = CrossBundlePosition::for_target(target);

    let (first, n_runs) = build_glb_with_overrides(
        bytes,
        bundle_name,
        root_hash,
        opts,
        ext,
        is_emote,
        None,
        None,
        None,
    )?;

    let Some(expected) = opts.expect_hash else {
        return Ok(first);
    };

    if hash_matches(&first.data, expected) {
        return Ok(first);
    }

    let alt_pos = match default_pos {
        ExternalsPosition::First => ExternalsPosition::Last,
        ExternalsPosition::Last => ExternalsPosition::First,
    };
    let alt_cb = match default_cb {
        CrossBundlePosition::Last => CrossBundlePosition::AfterShader,
        CrossBundlePosition::AfterShader => CrossBundlePosition::Last,
    };

    const MAX_PER_RUN_N: usize = 3;
    if n_runs == 0 {
        for (p, c) in [
            (default_pos, alt_cb),
            (alt_pos, default_cb),
            (alt_pos, alt_cb),
        ] {
            let (cand, _) = build_glb_with_overrides(
                bytes,
                bundle_name,
                root_hash,
                opts,
                ext,
                is_emote,
                Some(p),
                Some(c),
                None,
            )?;
            if hash_matches(&cand.data, expected) {
                return Ok(cand);
            }
        }
        eprintln!(
            "warning: ab-build expect_hash mismatch on all global positions for \
             bundle={bundle_name} root={root_hash} n_runs=0; returning per-target-default \
             build (first attempt)"
        );
        return Ok(first);
    }

    if n_runs <= MAX_PER_RUN_N {
        let total: u32 = 1u32 << (2 * n_runs);
        for mask in 1..total {
            let plan: Vec<(ExternalsPosition, CrossBundlePosition)> = (0..n_runs)
                .map(|i| {
                    let pb = (mask >> (2 * i)) & 1 == 1;
                    let cb = (mask >> (2 * i + 1)) & 1 == 1;
                    let p = if pb { alt_pos } else { default_pos };
                    let c = if cb { alt_cb } else { default_cb };
                    (p, c)
                })
                .collect();
            let (cand, _) = build_glb_with_overrides(
                bytes,
                bundle_name,
                root_hash,
                opts,
                ext,
                is_emote,
                None,
                None,
                Some(plan),
            )?;
            if hash_matches(&cand.data, expected) {
                return Ok(cand);
            }
        }
        eprintln!(
            "warning: ab-build expect_hash mismatch across {} per-run candidates for \
             bundle={bundle_name} root={root_hash} n_runs={n_runs}; returning \
             per-target-default build (first attempt)",
            total
        );
        return Ok(first);
    }

    for (p, c) in [
        (default_pos, alt_cb),
        (alt_pos, default_cb),
        (alt_pos, alt_cb),
    ] {
        let (cand, _) = build_glb_with_overrides(
            bytes,
            bundle_name,
            root_hash,
            opts,
            ext,
            is_emote,
            Some(p),
            Some(c),
            None,
        )?;
        if hash_matches(&cand.data, expected) {
            return Ok(cand);
        }
    }

    eprintln!(
        "warning: ab-build expect_hash mismatch on all global positions for \
         bundle={bundle_name} root={root_hash} n_runs={n_runs} (>cap {MAX_PER_RUN_N}); \
         returning per-target-default build (first attempt)"
    );
    Ok(first)
}

pub fn build_bundle_multi(
    bytes: &[u8],
    bundle_names: &[String],
    root_hash: &str,
    opts: &BuildOpts<'_>,
) -> Result<Vec<BundleArtifact>> {
    let shareable = bundle_names.len() >= 2
        && opts.expect_hash.is_none()
        && opts.lod.is_none()
        && bundle_names
            .iter()
            .all(|n| matches!(target_from_bundle_name(n), "windows" | "mac"));
    if !shareable {
        return bundle_names
            .iter()
            .map(|n| build_bundle(bytes, n, root_hash, opts))
            .collect();
    }

    let src = opts.source_file.unwrap_or("");
    let collection_mode = opts.collection_mode;
    let is_emote = (!collection_mode
        && matches!(opts.entity_type, Some(t) if t.eq_ignore_ascii_case("emote")))
        || src.to_lowercase().ends_with("_emote.glb");
    let ext = if src.to_lowercase().ends_with(".gltf") {
        ".gltf"
    } else {
        ".glb"
    };

    if !is_glb_or_gltf(bytes, ext) {
        let (mut bundle, proto, base) = load_template()?;
        let mut b = StandaloneTextureBuilder::new(
            proto,
            base,
            root_hash.to_string(),
            bundle_names[0].clone(),
            opts.source_file.map(|s| s.to_string()),
            opts.model_referenced,
            opts.standalone_color_space,
            opts.standalone_normal,
            Toggles::from_opts(opts),
        );
        let mut memo = unity::bundle_file::ChunkMemo::default();
        let data = b.build(bytes, &mut bundle, Some(&mut memo))?;
        let mut out = vec![BundleArtifact {
            data,
            image_uri: Vec::new(),
        }];
        for name in &bundle_names[1..] {
            let (mut sibling, _, _) = load_template()?;
            out.push(BundleArtifact {
                data: b.rebuild_for(name, &mut sibling, Some(&mut memo))?,
                image_uri: Vec::new(),
            });
        }
        return Ok(out);
    }

    let (mut bundle, proto, base) = load_template()?;
    let (gltf_json, gltf_buffers) =
        gltf::load_gltf_inputs(bytes, ext, opts.resolve).context("load gltf inputs")?;
    let scene = gltf::parse_with_inputs(
        &gltf_json,
        &gltf_buffers,
        opts.resolve,
        opts.magenta_missing,
        false,
    )
    .context("parse glb")?;
    let image_uri = scene.image_uri.clone();
    let is_gltf = ext == ".gltf";
    let is_wearable = !opts.collection_mode
        && matches!(opts.entity_type, Some(t) if t.eq_ignore_ascii_case("wearable"));
    let mut b = Builder::new(
        proto,
        base,
        root_hash.to_string(),
        bundle_names[0].clone(),
        opts.keep_forward_plus,
        is_emote,
        is_wearable,
        is_gltf,
        gltf_json,
        gltf_buffers,
        opts.resolve_hash,
        opts.metadata_dependencies.to_vec(),
        None,
        None,
        None,
        opts.force_default_material,
        Toggles::from_opts(opts),
        None,
    );
    b.build(&scene)?;
    b.finalize_pathids()?;
    b.commit(&mut bundle)?;
    let mut memo = unity::bundle_file::ChunkMemo::default();
    let mut out = vec![BundleArtifact {
        data: bundle_io::save_bundle_memo(&bundle, &mut memo)?,
        image_uri: image_uri.clone(),
    }];
    for name in &bundle_names[1..] {
        b.retarget(name);
        let (mut sibling, _, _) = load_template()?;
        b.commit(&mut sibling)?;
        out.push(BundleArtifact {
            data: bundle_io::save_bundle_memo(&sibling, &mut memo)?,
            image_uri: image_uri.clone(),
        });
    }
    Ok(out)
}

fn build_glb_with_overrides(
    bytes: &[u8],
    bundle_name: &str,
    root_hash: &str,
    opts: &BuildOpts<'_>,
    ext: &str,
    is_emote: bool,
    externals_position: Option<ExternalsPosition>,
    cross_bundle_position: Option<CrossBundlePosition>,
    material_externals_overrides: Option<Vec<(ExternalsPosition, CrossBundlePosition)>>,
) -> Result<(BundleArtifact, usize)> {
    let (mut bundle, proto, base) = load_template()?;

    let (gltf_json, gltf_buffers) =
        gltf::load_gltf_inputs(bytes, ext, opts.resolve).context("load gltf inputs")?;
    let scene = gltf::parse_with_inputs(
        &gltf_json,
        &gltf_buffers,
        opts.resolve,
        opts.magenta_missing,
        opts.lod.is_some(),
    )
    .context("parse glb")?;
    let image_uri = scene.image_uri.clone();

    let is_gltf = ext == ".gltf";

    let is_wearable = !opts.collection_mode
        && matches!(opts.entity_type, Some(t) if t.eq_ignore_ascii_case("wearable"));
    let mut b = Builder::new(
        proto,
        base,
        root_hash.to_string(),
        bundle_name.to_string(),
        opts.keep_forward_plus,
        is_emote,
        is_wearable,
        is_gltf,
        gltf_json,
        gltf_buffers,
        opts.resolve_hash,
        opts.metadata_dependencies.to_vec(),
        externals_position,
        cross_bundle_position,
        material_externals_overrides,
        opts.force_default_material,
        Toggles::from_opts(opts),
        opts.lod.cloned(),
    );
    b.build(&scene)?;
    b.finalize_pathids()?;
    let n_material_runs = b.material_entries.len();
    b.commit(&mut bundle)?;
    let data = bundle_io::save_bundle(&bundle)?;
    Ok((BundleArtifact { data, image_uri }, n_material_runs))
}

fn hash_matches(bytes: &[u8], expected: &str) -> bool {
    let exp = expected.trim();
    let got = hashes::sha256_hex(bytes);
    got.eq_ignore_ascii_case(exp)
}
