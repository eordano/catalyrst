use abgen::builder::{build_bundle, BuildOpts};
use abgen::glbscan::file_ext_lower;
use abgen::hashes;
use abgen::local_store::{LocalContentStore, ABGEN_CONTENT_ROOT_ENV, DEFAULT_CONTENT_ROOT};
use abgen::{naming, Result};
use anyhow::{anyhow, Context};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_LAMBDAS_URL: &str = "http://localhost:5141/lambdas";

const DEFAULT_CDN_AB_VERSION: &str = "v41";
const DEFAULT_CONTENT_SERVER_URL: &str = "https://peer.decentraland.org/content";

#[derive(Deserialize)]
struct Manifest {
    content_dir: String,
    entities: Vec<EntityEntry>,
}

#[derive(Deserialize)]
struct EntityEntry {
    entity_id: String,
    content: Vec<ContentItem>,
    bundles: Vec<BundleSpec>,
}

#[derive(Deserialize, Clone)]
struct ContentItem {
    file: String,
    hash: String,
}

#[derive(Deserialize)]
struct BundleSpec {
    cid: String,
    bundle_name: String,
    #[serde(default)]
    source_file: Option<String>,
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    metadata_deps: Vec<String>,
    #[serde(default)]
    model_referenced: bool,
    #[serde(default)]
    expect_hash: Option<String>,
    #[serde(default)]
    standalone_color_space: Option<i64>,

    #[serde(default)]
    standalone_normal: bool,

