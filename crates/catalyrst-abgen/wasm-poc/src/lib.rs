use std::collections::HashMap;

use abgen::builder::{build_bundle, BuildOpts};
use abgen::hashes::sha256_hex;
use abgen::naming;
use abgen::validate::{validate_bundle, Severity, ValidateCtx};

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_emit(kind: u32, ptr: *const u8, len: usize);
}

fn emit(kind: u32, bytes: &[u8]) {
    unsafe { host_emit(kind, bytes.as_ptr(), bytes.len()) }
}

fn emit_json(v: serde_json::Value) {
    let s = v.to_string();
    emit(0, s.as_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn poc_alloc(len: usize) -> *mut u8 {
    let layout = std::alloc::Layout::array::<u8>(len.max(1)).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn poc_free(ptr: *mut u8, len: usize) {
    let layout = std::alloc::Layout::array::<u8>(len.max(1)).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn poc_init() {
    unsafe extern "C" {
        fn __wasm_call_ctors();
    }
    unsafe { __wasm_call_ctors() };
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("panic: {info}");
        emit(2, msg.as_bytes());
    }));
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global();
}

struct Input {
    files: Vec<(String, Vec<u8>)>,
    platform: String,
    entity_type: String,
    magenta: bool,
    lod: bool,
    mode: u8,
    crop: bool,
    tri_cap: u32,
    entity_hash: Option<String>,
    only_glb: Option<String>,
    content_table: Option<Vec<(String, String)>>,
}

fn read_u32(buf: &[u8], off: &mut usize) -> Option<u32> {
    let b = buf.get(*off..*off + 4)?;
    *off += 4;
    Some(u32::from_le_bytes(b.try_into().ok()?))
}

fn read_chunk<'a>(buf: &'a [u8], off: &mut usize) -> Option<&'a [u8]> {
    let len = read_u32(buf, off)? as usize;
    let b = buf.get(*off..*off + len)?;
    *off += len;
    Some(b)
}

fn read_str(buf: &[u8], off: &mut usize) -> Option<String> {
    String::from_utf8(read_chunk(buf, off)?.to_vec()).ok()
}

fn parse_input(buf: &[u8]) -> Option<Input> {
    let mut off = 0usize;
    let n = read_u32(buf, &mut off)? as usize;
    let mut files = Vec::with_capacity(n);
    for _ in 0..n {
        let name = read_str(buf, &mut off)?;
        let data = read_chunk(buf, &mut off)?.to_vec();
        files.push((name, data));
    }
    let platform = read_str(buf, &mut off)?;
    let entity_type = read_str(buf, &mut off)?;
    let magenta = *buf.get(off)? != 0;
    let lod = buf.get(off + 1).copied().unwrap_or(0) != 0;
    let mode = buf.get(off + 2).copied().unwrap_or(0);
    // v2 tail: [mode][crop][tri_cap u32] then hash/only/table chunks; every
    // field defaults so a v1 blob parses identically.
    let crop = buf.get(off + 3).copied().unwrap_or(0) != 0;
    off = (off + 4).min(buf.len());
    let tri_cap = read_u32(buf, &mut off).unwrap_or(0);
    let entity_hash = read_str(buf, &mut off).filter(|s| !s.is_empty());
    let only_glb = read_str(buf, &mut off).filter(|s| !s.is_empty());
    let content_table = read_u32(buf, &mut off)
        .and_then(|n| {
            let mut t = Vec::with_capacity(n as usize);
            for _ in 0..n {
                t.push((read_str(buf, &mut off)?, read_str(buf, &mut off)?));
            }
            Some(t)
        })
        .filter(|t| !t.is_empty());
    Some(Input {
        files,
        platform,
        entity_type,
        magenta,
        lod,
        mode,
        crop,
        tri_cap,
        entity_hash,
        only_glb,
        content_table,
    })
}

fn emit_output(name: &str, data: &[u8]) {
    let mut blob = Vec::with_capacity(8 + name.len() + data.len());
    blob.extend_from_slice(&(name.len() as u32).to_le_bytes());
    blob.extend_from_slice(name.as_bytes());
    blob.extend_from_slice(&(data.len() as u32).to_le_bytes());
    blob.extend_from_slice(data);
    emit(1, &blob);
}

fn ext_of(name: &str) -> String {
    match name.rsplit('.').next() {
        Some(e) if e.len() < name.len() => format!(".{}", e.to_lowercase()),
        _ => String::new(),
    }
}

