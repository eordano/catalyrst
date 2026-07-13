use crate::build::{derive_one_entity, load_entity_json, IMAGE_EXTS};
use crate::{BundleSpec, ContentItem, EntityEntry, Manifest};
use abgen::glbscan::file_ext_lower;
use abgen::hashes;
use abgen::local_store::LocalContentStore;
use abgen::{naming, Result};
use anyhow::{anyhow, Context};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn reference_bundle_has_dcl_scene(path: &Path) -> bool {
    use abgen::unity::bundle_file::{Bundle, FileContent};
    const C_MATERIAL: i32 = 21;
    let Ok(bundle) = Bundle::load(path) else {
        return false;
    };
    for f in &bundle.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for obj in &sf.objects {
            if obj.class_id != C_MATERIAL {
                continue;
            }
            if let Ok(v) = sf.read_typetree(obj) {
                if v.get("m_Name").and_then(|x| x.as_str()) == Some("DCL_Scene") {
                    return true;
                }
            }
        }
    }
    false
}

fn cid_from_bundle_name(name: &str, platform: &str) -> String {
    let suffix = format!("_{platform}");
    let base = if let Some(stripped) = name.strip_suffix(&suffix) {
        stripped
    } else {
        name
    };
    match base.split_once('_') {
        Some((head, _)) => head.to_string(),
        None => base.to_string(),
    }
}

fn image_uris(data: &[u8], ext: &str) -> Vec<String> {
    naming::parse_gltf_image_uris(data, ext).unwrap_or_default()
}

fn collect_model_referenced_hashes(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
) -> HashSet<String> {
    let mut refs: HashSet<String> = HashSet::new();
    for (f, h) in content_by_file {
        let fl = f.to_lowercase();
        if !(fl.ends_with(".glb") || fl.ends_with(".gltf")) {
            continue;
        }
        let data = match store.fetch(h) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let ext = file_ext_lower(f);
        let uris = image_uris(&data, &ext);
        let mut per_glb: HashSet<String> = HashSet::new();
        let mut ok = true;
        for uri in &uris {
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
        if ok {
            refs.extend(per_glb);
        }
    }
    refs
}

fn collect_linear_texture_hashes(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut linear: HashSet<String> = HashSet::new();
    let mut normal: HashSet<String> = HashSet::new();
    for (f, h) in content_by_file {
        let fl = f.to_lowercase();
        if !(fl.ends_with(".glb") || fl.ends_with(".gltf")) {
            continue;
        }
        let data = match store.fetch(h) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let ext = if fl.ends_with(".gltf") { "gltf" } else { "glb" };
        let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
            let key = naming::resolve_uri_to_content_file(uri, f)
                .ok()?
                .to_lowercase();
            let hh = content_by_file.get(&key)?;
            store.fetch(hh).ok()
        };
        let resolve: abgen::gltf::Resolve = Some(&resolve_fn);
        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            abgen::gltf::parse(&data, ext, resolve, false, false)
        }));
        let scene = match parsed {
            Ok(Ok(s)) => s,
            _ => continue,
        };
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
                    linear.insert(c);
                }
            }
            if let Some(t) = &m.metallic_roughness_image {
                if let Some(c) = image_hash(t.image) {
                    linear.insert(c);
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
                normal.insert(c);
            }
        }
    }
    (linear, normal)
}

fn best_glb_file(
    glb_bytes: &[u8],
    candidates: &[String],
    content_by_file: &HashMap<String, String>,
) -> Option<String> {
    let mut best: Option<(usize, &String)> = None;
    for f in candidates {
        let ext = file_ext_lower(f);
        let uris = image_uris(glb_bytes, &ext);
        let resolved = uris
            .iter()
            .filter(|u| {
                naming::resolve_uri_to_content_file(u, f)
                    .ok()
                    .map(|r| content_by_file.contains_key(&r.to_lowercase()))
                    .unwrap_or(false)
            })
            .count();
        if best.as_ref().map(|(n, _)| resolved > *n).unwrap_or(true) {
            best = Some((resolved, f));
        }
    }
    best.map(|(_, f)| f.clone())
}