    /// Reference-driven: emit the unreferenced DCL_Scene default Material because
    /// the matching reference bundle actually contains it (production glTFast's
    /// first-conversion `s_DefaultMaterial` static-cache quirk). Set by
    /// `--from-reference` after inspecting the reference bundle; never present in
    /// the JSON manifest format.
    #[serde(default)]
    force_default_material: bool,
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  \
         abgen-corpus <manifest.json> <out-dir> [-j JOBS]\n  \
         abgen-corpus --from-reference <ref-dir> <out-dir> \\\n               \
             [--platform windows|mac] [--content-dir <dir>] [--entities <entities.json>] \\\n               \
             [--collection-mode] [--flat] [--expect-hash] [-j JOBS]\n  \
         abgen-corpus --collection-urn <urn> <out-dir> \\\n               \
             [--lambdas-url <url>] [--platform windows|mac] [--content-dir <dir>] [-j JOBS]\n\
         \n\
         --collection-urn resolves a wearables collection URN via a catalyst\n  \
         lambdas server (default http://localhost:5141/lambdas, a local catalyst) and builds every glb +\n  \
         image in the collection as a flat <out>/<content-hash>_<platform>,\n  \
         matching the converter's ConvertWearablesCollection output (implies\n  \
         --collection-mode + --flat).\n\
         \n\
         manifest format (mode 1):\n  \
         {{ \"content_dir\": \"/path\",\n    \
           \"entities\": [{{ \"entity_id\": \"<cid>\",\n                    \
                          \"content\": [{{\"file\":\"foo.glb\",\"hash\":\"<cid>\"}}, ...],\n                    \
                          \"bundles\": [{{\"cid\":\"<cid>\",\"bundle_name\":\"<cid>_windows\",\n                                       \
                                       \"source_file\":\"foo.glb\",\"entity_type\":\"scene\",\n                                       \
                                       \"metadata_deps\":[...],\"model_referenced\":false}}] }}] }}\n\
         \n\
         output layout (default):  <out-dir>/<entity_id>/<bundle_name>\n  \
         output layout (--flat):   <out-dir>/<bundle_name>\n  \
         output layout (--cdn-layout, AB-CDN serving shape):\n               \
             <out-dir>/<entity_id>/<platform>/<hash>_<platform>   (bundle binaries)\n               \
             <out-dir>/<entity_id>/<platform>.manifest.json        (per-entity manifest)\n\
         \n\
         --cdn-layout writes the on-disk shape an ab-cdn server serves: each\n  \
         entity gets a per-platform manifest JSON (version/files/exitCode/\n  \
         contentServerUrl/date, files = its bundles + \"dcl\") plus its bundle\n  \
         binaries nested under <entity>/<platform>/. Shared binaries are\n  \
         hardlinked across entities. Tune with --ab-version v<int> (default\n  \
         v41) and --content-server-url <url>. Only valid with --entity-ids.\n\
         \n\
         --real-textures: oversized standalone textures are downscaled and\n  \
         BC7-encoded for real (production-like) instead of the fork-faithful\n  \
         mean-color stub. Correct/leaner for serving; diverges from fork\n  \
         byte-parity (val300 windows 2652 -> ~2075). Default OFF (stub).\n\
         \n\
         --v38-compat: cluster node primitives into Unity Meshes the way\n  \
         glTFast does in production v38 (per-glTF-mesh accessor-keyed\n  \
         clusters, skinned/morph prims included, one submesh per primitive,\n  \
         shared vertex buffer), and emit metadata.json the v38 way (always\n  \
         present, Qm bundles included; version 7.0; real .NET-ticks build\n  \
         timestamp, override via ABGEN_V38_TIMESTAMP; lowercased deps plus\n  \
         the dcl/scene_ignore_<target> shader entry when the bundle has\n  \
         materials), and emit the unreferenced DCL_Scene default Material\n  \
         (+ DCL_Scene.mat container entry) in every glb bundle the v38 way\n  \
         (always, even for zero-material glbs; never bound by renderers;\n  \
         texture bundles unaffected). Matches prod bundles; may diverge\n  \
         from fork byte-parity. Default OFF.\n\
         \n\
         --skip-existing: skip any bundle whose output file already exists and is\n  \
         non-empty (incremental top-off). --force: rebuild even if it exists. The\n  \
         default rebuilds/overwrites every bundle (golden/determinism workflows\n  \
         rely on that)."
    );
    std::process::exit(2);
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut jobs: usize = num_cpus::get();
    let mut from_ref: Option<String> = None;
    let mut platform: String = "windows".to_string();
    let mut content_dir: Option<String> = None;
    let mut entities_path: Option<String> = None;
    let mut expect_hash_enable = false;
    let mut collection_urn: Option<String> = None;
    let mut entity_ids_path: Option<String> = None;
    let mut lambdas_url = DEFAULT_LAMBDAS_URL.to_string();
    let mut collection_mode = false;
    let mut flat_output = false;
    let mut cdn_layout = false;
    let mut ab_version = DEFAULT_CDN_AB_VERSION.to_string();
    let mut content_server_url = DEFAULT_CONTENT_SERVER_URL.to_string();
    let mut skip_existing = false;
    let mut force = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-j" | "--jobs" => {
                i += 1;
                jobs = argv
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--from-reference" => {
                i += 1;
                from_ref = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--collection-urn" => {
                i += 1;
                collection_urn = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--lambdas-url" => {
                i += 1;
                lambdas_url = argv.get(i).cloned().unwrap_or_else(|| usage());
            }
            "--collection-mode" => {
                collection_mode = true;
            }
            "--real-textures" => {
                std::env::set_var(BuildOpts::REAL_TEXTURES_ENV, "1");
            }
            "--v38-compat" => {
                std::env::set_var(BuildOpts::V38_COMPAT_ENV, "1");
            }
            "--flat" => {
                flat_output = true;
            }
            "--cdn-layout" => {
                cdn_layout = true;
            }
            "--ab-version" => {
                i += 1;
                ab_version = argv.get(i).cloned().unwrap_or_else(|| usage());
            }
            "--content-server-url" => {
                i += 1;
                content_server_url = argv.get(i).cloned().unwrap_or_else(|| usage());
            }
            "--platform" => {
                i += 1;
                platform = argv.get(i).cloned().unwrap_or_else(|| usage());
            }
            "--content-dir" => {
                i += 1;
                content_dir = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--entities" => {
                i += 1;
                entities_path = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--entity-ids" => {
                i += 1;
                entity_ids_path = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--expect-hash" => {
                expect_hash_enable = true;
            }
            "--skip-existing" => {
                skip_existing = true;
            }
            "--force" => {
                force = true;
            }
            "-h" | "--help" => usage(),
            other if other.starts_with("--") => {
                eprintln!("unknown option: {other}");
                usage();
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if collection_urn.is_some() {
        collection_mode = true;
        flat_output = true;
    }
    if cdn_layout && flat_output {
        eprintln!("error: --cdn-layout is incompatible with --flat / --collection-urn");
        usage();
    }
    if cdn_layout && entity_ids_path.is_none() {
        eprintln!("error: --cdn-layout currently requires --entity-ids");
        usage();
    }
    if cdn_layout {
        let valid_version = ab_version.len() >= 2
            && ab_version.as_bytes().first() == Some(&b'v')
            && ab_version[1..].parse::<i64>().is_ok();
        if !valid_version {
            eprintln!(
                "error: --ab-version must be 'v<int>' (the explorer parses int.Parse(version[1..])); got {ab_version:?}"
            );
            usage();
        }
    }
    if collection_mode {
        std::env::set_var(BuildOpts::COLLECTION_MODE_ENV, "1");
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let (manifest, out_root) = if let Some(ids_path) = entity_ids_path {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let m = from_entity_ids(&ids_path, &cdir, &platform, cdn_layout)?;
        (m, out_root)
    } else if let Some(urn) = collection_urn {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let m = from_collection_urn(&urn, &lambdas_url, &cdir, &platform)?;
        (m, out_root)
    } else if let Some(ref_dir) = from_ref {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let m = from_reference(
            Path::new(&ref_dir),
            &cdir,
            &platform,
            entities_path.as_deref(),
            expect_hash_enable,
        )?;
        (m, out_root)
    } else {
        if positional.len() != 2 {
            usage();
        }
        let manifest_path = &positional[0];
        let out_root = PathBuf::from(&positional[1]);
        let m: Manifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
        (m, out_root)
    };

    let store = LocalContentStore::new(&manifest.content_dir);
    std::fs::create_dir_all(&out_root)?;

    let total: usize = manifest.entities.iter().map(|e| e.bundles.len()).sum();
    let built = AtomicUsize::new(0);
    let errs = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);

    let first_written: Mutex<HashMap<String, PathBuf>> = Mutex::new(HashMap::new());

    manifest
        .entities
        .par_iter()
        .flat_map(|ent| ent.bundles.par_iter().map(move |b| (ent, b)))
        .for_each(|(ent, spec)| {
            let content_by_file: HashMap<String, String> = ent
                .content
                .iter()
                .map(|c| (c.file.to_lowercase(), c.hash.clone()))
                .collect();
            let ent_out = if flat_output {
                out_root.clone()
            } else if cdn_layout {
                out_root.join(&ent.entity_id).join(&platform)
            } else {
                out_root.join(&ent.entity_id)
            };
            if let Err(e) = std::fs::create_dir_all(&ent_out) {
                eprintln!("mkdir {}: {e}", ent_out.display());
                errs.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let out_path = ent_out.join(&spec.bundle_name);

            // Incremental top-off: skip a bundle whose output already exists and is
            // non-empty, unless --force. (Default rebuilds/overwrites — test and
            // golden/determinism workflows depend on that.)
            if skip_existing && !force {
                if let Ok(m) = std::fs::metadata(&out_path) {
                    if m.is_file() && m.len() > 0 {
                        skipped.fetch_add(1, Ordering::Relaxed);
                        if cdn_layout {
                            first_written
                                .lock()
                                .unwrap()
                                .entry(spec.bundle_name.clone())
                                .or_insert_with(|| out_path.clone());
                        }
                        return;
                    }
                }
            }

            if cdn_layout {
                let prior = first_written
                    .lock()
                    .unwrap()
                    .get(&spec.bundle_name)
                    .cloned();
                if let Some(src) = prior {
                    let linked = std::fs::hard_link(&src, &out_path).is_ok()
                        || std::fs::copy(&src, &out_path).is_ok();
                    if linked {
                        built.fetch_add(1, Ordering::Relaxed);
                    } else {
                        errs.fetch_add(1, Ordering::Relaxed);
                        eprintln!("link {}/{}: failed", ent.entity_id, spec.bundle_name);
                    }
                    return;
                }
            }

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                build_one(&store, &content_by_file, spec, &out_path)
            }));
            match result {
                Ok(Ok(_)) => {
                    if cdn_layout {
                        first_written
                            .lock()
                            .unwrap()
                            .entry(spec.bundle_name.clone())
                            .or_insert_with(|| out_path.clone());
                    }
                    let n = built.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(100) {
                        eprintln!("  {n}/{total}");
                    }
                }
                Ok(Err(e)) => {
                    errs.fetch_add(1, Ordering::Relaxed);
                    eprintln!("err {}/{}: {e}", ent.entity_id, spec.bundle_name);
                }
                Err(_) => {
                    errs.fetch_add(1, Ordering::Relaxed);
                    eprintln!("panic {}/{} (skipped)", ent.entity_id, spec.bundle_name);
                }
            }
        });

    let mut manifest_errs = 0usize;
    let mut manifest_incomplete = 0usize;
    if cdn_layout {
        for ent in &manifest.entities {
            match write_cdn_manifest(
                &out_root,
                &ent.entity_id,
                &platform,
                &ent.bundles,
                &ab_version,
                &content_server_url,
            ) {
                Err(e) => {
                    manifest_errs += 1;
                    eprintln!("manifest {}: {e}", ent.entity_id);
                }
                Ok(0) => {}
                Ok(n) => {
                    manifest_incomplete += 1;
                    eprintln!("manifest {}: {n} failed bundle(s) omitted", ent.entity_id);
                }
            }
        }
        eprintln!(
            "cdn-layout: wrote {} per-entity manifests ({} errors, {} incomplete)",
            manifest.entities.len(),
            manifest_errs,
            manifest_incomplete
        );
    }

    let n_built = built.load(Ordering::Relaxed);
    let n_errs = errs.load(Ordering::Relaxed) + manifest_errs;
    let n_skipped = skipped.load(Ordering::Relaxed);
    println!("DONE built={n_built} skipped={n_skipped} errs={n_errs} total={total}");
    if n_errs > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn build_one(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    spec: &BundleSpec,
    out_path: &std::path::Path,
) -> Result<()> {
    let glb = store.fetch_mmap(&spec.cid)?;
    let effective_source = spec
        .source_file
        .clone()
        .unwrap_or_else(|| format!("{}.glb", spec.cid));
    let sf_for_bytes = effective_source.clone();
    let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
        let key = naming::resolve_uri_to_content_file(uri, &sf_for_bytes)
            .ok()?
            .to_lowercase();
        let h = content_by_file.get(&key)?;
        store.fetch(h).ok()
    };
    let resolve: abgen::gltf::Resolve = if !content_by_file.is_empty() {
        Some(&resolve_fn)
    } else {
        None
    };
    let sf_for_hash = effective_source.clone();
    let resolve_hash_fn = |uri: &str| -> Option<String> {
        let key = naming::resolve_uri_to_content_file(uri, &sf_for_hash)
            .ok()?
            .to_lowercase();
        content_by_file.get(&key).cloned()
    };
    let resolve_hash: Option<abgen::builder::ResolveHash> =
        if !content_by_file.is_empty() && spec.source_file.is_some() {
            Some(&resolve_hash_fn)
        } else {
            None
        };
    let opts = BuildOpts {
        keep_forward_plus: true,
        source_file: Some(&effective_source),
        entity_type: spec.entity_type.as_deref(),
        resolve,
        resolve_hash,
        model_referenced: spec.model_referenced,
        metadata_dependencies: &spec.metadata_deps,
        expect_hash: spec.expect_hash.as_deref(),
        standalone_color_space: spec.standalone_color_space,
        standalone_normal: spec.standalone_normal,
        force_default_material: spec.force_default_material,
        magenta_missing: false,
    };
    let artifact = build_bundle(&glb[..], &spec.bundle_name, &spec.cid, &opts)?;
    std::fs::write(out_path, &artifact.data)?;
    Ok(())
}

