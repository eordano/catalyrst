use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::catalyst::{CatalystClient, Scene};
use crate::lods;

use super::gate::{push_check, self_gate_bundle_with, tri_cap_check, GateCheck};
use super::model::LodModel;
use super::{assemble, atlas, crop, emit, placements, simplify, simplify_meshopt};

pub fn parse_parcel(s: &str) -> Result<(i32, i32)> {
    let parts: Vec<&str> = s.trim().split(',').collect();
    if parts.len() != 2 {
        bail!("bad parcel {s:?} (want X,Y)");
    }
    Ok((
        parts[0]
            .trim()
            .parse()
            .with_context(|| format!("parcel x in {s:?}"))?,
        parts[1]
            .trim()
            .parse()
            .with_context(|| format!("parcel y in {s:?}"))?,
    ))
}

pub fn scene_geometry(ent: &Scene) -> Result<((i32, i32), Vec<(i32, i32)>)> {
    let scene_meta = ent
        .metadata
        .get("scene")
        .ok_or_else(|| anyhow!("entity {} metadata has no scene block", ent.entity_id))?;
    let base = scene_meta
        .get("base")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("entity {} metadata.scene has no base", ent.entity_id))?;
    let parcels: Vec<(i32, i32)> = scene_meta
        .get("parcels")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("entity {} metadata.scene has no parcels", ent.entity_id))?
        .iter()
        .filter_map(|p| p.as_str())
        .filter_map(|p| parse_parcel(p).ok())
        .collect();
    if parcels.is_empty() {
        bail!("entity {} has no parseable parcels", ent.entity_id);
    }
    Ok((parse_parcel(base)?, parcels))
}

pub fn acquire_placements(
    ent: &Scene,
    coords: Option<&str>,
    iss: &str,
    manifest_builder: Option<&str>,
    workdir: Option<&Path>,
) -> Result<Vec<placements::Placement>> {
    let iss_bytes: Option<Vec<u8>> = match iss {
        "off" => None,
        "auto" => {
            let got = placements::fetch_iss(&ent.entity_id)?;
            if got.is_none() {
                eprintln!(
                    "iss: no descriptor for {} (404), falling through",
                    ent.entity_id
                );
            }
            got
        }
        path => Some(std::fs::read(path).with_context(|| format!("read ISS file {path}"))?),
    };

    match iss_bytes {
        Some(bytes) => {
            let list = placements::parse_iss(&bytes)?;
            eprintln!("source: iss ({} placements)", list.len());
            Ok(list)
        }
        None => {
            let run_coords = match coords {
                Some(c) => c.to_string(),
                None => {
                    let base = ent
                        .metadata
                        .get("scene")
                        .and_then(|s| s.get("base"))
                        .and_then(|b| b.as_str())
                        .ok_or_else(|| {
                            anyhow!("entity {} metadata.scene has no base", ent.entity_id)
                        })?;
                    base.to_string()
                }
            };
            let tool_dir = manifest_builder
                .map(|s| s.to_string())
                .or_else(|| std::env::var("ABGEN_LOD_MANIFEST_BUILDER").ok())
                .ok_or_else(|| {
                    anyhow!(
                        "no ISS descriptor and no manifest-builder dir: pass --manifest-builder DIR or set ABGEN_LOD_MANIFEST_BUILDER to a checkout of {}",
                        placements::MANIFEST_BUILDER_REPO
                    )
                })?;
            let work_dir = match workdir {
                Some(w) => w.to_path_buf(),
                None => {
                    let home = std::env::var("HOME").context("HOME not set (need --workdir)")?;
                    PathBuf::from(home).join(".cache/abgen-lod/manifest-builder")
                }
            };
            let Some(manifest_path) =
                placements::run_manifest_builder(&run_coords, Path::new(&tool_dir), &work_dir)?
            else {
                eprintln!(
                    "manifest-builder: scene ran to completion but emitted no manifest \
                     (no CRDT renderer state); treating {} as an empty scene",
                    ent.entity_id
                );
                return Ok(Vec::new());
            };
            eprintln!("manifest: {}", manifest_path.display());
            if let Some(name) = manifest_path.file_name().and_then(|n| n.to_str()) {
                let manifest_scene = name.trim_end_matches(placements::MANIFEST_SUFFIX);
                if manifest_scene != ent.entity_id {
                    eprintln!(
                        "WARNING: manifest scene id {} != resolved entity {} (deployment drift)",
                        manifest_scene, ent.entity_id
                    );
                }
            }
            let bytes = std::fs::read(&manifest_path)
                .with_context(|| format!("read {}", manifest_path.display()))?;
            let full = placements::parse_lod_manifest_full(&bytes, &ent.content_by_file())?;
            eprintln!(
                "source: manifest-builder ({} placements, {} mesh-renderer-only skipped, {} unresolved src)",
                full.placements.len(),
                full.skipped_mesh_renderer,
                full.unresolved_src
            );
            Ok(full.placements)
        }
    }
}