fn metadata_deps_for_glb(
    glb_bytes: &[u8],
    glb_file: &str,
    content_by_file: &HashMap<String, String>,
    platform: &str,
) -> Vec<String> {
    let ext = file_ext_lower(glb_file);
    let uris = image_uris(glb_bytes, &ext);
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for uri in &uris {
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

pub(crate) fn from_entity_ids(
    ids_path: &str,
    content_dir: &str,
    platform: &str,
    cdn_layout: bool,
    fetch_from: Option<&str>,
) -> Result<Manifest> {
    let raw = std::fs::read_to_string(ids_path).with_context(|| format!("read {ids_path}"))?;
    let ids: Vec<String> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    if let Some(csu) = fetch_from {
        fetch_ids_into_store(&LocalContentStore::new(content_dir), csu, &ids);
    }
    manifest_from_ids(&ids, content_dir, platform, cdn_layout)
}

pub(crate) fn contents_base_url(content_server_url: &str) -> String {
    format!("{}/contents/", content_server_url.trim_end_matches('/'))
}

pub(crate) fn fetch_ids_into_store(
    store: &LocalContentStore,
    content_server_url: &str,
    ids: &[String],
) {
    let base_url = contents_base_url(content_server_url);
    eprintln!(
        "fetch-missing: filling the store from {base_url} ({} entities)",
        ids.len()
    );
    let t0 = Instant::now();
    let mut files = 0usize;
    let mut failed = 0usize;
    for id in ids {
        let had_entity = store.exists(id);
        let scene = abgen::worlds::WorldScene {
            entity_id: id.clone(),
            base_url: base_url.clone(),
        };
        match abgen::worlds::fetch_scene_into_store(store, &scene) {
            Ok((fetched, total)) => {
                if !had_entity || fetched > 0 {
                    eprintln!("fetch-missing: {id} ({fetched}/{total} content files fetched)");
                }
                files += fetched;
            }
            Err(e) => {
                failed += 1;
                eprintln!("fetch-missing: {id}: {e:#}");
            }
        }
    }
    eprintln!(
        "fetch-missing: {files} content files fetched, {failed} of {} entities unavailable ({:.1}s)",
        ids.len(),
        t0.elapsed().as_secs_f64()
    );
}

fn fetch_url_into_store(store: &LocalContentStore, url: &str, cid: &str) -> Result<bool> {
    if store.exists(cid) {
        return Ok(false);
    }
    let body = ureq::get(url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?
        .into_body()
        .into_with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .with_context(|| format!("read {url}"))?;
    store.write(cid, &body)?;
    Ok(true)
}

pub(crate) fn manifest_from_ids(
    ids: &[String],
    content_dir: &str,
    platform: &str,
    keep_shared_bundles: bool,
) -> Result<Manifest> {
    let store = LocalContentStore::new(content_dir);
    let missing = AtomicUsize::new(0);
    let t0 = Instant::now();
    let processed = AtomicUsize::new(0);
    let n_total = ids.len();
    let uri_cache = abgen::glbscan::UriCache::new();
    eprintln!("manifest: deriving from {n_total} entity ids (parallel)…");

    let per: Vec<EntityEntry> = ids
        .par_iter()
        .filter_map(|ent_id| {
            let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if done.is_multiple_of(5000) {
                let secs = t0.elapsed().as_secs_f64().max(0.001);
                eprintln!(
                    "  manifest: {done}/{n_total} entities ({:.0}/s, {:.0}s)",
                    done as f64 / secs,
                    secs,
                );
            }
            let entry = derive_one_entity(&store, ent_id, platform, &uri_cache);
            if entry.is_none() {
                missing.fetch_add(1, Ordering::Relaxed);
            }
            entry
        })
        .collect();

    let entities_out: Vec<EntityEntry> = if keep_shared_bundles {
        per
    } else {
        let mut seen_bundle: HashSet<String> = HashSet::new();
        let mut out: Vec<EntityEntry> = Vec::with_capacity(per.len());
        for mut e in per {
            e.bundles
                .retain(|b| seen_bundle.insert(b.bundle_name.clone()));
            if !e.bundles.is_empty() {
                out.push(e);
            }
        }
        out
    };

    let n_bundles: usize = entities_out.iter().map(|e| e.bundles.len()).sum();
    eprintln!(
        "manifest: derived in {:.1}s wall | {} requested, {} buildable entities, \
         {n_bundles} bundles ({} without buildable content)",
        t0.elapsed().as_secs_f64(),
        ids.len(),
        entities_out.len(),
        missing.into_inner(),
    );
    if entities_out.is_empty() {
        return Err(anyhow!(
            "no buildable bundles from {} entity ids",
            ids.len()
        ));
    }
    Ok(Manifest {
        content_dir: content_dir.to_string(),
        entities: entities_out,
    })
}

struct LiveRefEntity {
    entity_id: String,
    vintage: String,
    files: HashSet<String>,
}

pub(crate) fn parse_live_manifest(raw: &[u8], platform: &str) -> Option<(String, HashSet<String>)> {
    let doc: serde_json::Value = serde_json::from_slice(raw).ok()?;
    if doc.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(0) != 0 {
        return None;
    }
    let vintage = doc.get("version")?.as_str()?.to_string();
    let suffix = format!("_{platform}");
    let files: HashSet<String> = doc
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|f| f.as_str())
        .filter(|f| f.ends_with(&suffix))
        .map(|f| f.to_string())
        .collect();
    if files.is_empty() {
        return None;
    }
    Some((vintage, files))
}

fn scan_live_reference(ref_dir: &Path, platform: &str) -> Result<Vec<LiveRefEntity>> {
    let manifest_name = format!("{platform}.manifest.json");
    let mut out: Vec<LiveRefEntity> = Vec::new();
    let mut visit = |dir: &Path| {
        let Ok(raw) = std::fs::read(dir.join(&manifest_name)) else {
            return;
        };
        let Some((vintage, files)) = parse_live_manifest(&raw, platform) else {
            return;
        };
        let Some(id) = dir.file_name().and_then(|s| s.to_str()) else {
            return;
        };
        out.push(LiveRefEntity {
            entity_id: id.to_string(),
            vintage,
            files,
        });
    };
    for d1 in std::fs::read_dir(ref_dir)
        .with_context(|| format!("read_dir {}", ref_dir.display()))?
        .flatten()
    {
        if !d1.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let p1 = d1.path();
        if p1.join(&manifest_name).is_file() {
            visit(&p1);
            continue;
        }
        let Ok(inner) = std::fs::read_dir(&p1) else {
            continue;
        };
        for d2 in inner.flatten() {
            if !d2.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let p2 = d2.path();
            if p2.join(&manifest_name).is_file() {
                visit(&p2);
            }
        }
    }
    Ok(out)
}

pub(crate) fn sample_stride(total: usize, take: usize) -> Vec<usize> {
    if take == 0 || total == 0 {
        return Vec::new();
    }
    if total <= take {
        return (0..total).collect();
    }
    (0..take).map(|i| i * total / take).collect()
}

pub(crate) fn from_live_reference(
    ref_dir: &Path,
    content_dir: &str,
    platform: &str,
    per_vintage: usize,
) -> Result<(Manifest, serde_json::Value)> {
    let ents = scan_live_reference(ref_dir, platform)?;
    if ents.is_empty() {
        return Err(anyhow!(
            "live-mode: no usable {platform}.manifest.json under {} \
             (exitCode 0 + at least one _{platform} bundle required)",
            ref_dir.display()
        ));
    }
    let mut by_vintage: BTreeMap<String, Vec<&LiveRefEntity>> = BTreeMap::new();
    for e in &ents {
        by_vintage.entry(e.vintage.clone()).or_default().push(e);
    }

    let mut sampled: Vec<&LiveRefEntity> = Vec::new();
    let mut summary_vintages = serde_json::Map::new();
    for (vintage, mut list) in by_vintage {
        list.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        let picks: Vec<&LiveRefEntity> = sample_stride(list.len(), per_vintage)
            .into_iter()
            .map(|i| list[i])
            .collect();
        summary_vintages.insert(
            vintage.clone(),
            serde_json::json!({
                "total": list.len(),
                "sampled": picks.iter().map(|e| e.entity_id.clone()).collect::<Vec<_>>(),
            }),
        );
        eprintln!(
            "live-mode: vintage {vintage}: {} of {}",
            picks.len(),
            list.len()
        );
        sampled.extend(picks);
    }

    let ids: Vec<String> = sampled.iter().map(|e| e.entity_id.clone()).collect();
    let allowed: HashMap<&str, &HashSet<String>> = sampled
        .iter()
        .map(|e| (e.entity_id.as_str(), &e.files))
        .collect();

    let mut m = manifest_from_ids(&ids, content_dir, platform, true)?;
    let mut dropped_entities = 0usize;
    for e in &mut m.entities {
        if let Some(files) = allowed.get(e.entity_id.as_str()) {
            e.bundles.retain(|b| files.contains(&b.bundle_name));
        }
    }
    m.entities.retain(|e| {
        if e.bundles.is_empty() {
            dropped_entities += 1;
            false
        } else {
            true
        }
    });
    if m.entities.is_empty() {
        return Err(anyhow!(
            "live-mode: none of the {} sampled entities have buildable bundles that \
             overlap their upstream manifest",
            ids.len()
        ));
    }

    let summary = serde_json::json!({
        "mode": "live-mode",
        "reference": ref_dir.to_string_lossy(),
        "platform": platform,
        "per_vintage": per_vintage,
        "flags": { "real_textures": true, "v38_compat": true },
        "vintages": serde_json::Value::Object(summary_vintages),
        "sampled_entities": ids.len(),
        "buildable_entities": m.entities.len(),
        "entities_without_buildable_overlap": dropped_entities,
    });
    Ok((m, summary))
}

pub(crate) fn from_collection_urn(
    urn: &str,
    lambdas_url: &str,
    content_dir: &str,
    platform: &str,
    fetch_missing: bool,
) -> Result<Manifest> {
    let store = LocalContentStore::new(content_dir);
    let base = lambdas_url.trim_end_matches('/');
    let url = format!("{base}/collections/wearables?collectionId={urn}");
    eprintln!("resolving collection {urn} via {url}");
    let body = ureq::get(&url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?
        .into_body()
        .into_with_config()
        .limit(512 * 1024 * 1024)
        .read_to_string()
        .context("read lambdas response")?;
    let doc: serde_json::Value = serde_json::from_str(&body).context("parse lambdas JSON")?;
    let wearables = doc
        .get("wearables")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("lambdas response has no 'wearables' array (collection empty or wrong lambdas-url?)"))?;

    let mut entities_out: Vec<EntityEntry> = Vec::new();
    let mut seen_bundle: HashSet<String> = HashSet::new();
    for w in wearables {
        let wid = w
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut content_items: Vec<ContentItem> = Vec::new();
        let mut content_by_file: HashMap<String, String> = HashMap::new();
        let mut content_urls: Vec<(String, String)> = Vec::new();
        let reps = w
            .get("data")
            .and_then(|d| d.get("representations"))
            .and_then(|v| v.as_array());
        if let Some(reps) = reps {
            for rep in reps {
                let Some(contents) = rep.get("contents").and_then(|v| v.as_array()) else {
                    continue;
                };
                for c in contents {
                    let (Some(key), Some(u)) = (
                        c.get("key").and_then(|v| v.as_str()),
                        c.get("url").and_then(|v| v.as_str()),
                    ) else {
                        continue;
                    };
                    let hash = u.rsplit('/').next().unwrap_or(u).to_string();
                    if content_by_file.contains_key(&key.to_lowercase()) {
                        continue;
                    }
                    content_by_file.insert(key.to_lowercase(), hash.clone());
                    content_urls.push((hash.clone(), u.to_string()));
                    content_items.push(ContentItem {
                        file: key.to_string(),
                        hash,
                    });
                }
            }
        }
        if content_items.is_empty() {
            continue;
        }
        if fetch_missing {
            let fetched: usize = content_urls
                .par_iter()
                .map(
                    |(hash, url)| match fetch_url_into_store(&store, url, hash) {
                        Ok(new) => usize::from(new),
                        Err(e) => {
                            eprintln!("fetch-missing: {wid}: {hash}: {e:#}");
                            0
                        }
                    },
                )
                .sum();
            if fetched > 0 {
                eprintln!(
                    "fetch-missing: {wid} ({fetched}/{} content files fetched)",
                    content_urls.len()
                );
            }
        }
        let model_refs = collect_model_referenced_hashes(&store, &content_by_file);
        let (linear_refs, normal_refs) = collect_linear_texture_hashes(&store, &content_by_file);

        let mut bundles: Vec<BundleSpec> = Vec::new();
        for c in &content_items {
            let fl = c.file.to_lowercase();
            let is_glb = fl.ends_with(".glb") || fl.ends_with(".gltf");
            let is_image = IMAGE_EXTS.iter().any(|e| fl.ends_with(e));
            if !is_glb && !is_image {
                continue;
            }
            if !store.exists(&c.hash) {
                continue;
            }
            let bundle_name = format!("{}_{platform}", c.hash);
            if !seen_bundle.insert(bundle_name.clone()) {
                continue;
            }
            let m_deps = if is_glb {
                match store.fetch(&c.hash) {
                    Ok(bytes) => metadata_deps_for_glb(&bytes, &c.file, &content_by_file, platform),
                    Err(_) => Vec::new(),
                }
            } else {
                Vec::new()
            };
            let model_ref = is_image && model_refs.contains(&c.hash);
            let standalone_color_space = if is_image {
                Some(if linear_refs.contains(&c.hash) { 0 } else { 1 })
            } else {
                None
            };
            let standalone_normal = is_image && normal_refs.contains(&c.hash);
            bundles.push(BundleSpec {
                cid: c.hash.clone(),
                bundle_name,
                source_file: Some(c.file.clone()),

                entity_type: None,
                metadata_deps: m_deps,
                model_referenced: model_ref,
                expect_hash: None,
                standalone_color_space,
                standalone_normal,

                force_default_material: false,
            });
        }
        if !bundles.is_empty() {
            entities_out.push(EntityEntry {
                entity_id: if wid.is_empty() {
                    "wearable".into()
                } else {
                    wid
                },
                content: content_items,
                bundles,
            });
        }
    }

    if entities_out.is_empty() {
        return Err(anyhow!(
            "collection {urn}: no buildable bundles (content missing from store, or collection empty)"
        ));
    }
    let n_bundles: usize = entities_out.iter().map(|e| e.bundles.len()).sum();
    eprintln!(
        "collection {urn}: {} wearables, {n_bundles} bundles",
        entities_out.len()
    );
    Ok(Manifest {
        content_dir: content_dir.to_string(),
        entities: entities_out,
    })
}

pub(crate) fn from_reference(
    ref_dir: &Path,
    content_dir: &str,
    platform: &str,
    entities_path: Option<&str>,
    expect_hash_enable: bool,
) -> Result<Manifest> {
    let store = LocalContentStore::new(content_dir);
    let mut ent_type_lookup: HashMap<String, String> = HashMap::new();
    if let Some(ep) = entities_path {
        let raw = std::fs::read(ep).with_context(|| format!("read entities {ep}"))?;
        let doc: serde_json::Value = serde_json::from_slice(&raw)?;
        if let Some(arr) = doc.get("entities").and_then(|v| v.as_array()) {
            for e in arr {
                let id = match e.get("entity_id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let t = e
                    .get("entity_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("scene")
                    .to_string();
                ent_type_lookup.insert(id, t);
            }
        }
    }

    let mut ent_dirs: Vec<PathBuf> = std::fs::read_dir(ref_dir)
        .with_context(|| format!("read_dir {}", ref_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    ent_dirs.sort();

    let bundle_suffix = format!("_{platform}");
    let uri_cache = abgen::glbscan::UriCache::new();

    let per: Vec<Result<Option<EntityEntry>>> = ent_dirs
        .par_iter()
        .map(|ent_dir| -> Result<Option<EntityEntry>> {
            let ent_id = match ent_dir.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => return Ok(None),
            };
            let entity = match load_entity_json(&store, &ent_id) {
                Some(j) => j,
                None => {
                    eprintln!("skip {ent_id}: no entity file");
                    return Ok(None);
                }
            };

            let entity_type = ent_type_lookup
                .get(&ent_id)
                .cloned()
                .or_else(|| {
                    entity
                        .get("type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "scene".to_string());
            let content_items: Vec<ContentItem> = entity
                .get("content")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| {
                            let f = c.get("file").and_then(|v| v.as_str())?.to_string();
                            let h = c.get("hash").and_then(|v| v.as_str())?.to_string();
                            Some(ContentItem { file: f, hash: h })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let content_by_file: HashMap<String, String> = content_items
                .iter()
                .map(|c| (c.file.to_lowercase(), c.hash.clone()))
                .collect();
            let mut inv: HashMap<String, String> = HashMap::new();
            let mut occ: HashMap<String, Vec<String>> = HashMap::new();
            for c in &content_items {
                inv.entry(c.hash.clone()).or_insert_with(|| c.file.clone());
                occ.entry(c.hash.clone()).or_default().push(c.file.clone());
            }
            let scan = abgen::glbscan::scan_entity(&store, &content_by_file, &uri_cache);
            let (model_refs, linear_refs, normal_refs) =
                (&scan.model_refs, &scan.linear_refs, &scan.normal_refs);

            let mut bundle_paths: Vec<PathBuf> = std::fs::read_dir(ent_dir)
                .with_context(|| format!("read_dir {}", ent_dir.display()))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|n| n.ends_with(&bundle_suffix))
                        .unwrap_or(false)
                })
                .collect();
            bundle_paths.sort();

            let mut bundles: Vec<BundleSpec> = Vec::new();
            for bp in bundle_paths {
                let name = match bp.file_name().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let cid = cid_from_bundle_name(&name, platform);
                if !store.exists(&cid) {
                    continue;
                }
                let glb_bytes = store.fetch(&cid).ok();
                let glb_file = occ
                    .get(&cid)
                    .and_then(|cands| {
                        glb_bytes
                            .as_deref()
                            .and_then(|b| best_glb_file(b, cands, &content_by_file))
                    })
                    .or_else(|| inv.get(&cid).cloned())
                    .unwrap_or_else(|| format!("{cid}.glb"));
                let m_deps =
                    scan.metadata_deps(&store, &glb_file, &cid, &content_by_file, platform);
                let glb_file_l = glb_file.to_lowercase();
                let is_image = IMAGE_EXTS.iter().any(|e| glb_file_l.ends_with(e));
                let model_ref = is_image && model_refs.contains(&cid);
                let standalone_color_space = if is_image {
                    Some(if linear_refs.contains(&cid) { 0 } else { 1 })
                } else {
                    None
                };
                let standalone_normal = is_image && normal_refs.contains(&cid);
                let source_file = if (inv.contains_key(&cid)
                    && (glb_file_l.ends_with(".gltf")
                        || glb_file_l.ends_with("_emote.glb")
                        || glb_file_l.ends_with(".glb")))
                    || is_image
                {
                    Some(glb_file.clone())
                } else {
                    None
                };
                let expect_hash = if expect_hash_enable {
                    std::fs::read(&bp).ok().map(|b| hashes::sha256_hex(&b))
                } else {
                    None
                };

                let force_default_material = reference_bundle_has_dcl_scene(&bp);
                bundles.push(BundleSpec {
                    cid,
                    bundle_name: name,
                    source_file,
                    entity_type: if entity_type != "scene" {
                        Some(entity_type.clone())
                    } else {
                        None
                    },
                    metadata_deps: m_deps,
                    model_referenced: model_ref,
                    expect_hash,
                    standalone_color_space,
                    standalone_normal,
                    force_default_material,
                });
            }

            Ok(if bundles.is_empty() {
                None
            } else {
                Some(EntityEntry {
                    entity_id: ent_id,
                    content: content_items,
                    bundles,
                })
            })
        })
        .collect();
    let mut entities_out: Vec<EntityEntry> = Vec::with_capacity(per.len());
    for r in per {
        if let Some(e) = r? {
            entities_out.push(e);
        }
    }

    if entities_out.is_empty() {
        return Err(anyhow!(
            "from-reference: no buildable bundles found under {}",
            ref_dir.display()
        ));
    }

    Ok(Manifest {
        content_dir: content_dir.to_string(),
        entities: entities_out,
    })
}