fn write_cdn_manifest(
    out_root: &Path,
    entity_id: &str,
    platform: &str,
    bundles: &[BundleSpec],
    ab_version: &str,
    content_server_url: &str,
) -> Result<usize> {
    let mut names: Vec<String> = bundles.iter().map(|b| b.bundle_name.clone()).collect();
    names.sort();
    names.dedup();
    let bundle_dir = out_root.join(entity_id).join(platform);
    let (built, missing): (Vec<String>, Vec<String>) = names
        .into_iter()
        .partition(|n| bundle_dir.join(n).is_file());
    if built.is_empty() && !missing.is_empty() {
        return Ok(missing.len());
    }
    // Single emitter shared with the in-process JIT converter (live::Proxy) so a
    // batch-built and a live-built manifest are byte-identical.
    abgen::manifest::write_corpus_manifest(
        out_root,
        entity_id,
        platform,
        &built,
        ab_version,
        content_server_url,
    )?;
    Ok(missing.len())
}

#[allow(dead_code)]
fn iso8601_utc_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let days = (total_secs / 86_400) as i64;
    let secs_of_day = (total_secs % 86_400) as i64;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

const IMAGE_EXTS: [&str; 3] = [".png", ".jpg", ".jpeg"];

fn load_entity_json(store: &LocalContentStore, cid: &str) -> Option<serde_json::Value> {
    let bytes = store.fetch(cid).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Returns true if the reference AssetBundle at `path` contains a Material
/// (class 21) named "DCL_Scene" — the unreferenced default Material that
/// production glTFast injects for the first glb-bearing conversion of an editor
/// session. Used by `--from-reference` to mirror that presence per bundle.
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
            abgen::gltf::parse(&data, ext, resolve, false)
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

fn from_entity_ids(
    ids_path: &str,
    content_dir: &str,
    platform: &str,
    cdn_layout: bool,
) -> Result<Manifest> {
    let store = LocalContentStore::new(content_dir);
    let raw = std::fs::read_to_string(ids_path).with_context(|| format!("read {ids_path}"))?;
    let ids: Vec<String> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();

    let missing_entity = AtomicUsize::new(0);
    let missing_content = AtomicUsize::new(0);

    let t0 = Instant::now();
    let processed = AtomicUsize::new(0);
    let n_glb = AtomicUsize::new(0);
    let load_ns = AtomicU64::new(0);
    let scan_ns = AtomicU64::new(0);
    let metadeps_ns = AtomicU64::new(0);
    let n_total = ids.len();

    let uri_cache = abgen::glbscan::UriCache::new();
    eprintln!("manifest: deriving from {n_total} entity ids (parallel)…");

    let per: Vec<EntityEntry> = ids
        .par_iter()
        .filter_map(|ent_id| {
            let t_load = Instant::now();
            let loaded = load_entity_json(&store, ent_id);
            load_ns.fetch_add(t_load.elapsed().as_nanos() as u64, Ordering::Relaxed);
            let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if done.is_multiple_of(5000) {
                let secs = t0.elapsed().as_secs_f64().max(0.001);
                eprintln!(
                    "  manifest: {done}/{n_total} entities ({:.0}/s, {:.0}s) | glbs={} \
                     load={:.0}s scan={:.0}s metadeps={:.0}s (summed)",
                    done as f64 / secs,
                    secs,
                    n_glb.load(Ordering::Relaxed),
                    load_ns.load(Ordering::Relaxed) as f64 / 1e9,
                    scan_ns.load(Ordering::Relaxed) as f64 / 1e9,
                    metadeps_ns.load(Ordering::Relaxed) as f64 / 1e9,
                );
            }
            let entity = match loaded {
                Some(j) => j,
                None => {
                    missing_entity.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            };
            let entity_type = entity
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
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
            if content_items.is_empty() {
                return None;
            }
            let content_by_file: HashMap<String, String> = content_items
                .iter()
                .map(|c| (c.file.to_lowercase(), c.hash.clone()))
                .collect();
            let t_scan = Instant::now();
            let scan = abgen::glbscan::scan_entity(&store, &content_by_file, &uri_cache);
            scan_ns.fetch_add(t_scan.elapsed().as_nanos() as u64, Ordering::Relaxed);
            let (model_refs, linear_refs, normal_refs) =
                (&scan.model_refs, &scan.linear_refs, &scan.normal_refs);

            let mut bundles: Vec<BundleSpec> = Vec::new();
            let mut local_seen: HashSet<String> = HashSet::new();
            for c in &content_items {
                let fl = c.file.to_lowercase();
                let is_glb = fl.ends_with(".glb") || fl.ends_with(".gltf");
                let is_image = IMAGE_EXTS.iter().any(|e| fl.ends_with(e));
                if !is_glb && !is_image {
                    continue;
                }
                if !store.exists(&c.hash) {
                    missing_content.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                let bundle_name = format!("{}_{platform}", c.hash);
                if !local_seen.insert(bundle_name.clone()) {
                    continue;
                }
                let m_deps = if is_glb {
                    n_glb.fetch_add(1, Ordering::Relaxed);
                    let t_md = Instant::now();
                    let r =
                        scan.metadata_deps(&store, &c.file, &c.hash, &content_by_file, platform);
                    metadeps_ns.fetch_add(t_md.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    r
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
                    entity_type: entity_type.clone(),
                    metadata_deps: m_deps,
                    model_referenced: model_ref,
                    expect_hash: None,
                    standalone_color_space,
                    standalone_normal,
                    // collection mode already forces DCL_Scene via the env gate.
                    force_default_material: false,
                });
            }
            if bundles.is_empty() {
                return None;
            }
            Some(EntityEntry {
                entity_id: ent_id.clone(),
                content: content_items,
                bundles,
            })
        })
        .collect();

    let entities_out: Vec<EntityEntry> = if cdn_layout {
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

    let missing_entity = missing_entity.into_inner();
    let missing_content = missing_content.into_inner();
    let n_bundles: usize = entities_out.iter().map(|e| e.bundles.len()).sum();
    eprintln!(
        "manifest: derived in {:.1}s wall | glbs parsed={} | summed-thread time: \
         load={:.0}s scan={:.0}s metadeps={:.0}s",
        t0.elapsed().as_secs_f64(),
        n_glb.into_inner(),
        load_ns.into_inner() as f64 / 1e9,
        scan_ns.into_inner() as f64 / 1e9,
        metadeps_ns.into_inner() as f64 / 1e9,
    );
    eprintln!(
        "entity-ids: {} requested, {} buildable entities, {n_bundles} bundles \
         (missing entity json: {missing_entity}, missing content: {missing_content})",
        ids.len(),
        entities_out.len()
    );
    if entities_out.is_empty() {
        return Err(anyhow!("no buildable bundles from {ids_path}"));
    }
    Ok(Manifest {
        content_dir: content_dir.to_string(),
        entities: entities_out,
    })
}

fn from_collection_urn(
    urn: &str,
    lambdas_url: &str,
    content_dir: &str,
    platform: &str,
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
                // collection mode already forces DCL_Scene via the env gate.
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

fn from_reference(
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
                // Reference-driven DCL_Scene mirroring: production glTFast emits the
                // unreferenced DCL_Scene default Material only for the first
                // glb-bearing conversion of an editor session (its s_DefaultMaterial
                // static cache materializes once and the asset-bundle-converter
                // captures it into every bundle of that conversion). Whether a given
                // reference bundle has it is session/order state, not a per-bundle
                // property abgen could recompute from the glb. But in
                // --from-reference mode the matching reference bundle bytes are in
                // hand, so we mirror its presence exactly: emit DCL_Scene iff the
                // reference bundle contains a DCL_Scene Material. Texture bundles
                // never have it; this is a no-op for them.
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
