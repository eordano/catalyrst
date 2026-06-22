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

fn shader_pptr() -> Value {
    crate::value::pptr(SHADER_FILE_ID, SHADER_PATH_ID)
}

fn abgen_root() -> std::path::PathBuf {
    std::env::var("ABGEN_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_path_buf()
        })
}

fn template_path() -> PathBuf {
    abgen_root()
        .join("template")
        .join("all-types.windows.bundle")
}

pub fn template_dir() -> PathBuf {
    abgen_root().join("template")
}

/// Whether the build template (`all-types.windows.bundle`, mmapped for every
/// build and re-targeted per platform) is present at the resolved `ABGEN_ROOT`.
/// Callers running the in-process converter should check this at startup — a
/// missing template is a 500 on the first corpus miss, not a build error.
pub fn template_available() -> bool {
    template_path().is_file()
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

fn v38_compat() -> bool {
    std::env::var(BuildOpts::V38_COMPAT_ENV).is_ok()
}

fn collection_mode() -> bool {
    std::env::var(BuildOpts::COLLECTION_MODE_ENV).is_ok()
}

fn emits_metadata_textasset(root_hash: &str) -> bool {
    if v38_compat() {
        return true;
    }
    !root_hash.starts_with("Qm")
}

fn metadata_timestamp() -> i64 {
    if !v38_compat() {
        return 0;
    }
    if let Ok(v) = std::env::var(BuildOpts::V38_TIMESTAMP_ENV) {
        if let Ok(t) = v.trim().parse::<i64>() {
            return t;
        }
    }

    0
}

fn is_psd(raw: &[u8]) -> bool {
    raw.len() >= 4 && &raw[0..4] == b"8BPS"
}

fn png_gamma_is_nontrivial(gama_100k: u32) -> bool {
    let gamma = gama_100k as f64 / 100_000.0;
    if gamma <= 0.0 {
        return false;
    }
    let exp = 1.0 / (gamma * 2.2);

    let mid = (128.0f64 / 255.0).powf(exp) * 255.0;
    (mid - 128.0).abs() >= 0.5
}

fn png_gamma_to_apply(raw: &[u8]) -> Option<u32> {
    if raw.len() < 8 || &raw[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8usize;
    let mut gama: Option<u32> = None;
    let mut has_srgb = false;
    while pos + 8 <= raw.len() {
        let len = u32::from_be_bytes([raw[pos], raw[pos + 1], raw[pos + 2], raw[pos + 3]]) as usize;
        let typ = &raw[pos + 4..pos + 8];
        let dstart = pos + 8;
        let dend = dstart + len;
        if dend + 4 > raw.len() {
            break;
        }
        match typ {
            b"gAMA" if len >= 4 => {
                gama = Some(u32::from_be_bytes([
                    raw[dstart],
                    raw[dstart + 1],
                    raw[dstart + 2],
                    raw[dstart + 3],
                ]));
            }
            b"sRGB" => has_srgb = true,
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        pos = dend + 4;
    }
    match gama {
        Some(g) if !has_srgb && png_gamma_is_nontrivial(g) => Some(g),
        _ => None,
    }
}

fn apply_png_gamma(img: &mut RgbaImage, gama_100k: u32) {
    let gamma = gama_100k as f64 / 100_000.0;
    let exp = 1.0 / (gamma * 2.2);
    let mut lut = [0u8; 256];
    for (v, slot) in lut.iter_mut().enumerate() {
        let out = (v as f64 / 255.0).powf(exp) * 255.0;
        *slot = (out + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    for px in img.as_mut().chunks_exact_mut(4) {
        px[0] = lut[px[0] as usize];
        px[1] = lut[px[1] as usize];
        px[2] = lut[px[2] as usize];
    }
}

fn source_extension(raw: &[u8]) -> &'static str {
    if raw.len() >= 8 && &raw[0..8] == b"\x89PNG\r\n\x1a\n" {
        ".png"
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        ".jpg"
    } else if is_psd(raw) {
        ".psd"
    } else {
        ".png"
    }
}

fn decode_source_image(raw: &[u8]) -> Option<RgbaImage> {
    if is_psd(raw) {
        let p = psd::Psd::from_bytes(raw).ok()?;
        let (w, h) = (p.width(), p.height());
        let rgba = p.rgba();
        if rgba.len() != (w as usize) * (h as usize) * 4 {
            return None;
        }
        return RgbaImage::from_raw(w, h, rgba);
    }

    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        if let Ok((rgba, w, h)) = crate::ffi::decode_jpeg_rgba_box(raw) {
            return RgbaImage::from_raw(w, h, rgba);
        }
    }
    let mut img = image::load_from_memory(raw).ok().map(|d| d.to_rgba8())?;

    if let Some(g) = png_gamma_to_apply(raw) {
        apply_png_gamma(&mut img, g);
    }
    Some(img)
}

fn standalone_key_extension(source_file: Option<&str>, raw: &[u8]) -> String {
    if let Some(sf) = source_file {
        let last_seg = sf.rsplit(['/', '\\']).next().unwrap_or(sf);
        if let Some(dot) = last_seg.rfind('.') {
            let ext = &last_seg[dot..];

            let lo = ext.to_ascii_lowercase();
            if matches!(lo.as_str(), ".png" | ".jpg" | ".jpeg" | ".psd") {
                return ext.to_string();
            }
        }
    }
    source_extension(raw).to_string()
}

fn metadata_version_for_target(target: &str) -> &'static str {
    if v38_compat() {
        return "7.0";
    }
    match target {
        // Genuine Unity Linux64 + WebGL stamp 8.0 exactly like Windows/Mac standalone.
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

fn looks_like_normal_map(rgba: &[u8]) -> bool {
    let n = rgba.len() / 4;
    if n == 0 {
        return false;
    }

    for i in 0..n {
        if rgba[i * 4 + 3] != 255 {
            return false;
        }
    }
    let mut hits = 0usize;
    for i in 0..n {
        let nx = rgba[i * 4] as f64 / 127.5 - 1.0;
        let ny = rgba[i * 4 + 1] as f64 / 127.5 - 1.0;
        let nz = rgba[i * 4 + 2] as f64 / 127.5 - 1.0;
        let mag = (nx * nx + ny * ny + nz * nz).sqrt();
        if (mag - 1.0).abs() < 0.30 && nz >= -0.1 {
            hits += 1;
        }
    }
    (hits as f64 / n as f64) >= 0.95
}

fn pack_normal_map(rgba: &[u8]) -> Vec<u8> {
    let n = rgba.len() / 4;
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        out[i * 4] = 255;
        out[i * 4 + 1] = g;
        out[i * 4 + 2] = g;
        out[i * 4 + 3] = r;
    }
    out
}

const INGLB_BC7_CANONICAL_BLOCK: [u8; 16] = [
    0x20, 0x5a, 0xbf, 0xd6, 0xaf, 0xf5, 0x37, 0x37, 0xaf, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
];

const INGLB_BC7_CANONICAL_BLOCK_NORMAL: [u8; 16] = [
    0x20, 0xff, 0xbf, 0xd6, 0xaf, 0xf5, 0x37, 0x37, 0xaf, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
];

fn encode_inglb_bc7_stub(width: u32, height: u32, mips: i32, normal_lf: bool) -> (Vec<u8>, i32) {
    let total = bc7_pure::compute_mip_chain_size(width, height, mips);
    let blocks = total / 16;
    let block = if normal_lf {
        &INGLB_BC7_CANONICAL_BLOCK_NORMAL
    } else {
        &INGLB_BC7_CANONICAL_BLOCK
    };
    let mut out = Vec::with_capacity(total);
    for _ in 0..blocks {
        out.extend_from_slice(block);
    }
    (out, mips)
}

const INGLB_DXT5_CANONICAL_BLOCK: [u8; 16] = [
    0xcd, 0xcd, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24, 0x7a, 0xd6, 0x57, 0xbe, 0xaa, 0xaa, 0xaa, 0xaa,
];

fn encode_inglb_dxt5_stub(width: u32, height: u32, mips: i32) -> (Vec<u8>, i32) {
    let total = bc7_pure::compute_mip_chain_size(width, height, mips);
    let blocks = total / 16;
    let mut out = Vec::with_capacity(total);
    for _ in 0..blocks {
        out.extend_from_slice(&INGLB_DXT5_CANONICAL_BLOCK);
    }
    (out, mips)
}

fn encode_dxt5_mip_chain_real(img: &RgbaImage, mips: i32) -> (Vec<u8>, i32) {
    let (w, h) = img.dimensions();
    let mut cur: Vec<u8> = {
        let (w, h) = (w as usize, h as usize);
        let src = img.as_raw();
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            out[y * w * 4..(y + 1) * w * 4]
                .copy_from_slice(&src[(h - 1 - y) * w * 4..(h - y) * w * 4]);
        }
        out
    };
    let (mut cw, mut ch) = (w as usize, h as usize);
    let params = texpresso::Params {
        algorithm: texpresso::Algorithm::IterativeClusterFit,
        weights: texpresso::COLOUR_WEIGHTS_PERCEPTUAL,
        weigh_colour_by_alpha: false,
    };
    let mut parts: Vec<u8> = Vec::new();
    for m in 0..mips {
        let pw = cw.max(1);
        let ph = ch.max(1);
        let size = texpresso::Format::Bc3.compressed_size(pw, ph);
        let mut level = vec![0u8; size];
        texpresso::Format::Bc3.compress(&cur, pw, ph, params, &mut level);
        parts.extend_from_slice(&level);
        if m < mips - 1 {
            let (next, nw, nh) = crate::bc5_pure::box_halve_rgba_u8(&cur, cw, ch);
            cur = next;
            cw = nw;
            ch = nh;
        }
    }
    (parts, mips)
}

fn encode_texture_bc7(
    img: &RgbaImage,
    mips: i32,
    srgb: bool,
    normal_override: Option<bool>,
    profile: bc7_pure::Bc7Profile,
) -> (Vec<u8>, i32) {
    let (w, h) = img.dimensions();
    let rgba = img.as_raw();
    let is_normal = normal_override.unwrap_or_else(|| !srgb && looks_like_normal_map(rgba));
    let packed;
    let pixels: &[u8] = if is_normal {
        packed = pack_normal_map(rgba);
        &packed
    } else {
        rgba
    };
    let perceptual = srgb && !is_normal;
    bc7_pure::encode_bc7_mip_chain_with_profile(
        pixels,
        w,
        h,
        Some(mips),
        true,
        srgb,
        perceptual,
        profile,
    )
}

fn aux_types() -> &'static HashMap<String, (SerializedType, Value)> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<HashMap<String, (SerializedType, Value)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out: HashMap<String, (SerializedType, Value)> = HashMap::new();
        let root = abgen_root();

        let harvest = |out: &mut HashMap<String, (SerializedType, Value)>,
                       file: &str,
                       mapping: &[(&str, &str)]| {
            let path = root.join("template").join(file);
            if !path.exists() {
                return;
            }
            let bundle = match Bundle::load(&path) {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(sf) = bundle.serialized() {
                for obj in &sf.objects {
                    for (src, key) in mapping {
                        if obj.type_name == *src && !out.contains_key(*key) {
                            if let Ok(tree) = sf.read_typetree(obj) {
                                let st = sf.types[obj.type_id as usize].clone();
                                out.insert(key.to_string(), (st, tree));
                            }
                        }
                    }
                }
            }
        };

        harvest(
            &mut out,
            "animated-types.windows.bundle",
            &[
                ("Animation", "Animation"),
                ("AnimationClip", "AnimationClip"),
            ],
        );
        harvest(
            &mut out,
            "emote-types.windows.bundle",
            &[
                ("Animator", "Animator"),
                ("AnimatorController", "AnimatorController"),
                ("AnimationClip", "AnimationClip_mecanim"),
            ],
        );
        harvest(
            &mut out,
            "skinned-types.windows.bundle",
            &[("SkinnedMeshRenderer", "SkinnedMeshRenderer")],
        );
        out
    })
}

fn template_mmap() -> Result<&'static memmap2::Mmap> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Result<memmap2::Mmap, String>> = OnceLock::new();
    let entry = CACHE.get_or_init(|| {
        let path = template_path();
        if !path.exists() {
            return Err(format!("template bundle not found at {}", path.display()));
        }
        crate::local_store::mmap_file(&path).map_err(|e| e.to_string())
    });
    entry.as_ref().map_err(|e| anyhow!("{e}"))
}

fn load_template() -> Result<(
    Bundle,
    &'static HashMap<String, SerializedType>,
    &'static HashMap<String, Value>,
)> {
    type Cached = (
        crate::unity::bundle_file::DecompressedBundle,
        std::sync::Mutex<Option<Bundle>>,
        HashMap<String, SerializedType>,
        HashMap<String, Value>,
    );
    static CACHE: std::sync::OnceLock<std::result::Result<Cached, String>> =
        std::sync::OnceLock::new();
    let entry = CACHE.get_or_init(|| {
        let load = || -> Result<Cached> {
            let mm = template_mmap()?;
            let decompressed = Bundle::decompress_bytes(mm)?;
            let bundle = Bundle::from_decompressed(&decompressed)?;
            let mut proto: HashMap<String, SerializedType> = HashMap::new();
            let mut base: HashMap<String, Value> = HashMap::new();
            {
                let sf = bundle
                    .serialized()
                    .ok_or_else(|| anyhow!("template has no serialized file"))?;
                for obj in &sf.objects {
                    if !proto.contains_key(&obj.type_name) {
                        proto.insert(
                            obj.type_name.clone(),
                            sf.types[obj.type_id as usize].clone(),
                        );
                    }
                    if !base.contains_key(&obj.type_name) {
                        base.insert(obj.type_name.clone(), sf.read_typetree(obj)?);
                    }
                }
            }
            for (key, (st, tree)) in aux_types().iter() {
                proto.entry(key.clone()).or_insert_with(|| st.clone());
                base.entry(key.clone()).or_insert_with(|| tree.clone());
            }
            Ok((
                decompressed,
                std::sync::Mutex::new(Some(bundle)),
                proto,
                base,
            ))
        };
        load().map_err(|e| e.to_string())
    });
    match entry {
        Ok((decompressed, first_bundle, proto, base)) => {
            let bundle = match first_bundle.lock().unwrap().take() {
                Some(b) => b,
                None => Bundle::from_decompressed(decompressed)?,
            };
            Ok((bundle, proto, base))
        }
        Err(e) => Err(anyhow!("{e}")),
    }
}