pub fn write_iss_descriptor(
    out_dir: &Path,
    scene_id: &str,
    list: &[placements::Placement],
    content_by_file: &HashMap<String, String>,
) -> Result<(PathBuf, usize, usize)> {
    let mut assets: Vec<(String, &placements::Placement)> = Vec::new();
    let mut skipped = 0usize;
    for p in list {
        match assemble::resolve_placement_hash(p, content_by_file) {
            Ok(h) => assets.push((h, p)),
            Err(_) => skipped += 1,
        }
    }
    let doc = placements::iss_descriptor(scene_id, &assets);
    let text = serde_json::to_string_pretty(&doc)?;
    let dir = out_dir.join(scene_id);
    std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let path = dir.join(format!("{scene_id}{}", placements::ISS_SUFFIX));
    lods::write_atomic(&path, text.as_bytes())?;
    lods::write_brotli_sidecar(&path, text.as_bytes())?;
    Ok((path, assets.len(), skipped))
}

pub fn staged_glb_name(scene_id: &str, level: u32) -> String {
    format!("{}_{}.glb", scene_id.to_lowercase(), level)
}

pub fn expected_rel_path(scene_id: &str, level: u32, platform: &str) -> String {
    format!(
        "LOD/{}/{}",
        level,
        lods::lod_bundle_name(scene_id, level, platform)
    )
}

#[derive(Clone, Debug)]
pub struct GenerateParams {
    pub scene: String,
    pub out_dir: String,
    pub platform: String,
    pub platforms: Vec<String>,
    pub levels: Vec<u32>,
    pub ratio: f64,
    pub tri_cap: Option<u64>,
    pub tri_cap_auto: bool,
    pub atlas_max: u32,
    pub atlas_padding: u32,
    pub atlas_fixed: bool,
    pub crop: bool,
    pub catalyst: String,
    pub iss: String,
    pub manifest_builder: Option<String>,
    pub workdir: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    pub simplifier: simplify::SimplifierBackend,
    pub gltfpack: Option<PathBuf>,
    pub allow_unsimplified: bool,
    pub keep_glb: bool,
}

impl Default for GenerateParams {
    fn default() -> Self {
        GenerateParams {
            scene: String::new(),
            out_dir: "lodgen-out".to_string(),
            platform: "windows".to_string(),
            platforms: Vec::new(),
            levels: vec![0, 1],
            ratio: 0.1,
            tri_cap: None,
            tri_cap_auto: true,
            atlas_max: 512,
            atlas_padding: 0,
            atlas_fixed: false,
            crop: true,
            catalyst: "https://peer.decentraland.org/content".to_string(),
            iss: "auto".to_string(),
            manifest_builder: None,
            workdir: None,
            cache: None,
            simplifier: simplify::SimplifierBackend::from_env(),
            gltfpack: None,
            allow_unsimplified: false,
            keep_glb: false,
        }
    }
}