fn detect_entity_type(files: &[(String, Vec<u8>)]) -> &'static str {
    if files.iter().any(|(n, _)| {
        n.eq_ignore_ascii_case("scene.json") || n.to_lowercase().ends_with("/scene.json")
    }) {
        return "scene";
    }
    if files
        .iter()
        .any(|(n, _)| n.to_lowercase().ends_with("_emote.glb"))
    {
        return "emote";
    }
    "wearable"
}

#[unsafe(no_mangle)]
pub extern "C" fn poc_convert(ptr: *const u8, len: usize) -> i32 {
    let buf = unsafe { std::slice::from_raw_parts(ptr, len) };
    let input = match parse_input(buf) {
        Some(i) => i,
        None => {
            emit(2, b"malformed input blob");
            return 1;
        }
    };
    let r = match input.mode {
        1 => scan(input),
        2 => convert_only(input),
        3 => lod_only(input),
        _ => convert(input),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            let msg = format!("{e:#}");
            emit(2, msg.as_bytes());
            1
        }
    }
}

fn target_of(platform: &str) -> &str {
    match platform {
        "windows" | "mac" | "webgl" => platform,
        other => {
            emit_json(serde_json::json!({
                "ev": "note",
                "msg": format!("unknown platform {other:?}, using windows"),
            }));
            "windows"
        }
    }
}

fn entity_type_of(input: &Input) -> String {
    if input.entity_type.is_empty() {
        detect_entity_type(&input.files).to_string()
    } else {
        input.entity_type.clone()
    }
}

fn content_maps(
    files: &[(String, Vec<u8>)],
) -> (HashMap<String, String>, HashMap<String, &Vec<u8>>) {
    let mut content_by_file: HashMap<String, String> = HashMap::new();
    let mut bytes_by_hash: HashMap<String, &Vec<u8>> = HashMap::new();
    for (name, data) in files {
        let hash = sha256_hex(data);
        content_by_file.insert(name.to_lowercase(), hash.clone());
        bytes_by_hash.insert(hash, data);
    }
    (content_by_file, bytes_by_hash)
}

fn entity_hash_of(content_by_file: &HashMap<String, String>) -> String {
    let mut ids: Vec<String> = content_by_file
        .iter()
        .map(|(f, h)| format!("{f}:{h}"))
        .collect();
    ids.sort();
    sha256_hex(ids.join("\n").as_bytes())
}

fn glbs_of(files: &[(String, Vec<u8>)]) -> Vec<&(String, Vec<u8>)> {
    files
        .iter()
        .filter(|(n, _)| naming::GLTF_EXTENSIONS.contains(&ext_of(n).as_str()))
        .collect()
}

struct EntityCtx<'a> {
    target: &'a str,
    entity_type: &'a str,
    entity_hash: &'a str,
    magenta: bool,
    content_by_file: &'a HashMap<String, String>,
    bytes_by_hash: &'a HashMap<String, &'a Vec<u8>>,
}

