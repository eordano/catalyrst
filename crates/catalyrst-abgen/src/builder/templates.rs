use super::*;

fn abgen_root() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("ABGEN_ROOT") {
        return std::path::PathBuf::from(p);
    }
    let compiled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    if compiled.join("template").is_dir() {
        return compiled;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("template").is_dir() {
                return dir.to_path_buf();
            }
        }
    }
    compiled
}

pub(super) fn template_path() -> PathBuf {
    abgen_root()
        .join("template")
        .join("all-types.windows.bundle")
}

pub fn template_dir() -> PathBuf {
    abgen_root().join("template")
}

pub fn template_available() -> bool {
    template_path().is_file()
}

pub const REQUIRED_TEMPLATES: [&str; 4] = [
    "all-types.windows.bundle",
    "animated-types.windows.bundle",
    "emote-types.windows.bundle",
    "skinned-types.windows.bundle",
];

pub fn templates_missing_in(dir: &std::path::Path) -> Vec<String> {
    REQUIRED_TEMPLATES
        .iter()
        .filter(|f| !dir.join(f).is_file())
        .map(|f| f.to_string())
        .collect()
}

pub fn templates_missing() -> Vec<String> {
    templates_missing_in(&template_dir())
}

fn aux_types() -> &'static HashMap<String, (SerializedType, Value)> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<HashMap<String, (SerializedType, Value)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out: HashMap<String, (SerializedType, Value)> = HashMap::new();

        let harvest = |out: &mut HashMap<String, (SerializedType, Value)>,
                       file: &str,
                       mapping: &[(&str, &str)]| {
            let bundle = match read_template_bundle(file) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(
                        template = file,
                        error = %e,
                        "aux template unavailable — animation/skinned emission \
                         disabled for the types it provides"
                    );
                    return;
                }
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

#[cfg(not(target_arch = "wasm32"))]
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

// The wasm build has no filesystem: the four type-tree templates are
// embedded at compile time instead of mmapped from ABGEN_ROOT/template.
#[cfg(target_arch = "wasm32")]
fn embedded_template(file: &str) -> Option<&'static [u8]> {
    match file {
        "all-types.windows.bundle" => {
            Some(include_bytes!("../../template/all-types.windows.bundle"))
        }
        "animated-types.windows.bundle" => Some(include_bytes!(
            "../../template/animated-types.windows.bundle"
        )),
        "emote-types.windows.bundle" => {
            Some(include_bytes!("../../template/emote-types.windows.bundle"))
        }
        "skinned-types.windows.bundle" => Some(include_bytes!(
            "../../template/skinned-types.windows.bundle"
        )),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn template_all_bytes() -> Result<&'static [u8]> {
    Ok(&template_mmap()?[..])
}

#[cfg(target_arch = "wasm32")]
fn template_all_bytes() -> Result<&'static [u8]> {
    embedded_template("all-types.windows.bundle")
        .ok_or_else(|| anyhow!("all-types template not embedded"))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_template_bundle(file: &str) -> std::result::Result<Bundle, String> {
    let path = abgen_root().join("template").join(file);
    if !path.exists() {
        return Err(format!("missing at {}", path.display()));
    }
    Bundle::load(&path).map_err(|e| format!("{e:#}"))
}

#[cfg(target_arch = "wasm32")]
fn read_template_bundle(file: &str) -> std::result::Result<Bundle, String> {
    let bytes = embedded_template(file).ok_or_else(|| "not embedded".to_string())?;
    let d = Bundle::decompress_bytes(bytes).map_err(|e| format!("{e:#}"))?;
    Bundle::from_decompressed(&d).map_err(|e| format!("{e:#}"))
}

pub(super) fn load_template() -> Result<(
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
            let mm = template_all_bytes()?;
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

pub(super) fn cab_node_name(bundle: &Bundle) -> Result<String> {
    bundle
        .files
        .iter()
        .find(|e| !e.name.to_lowercase().ends_with(".ress"))
        .map(|e| e.name.clone())
        .ok_or_else(|| anyhow!("no SerializedFile node found in bundle container"))
}