#[derive(Debug)]
pub struct LevelBuild {
    pub level: u32,
    pub rel_path: String,
    pub bundle_path: PathBuf,
    pub bundle_bytes: usize,
    pub simplify: simplify::SimplifyReport,
    pub glb_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct GenerateOutcome {
    pub entity_id: String,
    pub scene_id: String,
    pub source_tris: usize,
    pub levels: Vec<LevelBuild>,
    pub gate: Vec<GateCheck>,
    pub log: Vec<String>,
}

pub fn normalize_levels(levels: &[u32]) -> Result<Vec<u32>> {
    if levels.is_empty() {
        bail!("generate needs at least one LOD level");
    }
    let mut out: Vec<u32> = Vec::new();
    for &l in levels {
        if l >= 2 {
            bail!(
                "LOD level {l} refused: production stopped emitting level 2 (~2024-04); \
                 only levels 0/1 are generated"
            );
        }
        if !out.contains(&l) {
            out.push(l);
        }
    }
    Ok(out)
}

pub const TRIS_PER_PARCEL: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimplifyLane {
    Passthrough,
    Uncapped { ratio: f64 },
    Capped { ratio: f64, cap: u64 },
}

pub fn effective_tri_cap(
    level: u32,
    tri_cap: Option<u64>,
    tri_cap_auto: bool,
    threshold: u64,
) -> Option<u64> {
    if level == 0 {
        return None;
    }
    if tri_cap_auto {
        Some(threshold)
    } else {
        tri_cap
    }
}

pub fn choose_lane(
    level: u32,
    tri_cap: Option<u64>,
    tri_cap_auto: bool,
    ratio: f64,
    source_tris: usize,
    threshold: u64,
) -> SimplifyLane {
    if level == 0 {
        return SimplifyLane::Passthrough;
    }
    match effective_tri_cap(level, tri_cap, tri_cap_auto, threshold) {
        Some(cap) if source_tris as u64 <= cap => SimplifyLane::Passthrough,
        Some(cap) => SimplifyLane::Capped { ratio, cap },
        None if source_tris as u64 <= threshold => SimplifyLane::Passthrough,
        None => SimplifyLane::Uncapped { ratio },
    }
}

fn run_gltfpack_lane(
    pre: &Path,
    out: &Path,
    params: &GenerateParams,
    ratio: f64,
    cap: Option<u64>,
) -> Result<simplify::SimplifyReport> {
    match simplify::resolve_gltfpack(params.gltfpack.as_deref()) {
        Ok(bin) => match simplify::simplify(pre, out, ratio, cap, &bin) {
            Ok(r) => Ok(r),
            Err(e) if params.allow_unsimplified => {
                eprintln!("WARNING: gltfpack failed ({e:#}); --allow-unsimplified passthrough");
                simplify::copy_unsimplified(pre, out)
            }
            Err(e) => Err(e),
        },
        Err(e) if params.allow_unsimplified => {
            eprintln!("WARNING: {e:#}; --allow-unsimplified passthrough");
            simplify::copy_unsimplified(pre, out)
        }
        Err(e) => Err(e),
    }
}

fn run_meshopt_lane(
    model: &LodModel,
    pre: &Path,
    out: &Path,
    params: &GenerateParams,
    target_tris: u64,
    enforce_cap: bool,
) -> Result<simplify::SimplifyReport> {
    let attempt = || -> Result<simplify::SimplifyReport> {
        let (m, report) = simplify_meshopt::simplify_model(model, target_tris, enforce_cap)?;
        let glb = emit::emit_glb(&m)?;
        std::fs::write(out, &glb).with_context(|| format!("write {}", out.display()))?;
        Ok(report)
    };
    match attempt() {
        Ok(r) => Ok(r),
        Err(e) if params.allow_unsimplified => {
            eprintln!("WARNING: meshopt simplify failed ({e:#}); --allow-unsimplified passthrough");
            simplify::copy_unsimplified(pre, out)
        }
        Err(e) => Err(e),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_simplify(
    model: &LodModel,
    pre: &Path,
    out: &Path,
    params: &GenerateParams,
    level: u32,
    source_tris: usize,
    parcel_count: usize,
    log: &mut Vec<String>,
) -> Result<simplify::SimplifyReport> {
    let threshold = TRIS_PER_PARCEL * parcel_count as u64;
    let lane = choose_lane(
        level,
        params.tri_cap,
        params.tri_cap_auto,
        params.ratio,
        source_tris,
        threshold,
    );
    match lane {
        SimplifyLane::Passthrough => {
            if level == 0 {
                if params.tri_cap.is_some() {
                    eprintln!(
                        "WARNING: --tri-cap is ignored at level 0; level 0 is always the \
                         un-decimated pass-through bake"
                    );
                }
                log.push(format!(
                    "simplify-lane[0]: level-0 pass-through ({source_tris} tris, ratio 1.0, no gltfpack)"
                ));
            } else {
                match effective_tri_cap(level, params.tri_cap, params.tri_cap_auto, threshold) {
                    Some(cap) => log.push(format!(
                        "simplify-lane[{level}]: pass-through under cap ({source_tris} tris <= cap {cap})"
                    )),
                    None => log.push(format!(
                        "simplify-lane[{level}]: pass-through ({source_tris} tris <= {threshold} = {TRIS_PER_PARCEL} x {parcel_count} parcels)"
                    )),
                }
            }
            simplify::passthrough(pre, out)
        }
        SimplifyLane::Uncapped { ratio } => {
            log.push(format!(
                "simplify-lane[{level}]: ratio {ratio} uncapped ({source_tris} tris > {threshold} = {TRIS_PER_PARCEL} x {parcel_count} parcels, {})",
                params.simplifier.name()
            ));
            match params.simplifier {
                simplify::SimplifierBackend::Gltfpack => {
                    run_gltfpack_lane(pre, out, params, ratio, None)
                }
                simplify::SimplifierBackend::Meshopt => {
                    let target = (source_tris as f64 * ratio.clamp(1e-3, 1.0)).round() as u64;
                    run_meshopt_lane(model, pre, out, params, target, false)
                }
            }
        }
        SimplifyLane::Capped { ratio, cap } => {
            log.push(format!(
                "simplify-lane[{level}]: capped (tri cap {cap}, {})",
                params.simplifier.name()
            ));
            match params.simplifier {
                simplify::SimplifierBackend::Gltfpack => {
                    run_gltfpack_lane(pre, out, params, ratio, Some(cap))
                }
                simplify::SimplifierBackend::Meshopt => {
                    run_meshopt_lane(model, pre, out, params, cap, true)
                }
            }
        }
    }
}

pub fn generate(params: &GenerateParams) -> Result<GenerateOutcome> {
    let levels = normalize_levels(&params.levels)?;
    if params.scene.is_empty() {
        bail!("generate needs --scene <pointer|entityId>");
    }
    let mut platforms: Vec<String> = if params.platforms.is_empty() {
        vec![params.platform.clone()]
    } else {
        params.platforms.clone()
    };
    let mut seen = HashSet::new();
    platforms.retain(|p| seen.insert(p.clone()));
    for p in &platforms {
        lods::validate_lod_platform(p)?;
    }
    let primary = platforms[0].clone();
    let client = CatalystClient::from_args(&params.catalyst, None);
    let ent = client
        .resolve_scene(&params.scene)
        .with_context(|| format!("resolve scene {:?}", params.scene))?;
    let sid = ent.entity_id.to_lowercase();
    let mut log: Vec<String> = Vec::new();
    log.push(format!("entity: {}", ent.entity_id));
    let (base, parcels) = scene_geometry(&ent)?;
    let parcel_count = parcels.len();
    log.push(format!("base={},{} parcels={parcel_count}", base.0, base.1));

    let t_total = std::time::Instant::now();
    let coords_hint = parse_parcel(&params.scene)
        .ok()
        .map(|_| params.scene.clone());
    let mb_workdir = params.workdir.as_ref().map(|w| w.join("manifest-builder"));
    let t = std::time::Instant::now();
    let placements = acquire_placements(
        &ent,
        coords_hint.as_deref(),
        &params.iss,
        params.manifest_builder.as_deref(),
        mb_workdir.as_deref(),
    )?;
    let placements_ms = t.elapsed().as_millis();
    log.push(format!("placements: {}", placements.len()));

    if let Some(dir) = params.cache.as_deref() {
        std::fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    }
    let expect_content = !placements.is_empty();
    let staging = params
        .workdir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&params.out_dir).join(".work"));
    std::fs::create_dir_all(&staging).with_context(|| format!("mkdir {}", staging.display()))?;
    let pre = staging.join(format!("{sid}_pre.glb"));

    let mut assemble_ms = 0;
    let mut atlas_ms = 0;
    let mut emit_ms = 0;
    let mut simplify_ms = 0;
    let mut crop_stats: Option<crop::UnionStats> = None;
    let mut source_tris = 0usize;
    let mut staged: Vec<(u32, PathBuf, simplify::SimplifyReport)> = Vec::new();
    if expect_content {
        let t = std::time::Instant::now();
        let mut model = assemble::assemble(
            &client,
            &ent,
            &placements,
            levels[0],
            params.cache.as_deref(),
        )?;
        assemble_ms = t.elapsed().as_millis();
        if params.crop {
            let rects = crop::crop_rects_rh(base, &parcels);
            let report = crop::crop(&mut model, &rects);
            eprintln!("crop: {}", report.summary());
            if !model.primitives.is_empty() {
                crop_stats = Some(crop::union_stats(&model, &rects, 1e-3));
            }
        }
        let t = std::time::Instant::now();
        let mode = if params.atlas_fixed {
            atlas::AtlasMode::FullBleed
        } else {
            atlas::AtlasMode::Native
        };
        let model = atlas::atlas_with(&model, params.atlas_max, params.atlas_padding, mode)?;
        atlas_ms = t.elapsed().as_millis();
        log.extend(model.log.iter().cloned());
        source_tris = model.total_tris();

        let t = std::time::Instant::now();
        let glb = emit::emit_glb(&model)?;
        std::fs::write(&pre, &glb).with_context(|| format!("write {}", pre.display()))?;
        emit_ms = t.elapsed().as_millis();

        for &level in &levels {
            let out = staging.join(staged_glb_name(&sid, level));
            let t = std::time::Instant::now();
            let sim = run_simplify(
                &model,
                &pre,
                &out,
                params,
                level,
                source_tris,
                parcel_count,
                &mut log,
            )?;
            simplify_ms += t.elapsed().as_millis();
            log.push(format!("simplify[{level}]: {}", sim.summary()));
            staged.push((level, out, sim));
        }
    } else {
        log.push("empty scene: no placements; emitting content-free LOD bundles".to_string());
        for &level in &levels {
            let out = staging.join(staged_glb_name(&sid, level));
            let t = std::time::Instant::now();
            let glb = emit::emit_empty_glb(&format!("{}_{}", sid, level))?;
            std::fs::write(&out, &glb).with_context(|| format!("write {}", out.display()))?;
            emit_ms += t.elapsed().as_millis();
            staged.push((
                level,
                out,
                simplify::SimplifyReport {
                    passthrough: true,
                    ..Default::default()
                },
            ));
        }
    }

    let opts = lods::LodOptions {
        platform: primary.clone(),
        lod: Some(lods::LodGenMeta {
            parcels,
            base,
            timestamp: None,
            vertical_override: None,
        }),
        ..Default::default()
    };
    let sources: Vec<String> = staged
        .iter()
        .map(|(_, p, _)| p.to_string_lossy().into_owned())
        .collect();
    let t = std::time::Instant::now();
    let conv = lods::convert_lods_platforms(&client, &sources, &params.out_dir, &opts, &platforms)?;
    let bundle_ms = t.elapsed().as_millis();
    log.push(format!(
        "timing: placements_ms={placements_ms} assemble_ms={assemble_ms} atlas_ms={atlas_ms} emit_ms={emit_ms} simplify_ms={simplify_ms} bundle_ms={bundle_ms} total_ms={}",
        t_total.elapsed().as_millis()
    ));
    if !conv.skipped.is_empty() {
        bail!("convert_lods skipped sources: {:?}", conv.skipped);
    }
    if conv.results.is_empty() {
        bail!("convert_lods produced no result");
    }
    let mut gate: Vec<GateCheck> = Vec::new();
    push_check(
        &mut gate,
        "scene-id",
        conv.scene_id == sid,
        format!("got {} want {sid}", conv.scene_id),
    );
    let manifest = PathBuf::from(&params.out_dir)
        .join(&conv.scene_id)
        .join("LOD.manifest.json");
    push_check(
        &mut gate,
        "lod-manifest",
        manifest.is_file(),
        manifest.display().to_string(),
    );
    let (iss_path, iss_assets, iss_skipped) = write_iss_descriptor(
        Path::new(&params.out_dir),
        &conv.scene_id,
        &placements,
        &ent.content_by_file(),
    )?;
    if iss_skipped > 0 {
        eprintln!(
            "WARNING: {iss_skipped} placement(s) resolved to no content hash; omitted from the ISS descriptor"
        );
    }
    log.push(format!(
        "iss-descriptor: {} ({iss_assets} assets, {iss_skipped} skipped)",
        iss_path.display()
    ));
    let iss_roundtrip = std::fs::read(&iss_path)
        .ok()
        .and_then(|b| placements::parse_iss(&b).ok())
        .map(|l| l.len());
    push_check(
        &mut gate,
        "iss-descriptor",
        iss_roundtrip == Some(iss_assets),
        format!(
            "{} parse_iss count {iss_roundtrip:?} want {iss_assets}",
            iss_path.display()
        ),
    );
    if let Some(s) = &crop_stats {
        let frac = s.outside_fraction();
        push_check(
            &mut gate,
            "crop-bounds",
            frac < 0.01,
            format!(
                "{} of {} referenced verts outside {}-rect parcel union (fraction {:.5}, margin 0.051)",
                s.outside, s.referenced_verts, s.rects, frac
            ),
        );
        push_check(
            &mut gate,
            "crop-orphans",
            s.referenced_verts == s.buffer_verts,
            format!(
                "referenced {} of {} buffer verts",
                s.referenced_verts, s.buffer_verts
            ),
        );
    }
    let mut level_builds: Vec<LevelBuild> = Vec::new();
    for (level, staged_glb, sim) in staged {
        if expect_content {
            if let Some(cap) = effective_tri_cap(
                level,
                params.tri_cap,
                params.tri_cap_auto,
                TRIS_PER_PARCEL * parcel_count as u64,
            ) {
                let c = tri_cap_check(cap, sim.tris_after, sim.unsimplified);
                push_check(&mut gate, format!("L{level}:{}", c.label), c.ok, c.detail);
            }
        }
        let mut primary_path = PathBuf::new();
        let mut primary_bytes = 0usize;
        for plat in &platforms {
            let rel = expected_rel_path(&sid, level, plat);
            let path = PathBuf::from(&params.out_dir)
                .join(&conv.scene_id)
                .join(&rel);
            let data = std::fs::read(&path)
                .with_context(|| format!("read built bundle {}", path.display()))?;
            let checks = self_gate_bundle_with(&data, &sid, level, plat, expect_content)?;
            for c in checks {
                push_check(
                    &mut gate,
                    format!("L{level}:{plat}:{}", c.label),
                    c.ok,
                    c.detail,
                );
            }
            push_check(
                &mut gate,
                format!("L{level}:{plat}:rel-path"),
                conv.results.iter().any(|r| r.rel_path == rel),
                rel.clone(),
            );
            let br = {
                let mut s = path.as_os_str().to_owned();
                s.push(".br");
                PathBuf::from(s)
            };
            push_check(
                &mut gate,
                format!("L{level}:{plat}:brotli-sidecar"),
                br.is_file(),
                br.display().to_string(),
            );
            if plat == &primary {
                primary_bytes = data.len();
                primary_path = path;
            }
        }
        let glb_path = if params.keep_glb {
            Some(staged_glb)
        } else {
            let _ = std::fs::remove_file(&staged_glb);
            None
        };
        level_builds.push(LevelBuild {
            level,
            rel_path: expected_rel_path(&sid, level, &primary),
            bundle_path: primary_path,
            bundle_bytes: primary_bytes,
            simplify: sim,
            glb_path,
        });
    }
    if !params.keep_glb {
        let _ = std::fs::remove_file(&pre);
    }

    Ok(GenerateOutcome {
        entity_id: ent.entity_id.clone(),
        scene_id: conv.scene_id.clone(),
        source_tris,
        levels: level_builds,
        gate,
        log,
    })
}
