use crate::local_store::LocalContentStore;
use crate::naming;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

pub fn file_ext_lower(name: &str) -> String {
    let l = name.to_lowercase();
    for e in [".gltf", ".glb", ".png", ".jpg", ".jpeg"] {
        if l.ends_with(e) {
            return e.to_string();
        }
    }
    String::new()
}

#[derive(Default)]
pub struct UriCache {
    map: RwLock<HashMap<(String, String), Arc<Vec<String>>>>,
}

impl UriCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_or_parse(&self, hash: &str, ext: &str, data: &[u8]) -> Arc<Vec<String>> {
        let key = (hash.to_string(), ext.to_string());
        if let Some(u) = self.map.read().unwrap().get(&key) {
            return u.clone();
        }
        let uris = Arc::new(naming::parse_gltf_image_uris(data, ext).unwrap_or_default());
        self.map
            .write()
            .unwrap()
            .entry(key)
            .or_insert_with(|| uris.clone())
            .clone()
    }
}

pub struct EntityScan {
    pub model_refs: HashSet<String>,

    pub linear_refs: HashSet<String>,

    pub normal_refs: HashSet<String>,

    uris: HashMap<(String, String), Arc<Vec<String>>>,
}

pub fn scan_entity(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    cache: &UriCache,
) -> EntityScan {
    let glb_files: Vec<(&String, &String)> = content_by_file
        .iter()
        .filter(|(f, _)| {
            let fl = f.to_lowercase();
            fl.ends_with(".glb") || fl.ends_with(".gltf")
        })
        .collect();

    type PerGlb = (
        Option<HashSet<String>>,
        Vec<String>,
        Vec<String>,
        Option<((String, String), Arc<Vec<String>>)>,
    );
    let per_glb: Vec<PerGlb> = glb_files
        .par_iter()
        .map(|&(f, h)| scan_one(store, content_by_file, cache, f, h))
        .collect();

    let mut model_refs: HashSet<String> = HashSet::new();
    let mut linear_refs: HashSet<String> = HashSet::new();
    let mut normal_refs: HashSet<String> = HashSet::new();
    let mut uris_map: HashMap<(String, String), Arc<Vec<String>>> = HashMap::new();
    for (mr, lr, nr, ue) in per_glb {
        if let Some(mr) = mr {
            model_refs.extend(mr);
        }
        linear_refs.extend(lr);

        normal_refs.extend(nr);
        if let Some((k, v)) = ue {
            uris_map.insert(k, v);
        }
    }
    EntityScan {
        model_refs,
        linear_refs,
        normal_refs,
        uris: uris_map,
    }
}

fn scan_one(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    cache: &UriCache,
    f: &str,
    h: &str,
) -> (
    Option<HashSet<String>>,
    Vec<String>,
    Vec<String>,
    Option<((String, String), Arc<Vec<String>>)>,
) {
    let fl = f.to_lowercase();
    let data = match store.fetch_mmap(h) {
        Ok(d) => d,
        Err(_) => return (None, Vec::new(), Vec::new(), None),
    };

    let ext = file_ext_lower(f);
    let uris = cache.get_or_parse(h, &ext, &data);
    let mut per_glb: HashSet<String> = HashSet::new();
    let mut ok = true;
    for uri in uris.iter() {
        let resolved = match naming::resolve_uri_to_content_file(uri, f) {
            Ok(r) => r,
            Err(_) => {
                ok = false;
                break;
            }
        };
        match content_by_file.get(&resolved.to_lowercase()) {
            Some(h2) => {
                per_glb.insert(h2.clone());
            }
            None => {
                ok = false;
                break;
            }
        }
    }
    let model_refs = if ok { Some(per_glb) } else { None };
    let uris_entry = Some(((h.to_string(), ext), uris));

    let mut linear_refs: Vec<String> = Vec::new();
    let mut normal_refs: Vec<String> = Vec::new();
    let parse_ext = if fl.ends_with(".gltf") { "gltf" } else { "glb" };
    let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
        let key = naming::resolve_uri_to_content_file(uri, f)
            .ok()?
            .to_lowercase();
        let hh = content_by_file.get(&key)?;
        store.fetch(hh).ok()
    };
    let resolve: crate::gltf::Resolve = Some(&resolve_fn);
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::gltf::parse_classify(&data, parse_ext, resolve)
    }));
    if let Ok(Ok(scene)) = parsed {
        let image_hash = |idx: usize| -> Option<String> {
            let uri = scene.image_uri.get(idx).and_then(|o| o.as_ref())?;
            let key = naming::resolve_uri_to_content_file(uri, f)
                .ok()?
                .to_lowercase();
            content_by_file.get(&key).cloned()
        };
        let mut normal_idx: HashSet<usize> = HashSet::new();
        let mut other_idx: HashSet<usize> = HashSet::new();
        for m in &scene.materials {
            if let Some(t) = &m.normal_image {
                normal_idx.insert(t.image);
                if let Some(c) = image_hash(t.image) {
                    linear_refs.push(c);
                }
            }
            if let Some(t) = &m.metallic_roughness_image {
                if let Some(c) = image_hash(t.image) {
                    linear_refs.push(c);
                }
            }

            for tr in [m.metallic_roughness_image, m.occlusion_image]
                .into_iter()
                .flatten()
            {
                other_idx.insert(tr.image);
            }
        }
        for idx in normal_idx.difference(&other_idx) {
            if let Some(c) = image_hash(*idx) {
                normal_refs.push(c);
            }
        }
    }
    (model_refs, linear_refs, normal_refs, uris_entry)
}

impl EntityScan {
    pub fn metadata_deps(
        &self,
        store: &LocalContentStore,
        glb_file: &str,
        glb_hash: &str,
        content_by_file: &HashMap<String, String>,
        platform: &str,
    ) -> Vec<String> {
        let ext = file_ext_lower(glb_file);
        let uris: Arc<Vec<String>> = match self.uris.get(&(glb_hash.to_string(), ext.clone())) {
            Some(u) => u.clone(),
            None => match store.fetch_mmap(glb_hash) {
                Ok(data) => {
                    Arc::new(naming::parse_gltf_image_uris(&data, &ext).unwrap_or_default())
                }
                Err(_) => Arc::new(Vec::new()),
            },
        };
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for uri in uris.iter() {
            let resolved = match naming::resolve_uri_to_content_file(uri, glb_file) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let h = match content_by_file.get(&resolved.to_lowercase()) {
                Some(h) => h,
                None => continue,
            };
            let name = format!("{h}_{platform}");
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
        out
    }
}