fn cab_node_name(bundle: &Bundle) -> Result<String> {
    bundle
        .files
        .iter()
        .find(|e| !e.name.to_lowercase().ends_with(".ress"))
        .map(|e| e.name.clone())
        .ok_or_else(|| anyhow!("no SerializedFile node found in bundle container"))
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
    glb_bytes: Vec<u8>,
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
        glb_bytes: Vec<u8>,
        gltf_json: serde_json::Value,
        gltf_buffers: Vec<Vec<u8>>,
        resolve_hash: Option<&'a dyn Fn(&str) -> Option<String>>,
        metadata_dependencies: Vec<String>,
        externals_position: Option<ExternalsPosition>,
        cross_bundle_position: Option<CrossBundlePosition>,
        material_externals_overrides: Option<Vec<(ExternalsPosition, CrossBundlePosition)>>,
        force_default_material: bool,
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
            glb_bytes,
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

    fn source_image(&self, scene: &Scene, idx: usize) -> texprofile::SourceImage {
        let img = scene.images[idx].as_ref().unwrap();
        let (w, h) = img.dimensions();
        let container = scene
            .image_bytes
            .get(idx)
            .and_then(|o| o.as_ref())
            .map(|raw| detect_container(raw))
            .unwrap_or_default();
        let has_real_alpha = img.as_raw().iter().skip(3).step_by(4).any(|&a| a < 255);
        texprofile::SourceImage {
            width: w,
            height: h,
            container,
            has_real_alpha,
        }
    }

    fn external_texture(&mut self, scene: &Scene, img_idx: Option<usize>) -> Option<(i64, i64)> {
        let idx = img_idx?;
        if idx >= scene.image_uri.len() {
            return None;
        }
        let uri = scene.image_uri[idx].clone()?;
        if uri.is_empty() {
            return None;
        }
        let resolver = self.resolve_hash?;
        if let Some(p) = self.ext_tex_pptr.get(&idx) {
            return Some(*p);
        }
        let ext_hash = resolver(&uri)?;
        if ext_hash.is_empty() {
            return None;
        }
        let bundle_file =
            crate::naming::canonical_filename(&ext_hash, ".png", self.target, None).ok()?;
        let file_id = match self.ext_bundle_fileid.get(&bundle_file) {
            Some(&f) => f,
            None => {
                let fid = 2 + self.ext_bundle_files.len() as i64;
                self.ext_bundle_fileid.insert(bundle_file.clone(), fid);
                self.ext_bundle_files.push(bundle_file);
                fid
            }
        };
        let tex_guid = pathids::asset_guid(&ext_hash);
        let tex_pid = pathids::prefab_packed_path_id(
            &tex_guid,
            TEXTURE_LOCAL_ID,
            pathids::FILE_TYPE_META_ASSET,
        );
        let pptr = (file_id, tex_pid);
        self.ext_tex_pptr.insert(idx, pptr);
        Some(pptr)
    }

    fn build_sampler_canon(&mut self, scene: &Scene) {
        let effective = |idx: usize,
                         raw: Option<usize>|
         -> (Option<i64>, Option<i64>, Option<i64>, Option<i64>) {
            match raw.and_then(|si| scene.samplers.get(si).copied()) {
                Some(s) => (s.mag_filter, s.min_filter, s.wrap_s, s.wrap_t),
                None => {
                    let (mag, mn) = scene
                        .image_sampler
                        .get(idx)
                        .copied()
                        .unwrap_or((None, None));
                    let (ws, wt) = scene.image_wrap.get(idx).copied().unwrap_or((None, None));
                    (mag, mn, ws, wt)
                }
            }
        };

        let mut per_image_first: HashMap<
            (usize, (Option<i64>, Option<i64>, Option<i64>, Option<i64>)),
            Option<usize>,
        > = HashMap::new();
        for tr in &scene.texture_refs {
            let sig = effective(tr.image, tr.sampler);
            let canon = *per_image_first.entry((tr.image, sig)).or_insert(tr.sampler);
            self.sampler_canon.insert((tr.image, tr.sampler), canon);
        }
    }

    fn texture(&mut self, scene: &Scene, tex: Option<TexRef>) -> Option<i64> {
        let tex = tex?;
        let idx = tex.image;
        if idx >= scene.images.len() || scene.images[idx].is_none() {
            return None;
        }

        if !texprofile::unity_load_image_would_succeed(&self.source_image(scene, idx)) {
            return None;
        }

        let canon = self
            .sampler_canon
            .get(&(idx, tex.sampler))
            .copied()
            .unwrap_or(tex.sampler);
        let key = (idx, canon);
        if let Some(&pid) = self.tex_pid.get(&key) {
            return Some(pid);
        }

        let first_sampler = *self.tex_first_sampler.entry(idx).or_insert(canon);
        let name = if canon == first_sampler {
            format!("image_{idx}")
        } else {
            match canon {
                Some(s) => format!("image_{idx}_sampler{s}"),
                None => format!("image_{idx}"),
            }
        };
        self.tex_name.insert(key, name.clone());

        let colorspace = *self.colorspaces.get(&idx).unwrap_or(&1);
        let sampler = canon.and_then(|si| scene.samplers.get(si).copied());
        let (mag, mn, ws, wt) = match sampler {
            Some(s) => (s.mag_filter, s.min_filter, s.wrap_s, s.wrap_t),
            None => {
                let (mag, mn) = scene
                    .image_sampler
                    .get(idx)
                    .copied()
                    .unwrap_or((None, None));
                let (ws, wt) = scene.image_wrap.get(idx).copied().unwrap_or((None, None));
                (mag, mn, ws, wt)
            }
        };
        let is_normal = scene.normal_images.contains(&idx);
        let src = self.source_image(scene, idx);
        let is_dxt1 = self.dxt1_images.contains(&idx);

        let is_bc5_normal = self.bc5_normal_images.contains(&idx);

        let (mut unc_p, mut bc7_p) = if is_bc5_normal {
            texprofile::texture_profile_bc5_normal(
                &src,
                colorspace,
                mag,
                mn,
                texprofile::max_texture_size_for(self.target),
            )
        } else if is_dxt1 {
            texprofile::texture_profile_dxt1(
                &src,
                colorspace,
                mag,
                mn,
                texprofile::max_texture_size_for(self.target),
            )
        } else {
            texprofile::texture_profile(
                &src,
                colorspace,
                is_normal,
                mag,
                mn,
                texprofile::max_texture_size_for(self.target),
            )
        };

        if self.spec_color_only_images.contains(&idx) {
            unc_p.color_space = 0;
            if bc7_p.texture_format == texprofile::TF_BC7 {
                bc7_p.texture_format = texprofile::TF_DXT5;
            }
        }

        if self.unbound_images.contains(&idx) && bc7_p.compressed {
            bc7_p.texture_format = texprofile::TF_DXT5;
            bc7_p.color_space = 1;
        }

        // Genuine Unity WebGL emits every compressed Texture2D as DXT5 (BC3,
        // format 12) — never BC7/DXT1/BC5 — and stores the image data inline.
        // Collapse whatever compressed format the per-platform profile picked
        // (BC7, DXT1, or BC5-normal) to DXT5 while preserving the rest of the
        // profile (dims, mips, color_space, lightmap_format, alpha-optional).
        if self.target == "webgl" && bc7_p.compressed {
            bc7_p.texture_format = texprofile::TF_DXT5;
        }

        let unc_wrap_u = texprofile::sampler_wrap_mode(ws);
        let unc_wrap_v = texprofile::sampler_wrap_mode(wt);
        let img = scene.images[idx].clone().unwrap();

        let n_distinct_samplers = self
            .image_distinct_samplers
            .get(&idx)
            .map(|s| s.len())
            .unwrap_or(1);
        let multi_sampler_uncompressed = !unc_p.compressed && n_distinct_samplers > 1;

        if !v38_compat() {
            let mut inglb_tree = self.texture_tree_with_wrap(
                &img,
                &name,
                &unc_p,
                Some((unc_wrap_u, unc_wrap_v)),
                Some(&src),
                false,
            );
            if multi_sampler_uncompressed {
                inglb_tree.insert("m_IsReadable", true);
            }
            let inglb = self.add(
                "Texture2D",
                inglb_tree,
                Role::Glb("Texture2D".into(), format!("textures/{name}")),
            );
            if multi_sampler_uncompressed {
                self.force_inline_tex.insert(inglb);
            }
            self.scene_object_pids.push(inglb);
        }

        let real_tex = std::env::var(BuildOpts::REAL_TEXTURES_ENV).is_ok();
        let ext_tree = self.texture_tree_with_wrap(
            &img,
            &name,
            &bc7_p,
            None,
            Some(&src),
            !real_tex && !multi_sampler_uncompressed,
        );
        let ext = self.add("Texture2D", ext_tree, Role::Tex(name.clone()));
        self.tex_pid.insert(key, ext);
        self.texture_entries.push((format!("{name}.png"), ext));
        Some(ext)
    }

    fn texture_tree_with_wrap(
        &self,
        img: &RgbaImage,
        name: &str,
        prof: &texprofile::Profile,
        wrap: Option<(i64, i64)>,
        src: Option<&texprofile::SourceImage>,
        force_inglb_stub: bool,
    ) -> Value {
        let mut t = self.base_clone("Texture2D");
        let (data, mips): (Vec<u8>, i32) = if prof.compressed {
            let (ow, oh) = img.dimensions();
            let max_size = texprofile::max_texture_size_for(self.target);
            let load_image_ok = src
                .map(texprofile::unity_load_image_would_succeed)
                .unwrap_or(true);

            let stub_bc7 = prof.texture_format == texprofile::TF_BC7
                && prof.color_space == 1
                && (ow > max_size || oh > max_size)
                && load_image_ok
                && std::env::var(BuildOpts::REAL_TEXTURES_ENV).is_err();
            if force_inglb_stub && prof.texture_format == texprofile::TF_DXT5 {
                let (data, mips) =
                    encode_inglb_dxt5_stub(prof.target_w, prof.target_h, prof.mip_count);
                t.insert("m_Name", name);
                t.insert("m_Width", prof.target_w);
                t.insert("m_Height", prof.target_h);
                t.insert("m_TextureFormat", prof.texture_format);
                t.insert("m_MipCount", mips);
                t.insert("m_CompleteImageSize", data.len() as i64);
                t.insert("m_IsReadable", false);
                t.insert("m_ColorSpace", prof.color_space);
                t.insert("m_LightmapFormat", prof.lightmap_format);
                t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
                t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
                if let Some(ts) = t.get_mut("m_TextureSettings") {
                    ts.insert("m_FilterMode", prof.filter_mode);
                    if let Some((wu, wv)) = wrap {
                        ts.insert("m_WrapU", wu);
                        ts.insert("m_WrapV", wv);
                    }
                }
                t.insert("image data", Value::Bytes(data));
                t.insert(
                    "m_StreamData",
                    map! {"offset" => 0, "size" => 0, "path" => ""},
                );
                return t;
            }
            if force_inglb_stub && prof.texture_format == texprofile::TF_BC7 {
                let (data, mips) = encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                );
                t.insert("m_Name", name);
                t.insert("m_Width", prof.target_w);
                t.insert("m_Height", prof.target_h);
                t.insert("m_TextureFormat", prof.texture_format);
                t.insert("m_MipCount", mips);
                t.insert("m_CompleteImageSize", data.len() as i64);
                t.insert("m_IsReadable", false);
                t.insert("m_ColorSpace", prof.color_space);
                t.insert("m_LightmapFormat", prof.lightmap_format);
                t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
                t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
                if let Some(ts) = t.get_mut("m_TextureSettings") {
                    ts.insert("m_FilterMode", prof.filter_mode);
                    if let Some((wu, wv)) = wrap {
                        ts.insert("m_WrapU", wu);
                        ts.insert("m_WrapV", wv);
                    }
                }
                t.insert("image data", Value::Bytes(data));
                t.insert(
                    "m_StreamData",
                    map! {"offset" => 0, "size" => 0, "path" => ""},
                );
                return t;
            }
            let stubbed_src;
            let img: &RgbaImage = if stub_bc7 {
                stubbed_src = mean_color_image(img);
                &stubbed_src
            } else {
                img
            };
            let resized;
            let src: &RgbaImage = if (prof.target_w, prof.target_h) != (ow, oh) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    ow as usize,
                    oh as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            if stub_bc7 && prof.texture_format == texprofile::TF_BC7 {
                encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                )
            } else if prof.texture_format == texprofile::TF_DXT5 {
                encode_dxt5_mip_chain_real(src, prof.mip_count)
            } else if prof.texture_format == texprofile::TF_DXT1 {
                let (sw, sh) = src.dimensions();
                crate::dxt1_pure::encode_dxt1_mip_chain(
                    src.as_raw(),
                    sw,
                    sh,
                    Some(prof.mip_count),
                    true,
                    prof.color_space == 1,
                )
            } else if prof.texture_format == texprofile::TF_BC5 {
                let (sw, sh) = src.dimensions();
                let packed = pack_normal_map(src.as_raw());

                crate::bc5_pure::encode_bc5_normal_crn_mip_chain(
                    &packed,
                    sw,
                    sh,
                    Some(prof.mip_count),
                    true,
                    96,
                )
            } else {
                encode_texture_bc7(
                    src,
                    prof.mip_count,
                    prof.color_space == 1,
                    None,
                    bc7_pure::Bc7Profile::Slow,
                )
            }
        } else if prof.texture_format == texprofile::TF_RGBA32 {
            (dxt_unity::encode_rgba32(img, true), 1)
        } else if prof.texture_format == texprofile::TF_RGBA32_UNITY {
            let (ow, oh) = img.dimensions();
            let resized;
            let src: &RgbaImage = if (prof.target_w, prof.target_h) != (ow, oh) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    ow as usize,
                    oh as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            let (data, mips) = bc7_pure::encode_rgba32_mip_chain(
                src.as_raw(),
                prof.target_w,
                prof.target_h,
                Some(prof.mip_count),
                true,
                prof.color_space == 1,
            );
            if force_inglb_stub {

                (vec![0xcd_u8; data.len()], mips)
            } else {
                (data, mips)
            }
        } else {
            (dxt_unity::encode_rgb24(img, true), 1)
        };

        t.insert("m_Name", name);
        t.insert("m_Width", prof.target_w);
        t.insert("m_Height", prof.target_h);
        t.insert("m_TextureFormat", prof.texture_format);
        t.insert("m_MipCount", mips);
        t.insert("m_CompleteImageSize", data.len() as i64);
        t.insert("m_IsReadable", false);
        t.insert("m_ColorSpace", prof.color_space);
        t.insert("m_LightmapFormat", prof.lightmap_format);
        t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
        t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
        if let Some(ts) = t.get_mut("m_TextureSettings") {
            ts.insert("m_FilterMode", prof.filter_mode);
            if let Some((wu, wv)) = wrap {
                ts.insert("m_WrapU", wu);
                ts.insert("m_WrapV", wv);
            }
        }
        t.insert("image data", Value::Bytes(data));
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    fn default_material(&mut self) -> i64 {
        if self.default_mat.is_none() {
            let base = self.base_clone("Material");
            let tree = materials::build_default_material_tree(&base, &shader_pptr());
            let pid = self.add("Material", tree, Role::Mat("DCL_Scene".into()));
            self.default_mat = Some(pid);
            self.material_entries
                .push(("DCL_Scene.mat".to_string(), pid, vec![]));
        }
        self.default_mat.unwrap()
    }

    fn material(&mut self, scene: &Scene, mat_idx: Option<usize>) -> Option<i64> {
        let oob = matches!(mat_idx, Some(m) if m >= scene.materials.len());
        if (mat_idx.is_none() || oob) && !scene.materials.is_empty() {
            return None;
        }
        Some(self.material_inner(scene, mat_idx, true))
    }

    fn material_orphan(&mut self, scene: &Scene, mat_idx: Option<usize>) -> i64 {
        self.material_inner(scene, mat_idx, false)
    }

    fn material_inner(
        &mut self,
        scene: &Scene,
        mat_idx: Option<usize>,
        referenced_by_glb: bool,
    ) -> i64 {
        let mat_idx = match mat_idx {
            Some(m) if m < scene.materials.len() => m,

            _ => {
                if scene.materials.is_empty() {
                    return 0;
                }
                let pid = self.default_material();
                if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
                    self.glb_referenced_mats.push(pid);
                }
                return pid;
            }
        };
        if let Some(&pid) = self.mat_pid.get(&mat_idx) {
            if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
                self.glb_referenced_mats.push(pid);
            }
            return pid;
        }
        let m = &scene.materials[mat_idx];

        let index = mat_idx;
        let name = materials::material_name(m, index);
        let mut tex_pid: HashMap<String, (i64, i64)> = HashMap::new();
        let mut local_tex_pids: Vec<i64> = Vec::new();
        let mut ext_pptrs: Vec<(i64, i64)> = Vec::new();
        for (slot, accessor) in materials::MATERIAL_TEXTURE_SLOTS.iter() {
            let tex = accessor(m);

            if let Some(pp) = self.external_texture(scene, tex.as_ref().map(|t| t.image)) {
                tex_pid.insert(slot.to_string(), pp);
                ext_pptrs.push(pp);
                continue;
            }
            if let Some(pid) = self.texture(scene, tex) {
                tex_pid.insert(slot.to_string(), (0, pid));
                local_tex_pids.push(pid);
            }
        }
        let base = self.base_clone("Material");
        let tree = materials::build_material_tree(
            &base,
            m,
            index,
            &shader_pptr(),
            self.keep_forward_plus,
            &tex_pid,
        );
        let pid = self.add("Material", tree, Role::Mat(name.clone()));
        self.mat_pid.insert(mat_idx, pid);
        self.material_entries
            .push((format!("{name}.mat"), pid, local_tex_pids));
        if !ext_pptrs.is_empty() {
            self.mat_external_pptrs.insert(pid, ext_pptrs);
        }
        if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
            self.glb_referenced_mats.push(pid);
        }
        pid
    }

    fn mesh_tree(
        &self,
        prim: &Primitive,
        usage_flags: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> Value {
        let mut t = self.base_clone("Mesh");
        let n = prim.positions.len();

        let use_u16 = prim.from_draco && n <= u16::MAX as usize + 1;
        let mut idx_bytes: Vec<u8> =
            Vec::with_capacity(prim.indices.len() * if use_u16 { 2 } else { 4 });
        if use_u16 {
            for i in &prim.indices {
                idx_bytes.extend_from_slice(&(*i as u16).to_le_bytes());
            }
        } else {
            for i in &prim.indices {
                idx_bytes.extend_from_slice(&i.to_le_bytes());
            }
        }
        let (data, channels) = gltf::vertex_buffer(prim);
        let (center, extent) = gltf::aabb(
            &prim.positions,
            prim.position_min_decl,
            prim.position_max_decl,
        );
        t.insert("m_Name", prim.name.clone());
        if let Some(bps) = bind_poses {
            let bp: Vec<Value> = bps
                .iter()
                .map(|bp| mesh_layout::bind_pose_tree(*bp))
                .collect();
            t.insert("m_BindPose", Value::Array(bp));
            if let (Some(weights), Some(joints)) = (&prim.weights, &prim.joints) {
                t.insert(
                    "m_BonesAABB",
                    mesh_layout::compute_bones_aabb(
                        &prim.positions,
                        weights,
                        joints,
                        bps,
                        &prim.morph_targets,
                    ),
                );
            }
        }
        let submesh = map! {
            "firstByte" => 0,
            "indexCount" => prim.indices.len() as i64,
            "topology" => 0,
            "baseVertex" => 0,
            "firstVertex" => 0,
            "vertexCount" => n as i64,
            "localAABB" => map!{"m_Center" => center.clone(), "m_Extent" => extent.clone()},
        };
        t.insert("m_SubMeshes", Value::Array(vec![submesh]));
        t.insert("m_IndexFormat", if use_u16 { 0i64 } else { 1i64 });
        t.insert("m_IndexBuffer", Value::Bytes(idx_bytes));
        t.insert(
            "m_VertexData",
            map! {
                "m_VertexCount" => n as i64,
                "m_Channels" => Value::Array(channels),
                "m_DataSize" => Value::Bytes(data),
            },
        );
        t.insert(
            "m_LocalAABB",
            map! {"m_Center" => center, "m_Extent" => extent},
        );

        let has_morph = !prim.morph_targets.is_empty();
        let (m_shapes, _has_shapes) =
            mesh_layout::build_m_shapes(&prim.morph_targets, &prim.morph_target_names);
        t.insert("m_Shapes", m_shapes);
        let final_usage = if has_morph { 1 } else { usage_flags };
        t.insert("m_MeshUsageFlags", final_usage);
        if has_morph {
            t.insert("m_KeepVertices", true);
        }
        if final_usage == 36 {
            t.insert("m_KeepIndices", true);
            t.insert("m_KeepVertices", true);
        }
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    fn unique_recycle(&mut self, prefix: &str, name: &str) -> String {
        let key = format!("{prefix}/{name}");
        let n = *self.recycle_seen.get(&key).unwrap_or(&0);
        self.recycle_seen.insert(key.clone(), n + 1);
        if n == 0 {
            key
        } else {
            format!("{key}_{}", n - 1)
        }
    }

    fn add_mesh(
        &mut self,
        prim: &Primitive,
        usage: i64,
        bind_poses: Option<&[[f64; 16]]>,
        mesh_base: &str,
    ) -> i64 {
        if let Some(mi) = prim.gltf_mesh_index {
            let key = (mi, prim.gltf_prim_index, usage, prim.skin_index);
            if let Some(&pid) = self.mesh_pid_by_gltf.get(&key) {
                return pid;
            }
            let recycle = self.unique_recycle("meshes", mesh_base);
            let pid = self.add(
                "Mesh",
                self.mesh_tree(prim, usage, bind_poses),
                Role::Glb("Mesh".into(), recycle),
            );
            self.mesh_pid_by_gltf.insert(key, pid);
            return pid;
        }
        let recycle = self.unique_recycle("meshes", mesh_base);
        self.add(
            "Mesh",
            self.mesh_tree(prim, usage, bind_poses),
            Role::Glb("Mesh".into(), recycle),
        )
    }

    fn mesh_tree_merged(
        &self,
        prims: &[&Primitive],
        usage_flags: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> Value {
        let p0 = prims[0];
        let mut t = self.base_clone("Mesh");
        let n = p0.positions.len();
        let (data, channels) = gltf::vertex_buffer(p0);
        let (center, extent) =
            gltf::aabb(&p0.positions, p0.position_min_decl, p0.position_max_decl);

        let merged_from_draco = prims.iter().all(|p| p.from_draco);
        let use_u16 = merged_from_draco && n <= u16::MAX as usize + 1;
        let idx_width: i64 = if use_u16 { 2 } else { 4 };
        let mut idx_bytes: Vec<u8> = Vec::new();
        let mut submeshes: Vec<Value> = Vec::with_capacity(prims.len());
        let mut first_byte: i64 = 0;
        for p in prims {
            if use_u16 {
                for i in &p.indices {
                    idx_bytes.extend_from_slice(&(*i as u16).to_le_bytes());
                }
            } else {
                for i in &p.indices {
                    idx_bytes.extend_from_slice(&i.to_le_bytes());
                }
            }
            let count = p.indices.len() as i64;
            submeshes.push(map! {
                "firstByte" => first_byte,
                "indexCount" => count,
                "topology" => 0,
                "baseVertex" => 0,
                "firstVertex" => 0,
                "vertexCount" => n as i64,
                "localAABB" => map!{"m_Center" => center.clone(), "m_Extent" => extent.clone()},
            });
            first_byte += count * idx_width;
        }
        t.insert("m_Name", p0.name.clone());
        if let Some(bps) = bind_poses {
            let bp: Vec<Value> = bps
                .iter()
                .map(|bp| mesh_layout::bind_pose_tree(*bp))
                .collect();
            t.insert("m_BindPose", Value::Array(bp));
            if let (Some(weights), Some(joints)) = (&p0.weights, &p0.joints) {
                t.insert(
                    "m_BonesAABB",
                    mesh_layout::compute_bones_aabb(
                        &p0.positions,
                        weights,
                        joints,
                        bps,
                        &p0.morph_targets,
                    ),
                );
            }
        }
        t.insert("m_SubMeshes", Value::Array(submeshes));
        t.insert("m_IndexFormat", if use_u16 { 0i64 } else { 1i64 });
        t.insert("m_IndexBuffer", Value::Bytes(idx_bytes));
        t.insert(
            "m_VertexData",
            map! {
                "m_VertexCount" => n as i64,
                "m_Channels" => Value::Array(channels),
                "m_DataSize" => Value::Bytes(data),
            },
        );
        t.insert(
            "m_LocalAABB",
            map! {"m_Center" => center, "m_Extent" => extent},
        );

        let lod_subs: Vec<Value> = (0..prims.len())
            .map(|_| {
                map! {
                    "m_Levels" => Value::Array(vec![
                        map!{"m_IndexStart" => 0, "m_IndexCount" => 0}
                    ]),
                }
            })
            .collect();
        let mut lod_info = t.get("m_MeshLodInfo").cloned().unwrap_or_else(Value::map);
        lod_info.insert("m_SubMeshes", Value::Array(lod_subs));
        t.insert("m_MeshLodInfo", lod_info);

        let has_morph = !p0.morph_targets.is_empty();
        let (m_shapes, _has_shapes) =
            mesh_layout::build_m_shapes(&p0.morph_targets, &p0.morph_target_names);
        t.insert("m_Shapes", m_shapes);
        let final_usage = if has_morph { 1 } else { usage_flags };
        t.insert("m_MeshUsageFlags", final_usage);
        if has_morph {
            t.insert("m_KeepVertices", true);
        }
        if final_usage == 36 {
            t.insert("m_KeepIndices", true);
            t.insert("m_KeepVertices", true);
        }
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    fn add_mesh_merged(&mut self, prims: &[Primitive], mesh_base: &str, usage: i64) -> i64 {
        let recycle = self.unique_recycle("meshes", mesh_base);
        let refs: Vec<&Primitive> = prims.iter().collect();
        let tree = self.mesh_tree_merged(&refs, usage, None);
        let pid = self.add("Mesh", tree, Role::Glb("Mesh".into(), recycle));
        if let Some(mi) = prims[0].gltf_mesh_index {
            let key = (mi, prims[0].gltf_prim_index, usage, prims[0].skin_index);
            self.mesh_pid_by_gltf.entry(key).or_insert(pid);
        }
        pid
    }

    fn add_mesh_merged_v38(
        &mut self,
        prims: &[&Primitive],
        mesh_base: &str,
        usage: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> i64 {
        let p0 = prims[0];
        if let Some(mi) = p0.gltf_mesh_index {
            let key = (mi, p0.gltf_prim_index, usage, p0.skin_index);
            if let Some(&pid) = self.mesh_pid_by_gltf.get(&key) {
                return pid;
            }
        }
        let recycle = self.unique_recycle("meshes", mesh_base);
        let tree = self.mesh_tree_merged(prims, usage, bind_poses);
        let pid = self.add("Mesh", tree, Role::Glb("Mesh".into(), recycle));
        if let Some(mi) = p0.gltf_mesh_index {
            let key = (mi, p0.gltf_prim_index, usage, p0.skin_index);
            self.mesh_pid_by_gltf.insert(key, pid);
        }
        pid
    }

    fn try_attach_primitives_merged(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        parent_tr: i64,
        node: &crate::scene::Node,
        node_path: &str,
        mesh_base: &str,
    ) -> Option<Vec<i64>> {
        let prims = &node.primitives;
        if prims.len() < 2 {
            return None;
        }
        if std::env::var(BuildOpts::V38_COMPAT_ENV).is_ok() {
            return self
                .try_attach_clusters_v38(scene, go_pid, parent_tr, node, node_path, mesh_base);
        }
        let sig0 = match &prims[0].gltf_attr_sig {
            Some(s) => s.clone(),
            None => return None,
        };
        if prims[0].skin_index.is_some() || !prims[0].morph_targets.is_empty() {
            return None;
        }
        for p in &prims[1..] {
            match &p.gltf_attr_sig {
                Some(s) if *s == sig0 => {}
                _ => return None,
            }
            if p.skin_index.is_some() || !p.morph_targets.is_empty() {
                return None;
            }
        }

        if node.is_collider {
            let usage: i64 = 16;
            let mesh_pid = self.add_mesh_merged(prims, mesh_base, usage);
            self.scene_object_pids.push(mesh_pid);

            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
            }
            let mf = self.add(
                "MeshFilter",
                map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
                Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
            );
            let mut mc = self.base_clone("MeshCollider");
            mc.insert("m_GameObject", crate::value::pptr(0, go_pid));
            mc.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
            let mc_pid = self.add(
                "MeshCollider",
                mc,
                Role::Glb("MeshCollider".into(), format!("{node_path}/MeshCollider")),
            );
            self.scene_object_pids.push(mf);
            self.scene_object_pids.push(mc_pid);
            self.component_pids = vec![mf, mc_pid];
            self.component_roles = Vec::new();
            return Some(Vec::new());
        }

        let usage: i64 = if self.mesh_collidable(&prims[0]) { 16 } else { 0 };
        let mesh_pid = self.add_mesh_merged(prims, mesh_base, usage);
        self.scene_object_pids.push(mesh_pid);
        let mf = self.add(
            "MeshFilter",
            map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
            Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
        );
        self.scene_object_pids.push(mf);
        let mat_pids: Vec<i64> = prims
            .iter()
            .filter_map(|p| self.material(scene, p.material_index))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert(
            "m_Materials",
            Value::Array(mat_pids.iter().map(|p| crate::value::pptr(0, *p)).collect()),
        );
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
        Some(Vec::new())
    }

    fn try_attach_clusters_v38(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        parent_tr: i64,
        node: &crate::scene::Node,
        node_path: &str,
        mesh_base: &str,
    ) -> Option<Vec<i64>> {
        type ClusterKey = (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Vec<i64>,
            Option<i64>,
            Vec<crate::scene::MorphSig>,
        );
        let prims = &node.primitives;
        let mut clusters: Vec<(ClusterKey, Vec<&Primitive>)> = Vec::new();
        for p in prims {
            let sig = p.gltf_attr_sig.as_ref()?;
            let key: ClusterKey = (
                sig.position,
                sig.normal,
                sig.tangent,
                sig.texcoords.clone(),
                sig.color,
                p.gltf_morph_sig.clone(),
            );
            match clusters.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push(p),
                None => clusters.push((key, vec![p])),
            }
        }

        if clusters.iter().all(|(_, v)| v.len() == 1) {
            return None;
        }

        self.attach_cluster_v38(
            scene,
            go_pid,
            &clusters[0].1,
            node.is_collider,
            true,
            node_path,
            mesh_base,
            node.extra_colliders,
        );
        let node_components = self.component_pids.clone();
        let node_roles = std::mem::take(&mut self.component_roles);

        let mut extra_child_transforms: Vec<i64> = Vec::new();
        let child_base = if mesh_base.is_empty() { "Primitive" } else { mesh_base };
        for (ci, (_, cluster)) in clusters.iter().enumerate().skip(1) {
            let child_name = format!("{child_base}_{ci}");
            let child_path = format!("{node_path}/{child_name}");
            let cgo = self.npid();
            let ctr = self.npid();
            self.attach_cluster_v38(
                scene,
                cgo,
                cluster,
                node.is_collider,
                false,
                &child_path,
                mesh_base,
                0,
            );
            let mut ccomp = vec![ctr];
            ccomp.extend(self.component_pids.clone());
            let go_tree = self.go_tree(&child_name, &ccomp);
            self.set_obj(
                cgo,
                "GameObject",
                go_tree,
                Role::Glb("GameObject".into(), child_path.clone()),
            );
            let tr_tree = self.transform_tree(
                cgo,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                &[],
                parent_tr,
            );
            self.set_obj(
                ctr,
                "Transform",
                tr_tree,
                Role::Glb("Transform".into(), format!("{child_path}/Transform")),
            );
            for (cp, cr) in std::mem::take(&mut self.component_roles) {
                self.insert_role(cp, cr);
            }
            self.scene_object_pids.push(cgo);
            self.scene_object_pids.push(ctr);
            extra_child_transforms.push(ctr);
        }

        self.component_pids = node_components;
        self.component_roles = node_roles;
        Some(extra_child_transforms)
    }

    fn attach_cluster_v38(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        prims: &[&Primitive],
        is_collider: bool,
        is_parent_prim: bool,
        node_path: &str,
        mesh_base: &str,
        extra_colliders: usize,
    ) {
        let p0 = prims[0];
        let has_smr_proto = self.proto.contains_key("SkinnedMeshRenderer");
        let is_skinned_node = p0.skin_index.is_some() && p0.weights.is_some() && has_smr_proto;
        let has_morph_targets = !p0.morph_targets.is_empty() && has_smr_proto;
        let becomes_smr = is_skinned_node || has_morph_targets;

        let suppress_emission = is_parent_prim && is_collider && becomes_smr;
        let becomes_collider = is_collider && !becomes_smr;

        let usage: i64 = if becomes_smr {
            1
        } else if becomes_collider || self.mesh_collidable(p0) {
            16
        } else {
            0
        };
        let bind_poses: Option<Vec<[f64; 16]>> = if is_skinned_node {
            p0.skin_index
                .filter(|&si| si < scene.skins.len())
                .map(|si| scene.skins[si].bind_poses.clone())
        } else {
            None
        };

        let mesh_usage = if suppress_emission { 0 } else { usage };
        let mesh_pid =
            self.add_mesh_merged_v38(prims, mesh_base, mesh_usage, bind_poses.as_deref());
        self.scene_object_pids.push(mesh_pid);

        if suppress_emission {
            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
            }
            self.component_pids = Vec::new();
            self.component_roles = Vec::new();
            return;
        }

        if becomes_collider {
            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
            }
            let mf = self.add(
                "MeshFilter",
                map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
                Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
            );
            let mut mc = self.base_clone("MeshCollider");
            mc.insert("m_GameObject", crate::value::pptr(0, go_pid));
            mc.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
            let mc_pid = self.add(
                "MeshCollider",
                mc,
                Role::Glb("MeshCollider".into(), format!("{node_path}/MeshCollider")),
            );
            self.scene_object_pids.push(mf);
            self.scene_object_pids.push(mc_pid);
            self.component_pids = vec![mf, mc_pid];
            self.component_roles = Vec::new();

            let go_name = node_path.rsplit('/').next().unwrap_or("");

            let n_extra = extra_colliders
                + usize::from(!is_parent_prim && go_name.to_lowercase().contains("_collider"));
            for idx in 1..=n_extra {
                let mut mc2 = self.base_clone("MeshCollider");
                mc2.insert("m_GameObject", crate::value::pptr(0, go_pid));
                mc2.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
                let mc2_pid = self.add(
                    "MeshCollider",
                    mc2,
                    Role::GlbIdx(
                        "MeshCollider".into(),
                        format!("{node_path}/MeshCollider"),
                        idx as u32,
                    ),
                );
                self.scene_object_pids.push(mc2_pid);
                self.component_pids.push(mc2_pid);
            }
            return;
        }

        if becomes_smr {
            let mat_pids: Vec<i64> = prims
                .iter()
                .filter_map(|p| self.material(scene, p.material_index))
                .collect();
            let smr_pid = self.npid();
            self.set_obj(
                smr_pid,
                "SkinnedMeshRenderer",
                Value::map(),
                Role::Glb(
                    "SkinnedMeshRenderer".into(),
                    format!("{node_path}/SkinnedMeshRenderer"),
                ),
            );
            self.pending_smr.push((
                smr_pid,
                go_pid,
                mesh_pid,
                mat_pids,
                p0.skin_index,
                p0.morph_weights.clone(),
            ));
            self.scene_object_pids.push(smr_pid);
            self.component_pids = vec![smr_pid];
            self.component_roles = Vec::new();
            return;
        }

        let mf = self.add(
            "MeshFilter",
            map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
            Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
        );
        self.scene_object_pids.push(mf);
        let mat_pids: Vec<i64> = prims
            .iter()
            .filter_map(|p| self.material(scene, p.material_index))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert(
            "m_Materials",
            Value::Array(mat_pids.iter().map(|p| crate::value::pptr(0, *p)).collect()),
        );
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
    }

    fn mesh_collidable(&self, prim: &Primitive) -> bool {
        match prim.gltf_mesh_index {
            Some(mi) => self
                .collidable_mesh_keys
                .contains(&(mi, prim.gltf_prim_index, prim.skin_index)),
            None => false,
        }
    }

    fn collect_collidable_mesh_keys(&mut self, scene: &Scene) {
        self.collidable_mesh_keys.clear();
        for node in &scene.nodes {
            if !node.is_collider {
                continue;
            }
            for p in &node.primitives {
                if let Some(mi) = p.gltf_mesh_index {
                    self.collidable_mesh_keys
                        .insert((mi, p.gltf_prim_index, p.skin_index));
                }
            }
        }
    }

    fn build(&mut self, scene: &Scene) -> Result<()> {
        self.collect_collidable_mesh_keys(scene);
        self.colorspaces = materials::classify_texture_colorspaces(scene);
        self.dxt1_images = materials::classify_dxt1_images(scene);
        self.bc5_normal_images = materials::classify_bc5_normal_images(scene);
        self.spec_color_only_images = materials::classify_spec_color_only_images(scene);
        self.unbound_images = materials::classify_unbound_images(scene);

        self.build_sampler_canon(scene);
        for tr in &scene.texture_refs {
            self.image_distinct_samplers
                .entry(tr.image)
                .or_default()
                .insert(tr.sampler);
        }

        for tr in &scene.texture_refs {
            let img = tr.image;
            let is_external = img < scene.image_uri.len() && scene.image_uri[img].is_some();
            if !is_external {
                self.texture(scene, Some(*tr));
            }
        }

        if v38_compat() || collection_mode() || self.force_default_material {
            let _ = self.default_material();
        }

        let glb_is_binary = self.glb_bytes.len() >= 4 && &self.glb_bytes[0..4] == b"glTF";

        let mut has_anim = !self.is_wearable
            && ((self.is_emote && glb_is_binary && self.proto.contains_key("AnimatorController"))
                || (!self.is_emote && self.proto.contains_key("AnimationClip")));

        let mut prebuilt_clips: Option<Vec<Value>> = None;
        if has_anim {
            let clips = if self.is_emote {
                let base_clip = self.base_clone("AnimationClip_mecanim");
                animation_mecanim::build_mecanim_clips(&self.glb_bytes, &base_clip)
            } else {
                animation::build_animation_clips_from_gltf(&self.gltf_json, &self.gltf_buffers)
            };
            has_anim = !clips.is_empty();
            prebuilt_clips = Some(clips);
        }

        let scene_name: &str = scene.name.as_deref().unwrap_or("");
        let scene_path = format!("scenes/{scene_name}");

        let wrap = scene.root_nodes.len() != 1 || has_anim;

        self.count_scene_visits(scene, &scene.root_nodes);

        let root_go;
        if !wrap {
            let (go, _tr) = self.build_node(scene, scene.root_nodes[0], 0, &scene_path);
            root_go = go;
        } else {
            let wrap_is_importer_parent = scene.root_nodes.len() <= 1 && has_anim;
            let wrap_inner: &str = if !scene_name.is_empty() {
                scene_name
            } else if wrap_is_importer_parent {
                "New Game Object"
            } else {
                "Scene"
            };

            let inner_name: &str = if scene_name.is_empty() {
                "Scene"
            } else {
                scene_name
            };
            let wrap_path = format!("{scene_path}/{wrap_inner}");
            root_go = self.npid();
            let root_tr = self.npid();

            let has_anim_component = self.proto.contains_key("AnimatorController")
                || self.proto.contains_key("AnimatorOverrideController")
                || self.proto.contains_key("Animation");
            let inline_root: Option<usize> = if scene.root_nodes.len() == 1 {
                let ri = scene.root_nodes[0];
                let r = &scene.nodes[ri];
                let identity_trs = r.translation == [0.0, 0.0, 0.0]
                    && r.rotation == [0.0, 0.0, 0.0, 1.0]
                    && r.scale == [1.0, 1.0, 1.0];
                if r.primitives.is_empty()
                    && !r.children.is_empty()
                    && identity_trs
                    && !has_anim_component
                {
                    Some(ri)
                } else {
                    None
                }
            } else {
                None
            };

            let empty_anim_scene = scene.root_nodes.is_empty() && has_anim;
            let (wrap_t, wrap_r, wrap_s, child_transforms) = if empty_anim_scene {
                let inner_path = format!("{wrap_path}/{inner_name}");
                let inner_go = self.npid();
                let inner_tr = self.npid();
                let inner_tr_tree = self.transform_tree(
                    inner_go,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    &[],
                    root_tr,
                );
                self.set_obj(
                    inner_tr,
                    "Transform",
                    inner_tr_tree,
                    Role::Glb("Transform".into(), format!("{inner_path}/Transform")),
                );
                let inner_go_tree = self.go_tree(inner_name, &[inner_tr]);
                self.set_obj(
                    inner_go,
                    "GameObject",
                    inner_go_tree,
                    Role::Glb("GameObject".into(), inner_path.clone()),
                );
                self.scene_object_pids.push(inner_tr);
                self.scene_object_pids.push(inner_go);
                self.anim_target_go = inner_go;
                self.anim_target_recycle = inner_path;
                self.emit_orphan_node_assets(scene);
                (
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    vec![inner_tr],
                )
            } else if let Some(ri) = inline_root {
                let r = scene.nodes[ri].clone();

                let child_trs: Vec<i64> = r
                    .children
                    .iter()
                    .map(|ci| self.build_node(scene, *ci, root_tr, &wrap_path).1)
                    .collect();
                (r.translation, r.rotation, r.scale, child_trs)
            } else {
                let roots = scene.root_nodes.clone();
                let child_trs: Vec<i64> = roots
                    .iter()
                    .map(|ni| self.build_node(scene, *ni, root_tr, &wrap_path).1)
                    .collect();
                (
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    child_trs,
                )
            };
            let tr_tree =
                self.transform_tree(root_go, wrap_t, wrap_r, wrap_s, &child_transforms, 0);
            self.set_obj(
                root_tr,
                "Transform",
                tr_tree,
                Role::Glb("Transform".into(), format!("{wrap_path}/Transform")),
            );
            let go_tree = self.go_tree(&self.root_hash.clone(), &[root_tr]);
            self.set_obj(
                root_go,
                "GameObject",
                go_tree,
                Role::Glb("GameObject".into(), wrap_path),
            );
            self.scene_object_pids.push(root_tr);
            self.scene_object_pids.push(root_go);
            self.bundle_root_assigned = true;
        }
        self.root_go_pid = root_go;

        if !self.orphan_assets_emitted {
            self.emit_orphan_node_assets(scene);
        }

        let root_recycle = match self.roles.get(&root_go) {
            Some(Role::Glb(_, r)) => r.clone(),
            _ => String::new(),
        };
        if self.anim_target_go == 0 {
            self.anim_target_go = root_go;
            self.anim_target_recycle = root_recycle.clone();
        }
        let anim_go = self.anim_target_go;
        let anim_recycle = self.anim_target_recycle.clone();

        let mut anim_component_pid: Option<i64> = None;
        if self.is_emote && self.proto.contains_key("AnimatorController") && glb_is_binary {
            let clips = prebuilt_clips.take().unwrap_or_else(|| {
                let base_clip = self.base_clone("AnimationClip_mecanim");
                animation_mecanim::build_mecanim_clips(&self.glb_bytes, &base_clip)
            });
            let mut clip_specs: Vec<(String, i64)> = Vec::new();

            let clip_names: Vec<String> = clips
                .iter()
                .map(|c| {
                    c.get("m_Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
            let mut order: Vec<usize> = (0..clip_names.len()).collect();
            order.sort_by(|&a, &b| clip_names[a].cmp(&clip_names[b]).then(a.cmp(&b)));
            let mut rank = vec![0usize; order.len()];
            for (r, &orig) in order.iter().enumerate() {
                rank[orig] = r;
            }
            for (idx, mut clip) in clips.into_iter().enumerate() {
                self.set_muscle_clip_size(&mut clip);
                let name = clip_names[idx].clone();
                let pid = self.add(
                    "AnimationClip",
                    clip,
                    Role::AnimControllerSubClip(rank[idx]),
                );

                clip_specs.push((name, pid));
            }
            if !clip_specs.is_empty() {
                let base_ctrl = self.base_clone("AnimatorController");
                let ctrl = animation_mecanim::build_animator_controller(&clip_specs, &base_ctrl);
                let ctrl_pid = self.add("AnimatorController", ctrl, Role::AnimController);
                self.scene_object_pids.push(ctrl_pid);
                let clip_pids: Vec<i64> = clip_specs.iter().map(|(_, p)| *p).collect();
                self.animator_controller_entry = Some((ctrl_pid, clip_pids));
                let animator = animation_mecanim::build_animator_component(root_go, ctrl_pid);
                anim_component_pid = Some(self.add(
                    "Animator",
                    animator,
                    Role::Glb("Animator".into(), format!("{root_recycle}/Animator")),
                ));
            }
        } else if !self.is_emote && !self.is_wearable && self.proto.contains_key("AnimationClip") {
            let clips = prebuilt_clips.take().unwrap_or_else(|| {
                animation::build_animation_clips_from_gltf(&self.gltf_json, &self.gltf_buffers)
            });
            let mut clip_name_pids: Vec<(String, i64)> = Vec::new();
            for clip in clips {
                let name = clip
                    .get("m_Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut tree = self.base_clone("AnimationClip");
                merge_into(&mut tree, clip);
                let recycle = self.unique_recycle("animations", &name);
                let pid = self.add(
                    "AnimationClip",
                    tree,
                    Role::Glb("AnimationClip".into(), recycle),
                );
                clip_name_pids.push((name, pid));
                self.scene_object_pids.push(pid);
            }
            if !clip_name_pids.is_empty() && self.proto.contains_key("Animation") {
                let comp = animation::build_animation_component(anim_go, &clip_name_pids);
                anim_component_pid = Some(self.add(
                    "Animation",
                    comp,
                    Role::Glb("Animation".into(), format!("{anim_recycle}/Animation")),
                ));
                self.anim_clip_name_pids = clip_name_pids;
            }
        }

        if let Some(acp) = anim_component_pid {
            self.scene_object_pids.push(acp);
            if let Some((_, tree)) = self.objects.get_mut(&anim_go) {
                if let Some(comp) = tree.get_mut("m_Component").and_then(|v| v.as_array_mut()) {
                    comp.push(map! {"component" => crate::value::pptr(0, acp)});
                }
            }
        }

        let pending = std::mem::take(&mut self.pending_smr);
        let node_children: Vec<Vec<usize>> = if pending.is_empty() {
            Vec::new()
        } else {
            scene.nodes.iter().map(|n| n.children.clone()).collect()
        };
        for (smr_pid, go_pid, mesh_pid, mat_pid, skin_idx, blend_shape_weights) in pending {
            let (bones, root_bone) = match skin_idx {
                Some(si) => {
                    let skin = &scene.skins[si];
                    let bs: Vec<Value> = skin
                        .joints
                        .iter()
                        .map(|j| crate::value::pptr(0, *self.node_tr.get(j).unwrap_or(&0)))
                        .collect();
                    let view = skeleton::SkinView {
                        node_children: &node_children,
                        joints: &skin.joints,
                        skeleton: skin.skeleton,
                    };
                    let rb = match skeleton::resolve_root_joint(&view) {
                        Some(rj) if self.node_tr.contains_key(&rj) => {
                            crate::value::pptr(0, self.node_tr[&rj])
                        }
                        _ => crate::value::pptr(0, 0),
                    };
                    (bs, rb)
                }
                None => (Vec::new(), crate::value::pptr(0, 0)),
            };
            let base = self.base_clone("SkinnedMeshRenderer");
            let mats: Vec<Value> = mat_pid
                .into_iter()
                .map(|p| crate::value::pptr(0, p))
                .collect();
            let smr = mesh_layout::skinned_mesh_renderer_tree(
                &base,
                go_pid,
                mesh_pid,
                mats,
                bones,
                root_bone,
                &blend_shape_weights,
            );

            if let Some(slot) = self.objects.get_mut(&smr_pid) {
                slot.1 = smr;
            }
        }

        for (name, roots) in scene.extra_scenes.clone() {
            self.build_extra_scene(scene, name.as_deref(), &roots);
        }

        for mat_idx in 0..scene.materials.len() {
            if !self.mat_pid.contains_key(&mat_idx) {
                let _ = self.material_orphan(scene, Some(mat_idx));
            }
        }

        if emits_metadata_textasset(&self.root_hash) {
            let mut meta = self.base_clone("TextAsset");
            meta.insert("m_Name", "metadata");
            let mut deps: Vec<String> = self.metadata_dependencies.to_vec();
            for f in &self.ext_bundle_files {
                if !deps.contains(f) {
                    deps.push(f.clone());
                }
            }
            if v38_compat() {
                for d in &mut deps {
                    *d = d.to_ascii_lowercase();
                }
                if !self.material_entries.is_empty() {
                    deps.push(format!("dcl/scene_ignore_{}", self.target));
                }
            }
            deps.sort_unstable_by(|x, y| natural_bundle_cmp(x, y));
            if v38_compat() {
                deps.dedup();
            }
            let deps_json: String = {
                let parts: Vec<String> = deps
                    .iter()
                    .map(|d| serde_json::to_string(d).expect("serialize metadata dep"))
                    .collect();
                format!("[{}]", parts.join(","))
            };
            let version = metadata_version_for_target(self.target);
            let ts = metadata_timestamp();
            let meta_json = format!(
                "{{\"timestamp\":{ts},\"version\":\"{version}\",\"dependencies\":{deps_json},\"mainAsset\":\"\"}}"
            );
            meta.insert("m_Script", meta_json);
            self.meta_pid = self.add("TextAsset", meta, Role::Meta);
        }

        self.build_assetbundle();
        Ok(())
    }

    fn count_scene_visits(&mut self, scene: &Scene, roots: &[usize]) {
        fn dfs(
            scene: &Scene,
            idx: usize,
            on_path: &mut HashSet<usize>,
            counts: &mut HashMap<usize, usize>,
        ) {
            if idx >= scene.nodes.len() || !on_path.insert(idx) {
                return;
            }
            *counts.entry(idx).or_insert(0) += 1;
            for ci in &scene.nodes[idx].children {
                dfs(scene, *ci, on_path, counts);
            }
            on_path.remove(&idx);
        }
        self.visits_left.clear();
        let mut on_path = HashSet::new();
        for r in roots {
            dfs(scene, *r, &mut on_path, &mut self.visits_left);
        }
    }

    fn build_bare_node(
        &mut self,
        scene: &Scene,
        node_idx: usize,
        parent_tr: i64,
        parent_path: &str,
    ) -> i64 {
        if let Some(c) = self.visits_left.get_mut(&node_idx) {
            *c = c.saturating_sub(1);
        }
        let node = &scene.nodes[node_idx];
        let (t, r, s, children) = (
            node.translation,
            node.rotation,
            node.scale,
            node.children.clone(),
        );
        let node_path = format!("{parent_path}/New Game Object");
        let go = self.npid();
        let tr = self.npid();
        let mut child_trs: Vec<i64> = Vec::new();
        for ci in &children {
            if *ci >= scene.nodes.len() || self.visits_left.get(ci).copied().unwrap_or(0) == 0 {
                continue;
            }
            child_trs.push(self.build_bare_node(scene, *ci, tr, &node_path));
        }
        let go_tree = self.go_tree("New Game Object", &[tr]);
        self.set_obj(
            go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), node_path.clone()),
        );
        let tr_tree = self.transform_tree(go, t, r, s, &child_trs, parent_tr);
        self.set_obj(
            tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        );
        self.scene_object_pids.push(go);
        self.scene_object_pids.push(tr);
        tr
    }

    fn build_node(
        &mut self,
        scene: &Scene,
        node_idx: usize,
        parent_tr: i64,
        parent_path: &str,
    ) -> (i64, i64) {
        if let Some(c) = self.visits_left.get_mut(&node_idx) {
            *c = c.saturating_sub(1);
        }
        let node = scene.nodes[node_idx].clone();

        let node_name = match scene.unique_node_names.get(node_idx) {
            Some(n) => n.clone(),
            None => {
                if !node.name.is_empty() {
                    node.name.clone()
                } else if let Some(p) = node.primitives.first() {
                    p.name.clone()
                } else {
                    format!("Node-{node_idx}")
                }
            }
        };
        let node_path = format!("{parent_path}/{node_name}");
        let go = self.npid();
        let tr = self.npid();
        let mut components = vec![tr];
        let mut comp_roles: Vec<(i64, Role)> = vec![(
            tr,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        )];

        let mut extra_child_transforms: Vec<i64> = Vec::new();
        if !node.primitives.is_empty() {
            let mesh_base = node.primitives[0].name.clone();

            let merged =
                self.try_attach_primitives_merged(scene, go, tr, &node, &node_path, &mesh_base);
            if let Some(extra) = merged {
                components.extend(self.component_pids.clone());
                comp_roles.extend(std::mem::take(&mut self.component_roles));
                extra_child_transforms.extend(extra);
            } else {
                self.attach_primitive(
                    scene,
                    go,
                    &node.primitives[0],
                    node.is_collider,
                    true,
                    &node_path,
                    &mesh_base,
                    node.extra_colliders,
                );
                components.extend(self.component_pids.clone());
                comp_roles.extend(std::mem::take(&mut self.component_roles));
                let child_base = if mesh_base.is_empty() { "Primitive" } else { mesh_base.as_str() };
                for pi in 1..node.primitives.len() {
                    let prim = &node.primitives[pi].clone();
                    let child_name = format!("{child_base}_{pi}");
                    let child_path = format!("{node_path}/{child_name}");
                    let cgo = self.npid();
                    let ctr = self.npid();
                    self.attach_primitive(
                        scene,
                        cgo,
                        prim,
                        node.is_collider,
                        false,
                        &child_path,
                        &mesh_base,
                        0,
                    );
                    let mut ccomp = vec![ctr];
                    ccomp.extend(self.component_pids.clone());
                    let go_tree = self.go_tree(&child_name, &ccomp);
                    self.set_obj(
                        cgo,
                        "GameObject",
                        go_tree,
                        Role::Glb("GameObject".into(), child_path.clone()),
                    );
                    let tr_tree = self.transform_tree(
                        cgo,
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 0.0, 1.0],
                        [1.0, 1.0, 1.0],
                        &[],
                        tr,
                    );
                    self.set_obj(
                        ctr,
                        "Transform",
                        tr_tree,
                        Role::Glb("Transform".into(), format!("{child_path}/Transform")),
                    );
                    for (cp, cr) in std::mem::take(&mut self.component_roles) {
                        self.insert_role(cp, cr);
                    }
                    self.scene_object_pids.push(cgo);
                    self.scene_object_pids.push(ctr);
                    extra_child_transforms.push(ctr);
                }
            }
        }

        self.node_tr.insert(node_idx, tr);

        let mut child_transforms: Vec<i64> = Vec::new();
        for ci in &node.children {
            if *ci >= scene.nodes.len() {
                continue;
            }

            if self.visits_left.get(ci).copied().unwrap_or(1) > 1 {
                child_transforms.push(self.build_bare_node(scene, *ci, tr, &node_path));
                continue;
            }
            if self.node_tr.contains_key(ci) {
                continue;
            }
            child_transforms.push(self.build_node(scene, *ci, tr, &node_path).1);
        }
        child_transforms.extend(extra_child_transforms);

        let is_root = parent_tr == 0;
        let go_name = if is_root && !self.bundle_root_assigned {
            self.bundle_root_assigned = true;
            self.root_hash.clone()
        } else {
            node_name
        };
        let go_tree = self.go_tree(&go_name, &components);
        self.set_obj(
            go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), node_path.clone()),
        );
        for (p, r) in comp_roles {
            self.insert_role(p, r);
        }
        let tr_tree = self.transform_tree(
            go,
            node.translation,
            node.rotation,
            node.scale,
            &child_transforms,
            parent_tr,
        );

        self.set_obj(
            tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        );
        self.scene_object_pids.push(go);
        self.scene_object_pids.push(tr);
        (go, tr)
    }

    fn build_extra_scene(&mut self, scene: &Scene, name: Option<&str>, roots: &[usize]) {
        self.count_scene_visits(scene, roots);
        let scene_name = name.unwrap_or("");
        let scene_path = format!("scenes/{scene_name}");
        let has_anim_component = self.proto.contains_key("AnimatorController")
            || self.proto.contains_key("AnimatorOverrideController")
            || self.proto.contains_key("Animation");
        let use_first_child = !has_anim_component && roots.len() == 1;
        if use_first_child {
            self.build_node(scene, roots[0], 0, &scene_path);
            return;
        }
        let wrap_inner = if scene_name.is_empty() {
            "Scene"
        } else {
            scene_name
        };
        let wrap_path = format!("{scene_path}/{wrap_inner}");
        let wrap_go = self.npid();
        let wrap_tr = self.npid();
        let mut child_trs: Vec<i64> = roots
            .iter()
            .map(|ri| self.build_node(scene, *ri, wrap_tr, &wrap_path).1)
            .collect();
        let has_animation_clips =
            !self.anim_clip_name_pids.is_empty() && self.proto.contains_key("Animation");
        if roots.is_empty() && has_animation_clips {
            let inner_name = if scene_name.is_empty() {
                "Scene"
            } else {
                scene_name
            };
            let inner_path = format!("{wrap_path}/{inner_name}");
            let inner_go = self.npid();
            let inner_tr = self.npid();
            let inner_tr_tree = self.transform_tree(
                inner_go,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                &[],
                wrap_tr,
            );
            self.set_obj(
                inner_tr,
                "Transform",
                inner_tr_tree,
                Role::Glb("Transform".into(), format!("{inner_path}/Transform")),
            );
            let clips = self.anim_clip_name_pids.clone();
            let comp = animation::build_animation_component(inner_go, &clips);
            let acp = self.add(
                "Animation",
                comp,
                Role::Glb("Animation".into(), format!("{inner_path}/Animation")),
            );
            self.scene_object_pids.push(acp);
            let inner_go_tree = self.go_tree(inner_name, &[inner_tr, acp]);
            self.set_obj(
                inner_go,
                "GameObject",
                inner_go_tree,
                Role::Glb("GameObject".into(), inner_path),
            );
            self.scene_object_pids.push(inner_tr);
            self.scene_object_pids.push(inner_go);
            child_trs.push(inner_tr);
        }
        let tr_tree = self.transform_tree(
            wrap_go,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            &child_trs,
            0,
        );
        self.set_obj(
            wrap_tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{wrap_path}/Transform")),
        );
        let mut wrap_components = vec![wrap_tr];
        if !roots.is_empty() && has_animation_clips {
            let clips = self.anim_clip_name_pids.clone();
            let comp = animation::build_animation_component(wrap_go, &clips);
            let acp = self.add(
                "Animation",
                comp,
                Role::Glb("Animation".into(), format!("{wrap_path}/Animation")),
            );
            self.scene_object_pids.push(acp);
            wrap_components.push(acp);
        }
        let go_tree = self.go_tree(wrap_inner, &wrap_components);
        self.set_obj(
            wrap_go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), wrap_path),
        );
        self.scene_object_pids.push(wrap_tr);
        self.scene_object_pids.push(wrap_go);
    }

    fn emit_orphan_node_assets(&mut self, scene: &Scene) {
        self.orphan_assets_emitted = true;
        let mut reachable: HashSet<usize> = HashSet::new();
        let mut stack: Vec<usize> = scene
            .root_nodes
            .iter()
            .chain(scene.extra_scenes.iter().flat_map(|(_, r)| r.iter()))
            .copied()
            .collect();
        while let Some(n) = stack.pop() {
            if n >= scene.nodes.len() || !reachable.insert(n) {
                continue;
            }
            stack.extend(scene.nodes[n].children.iter().copied());
        }
        for (idx, node) in scene.nodes.iter().enumerate() {
            if reachable.contains(&idx) {
                continue;
            }
            for prim in &node.primitives {
                let mesh_base = prim.name.clone();
                let mesh_pid = self.add_mesh(prim, 0, None, &mesh_base);
                self.scene_object_pids.push(mesh_pid);

                if prim.material_index.is_some() {
                    let _ = self.material_orphan(scene, prim.material_index);
                }
            }
        }
    }

    fn attach_primitive(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        prim: &Primitive,
        is_collider: bool,
        is_parent_prim: bool,
        node_path: &str,
        mesh_base: &str,
        extra_colliders: usize,
    ) {
        let has_smr_proto = self.proto.contains_key("SkinnedMeshRenderer");
        let is_skinned_node = prim.skin_index.is_some() && prim.weights.is_some() && has_smr_proto;

        let has_morph_targets = !prim.morph_targets.is_empty() && has_smr_proto;
        let becomes_smr = is_skinned_node || has_morph_targets;

        let suppress_emission = is_parent_prim && is_collider && becomes_smr;
        let becomes_collider = is_collider && !becomes_smr;

        let usage: i64 = if becomes_smr {
            1
        } else if becomes_collider || self.mesh_collidable(prim) {
            16
        } else {
            0
        };
        let bind_poses: Option<Vec<[f64; 16]>> = if is_skinned_node {
            prim.skin_index
                .filter(|&si| si < scene.skins.len())
                .map(|si| scene.skins[si].bind_poses.clone())
        } else {
            None
        };

        let mesh_usage = if suppress_emission { 0 } else { usage };
        let mesh_pid = self.add_mesh(prim, mesh_usage, bind_poses.as_deref(), mesh_base);
        self.scene_object_pids.push(mesh_pid);

        if suppress_emission {
            if prim.material_index.is_some() {
                let _ = self.material_orphan(scene, prim.material_index);
            }
            self.component_pids = Vec::new();
            self.component_roles = Vec::new();
            return;
        }

        if becomes_collider {
            if prim.material_index.is_some() {
                let _ = self.material_orphan(scene, prim.material_index);
            }
            let mf = self.add(
                "MeshFilter",
                map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
                Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
            );
            let mut mc = self.base_clone("MeshCollider");
            mc.insert("m_GameObject", crate::value::pptr(0, go_pid));
            mc.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
            let mc_pid = self.add(
                "MeshCollider",
                mc,
                Role::Glb("MeshCollider".into(), format!("{node_path}/MeshCollider")),
            );
            self.scene_object_pids.push(mf);
            self.scene_object_pids.push(mc_pid);
            self.component_pids = vec![mf, mc_pid];
            self.component_roles = Vec::new();

            let go_name = node_path.rsplit('/').next().unwrap_or("");

            let n_extra = extra_colliders
                + usize::from(!is_parent_prim && go_name.to_lowercase().contains("_collider"));
            for idx in 1..=n_extra {
                let mut mc2 = self.base_clone("MeshCollider");
                mc2.insert("m_GameObject", crate::value::pptr(0, go_pid));
                mc2.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
                let mc2_pid = self.add(
                    "MeshCollider",
                    mc2,
                    Role::GlbIdx(
                        "MeshCollider".into(),
                        format!("{node_path}/MeshCollider"),
                        idx as u32,
                    ),
                );
                self.scene_object_pids.push(mc2_pid);
                self.component_pids.push(mc2_pid);
            }
            return;
        }

        if becomes_smr {
            let mat_pid = self.material(scene, prim.material_index);
            let smr_pid = self.npid();
            self.set_obj(
                smr_pid,
                "SkinnedMeshRenderer",
                Value::map(),
                Role::Glb(
                    "SkinnedMeshRenderer".into(),
                    format!("{node_path}/SkinnedMeshRenderer"),
                ),
            );
            self.pending_smr.push((
                smr_pid,
                go_pid,
                mesh_pid,
                mat_pid.into_iter().collect(),
                prim.skin_index,
                prim.morph_weights.clone(),
            ));
            self.scene_object_pids.push(smr_pid);
            self.component_pids = vec![smr_pid];
            self.component_roles = Vec::new();
            return;
        }

        let mf = self.add(
            "MeshFilter",
            map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
            Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
        );
        self.scene_object_pids.push(mf);
        let mat_pids: Vec<Value> = self
            .material(scene, prim.material_index)
            .into_iter()
            .map(|p| crate::value::pptr(0, p))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert("m_Materials", Value::Array(mat_pids));
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
    }

    fn go_tree(&self, name: &str, component_pids: &[i64]) -> Value {
        let comps: Vec<Value> = component_pids
            .iter()
            .map(|p| map! {"component" => crate::value::pptr(0, *p)})
            .collect();
        map! {
            "m_Component" => Value::Array(comps),
            "m_Layer" => 0,
            "m_Name" => name,
            "m_Tag" => 0,
            "m_IsActive" => true,
        }
    }

    fn transform_tree(
        &self,
        go_pid: i64,
        t: [f64; 3],
        r: [f64; 4],
        s: [f64; 3],
        children: &[i64],
        father: i64,
    ) -> Value {
        let kids: Vec<Value> = children.iter().map(|c| crate::value::pptr(0, *c)).collect();
        map! {
            "m_GameObject" => crate::value::pptr(0, go_pid),
            "m_LocalRotation" => map!{"x" => r[0], "y" => r[1], "z" => r[2], "w" => r[3]},
            "m_LocalPosition" => map!{"x" => t[0], "y" => t[1], "z" => t[2]},
            "m_LocalScale" => map!{"x" => s[0], "y" => s[1], "z" => s[2]},
            "m_Children" => Value::Array(kids),
            "m_Father" => crate::value::pptr(0, father),
        }
    }

    fn build_assetbundle(&mut self) {
        let mut ab = self.base_clone("AssetBundle");

        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);
        self.ab_pid = self.npid();
        self.set_obj(self.ab_pid, "AssetBundle", ab, Role::Bundle);
    }

    fn finalize_pathids(&mut self) -> Result<()> {
        let mut old2new: HashMap<i64, i64> = HashMap::new();
        for (&old_pid, role) in self.roles.iter() {
            old2new.insert(old_pid, self.resolve_pathid(role));
        }

        let mut seen: HashMap<i64, i64> = HashMap::new();
        for (&old, &new) in old2new.iter() {
            if let Some(&prev) = seen.get(&new) {
                if prev != old {
                    return Err(anyhow!(
                        "PathID collision {new}: roles {:?} and {:?}",
                        self.roles.get(&prev),
                        self.roles.get(&old)
                    ));
                }
            }
            seen.insert(new, old);
        }

        fn remap(node: &mut Value, m: &HashMap<i64, i64>) {
            match node {
                Value::Map(map) => {
                    let is_pptr = map.len() == 2
                        && map.contains_key("m_FileID")
                        && map.contains_key("m_PathID");
                    if is_pptr {
                        let fid = map.get("m_FileID").and_then(|v| v.as_i64()).unwrap_or(0);
                        let pid = map.get("m_PathID").and_then(|v| v.as_i64()).unwrap_or(0);
                        if fid == 0 {
                            if let Some(&np) = m.get(&pid) {
                                map.insert("m_PathID", np);
                            }
                        }
                        return;
                    }
                    for (_, v) in map.0.iter_mut() {
                        remap(v, m);
                    }
                }
                Value::Array(a) => {
                    for v in a.iter_mut() {
                        remap(v, m);
                    }
                }
                _ => {}
            }
        }

        let mut new_objects: BTreeMap<i64, (String, Value)> = BTreeMap::new();
        for (old_pid, (tn, mut tree)) in std::mem::take(&mut self.objects) {
            remap(&mut tree, &old2new);
            let np = *old2new.get(&old_pid).unwrap();
            new_objects.insert(np, (tn, tree));
        }
        self.order = self
            .order
            .iter()
            .map(|p| *old2new.get(p).unwrap())
            .collect();
        self.objects = new_objects;

        let map_pid = |p: &i64| *old2new.get(p).unwrap();
        self.scene_object_pids = self.scene_object_pids.iter().map(map_pid).collect();
        self.root_go_pid = old2new[&self.root_go_pid];
        self.material_entries = self
            .material_entries
            .iter()
            .map(|(k, p, deps)| (k.clone(), old2new[p], deps.iter().map(map_pid).collect()))
            .collect();

        self.mat_external_pptrs = self
            .mat_external_pptrs
            .iter()
            .map(|(p, v)| (old2new[p], v.clone()))
            .collect();
        self.texture_entries = self
            .texture_entries
            .iter()
            .map(|(k, p)| (k.clone(), old2new[p]))
            .collect();
        self.glb_referenced_mats = self.glb_referenced_mats.iter().map(map_pid).collect();
        self.animator_controller_entry = self
            .animator_controller_entry
            .take()
            .map(|(c, clips)| (old2new[&c], clips.iter().map(map_pid).collect()));
        if self.meta_pid != 0 {
            self.meta_pid = old2new[&self.meta_pid];
        }
        self.ab_pid = old2new[&self.ab_pid];

        self.force_inline_tex = self.force_inline_tex.iter().map(map_pid).collect();

        let ab_tree = self.fill_assetbundle();
        if let Some(slot) = self.objects.get_mut(&self.ab_pid) {
            slot.1 = ab_tree;
        }
        Ok(())
    }

    fn fill_assetbundle(&self) -> Value {
        let mut ab = self.base_clone("AssetBundle");
        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);

        let mut entries: Vec<sbp_order::Entry> = Vec::new();

        let mut glb_objs: Vec<sbp_order::Obj> = vec![sbp_order::Obj::new(0, self.root_go_pid)];
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        seen.insert((0, self.root_go_pid));
        let glb_set: Vec<i64> = self
            .scene_object_pids
            .iter()
            .chain(self.glb_referenced_mats.iter())
            .copied()
            .filter(|p| *p != self.root_go_pid)
            .collect();
        for p in glb_set {
            if seen.insert((0, p)) {
                glb_objs.push(sbp_order::Obj::new(0, p));
            }
        }

        let pos_default = self
            .externals_position
            .unwrap_or_else(|| ExternalsPosition::for_target(self.target));
        let cb_default = self
            .cross_bundle_position
            .unwrap_or_else(|| CrossBundlePosition::for_target(self.target));
        let mut entry_pos: Vec<(ExternalsPosition, CrossBundlePosition)> = Vec::new();

        let glb_ext = if self.is_gltf { "gltf" } else { "glb" };
        entries.push(sbp_order::Entry {
            guid: self.glb_guid.clone(),
            key: format!("{}.{}", self.root_hash, glb_ext),
            objects: glb_objs,
            asset: Some(sbp_order::Obj::new(0, self.root_go_pid)),
        });
        entry_pos.push((pos_default, cb_default));

        if emits_metadata_textasset(&self.root_hash) {
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/metadata", self.root_hash)),
                key: "metadata.json".into(),
                objects: vec![sbp_order::Obj::new(0, self.meta_pid)],
                asset: Some(sbp_order::Obj::new(0, self.meta_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        for (mi, (key, mat_pid, tex_pids)) in self.material_entries.iter().enumerate() {
            let mut objs: Vec<sbp_order::Obj> = vec![sbp_order::Obj::new(0, *mat_pid)];
            let mut seen: HashSet<(i64, i64)> = HashSet::new();
            seen.insert((0, *mat_pid));
            let mut deps: Vec<sbp_order::Obj> =
                vec![sbp_order::Obj::new(SHADER_FILE_ID, SHADER_PATH_ID)];
            deps.extend(tex_pids.iter().map(|p| sbp_order::Obj::new(0, *p)));
            if let Some(ext) = self.mat_external_pptrs.get(mat_pid) {
                deps.extend(ext.iter().map(|&(f, p)| sbp_order::Obj::new(f, p)));
            }
            for d in deps {
                if seen.insert((d.file_id, d.path_id)) {
                    objs.push(d);
                }
            }
            let stem = key.strip_suffix(".mat").unwrap_or(key);
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/material/{}", self.root_hash, stem)),
                key: key.clone(),
                objects: objs,
                asset: Some(sbp_order::Obj::new(0, *mat_pid)),
            });
            let p = self
                .material_externals_overrides
                .as_ref()
                .and_then(|v| v.get(mi).copied())
                .unwrap_or((pos_default, cb_default));
            entry_pos.push(p);
        }

        for (key, tex_pid) in &self.texture_entries {
            let stem = key.strip_suffix(".png").unwrap_or(key);
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/texture/{}", self.root_hash, stem)),
                key: key.clone(),
                objects: vec![sbp_order::Obj::new(0, *tex_pid)],
                asset: Some(sbp_order::Obj::new(0, *tex_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        if let Some((ctrl_pid, clip_pids)) = &self.animator_controller_entry {
            let ctrl_pid = *ctrl_pid;

            let mut objs: Vec<sbp_order::Obj> = clip_pids
                .iter()
                .map(|p| sbp_order::Obj::new(0, *p))
                .collect();
            objs.push(sbp_order::Obj::new(0, ctrl_pid));
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/animatorController", self.root_hash)),
                key: "animatorController.controller".into(),
                objects: objs,
                asset: Some(sbp_order::Obj::new(0, ctrl_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        let pos_by_guid: HashMap<String, (ExternalsPosition, CrossBundlePosition)> = entries
            .iter()
            .zip(entry_pos.iter())
            .map(|(e, p)| (e.guid.clone(), *p))
            .collect();
        let shader_cab = cabname::shader_bundle_cab(self.target).to_lowercase();
        let ext_cabs: Vec<String> = self
            .ext_bundle_files
            .iter()
            .map(|bf| cabname::cab_name(bf).to_lowercase())
            .collect();
        let use_cab_merge = matches!(self.target, "windows" | "mac" | "linux" | "webgl")
            && self.externals_position.is_none()
            && self.cross_bundle_position.is_none()
            && self.material_externals_overrides.is_none();

        let own_cab = cabname::cab_name(&self.bundle_name).to_lowercase();
        let cab_for = |fid: i64| -> String {
            if fid == 1 {
                shader_cab.clone()
            } else if fid >= 2 {
                ext_cabs
                    .get((fid - 2) as usize)
                    .cloned()
                    .unwrap_or_default()
            } else {
                own_cab.clone()
            }
        };
        let (preload, container) = if use_cab_merge {
            let mut order_idx: Vec<usize> = (0..entries.len()).collect();
            order_idx.sort_by_key(|&i| sbp_order::guid_sort_key(&entries[i].guid));
            let mut pl: Vec<sbp_order::Obj> = Vec::new();
            let mut by_key: Vec<(String, sbp_order::ContainerSlot)> = Vec::new();
            for &orig_idx in &order_idx {
                let e = &entries[orig_idx];
                let run = sbp_order::order_run_cab_merge(&e.objects, cab_for);
                let start = pl.len();
                let size = run.len();
                let asset = e
                    .asset
                    .or_else(|| run.first().copied())
                    .unwrap_or(sbp_order::Obj {
                        file_id: 0,
                        path_id: 0,
                    });
                pl.extend(run);
                let slot = sbp_order::ContainerSlot {
                    preload_index: start,
                    preload_size: size,
                    asset,
                };
                match by_key.iter_mut().find(|(k, _)| *k == e.key) {
                    Some((_, existing)) => *existing = slot,
                    None => by_key.push((e.key.clone(), slot)),
                }
            }
            by_key.sort_by(|a, b| a.0.cmp(&b.0));
            (pl, by_key)
        } else {
            sbp_order::build_preload_and_container_per_entry(&entries, |e| {
                pos_by_guid
                    .get(&e.guid)
                    .copied()
                    .unwrap_or((pos_default, cb_default))
            })
        };
        let (preload_v, container_v) = sbp_order::to_values(&preload, &container);
        ab.insert("m_PreloadTable", preload_v);
        ab.insert("m_Container", container_v);
        ab.insert("m_MainAsset", sbp_order::empty_main_asset());

        let mut dep_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        if !self.material_entries.is_empty() {
            dep_set.insert(cabname::shader_bundle_cab(self.target).to_lowercase());
        }
        for bf in &self.ext_bundle_files {
            dep_set.insert(cabname::cab_name(bf).to_lowercase());
        }
        for bf in &self.metadata_dependencies {
            dep_set.insert(cabname::cab_name(bf).to_lowercase());
        }
        ab.insert(
            "m_Dependencies",
            Value::Array(dep_set.into_iter().map(Value::Str).collect()),
        );
        ab
    }

    fn commit(&self, bundle: &mut Bundle) -> Result<()> {
        let mut blobs: Vec<ress::TextureBlob> = Vec::new();
        for (&pid, (tn, tree)) in self.objects.iter() {
            if tn != "Texture2D" {
                continue;
            }

            if matches!(tree.get("m_IsReadable"), Some(Value::Bool(true))) {
                continue;
            }
            if let Some(Value::Bytes(b)) = tree.get("image data") {
                if !b.is_empty() {
                    blobs.push(ress::TextureBlob::new(pid, b.clone(), ""));
                }
            }
        }

        let externals = if self.material_entries.is_empty() && self.ext_bundle_files.is_empty() {
            ExternalsPolicy::Clear
        } else {
            ExternalsPolicy::ShaderRef {
                ext_bundle_files: &self.ext_bundle_files,
            }
        };
        commit_objects(
            bundle,
            &self.bundle_name,
            self.target,
            self.proto,
            &self.objects,
            &blobs,
            &self.force_inline_tex,
            externals,
        )
    }
}

fn detect_container(raw: &[u8]) -> String {
    if raw.len() >= 8 && &raw[0..8] == b"\x89PNG\r\n\x1a\n" {
        "PNG".to_string()
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xD8 {
        "JPEG".to_string()
    } else {
        String::new()
    }
}

fn merge_into(dst: &mut Value, src: Value) {
    if let (Value::Map(d), Value::Map(mut s)) = (dst, src) {
        for (k, v) in std::mem::take(&mut s.0) {
            d.insert(k, v);
        }
    }
}

fn mean_color_image(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let raw = img.as_raw();
    let n = (w as u64) * (h as u64);
    let (mut sr, mut sg, mut sb, mut sa) = (0u64, 0u64, 0u64, 0u64);
    for c in raw.chunks_exact(4) {
        sr += c[0] as u64;
        sg += c[1] as u64;
        sb += c[2] as u64;
        sa += c[3] as u64;
    }
    let mean = [
        (sr / n) as u8,
        (sg / n) as u8,
        (sb / n) as u8,
        (sa / n) as u8,
    ];
    let mut buf = Vec::with_capacity(raw.len());
    for _ in 0..n {
        buf.extend_from_slice(&mean);
    }
    RgbaImage::from_raw(w, h, buf).expect("mean-color buffer size mismatch")
}

struct StandaloneTextureBuilder<'a> {
    proto: &'a HashMap<String, SerializedType>,
    base: &'a HashMap<String, Value>,
    root_hash: String,
    bundle_name: String,
    source_file: Option<String>,
    target: &'static str,
    model_referenced: bool,
    color_space_override: Option<i64>,
    standalone_normal: bool,
    objects: BTreeMap<i64, (String, Value)>,
    order: Vec<i64>,
    stream: Option<(i64, Vec<u8>)>,
}

impl<'a> StandaloneTextureBuilder<'a> {
    fn new(
        proto: &'a HashMap<String, SerializedType>,
        base: &'a HashMap<String, Value>,
        root_hash: String,
        bundle_name: String,
        source_file: Option<String>,
        model_referenced: bool,
        color_space_override: Option<i64>,
        standalone_normal: bool,
    ) -> Self {
        let target = target_from_bundle_name(&bundle_name);
        StandaloneTextureBuilder {
            proto,
            base,
            root_hash,
            bundle_name,
            source_file,
            target,
            model_referenced,
            color_space_override,
            standalone_normal,
            objects: BTreeMap::new(),
            order: Vec::new(),
            stream: None,
        }
    }

    fn set_obj(&mut self, pid: i64, type_name: &str, tree: Value) {
        if !self.objects.contains_key(&pid) {
            self.order.push(pid);
        }
        self.objects.insert(pid, (type_name.to_string(), tree));
    }

    fn texture_pid(&self) -> i64 {
        let guid = pathids::asset_guid(&self.root_hash);
        pathids::prefab_packed_path_id(&guid, TEXTURE_LOCAL_ID, pathids::FILE_TYPE_META_ASSET)
    }

    fn metadata_pid(&self) -> i64 {
        let guid = pathids::asset_guid(&format!("{}/metadata", self.root_hash));
        pathids::prefab_packed_path_id(&guid, 4900000, pathids::FILE_TYPE_META_ASSET)
    }

    fn texture_tree(
        &self,
        prof: &texprofile::Profile,
        data: Vec<u8>,
        mips: i32,
        name: &str,
        readable: bool,
    ) -> Value {
        let mut t = self.base.get("Texture2D").cloned().unwrap_or(Value::Null);
        t.insert("m_Name", name);
        t.insert("m_Width", prof.target_w);
        t.insert("m_Height", prof.target_h);
        t.insert("m_TextureFormat", prof.texture_format);
        t.insert("m_MipCount", mips);
        t.insert("m_CompleteImageSize", data.len() as i64);
        t.insert("m_IsReadable", readable);
        t.insert("m_ColorSpace", prof.color_space);
        t.insert("m_LightmapFormat", prof.lightmap_format);
        t.insert("m_IsAlphaChannelOptional", prof.is_alpha_channel_optional);
        t.insert("m_IgnoreMipmapLimit", prof.ignore_mipmap_limit);
        if let Some(ts) = t.get_mut("m_TextureSettings") {
            ts.insert("m_FilterMode", prof.filter_mode);
        }
        t.insert("image data", Value::Bytes(data));
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    fn build(&mut self, raw: &[u8], bundle: &mut Bundle) -> Result<Vec<u8>> {
        let decoded = decode_source_image(raw);

        let mut tex_pid: Option<i64> = None;
        if let Some(img) = &decoded {
            let (w, h) = img.dimensions();
            let container = detect_container(raw);
            let has_real_alpha = img.as_raw().iter().skip(3).step_by(4).any(|&a| a < 255);
            let src = texprofile::SourceImage {
                width: w,
                height: h,
                container,
                has_real_alpha,
            };
            let load_image_ok = texprofile::unity_load_image_would_succeed(&src);
            let cap = if load_image_ok {
                texprofile::max_texture_size_for(self.target)
            } else {
                texprofile::TEXTURE_IMPORTER_DEFAULT_MAX
            };

            let usage_normal: Option<bool> = if self.standalone_normal {
                Some(true)
            } else if self.color_space_override == Some(0) {
                Some(false)
            } else {
                None
            };
            let mut prof = texprofile::standalone_texture_profile_named(&src, cap, usage_normal);
            if let Some(cs) = self.color_space_override {
                prof.color_space = cs;
            }
            // WebGL: compressed standalone textures are DXT5 (BC3), never BC7.
            if self.target == "webgl" && prof.compressed {
                prof.texture_format = texprofile::TF_DXT5;
            }

            let fancy_buf;
            let img: &RgbaImage = if raw.len() >= 2
                && raw[0] == 0xFF
                && raw[1] == 0xD8
                && (prof.target_w, prof.target_h) == (w, h)
            {
                match libjpeg9c::decode_rgba(raw, true) {
                    Some((rgba, fw, fh)) if (fw, fh) == (w, h) => {
                        fancy_buf =
                            RgbaImage::from_raw(fw, fh, rgba).expect("jpeg fancy buffer size");
                        &fancy_buf
                    }
                    _ => img,
                }
            } else {
                img
            };

            let real_textures = std::env::var(BuildOpts::REAL_TEXTURES_ENV).is_ok();
            let max_size = texprofile::max_texture_size_for(self.target);
            let oversized = (w > max_size || h > max_size) && load_image_ok;
            let stub_canonical = oversized
                && !real_textures
                && prof.compressed
                && prof.texture_format == texprofile::TF_BC7;
            let stubbed_buf;
            let img: &RgbaImage = if oversized && !real_textures {
                stubbed_buf = mean_color_image(img);
                &stubbed_buf
            } else {
                img
            };

            let bled_src;
            let img: &RgbaImage = if has_real_alpha && prof.compressed {
                let mut buf = img.as_raw().clone();
                crate::alpha_bleed::alpha_bleed_inplace(&mut buf, w, h);
                bled_src =
                    RgbaImage::from_raw(w, h, buf).expect("alpha-bleed buffer size mismatch");
                &bled_src
            } else {
                img
            };

            let resized;
            let pil: &RgbaImage = if (prof.target_w, prof.target_h) != (w, h) {
                let buf = crate::resize::box_downscale_rgba(
                    img.as_raw(),
                    w as usize,
                    h as usize,
                    prof.target_w as usize,
                    prof.target_h as usize,
                );
                resized = RgbaImage::from_raw(prof.target_w, prof.target_h, buf)
                    .expect("resize buffer size mismatch");
                &resized
            } else {
                img
            };
            let bc7_profile = match self.target {
                "windows" | "mac" | "linux" if !self.model_referenced => bc7_pure::Bc7Profile::Basic,
                _ => bc7_pure::Bc7Profile::Slow,
            };
            let (data, mips) = if stub_canonical {
                encode_inglb_bc7_stub(
                    prof.target_w,
                    prof.target_h,
                    prof.mip_count,
                    prof.lightmap_format == 3,
                )
            } else if prof.compressed && prof.texture_format == texprofile::TF_DXT5 {
                // WebGL standalone textures: DXT5 (BC3) full mip chain.
                encode_dxt5_mip_chain_real(pil, prof.mip_count)
            } else if prof.compressed {
                encode_texture_bc7(
                    pil,
                    prof.mip_count,
                    prof.color_space == 1,
                    usage_normal,
                    bc7_profile,
                )
            } else {
                debug_assert_eq!(prof.texture_format, texprofile::TF_RGBA32_UNITY);

                bc7_pure::encode_rgba32_mip_chain(
                    pil.as_raw(),
                    prof.target_w,
                    prof.target_h,
                    Some(prof.mip_count),
                    true,
                    prof.color_space == 1,
                )
            };
            let pid = self.texture_pid();

            // WebGL never streams texture data to a .resS sidecar.
            let do_stream =
                self.target != "webgl" && self.model_referenced && prof.texture_format == 25;
            let tree = self.texture_tree(
                &prof,
                data.clone(),
                mips,
                &self.root_hash.clone(),
                !do_stream,
            );
            self.set_obj(pid, "Texture2D", tree);
            tex_pid = Some(pid);
            if do_stream {
                self.stream = Some((pid, data));
            }
        }

        let meta_pid_opt = if emits_metadata_textasset(&self.root_hash) {
            let mut meta = self.base.get("TextAsset").cloned().unwrap_or(Value::Null);
            meta.insert("m_Name", "metadata");
            let version = metadata_version_for_target(self.target);
            let ts = metadata_timestamp();

            meta.insert(
                "m_Script",
                format!(
                    r#"{{"timestamp":{ts},"version":"{version}","dependencies":[],"mainAsset":""}}"#
                ),
            );
            let meta_pid = self.metadata_pid();
            self.set_obj(meta_pid, "TextAsset", meta);
            Some(meta_pid)
        } else {
            None
        };

        let mut ab = self.base.get("AssetBundle").cloned().unwrap_or(Value::Null);
        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);
        ab.insert("m_Dependencies", Value::Array(vec![]));

        let tex_ext = standalone_key_extension(self.source_file.as_deref(), raw);
        let tex_key = format!("{}{}", self.root_hash, tex_ext);
        let mut entries: Vec<sbp_order::Entry> = Vec::new();
        let (objects, asset) = match tex_pid {
            Some(p) => (
                vec![sbp_order::Obj::new(0, p)],
                Some(sbp_order::Obj::new(0, p)),
            ),
            None => (vec![], Some(sbp_order::Obj::new(0, 0))),
        };
        entries.push(sbp_order::Entry {
            guid: pathids::asset_guid(&self.root_hash),
            key: tex_key,
            objects,
            asset,
        });
        if let Some(meta_pid) = meta_pid_opt {
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/metadata", self.root_hash)),
                key: "metadata.json".into(),
                objects: vec![sbp_order::Obj::new(0, meta_pid)],
                asset: Some(sbp_order::Obj::new(0, meta_pid)),
            });
        }

        let (preload, container) = sbp_order::build_preload_and_container(&entries);
        let (preload_v, container_v) = sbp_order::to_values(&preload, &container);
        ab.insert("m_PreloadTable", preload_v);
        ab.insert("m_Container", container_v);
        ab.insert("m_MainAsset", sbp_order::empty_main_asset());
        self.set_obj(1, "AssetBundle", ab);

        self.commit(bundle)?;
        bundle_io::save_bundle(bundle)
    }

    fn commit(&self, bundle: &mut Bundle) -> Result<()> {
        let blobs: Vec<ress::TextureBlob> = self
            .stream
            .as_ref()
            .map(|(pid, data)| vec![ress::TextureBlob::new(*pid, data.clone(), "")])
            .unwrap_or_default();
        commit_objects(
            bundle,
            &self.bundle_name,
            self.target,
            self.proto,
            &self.objects,
            &blobs,
            &HashSet::new(),
            ExternalsPolicy::Clear,
        )
    }
}

enum ExternalsPolicy<'a> {
    ShaderRef { ext_bundle_files: &'a [String] },
    Clear,
}

fn collect_pptr_first_use(
    value: &Value,
    node: &crate::unity::typetree_node::TypeTreeNode,
    order: &mut Vec<i64>,
) {
    if node.m_Type.starts_with("PPtr<") {
        if let Some(m) = value.as_map() {
            if let Some(fid) = m.get("m_FileID").and_then(|v| v.as_i64()) {
                if fid >= 1 && !order.contains(&fid) {
                    order.push(fid);
                }
            }
        }
        return;
    }
    match value {
        Value::Map(m) => {
            for child in &node.m_Children {
                if let Some(cv) = m.get(&child.m_Name) {
                    collect_pptr_first_use(cv, child, order);
                }
            }
        }
        Value::Array(a) => {
            if node.m_Type == "pair" {
                for (v, child) in a.iter().zip(node.m_Children.iter()) {
                    collect_pptr_first_use(v, child, order);
                }
            } else if !node.m_Children.is_empty() && node.m_Children[0].m_Type == "Array" {
                let subtype = &node.m_Children[0].m_Children[1];
                for v in a {
                    collect_pptr_first_use(v, subtype, order);
                }
            }
        }
        _ => {}
    }
}

fn remap_pptr_fids(value: &mut Value, remap: &HashMap<i64, i64>) {
    match value {
        Value::Map(m) => {
            if m.len() == 2 && m.get("m_PathID").is_some() {
                if let Some(old) = m.get("m_FileID").and_then(|v| v.as_i64()) {
                    if let Some(&new) = remap.get(&old) {
                        if new != old {
                            m.insert("m_FileID", Value::Int(new));
                        }
                    }
                }
                return;
            }
            for i in 0..m.len() {
                remap_pptr_fids(&mut m.0[i].1, remap);
            }
        }
        Value::Array(a) => {
            for v in a {
                remap_pptr_fids(v, remap);
            }
        }
        _ => {}
    }
}

fn commit_objects(
    bundle: &mut Bundle,
    bundle_name: &str,
    target: &str,
    proto: &HashMap<String, SerializedType>,
    objects: &BTreeMap<i64, (String, Value)>,
    blobs: &[ress::TextureBlob],
    inline_pids: &HashSet<i64>,
    externals: ExternalsPolicy<'_>,
) -> Result<()> {
    let big_endian = bundle.serialized().map(|sf| sf.big_endian).unwrap_or(false);

    let new_cab = cabname::cab_name(bundle_name);
    let old_cab = cab_node_name(bundle)?;
    if new_cab != old_cab {
        for e in bundle.files.iter_mut() {
            if e.name.starts_with(&old_cab) {
                e.name = format!("{new_cab}{}", &e.name[old_cab.len()..]);
            }
        }
    }
    let cab = cab_node_name(bundle)?;

    // Genuine Unity WebGL never emits a .resS sidecar: every Texture2D keeps
    // its image data inline (m_StreamData empty). Force all texture blobs inline
    // for the webgl target so no .resS node is produced.
    let webgl_inline_all = target == "webgl";
    let built = if blobs.is_empty() {
        None
    } else {
        let pred =
            |t: &ress::TextureBlob| webgl_inline_all || inline_pids.contains(&t.path_id);
        let predicate: Option<&dyn Fn(&ress::TextureBlob) -> bool> =
            if !webgl_inline_all && inline_pids.is_empty() {
                None
            } else {
                Some(&pred)
            };
        Some(ress::build_ress(blobs, &cab, predicate))
    };

    let mut overrides: HashMap<i64, Value> = HashMap::new();
    if let Some(b) = &built {
        for (pid, sd) in &b.stream_data {
            if let Some((tn, tree)) = objects.get(pid) {
                if tn == "Texture2D" {
                    let mut tree = tree.clone();
                    tree.insert(
                        "m_StreamData",
                        map! {"offset" => sd.offset as i64, "size" => sd.size as i64, "path" => sd.path.clone()},
                    );
                    if !sd.path.is_empty() {
                        tree.insert("image data", Value::Bytes(Vec::new()));
                    }
                    overrides.insert(*pid, tree);
                }
            }
        }
    }

    let mut ext_perm: Option<HashMap<i64, i64>> = None;
    if let ExternalsPolicy::ShaderRef { ext_bundle_files } = &externals {
        let n_ext = 1 + ext_bundle_files.len() as i64;
        if n_ext > 1 {
            let mut first_use: Vec<i64> = Vec::new();
            for (_pid, (type_name, tree)) in objects.iter() {
                if let Some(node) = proto.get(type_name).and_then(|st| st.node.as_ref()) {
                    collect_pptr_first_use(tree, node, &mut first_use);
                }
            }

            for fid in 1..=n_ext {
                if !first_use.contains(&fid) {
                    first_use.push(fid);
                }
            }
            if first_use
                .iter()
                .enumerate()
                .any(|(i, &f)| f != i as i64 + 1)
            {
                let remap: HashMap<i64, i64> = first_use
                    .iter()
                    .enumerate()
                    .map(|(i, &old)| (old, i as i64 + 1))
                    .collect();
                for (pid, (_tn, tree)) in objects.iter() {
                    let mut t = overrides.remove(pid).unwrap_or_else(|| tree.clone());
                    remap_pptr_fids(&mut t, &remap);
                    overrides.insert(*pid, t);
                }
                ext_perm = Some(remap);
            }
        }
    }

    let mut used_types: Vec<SerializedType> = Vec::new();
    let mut type_index: HashMap<String, i32> = HashMap::new();
    let mut out_objects: Vec<unity::Object> = Vec::new();
    for (pid, (type_name, tree)) in objects.iter() {
        let tree = overrides.get(pid).unwrap_or(tree);
        if !type_index.contains_key(type_name) {
            let st = proto
                .get(type_name)
                .ok_or_else(|| anyhow!("no proto type for {type_name}"))?
                .clone();
            type_index.insert(type_name.clone(), used_types.len() as i32);
            used_types.push(st);
        }
        let tid = type_index[type_name];
        let node = used_types[tid as usize]
            .node
            .as_ref()
            .ok_or_else(|| anyhow!("type {type_name} has no node"))?;
        let data = unity::write_typetree(tree, node, big_endian);
        out_objects.push(unity::Object {
            path_id: *pid,
            type_id: tid,
            class_id: used_types[tid as usize].class_id,
            type_name: type_name.clone(),
            data,
        });
    }

    let shcab = cabname::shader_bundle_cab(target).to_string();
    {
        let sf = bundle
            .serialized_mut()
            .ok_or_else(|| anyhow!("bundle has no serialized file"))?;
        sf.types = used_types;
        sf.objects = out_objects;
        match externals {
            ExternalsPolicy::ShaderRef { ext_bundle_files } => {
                if !sf.externals.is_empty() {
                    let mut shader_ext = sf.externals[0].clone();
                    shader_ext.path = format!("archive:/{shcab}/{shcab}");
                    let mut new_ext = vec![shader_ext.clone()];
                    for bf in ext_bundle_files {
                        let cab = cabname::cab_name(bf);
                        let mut fi = shader_ext.clone();
                        fi.path = format!("archive:/{cab}/{cab}");
                        fi.guid = [0u8; 16];
                        fi.r#type = 0;
                        new_ext.push(fi);
                    }

                    if let Some(remap) = &ext_perm {
                        let mut permuted = new_ext.clone();
                        for (i, fi) in new_ext.into_iter().enumerate() {
                            let new_fid = remap[&(i as i64 + 1)];
                            permuted[(new_fid - 1) as usize] = fi;
                        }
                        new_ext = permuted;
                    }
                    sf.externals = new_ext;
                }
            }
            ExternalsPolicy::Clear => {
                sf.externals = Vec::new();
            }
        }
        if let Some(tp) = target_platform_for(target) {
            sf.target_platform = tp;
        }
    }

    if let Some(b) = built.as_ref().filter(|b| !b.payload.is_empty()) {
        let target_name = ress::ress_node_name(&cab);

        bundle.files.retain(|e| {
            let lower = e.name.to_lowercase();
            !(lower.ends_with(".ress") && e.name != target_name)
        });
        if let Some(e) = bundle.files.iter_mut().find(|e| e.name == target_name) {
            e.content = unity::bundle_file::FileContent::Raw(b.payload.clone());
            e.flags = ress::RESS_NODE_FLAGS;
        } else {
            bundle.files.push(unity::bundle_file::BundleEntry {
                name: target_name,
                content: unity::bundle_file::FileContent::Raw(b.payload.clone()),
                flags: ress::RESS_NODE_FLAGS,
            });
        }
    } else {
        let ress_nodes: Vec<String> = bundle
            .files
            .iter()
            .filter(|e| e.name.to_lowercase().ends_with(".ress"))
            .map(|e| e.name.clone())
            .collect();
        for name in ress_nodes {
            bundle.remove_file(&name);
        }
    }
    Ok(())
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

    /// Reference-driven: emit the unreferenced "DCL_Scene" default Material (+ its
    /// `DCL_Scene.mat` container/preload entries) for this glb bundle, the way
    /// production glTFast does for the first glb-bearing conversion of an editor
    /// session (its `s_DefaultMaterial` static cache materializes once and the
    /// asset-bundle-converter captures it into every bundle produced during that
    /// first conversion). Set per-bundle by `--from-reference` when the matching
    /// reference bundle actually contains DCL_Scene, so abgen mirrors the
    /// reference's presence exactly without over-emitting on bundles that
    /// genuinely lack it.
    pub force_default_material: bool,

    /// `--magenta-missing`: instead of dropping a texture that fails to resolve
    /// or decode, substitute a magenta placeholder with the failure baked in as
    /// text (see `crate::placeholder`). Makes broken content renderable and
    /// obviously broken. Off by default so parity output stays byte-exact.
    pub magenta_missing: bool,
}

impl<'a> BuildOpts<'a> {
    pub const COLLECTION_MODE_ENV: &'static str = "ABGEN_COLLECTION_MODE";

    pub const REAL_TEXTURES_ENV: &'static str = "ABGEN_REAL_TEXTURES";

    pub const V38_COMPAT_ENV: &'static str = "ABGEN_V38_COMPAT";

    pub const V38_TIMESTAMP_ENV: &'static str = "ABGEN_V38_TIMESTAMP";
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

    let collection_mode = std::env::var(BuildOpts::COLLECTION_MODE_ENV).is_ok();
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
        );
        let data = b.build(bytes, &mut bundle)?;
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
    let scene = gltf::parse(bytes, ext, opts.resolve, opts.magenta_missing).context("parse glb")?;
    let image_uri = scene.image_uri.clone();

    let is_gltf = ext == ".gltf";

    let is_wearable = std::env::var(BuildOpts::COLLECTION_MODE_ENV).is_err()
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
        bytes.to_vec(),
        gltf_json,
        gltf_buffers,
        opts.resolve_hash,
        opts.metadata_dependencies.to_vec(),
        externals_position,
        cross_bundle_position,
        material_externals_overrides,
        opts.force_default_material,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity::bundle_file::{Bundle as ReadBundle, FileContent};

    fn png_with_chunks(extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut push = |typ: &[u8; 4], body: &[u8]| {
            v.extend_from_slice(&(body.len() as u32).to_be_bytes());
            v.extend_from_slice(typ);
            v.extend_from_slice(body);
            v.extend_from_slice(&[0, 0, 0, 0]);
        };

        push(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        for (t, b) in extra {
            push(t, b);
        }
        push(b"IDAT", &[]);
        push(b"IEND", &[]);
        v
    }

    #[test]
    fn png_gamma_gate_fires_only_on_nontrivial_gama_without_srgb() {
        let nontrivial = 55531u32.to_be_bytes().to_vec();
        let trivial = 45455u32.to_be_bytes().to_vec();

        let png = png_with_chunks(&[(b"gAMA", nontrivial.clone())]);
        assert_eq!(png_gamma_to_apply(&png), Some(55531));

        let png = png_with_chunks(&[(b"gAMA", trivial)]);
        assert_eq!(png_gamma_to_apply(&png), None);

        let png = png_with_chunks(&[(b"sRGB", vec![0]), (b"gAMA", nontrivial)]);
        assert_eq!(png_gamma_to_apply(&png), None);

        let png = png_with_chunks(&[
            (b"iCCP", b"Adobe RGB (1998)\0\0".to_vec()),
            (b"cHRM", vec![0; 32]),
        ]);
        assert_eq!(png_gamma_to_apply(&png), None);
    }

    #[test]
    fn png_gamma_lut_matches_freeimage_curve() {
        let mut img = RgbaImage::from_pixel(4, 1, image::Rgba([0, 0, 0, 255]));
        let vals = [28u8, 64, 128, 192, 255];

        let expect = [42u8, 82, 145, 202, 255];
        for (&v, &e) in vals.iter().zip(expect.iter()) {
            let mut one = RgbaImage::from_pixel(1, 1, image::Rgba([v, v, v, 200]));
            apply_png_gamma(&mut one, 55531);
            let p = one.get_pixel(0, 0);
            assert_eq!(p[0], e, "in={v}");
            assert_eq!(p[3], 200, "alpha untouched");
        }

        apply_png_gamma(&mut img, 55531);
        assert_eq!(img.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn natural_bundle_cmp_orders_digit_runs_numerically() {
        use std::cmp::Ordering;

        assert_eq!(
            natural_bundle_cmp(
                "bafkreig7pqew5umjh46onc3zowyub2pkjoikltldxtxi26rnists3k3rdm_windows",
                "bafkreig42hknvr5derr24elh4l3uxwnsef6ddvzcfv7x2ys64goj4ov6vy_windows",
            ),
            Ordering::Less
        );

        assert_eq!(natural_bundle_cmp("abc", "abd"), Ordering::Less);
        assert_eq!(natural_bundle_cmp("abc", "abc"), Ordering::Equal);
        assert_eq!(natural_bundle_cmp("ab", "abc"), Ordering::Less);

        assert_eq!(natural_bundle_cmp("a4x", "abx"), Ordering::Less);

        assert_eq!(natural_bundle_cmp("a7b", "a07b"), Ordering::Less);

        assert_eq!(natural_bundle_cmp("a42b", "a42c"), Ordering::Less);
        assert_eq!(natural_bundle_cmp("a42b", "a43a"), Ordering::Less);
    }

    fn tiny_gltf(n_materials: usize) -> Vec<u8> {
        const BUF_B64: &str = "AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAAAAABAAIA";
        let mats: Vec<String> = (0..n_materials)
            .map(|i| format!("{{\"name\":\"mat_{i}\",\"pbrMetallicRoughness\":{{}}}}"))
            .collect();
        let mat_block = if mats.is_empty() {
            String::new()
        } else {
            format!(",\"materials\":[{}]", mats.join(","))
        };
        let mat_ref = if n_materials > 0 {
            ",\"material\":0"
        } else {
            ""
        };
        format!(
            "{{\"asset\":{{\"version\":\"2.0\"}},\
             \"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\
             \"nodes\":[{{\"mesh\":0,\"name\":\"tri\"}}],\
             \"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0}},\"indices\":1{mat_ref}}}]}}]\
             {mat_block},\
             \"accessors\":[\
               {{\"bufferView\":0,\"componentType\":5126,\"count\":3,\"type\":\"VEC3\",\
                 \"min\":[0,0,0],\"max\":[1,1,0]}},\
               {{\"bufferView\":1,\"componentType\":5123,\"count\":3,\"type\":\"SCALAR\"}}],\
             \"bufferViews\":[\
               {{\"buffer\":0,\"byteOffset\":0,\"byteLength\":36}},\
               {{\"buffer\":0,\"byteOffset\":36,\"byteLength\":6}}],\
             \"buffers\":[{{\"byteLength\":42,\"uri\":\"data:application/octet-stream;base64,{BUF_B64}\"}}]}}"
        )
        .into_bytes()
    }

    struct BundleProbe {
        dcl_scene_materials: usize,
        material_names: Vec<String>,

        dcl_scene_container: Option<usize>,

        renderer_mat_pids: Vec<i64>,
        dcl_scene_pid: Option<i64>,
        keywords_empty: bool,
    }

    fn probe(data: &[u8]) -> BundleProbe {
        let b = ReadBundle::load_bytes(data).expect("bundle parses");
        let mut p = BundleProbe {
            dcl_scene_materials: 0,
            material_names: Vec::new(),
            dcl_scene_container: None,
            renderer_mat_pids: Vec::new(),
            dcl_scene_pid: None,
            keywords_empty: false,
        };
        for f in &b.files {
            let FileContent::Serialized(sf) = &f.content else {
                continue;
            };
            for o in &sf.objects {
                match o.class_id {
                    21 => {
                        let v = sf.read_typetree(o).unwrap();
                        let name = v
                            .get("m_Name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        if name == "DCL_Scene" {
                            p.dcl_scene_materials += 1;
                            p.dcl_scene_pid = Some(o.path_id);
                            let empty =
                                |k: &str| matches!(v.get(k), Some(Value::Array(a)) if a.is_empty());
                            p.keywords_empty =
                                empty("m_ValidKeywords") && empty("m_InvalidKeywords");
                        }
                        p.material_names.push(name);
                    }
                    23 | 137 => {
                        let v = sf.read_typetree(o).unwrap();
                        if let Some(Value::Array(mats)) = v.get("m_Materials") {
                            for m in mats {
                                let fid = m.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(0);
                                let pid = m.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(0);
                                if fid == 0 {
                                    p.renderer_mat_pids.push(pid);
                                }
                            }
                        }
                    }
                    142 => {
                        let v = sf.read_typetree(o).unwrap();
                        if let Some(Value::Array(cont)) = v.get("m_Container") {
                            for e in cont {
                                let Value::Array(pair) = e else { continue };
                                if pair.len() == 2 && pair[0].as_str() == Some("DCL_Scene.mat") {
                                    let sz = pair[1]
                                        .get("preloadSize")
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(-1);
                                    p.dcl_scene_container = Some(sz as usize);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        p
    }

    fn build_tiny(n_materials: usize) -> BundleProbe {
        let gltf = tiny_gltf(n_materials);
        let opts = BuildOpts {
            source_file: Some("test.gltf"),
            ..BuildOpts::default()
        };
        let art = build_bundle(&gltf, "QmTestTinyTri_windows", "QmTestTinyTri", &opts)
            .expect("build_bundle");
        probe(&art.data)
    }

    #[test]
    fn v38_compat_dcl_scene_default_material() {
        if !template_path().exists() {
            eprintln!(
                "skipping: template bundle not found at {}",
                template_path().display()
            );
            return;
        }
        std::env::remove_var(BuildOpts::V38_COMPAT_ENV);

        let off = build_tiny(1);
        assert_eq!(off.dcl_scene_materials, 0);
        assert_eq!(off.material_names, vec!["material_0".to_string()]);
        assert_eq!(off.dcl_scene_container, None);
        let off_renderer_mats = off.renderer_mat_pids.clone();
        let off_zero = build_tiny(0);
        assert!(off_zero.material_names.is_empty());
        assert_eq!(off_zero.dcl_scene_container, None);

        std::env::set_var(BuildOpts::V38_COMPAT_ENV, "1");
        std::env::set_var(BuildOpts::V38_TIMESTAMP_ENV, "0");
        let on = build_tiny(1);
        let on_zero = build_tiny(0);
        std::env::remove_var(BuildOpts::V38_COMPAT_ENV);
        std::env::remove_var(BuildOpts::V38_TIMESTAMP_ENV);

        assert_eq!(on.dcl_scene_materials, 1);
        assert_eq!(on.material_names.len(), 2);
        assert!(on.keywords_empty);
        assert_eq!(on.dcl_scene_container, Some(2));

        assert_eq!(on.renderer_mat_pids.len(), off_renderer_mats.len());
        let ds = on.dcl_scene_pid.unwrap();
        assert!(on.renderer_mat_pids.iter().all(|&p| p != ds));

        assert_eq!(on_zero.dcl_scene_materials, 1);
        assert_eq!(on_zero.material_names, vec!["DCL_Scene".to_string()]);
        assert_eq!(on_zero.dcl_scene_container, Some(2));
        let dz = on_zero.dcl_scene_pid.unwrap();
        assert!(on_zero.renderer_mat_pids.iter().all(|&p| p != dz));
    }

    fn build_tiny_force_dcl_scene(n_materials: usize) -> BundleProbe {
        let gltf = tiny_gltf(n_materials);
        let opts = BuildOpts {
            source_file: Some("test.gltf"),
            force_default_material: true,
            ..BuildOpts::default()
        };
        let art = build_bundle(&gltf, "QmTestForceDcl_windows", "QmTestForceDcl", &opts)
            .expect("build_bundle");
        probe(&art.data)
    }

    #[test]
    fn force_default_material_emits_dcl_scene_without_v38() {
        if !template_path().exists() {
            eprintln!(
                "skipping: template bundle not found at {}",
                template_path().display()
            );
            return;
        }
        // The per-bundle opts flag (set by --from-reference) must emit DCL_Scene
        // even though V38_COMPAT / COLLECTION_MODE are unset, mirroring a reference
        // bundle that contains it — and it must NOT bind DCL_Scene to renderers.
        std::env::remove_var(BuildOpts::V38_COMPAT_ENV);
        std::env::remove_var(BuildOpts::COLLECTION_MODE_ENV);

        // glb with a real material: DCL_Scene is the second, unreferenced material.
        let on = build_tiny_force_dcl_scene(1);
        assert_eq!(on.dcl_scene_materials, 1);
        assert_eq!(on.material_names.len(), 2);
        assert_eq!(on.dcl_scene_container, Some(2));
        let ds = on.dcl_scene_pid.unwrap();
        assert!(on.renderer_mat_pids.iter().all(|&p| p != ds));

        // zero-material glb: DCL_Scene is the only material.
        let on_zero = build_tiny_force_dcl_scene(0);
        assert_eq!(on_zero.dcl_scene_materials, 1);
        assert_eq!(on_zero.material_names, vec!["DCL_Scene".to_string()]);
        assert_eq!(on_zero.dcl_scene_container, Some(2));
    }
}