fn convert_one(ctx: &EntityCtx, name: &str, data: &[u8], emit_plan: bool) -> Option<String> {
    let ext = ext_of(name);
    let hash = match ctx.content_by_file.get(&name.to_lowercase()) {
        Some(h) => h.clone(),
        None => {
            emit_json(serde_json::json!({
                "ev": "file-error",
                "file": name,
                "error": "file missing from the content table",
            }));
            return None;
        }
    };

    if emit_plan {
        if let Ok(scene) = abgen::gltf::parse_classify(data, &ext, None) {
            emit_json(serde_json::json!({
                "ev": "plan",
                "file": name,
                "nodes": scene.nodes.len(),
                "materials": scene.materials.len(),
                "images": scene.images.len(),
                "skins": scene.skins.len(),
            }));
        }
    }

    let digest = match naming::deps_digest_for_glb(data, name, ctx.content_by_file, ctx.magenta) {
        Ok(d) => d,
        Err(e) => {
            emit_json(serde_json::json!({
                "ev": "file-error",
                "file": name,
                "error": format!("dependency resolution: {e:#}"),
            }));
            return None;
        }
    };
    let bundle_name = match naming::canonical_filename(&hash, &ext, ctx.target, Some(&digest)) {
        Ok(n) => n,
        Err(e) => {
            emit_json(serde_json::json!({
                "ev": "file-error",
                "file": name,
                "error": format!("{e:#}"),
            }));
            return None;
        }
    };

    emit_json(serde_json::json!({
        "ev": "file-start",
        "file": name,
        "bytes": data.len(),
        "bundle": bundle_name,
    }));

    let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
        let key = naming::resolve_uri_to_content_file(uri, name).ok()?;
        let h = ctx.content_by_file.get(&key.to_lowercase())?;
        ctx.bytes_by_hash.get(h).map(|b| (*b).clone())
    };
    let resolve_hash_fn = |uri: &str| -> Option<String> {
        let key = naming::resolve_uri_to_content_file(uri, name).ok()?;
        ctx.content_by_file.get(&key.to_lowercase()).cloned()
    };

    let opts = BuildOpts {
        source_file: Some(name),
        entity_type: Some(ctx.entity_type),
        resolve: Some(&resolve_fn),
        resolve_hash: Some(&resolve_hash_fn),
        magenta_missing: ctx.magenta,
        real_textures: true,
        ..Default::default()
    };

    match build_bundle(data, &bundle_name, ctx.entity_hash, &opts) {
        Ok(artifact) => {
            let findings =
                validate_bundle(&artifact.data, &bundle_name, &ValidateCtx::single_file());
            let fjson: Vec<serde_json::Value> = findings
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "severity": match f.severity { Severity::Error => "error", Severity::Warn => "warn" },
                        "code": f.code,
                        "msg": f.msg,
                    })
                })
                .collect();
            emit_json(serde_json::json!({
                "ev": "validate",
                "bundle": bundle_name,
                "findings": fjson,
            }));
            emit_output(&bundle_name, &artifact.data);
            emit_json(serde_json::json!({
                "ev": "file-done",
                "file": name,
                "bundle": bundle_name,
                "bytes": artifact.data.len(),
            }));
            Some(bundle_name)
        }
        Err(e) => {
            emit_json(serde_json::json!({
                "ev": "file-error",
                "file": name,
                "error": format!("{e:#}"),
            }));
            None
        }
    }
}

fn convert(input: Input) -> abgen::Result<()> {
    let target = target_of(&input.platform);
    let entity_type = entity_type_of(&input);
    let (content_by_file, bytes_by_hash) = content_maps(&input.files);
    let entity_hash = entity_hash_of(&content_by_file);
    let glbs = glbs_of(&input.files);

    emit_json(serde_json::json!({
        "ev": "entity",
        "entityType": entity_type,
        "entityHash": entity_hash,
        "platform": target,
        "files": input.files.len(),
        "models": glbs.len(),
    }));

    if glbs.is_empty() {
        emit(2, b"no .glb/.gltf files in the upload");
        return Ok(());
    }

    let ctx = EntityCtx {
        target,
        entity_type: &entity_type,
        entity_hash: &entity_hash,
        magenta: input.magenta,
        content_by_file: &content_by_file,
        bytes_by_hash: &bytes_by_hash,
    };

    let mut built: Vec<String> = Vec::new();
    let mut failures = 0usize;

    for (name, data) in glbs {
        match convert_one(&ctx, name, data, true) {
            Some(bundle) => built.push(bundle),
            None => failures += 1,
        }
    }

    if input.lod {
        if target == "webgl" {
            emit_json(serde_json::json!({
                "ev": "note",
                "msg": "LOD skipped: webgl has no LOD lane (windows/mac/linux only)",
            }));
        } else {
            let (base, parcels) = scene_parcels(&input.files);
            let job = LodJob {
                sid: &entity_hash,
                target,
                entity_type: &entity_type,
                files: &input.files,
                content_by_file: &content_by_file,
                bytes_by_hash: &bytes_by_hash,
                base,
                parcels: &parcels,
                crop: input.crop,
                tri_cap: input.tri_cap,
            };
            if let Err(e) = bake_lod(&job) {
                emit_json(serde_json::json!({
                    "ev": "file-error",
                    "file": "LOD",
                    "error": format!("{e:#}"),
                }));
            }
        }
    }

    built.sort();
    built.dedup();
    let mut files_field = built.clone();
    files_field.push("dcl".to_string());
    let manifest = serde_json::json!({
        "version": "v-wasm-poc",
        "files": files_field,
        "exitCode": if failures == 0 { 0 } else { 12 },
        "contentServerUrl": "wasm://in-browser",
    });
    emit(3, manifest.to_string().as_bytes());
    Ok(())
}

