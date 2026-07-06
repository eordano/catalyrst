// Browser proof-of-concept: the abgen converter core compiled to wasm.
// Hand-rolled C ABI (no wasm-bindgen): the host passes a TLV blob of files,
// events stream back through the imported host_emit(kind, ptr, len).
//
// kinds: 0 = json event, 1 = output file (u32 name_len, name, u32 data_len,
// data), 2 = fatal error string, 3 = manifest json.

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
    // Run the C++ static constructors (crnlib/draco globals) once, reactor
    // style. Referencing __wasm_call_ctors also stops wasm-ld from wrapping
    // every export in command-model ctor/dtor calls.
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

fn parse_input(buf: &[u8]) -> Option<Input> {
    let mut off = 0usize;
    let n = read_u32(buf, &mut off)? as usize;
    let mut files = Vec::with_capacity(n);
    for _ in 0..n {
        let name = String::from_utf8(read_chunk(buf, &mut off)?.to_vec()).ok()?;
        let data = read_chunk(buf, &mut off)?.to_vec();
        files.push((name, data));
    }
    let platform = String::from_utf8(read_chunk(buf, &mut off)?.to_vec()).ok()?;
    let entity_type = String::from_utf8(read_chunk(buf, &mut off)?.to_vec()).ok()?;
    let magenta = *buf.get(off)? != 0;
    let lod = buf.get(off + 1).copied().unwrap_or(0) != 0;
    Some(Input {
        files,
        platform,
        entity_type,
        magenta,
        lod,
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
    match convert(input) {
        Ok(()) => 0,
        Err(e) => {
            let msg = format!("{e:#}");
            emit(2, msg.as_bytes());
            1
        }
    }
}

fn convert(input: Input) -> abgen::Result<()> {
    let target = match input.platform.as_str() {
        "windows" | "mac" | "webgl" => input.platform.as_str(),
        other => {
            emit_json(serde_json::json!({
                "ev": "note",
                "msg": format!("unknown platform {other:?}, using windows"),
            }));
            "windows"
        }
    };

    let entity_type = if input.entity_type.is_empty() {
        detect_entity_type(&input.files).to_string()
    } else {
        input.entity_type.clone()
    };

    let mut content_by_file: HashMap<String, String> = HashMap::new();
    let mut bytes_by_hash: HashMap<String, &Vec<u8>> = HashMap::new();
    for (name, data) in &input.files {
        let hash = sha256_hex(data);
        content_by_file.insert(name.to_lowercase(), hash.clone());
        bytes_by_hash.insert(hash, data);
    }

    let mut ids: Vec<String> = content_by_file
        .iter()
        .map(|(f, h)| format!("{f}:{h}"))
        .collect();
    ids.sort();
    let entity_hash = sha256_hex(ids.join("\n").as_bytes());

    let glbs: Vec<&(String, Vec<u8>)> = input
        .files
        .iter()
        .filter(|(n, _)| naming::GLTF_EXTENSIONS.contains(&ext_of(n).as_str()))
        .collect();

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

    let mut built: Vec<String> = Vec::new();
    let mut failures = 0usize;

    for (name, data) in glbs {
        let ext = ext_of(name);
        let hash = content_by_file[&name.to_lowercase()].clone();

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

        let digest = match naming::deps_digest_for_glb(data, name, &content_by_file, input.magenta)
        {
            Ok(d) => d,
            Err(e) => {
                failures += 1;
                emit_json(serde_json::json!({
                    "ev": "file-error",
                    "file": name,
                    "error": format!("dependency resolution: {e:#}"),
                }));
                continue;
            }
        };
        let bundle_name = match naming::canonical_filename(&hash, &ext, target, Some(&digest)) {
            Ok(n) => n,
            Err(e) => {
                failures += 1;
                emit_json(serde_json::json!({
                    "ev": "file-error",
                    "file": name,
                    "error": format!("{e:#}"),
                }));
                continue;
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
            let h = content_by_file.get(&key.to_lowercase())?;
            bytes_by_hash.get(h).map(|b| (*b).clone())
        };
        let resolve_hash_fn = |uri: &str| -> Option<String> {
            let key = naming::resolve_uri_to_content_file(uri, name).ok()?;
            content_by_file.get(&key.to_lowercase()).cloned()
        };

        let opts = BuildOpts {
            source_file: Some(name),
            entity_type: Some(&entity_type),
            resolve: Some(&resolve_fn),
            resolve_hash: Some(&resolve_hash_fn),
            magenta_missing: input.magenta,
            real_textures: true,
            ..Default::default()
        };

        match build_bundle(data, &bundle_name, &entity_hash, &opts) {
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
                built.push(bundle_name);
            }
            Err(e) => {
                failures += 1;
                emit_json(serde_json::json!({
                    "ev": "file-error",
                    "file": name,
                    "error": format!("{e:#}"),
                }));
            }
        }
    }

    if input.lod {
        if target == "webgl" {
            emit_json(serde_json::json!({
                "ev": "note",
                "msg": "LOD skipped: webgl has no LOD lane (windows/mac/linux only)",
            }));
        } else {
            let models: Vec<(String, Vec<u8>)> = input
                .files
                .iter()
                .filter(|(n, _)| naming::GLTF_EXTENSIONS.contains(&ext_of(n).as_str()))
                .cloned()
                .collect();
            let (base, parcels) = scene_parcels(&input.files);
            if let Err(e) = bake_lod(&entity_hash, target, &models, base, &parcels) {
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

fn bake_lod(
    sid: &str,
    target: &str,
    models: &[(String, Vec<u8>)],
    base: (i32, i32),
    parcels: &[(i32, i32)],
) -> abgen::Result<()> {
    use abgen::lodgen::{self, model as lmodel};

    let level = 1u32;
    let mut merged = lmodel::LodModel {
        root_name: format!("{sid}_{level}"),
        ..Default::default()
    };
    let mut loaded = 0usize;
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

    emit_json(serde_json::json!({
        "ev": "lod-start",
        "models": loaded,
        "tris": merged.total_tris(),
        "parcels": parcels.len(),
    }));
    emit_json(serde_json::json!({
        "ev": "note",
        "msg": "LOD demo lane: merge + atlas + bundle; decimation (gltfpack/meshopt), \
                scene placements and parcel crop run only in native abgen",
    }));

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

    let glb = lodgen::emit::emit_glb(&atlased)?;
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

    // Mirror the sidecars native convert_lods writes next to the bundle
    // (.br + LOD.manifest.json) so the parity gate byte-compares them too.
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