fn scan(input: Input) -> abgen::Result<()> {
    let target = target_of(&input.platform);
    let entity_type = entity_type_of(&input);
    let (content_by_file, _) = content_maps(&input.files);
    let entity_hash = entity_hash_of(&content_by_file);
    let glbs = glbs_of(&input.files);

    emit_json(serde_json::json!({
        "ev": "entity",
        "entityType": entity_type,
        "entityHash": entity_hash,
        "platform": target,
        "files": input.files.len(),
        "models": glbs.len(),
    }));

    if glbs.is_empty() {
        emit(2, b"no .glb/.gltf files in the upload");
        return Ok(());
    }

    let orig_by_lower: HashMap<String, &str> = input
        .files
        .iter()
        .map(|(n, _)| (n.to_lowercase(), n.as_str()))
        .collect();

    for (name, data) in glbs {
        let ext = ext_of(name);
        if let Ok(scene) = abgen::gltf::parse_classify(data, &ext, None) {
            emit_json(serde_json::json!({
                "ev": "plan",
                "file": name,
                "nodes": scene.nodes.len(),
                "materials": scene.materials.len(),
                "images": scene.images.len(),
                "skins": scene.skins.len(),
            }));
        }
        // parse_gltf_dep_refs is the same uri source deps_digest_for_glb and the
        // build-time resolvers read, so this dep list is exactly the byte surface
        // a per-file convert job needs shipped alongside the glb.
        let mut deps: Vec<String> = naming::parse_gltf_dep_refs(data, &ext)
            .map(|uris| {
                uris.iter()
                    .filter_map(|u| naming::resolve_uri_to_content_file(u, name).ok())
                    .filter_map(|k| orig_by_lower.get(&k.to_lowercase()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        deps.sort();
        deps.dedup();
        emit_json(serde_json::json!({
            "ev": "deps",
            "file": name,
            "deps": deps,
        }));
    }
    Ok(())
}

fn convert_only(input: Input) -> abgen::Result<()> {
    let target = target_of(&input.platform);
    let entity_type = entity_type_of(&input);
    let (own_by_file, bytes_by_hash) = content_maps(&input.files);
    let content_by_file: HashMap<String, String> = match &input.content_table {
        Some(t) => t
            .iter()
            .map(|(n, h)| (n.to_lowercase(), h.clone()))
            .collect(),
        None => own_by_file,
    };
    let entity_hash = input
        .entity_hash
        .clone()
        .unwrap_or_else(|| entity_hash_of(&content_by_file));
    let only = input.only_glb.clone().unwrap_or_default();

    let ctx = EntityCtx {
        target,
        entity_type: &entity_type,
        entity_hash: &entity_hash,
        magenta: input.magenta,
        content_by_file: &content_by_file,
        bytes_by_hash: &bytes_by_hash,
    };
    match input.files.iter().find(|(n, _)| *n == only) {
        Some((name, data)) => {
            convert_one(&ctx, name, data, false);
        }
        None => emit_json(serde_json::json!({
            "ev": "file-error",
            "file": only,
            "error": "only_glb not present in the job files",
        })),
    }
    Ok(())
}

fn lod_only(input: Input) -> abgen::Result<()> {
    let target = target_of(&input.platform);
    if target == "webgl" {
        emit_json(serde_json::json!({
            "ev": "note",
            "msg": "LOD skipped: webgl has no LOD lane (windows/mac/linux only)",
        }));
        return Ok(());
    }
    let entity_type = entity_type_of(&input);
    let (content_by_file, bytes_by_hash) = content_maps(&input.files);
    let entity_hash = input
        .entity_hash
        .clone()
        .unwrap_or_else(|| entity_hash_of(&content_by_file));
    let (base, parcels) = scene_parcels(&input.files);
    let job = LodJob {
        sid: &entity_hash,
        target,
        entity_type: &entity_type,
        files: &input.files,
        content_by_file: &content_by_file,
        bytes_by_hash: &bytes_by_hash,
        base,
        parcels: &parcels,
        crop: input.crop,
        tri_cap: input.tri_cap,
    };
    if let Err(e) = bake_lod(&job) {
        emit_json(serde_json::json!({
            "ev": "file-error",
            "file": "LOD",
            "error": format!("{e:#}"),
        }));
    }
    Ok(())
}

fn parse_parcel_str(s: &str) -> Option<(i32, i32)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn scene_parcels(files: &[(String, Vec<u8>)]) -> ((i32, i32), Vec<(i32, i32)>) {
    for (name, data) in files {
        if !name.to_lowercase().ends_with("scene.json") {
            continue;
        }
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
            let base = v["scene"]["base"]
                .as_str()
                .and_then(parse_parcel_str)
                .unwrap_or((0, 0));
            let parcels: Vec<(i32, i32)> = v["scene"]["parcels"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str().and_then(parse_parcel_str))
                        .collect()
                })
                .unwrap_or_default();
            let parcels = if parcels.is_empty() {
                vec![base]
            } else {
                parcels
            };
            return (base, parcels);
        }
    }
    ((0, 0), vec![(0, 0)])
}

fn lod_params_for(
    parcels: &[(i32, i32)],
    base: (i32, i32),
    sid: &str,
    level: u32,
) -> abgen::builder::LodBuildParams {
    abgen::builder::LodBuildParams {
        level,
        plane_clipping: abgen::lods::plane_clipping(parcels),
        vertical_clipping: abgen::lods::vertical_clipping(parcels.len()),
        root_position: abgen::lods::root_position(base),
        main_asset: abgen::lods::lod_main_asset(sid, level),
        timestamp: None,
    }
}

struct LodJob<'a> {
    sid: &'a str,
    target: &'a str,
    entity_type: &'a str,
    files: &'a [(String, Vec<u8>)],
    content_by_file: &'a HashMap<String, String>,
    bytes_by_hash: &'a HashMap<String, &'a Vec<u8>>,
    base: (i32, i32),
    parcels: &'a [(i32, i32)],
    crop: bool,
    tri_cap: u32,
}

fn bake_lod(job: &LodJob) -> abgen::Result<()> {
    use abgen::lodgen::{self, model as lmodel, placements};

    let level = 1u32;
    let sid = job.sid;
    let (target, base, parcels) = (job.target, job.base, job.parcels);
    let root_name = format!("{sid}_{level}");
    let iss = job
        .files
        .iter()
        .find(|(n, _)| n.ends_with(placements::ISS_SUFFIX));

    let mut merged;
    let sources;
    if let Some((_, iss_bytes)) = iss {
        let list = placements::parse_iss(iss_bytes)?;
        let mut file_by_hash: HashMap<&str, &str> = HashMap::new();
        for (name, _) in job.files {
            if let Some(h) = job.content_by_file.get(&name.to_lowercase()) {
                file_by_hash.entry(h.as_str()).or_insert(name.as_str());
            }
        }
        let fetch = |hash: &str| -> abgen::Result<Vec<u8>> {
            job.bytes_by_hash
                .get(hash)
                .map(|b| (*b).clone())
                .ok_or_else(|| abgen::anyhow!("content {hash} not in the upload"))
        };
        merged = lodgen::assemble::assemble_from(
            &root_name,
            job.content_by_file,
            &file_by_hash,
            &list,
            &fetch,
        )?;
        sources = list.len();
        emit_json(serde_json::json!({
            "ev": "note",
            "msg": format!("placements: {sources} from ISS"),
        }));
    } else {
        merged = lmodel::LodModel {
            root_name: root_name.clone(),
            ..Default::default()
        };
        let mut loaded = 0usize;
        let models = job
            .files
            .iter()
            .filter(|(n, _)| naming::GLTF_EXTENSIONS.contains(&ext_of(n).as_str()));
        for (name, bytes) in models {
            match lmodel::from_glb_bytes(bytes, name) {
                Ok(m) => {
                    let img_off = merged.images.len();
                    let mat_off = merged.materials.len();
                    merged.images.extend(m.images);
                    merged
                        .materials
                        .extend(m.materials.into_iter().map(|mut mat| {
                            if let Some(i) = mat.image.as_mut() {
                                *i += img_off;
                            }
                            mat
                        }));
                    merged
                        .primitives
                        .extend(m.primitives.into_iter().map(|mut p| {
                            p.material += mat_off;
                            p
                        }));
                    loaded += 1;
                }
                Err(e) => emit_json(serde_json::json!({
                    "ev": "file-error",
                    "file": name,
                    "error": format!("lod load: {e:#}"),
                })),
            }
        }
        if loaded == 0 {
            emit_json(serde_json::json!({
                "ev": "note",
                "msg": "LOD skipped: no model loaded",
            }));
            return Ok(());
        }
        sources = loaded;
    }

    emit_json(serde_json::json!({
        "ev": "lod-start",
        "models": sources,
        "tris": merged.total_tris(),
        "parcels": parcels.len(),
    }));
    emit_json(serde_json::json!({
        "ev": "note",
        "msg": "LOD lane: merge/ISS placements, optional parcel crop, atlas, optional meshopt \
                decimation, bundle; placements acquisition (manifest builder) and the gltfpack \
                backend stay native-only",
    }));

    if job.crop && job.entity_type == "scene" {
        let rects = lodgen::crop::crop_rects_rh(base, parcels);
        let report = lodgen::crop::crop(&mut merged, &rects);
        emit_json(serde_json::json!({
            "ev": "lod-crop",
            "rects": report.rects,
            "trisIn": report.tris_in,
            "trisOut": report.tris_out,
            "trisClipped": report.tris_clipped,
            "trisDropped": report.tris_dropped,
            "primsDropped": report.prims_dropped,
            "vertsDropped": report.verts_dropped,
        }));
    }

    let atlased = lodgen::atlas::atlas(&merged, 1024, 2)?;
    for line in &atlased.log {
        emit_json(serde_json::json!({ "ev": "note", "msg": format!("atlas: {line}") }));
    }
    emit_json(serde_json::json!({
        "ev": "lod-atlas",
        "tris": atlased.total_tris(),
        "materials": atlased.materials.len(),
        "images": atlased.images.len(),
    }));

    // Over-cap decimation re-parses the emitted atlas GLB so the model fed to
    // the simplifier matches the native CLI chain (atlas file -> simplify file)
    // bit-for-bit; at or under the cap the native lane byte-copies, so the
    // atlas model bundles directly.
    let final_model = if job.tri_cap > 0 && atlased.total_tris() as u64 > job.tri_cap as u64 {
        let pre = lodgen::emit::emit_glb(&atlased)?;
        let reparsed = lmodel::from_glb_bytes(&pre, &root_name)?;
        let (sim, report) =
            lodgen::simplify_meshopt::simplify_model(&reparsed, job.tri_cap as u64, true)?;
        emit_json(serde_json::json!({
            "ev": "lod-simplify",
            "trisBefore": report.tris_before,
            "trisAfter": report.tris_after,
            "sloppy": report.aggressive_final,
        }));
        sim
    } else {
        atlased
    };

    let glb = lodgen::emit::emit_glb(&final_model)?;
    let bundle_name = abgen::lods::lod_bundle_name(sid, level, target);
    let root_hash = format!("{sid}_{level}");
    let src_name = format!("{sid}_{level}.glb");
    let params = lod_params_for(parcels, base, sid, level);
    let opts = BuildOpts {
        source_file: Some(&src_name),
        lod: Some(&params),
        real_textures: true,
        ..Default::default()
    };
    let data = build_bundle(&glb, &bundle_name, &root_hash, &opts)?.data;

    let checks = lodgen::self_gate_bundle(&data, sid, level, target)?;
    let cjson: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "label": c.label,
                "ok": c.ok,
                "detail": c.detail,
            })
        })
        .collect();
    emit_json(serde_json::json!({
        "ev": "gate",
        "bundle": bundle_name,
        "failures": lodgen::gate_failures(&checks),
        "checks": cjson,
    }));
    emit_output(&bundle_name, &data);

    let rel = format!("LOD/{level}/{bundle_name}");
    emit_output(
        &format!("{bundle_name}.br"),
        &abgen::compress::brotli(&data)?,
    );
    let lod_manifest = serde_json::json!({
        "version": abgen::manifest::DEFAULT_AB_VERSION,
        "sceneId": sid,
        "levels": [level],
        "files": [rel.as_str()],
        "exitCode": 0,
    });
    let text = serde_json::to_string_pretty(&lod_manifest)?;
    emit_output("LOD.manifest.json", text.as_bytes());
    emit_output(
        "LOD.manifest.json.br",
        &abgen::compress::brotli(text.as_bytes())?,
    );

    emit_json(serde_json::json!({
        "ev": "lod-done",
        "bundle": bundle_name,
        "bytes": data.len(),
        "servePath": rel,
    }));
    Ok(())
}
