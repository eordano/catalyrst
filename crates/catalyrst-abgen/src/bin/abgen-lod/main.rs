#![cfg_attr(target_arch = "wasm32", no_main)]
#![cfg(not(target_arch = "wasm32"))]

use abgen::catalyst::CatalystClient;
use abgen::lodgen::assemble;
use abgen::lodgen::placements;
use abgen::lodgen::simplify;
use abgen::lods;
use anyhow::{anyhow, bail, Context, Result};
use std::path::PathBuf;

mod compare;

use compare::cmd_compare;

const BIN_NAME: &str = "abgen-lod";

fn usage() -> ! {
    abgen::clihelp::usage_error(usage_text());
}

fn usage_text() -> &'static str {
    "abgen-lod — LOD asset-bundle builder + structural comparator

USAGE:
  abgen-lod bundle <src.glb> --entity <entityId> [--level 1]
            [--platform windows|mac|linux] [--out DIR] [--catalyst URL]
            [--base X,Y --parcels 'x,y;x,y;...'] [--timestamp N] [--vertical-clip H]
  abgen-lod compare <ours> <prod>
  abgen-lod placements (--coords X,Y | --scene <entityId>) [--iss FILE|auto|off]
            [--catalyst URL] [--manifest-builder DIR] [--workdir DIR]
  abgen-lod assemble --scene <entityId|X,Y> -o out.glb [--catalyst URL]
            [--iss FILE|auto|off] [--manifest-builder DIR] [--workdir DIR]
            [--cache DIR] [--level 1] [--no-crop] [--no-atlas] [--max-size 512]
            [--padding 0] [--atlas-fixed]
  abgen-lod atlas -i in.glb -o out.glb [--max-size 512] [--padding 0]
            [--atlas-fixed]
  abgen-lod simplify -i in.glb -o out.glb [--ratio 0.1] [--tri-cap N]
            [--simplifier meshopt|gltfpack] [--gltfpack PATH]
            [--allow-unsimplified]
  abgen-lod generate --scene <pointer|entityId> --out DIR
            [--platform windows|mac|linux[,windows|mac|linux...]]
            [--level 0,1] [--ratio 0.1] [--tri-cap N|auto|off] [--atlas-max 512]
            [--atlas-fixed] [--no-crop] [--catalyst URL] [--iss FILE|auto|off]
            [--manifest-builder DIR] [--workdir DIR] [--cache DIR]
            [--simplifier meshopt|gltfpack] [--gltfpack PATH]
            [--allow-unsimplified] [--keep-glb] [--gpu]

bundle: stages <src.glb> as {entityIdLower}_{level}.glb and builds
  {out}/{entityIdLower}/LOD/{level}/{entityIdLower}_{level}_{platform} (+.br).
  Scene base/parcels come from the catalyst entity unless --base/--parcels override.
compare: parses both bundles and prints PASS/FAIL per structural check; exits 1 on FAIL.
placements: resolves the scene, then prints its GLB placement list as JSON.
  --iss auto (default) tries the production InitialSceneState descriptor first
  (404 falls through); --iss FILE reads a local descriptor; --iss off skips ISS.
  Without ISS the scene-lod-entities-manifest-builder runs as a black box
  (tool dir: --manifest-builder > ABGEN_LOD_MANIFEST_BUILDER, required — a
  checkout of decentraland/scene-lod-entities-manifest-builder;
  copied into --workdir, default ~/.cache/abgen-lod/manifest-builder).
assemble: resolves placements like `placements`, fetches every referenced GLB
  (--cache DIR caches content by hash), bakes all instances into one flat
  merged GLB in glTF right-handed space (the bundler applies the RH->LH flip),
  crops it on x/z to the scene's plane-clipping rect (parcel bounds +-0.05 —
  production ISS-lane merged GLBs are cropped the same way: geometry
  overhanging neighbouring parcels is clipped, not dropped; disable with
  --no-crop), atlases it into per-alpha-class TextureBakeResult materials
  (disable with --no-atlas) and writes it to -o.
atlas: re-runs only the atlas stage on an existing merged GLB: dedupe +
  skyline-pack tiles into one square power-of-two atlas per alpha class
  (opaque JPEG, mask/transparent PNG after alpha bleed), merge each class
  into a single primitive (welding duplicate verts), remap uvs. Default is
  the current-lane native bake (tiles cropped to their used UV window at
  native texels, flat tiles 8x8, canvas shrunk to the packed extent, never
  upscaled); --atlas-fixed selects the retired-lane full-bleed bake (canvas
  pinned to --max-size, tiles scaled to fill).
simplify: decimates a GLB. --simplifier picks the backend (default from
  ABGEN_SIMPLIFIER, else meshopt). meshopt runs the in-crate meshoptimizer
  simplifier: the tri budget (--tri-cap, else ratio x input tris) is
  apportioned per primitive by triangle share, each primitive gets one
  topology-preserving pass with a loose error bound so the count target
  dominates, a sloppy (topology-ignoring) retry when that stops early
  above target, then orphan-vertex compaction; a capped result still over
  budget is a hard error. gltfpack shells out (-si <ratio> -sp -noq;
  binary resolved --gltfpack > ABGEN_GLTFPACK > PATH): --tri-cap N re-runs
  up to 3 times with the ratio rescaled by target/actual*0.9 (-sa on the
  final attempt); if the cap is still unreached at gltfpack's default -se
  error bound it binary-searches a relaxed -se (aggressive) for the
  largest result under the cap, else it fills back toward [0.8*cap, cap]
  when the input was above the cap. In both backends inputs already
  satisfying ratio>=1 + cap pass through untouched.
  --allow-unsimplified copies the input through verbatim (loud warning)
  when the simplifier is unavailable or fails.
generate: the full sync chain: resolve scene -> placements (iss|manifest
  builder) -> assemble -> crop -> atlas -> simplify -> bundle via the LOD build mode
  into {out}/{sceneId}/LOD/{level}/{sceneId}_{level}_{platform} (+.br,
  LOD.manifest.json). --level takes a comma-separated list (default 0,1;
  level 2 is refused; production stopped emitting it): every level shares
  ONE assemble/crop/atlas bake and gets its own simplify pass, staged
  {sceneId}_{level}.glb, bundles and self-gate table (labels L{level}: /
  L{level}:{platform}:). Level 0 = that bake un-decimated (ratio 1.0):
  always the pass-through lane, gltfpack is neither run nor resolved, and a
  numeric --tri-cap is ignored with a warning. This DIVERGES from legacy
  production LOD0 (a real-scene bundle with per-source meshes/materials on
  dcl/scene_ignore_windows per prod-inspection.md and the LOD0 section of
  PROD-CHARACTERIZATION.md); the ISS path is the
  production-current LOD0 replacement. At level 1 the tri budget defaults to
  --tri-cap auto: cap = 500 x parcels, the production budget, so the final
  mesh is min(source, 500 x parcels) tris. Scenes at or under the cap pass
  through bit-identically (without resolving gltfpack); larger scenes are
  decimated into [0.8*cap, cap] (hard error if the cap is unreachable).
  --tri-cap N overrides the cap; --tri-cap off restores the legacy
  ratio-only lane (pass-through at or under 500 x parcels, else an
  uncapped ratio decimation). --simplifier picks the decimation backend
  exactly as in `simplify` above (default from ABGEN_SIMPLIFIER, else
  meshopt). Every capped run adds a tri-cap self-gate
  check (tris_after <= cap); an --allow-unsimplified verbatim copy passes
  it with a recorded waiver. A scene whose
  placements resolve to nothing (e.g. the manifest builder runs clean but
  emits no manifest) builds a content-free bundle: no meshes, materials or
  textures, metadata dependencies []. The crop stage (default on, matching
  production; --no-crop disables) clips merged geometry to the plane-clipping
  rect and adds a crop-bounds self-gate check. --platform takes a
  comma-separated list (windows|mac|linux; webgl is refused — upstream webgl
  LOD bundles use an empty suffix and are unsupported here): every platform
  bundle is built from the same bake and simplify pass, written with its own
  .br sidecar, listed in ONE union LOD.manifest.json, and self-gated
  separately (one gate table per platform, including a target-platform
  check: windows=19 mac=2 linux=24). Every run also writes the ISS
  descriptor {out}/{sceneId}/{sceneId}_InitialSceneState.json (+.br)
  next to LOD.manifest.json — the production InitialSceneState shape
  ({version, sceneId, assets:[{hash, position, rotation, scale}]}) with the
  acquired placements serialized verbatim in the pinned base-relative
  frame, in BOTH lanes (ISS pass-through and manifest-builder; empty
  scene => assets []); the abcdn server serves it at
  /lods-unity/manifests/{sceneId}_InitialSceneState.json and an
  iss-descriptor self-gate check re-parses it. Every run ends with a
  structural self-gate; any FAIL exits nonzero. --keep-glb keeps the
  intermediate merged GLBs in the workdir.

--help/-h prints this help; --version/-V prints the version."
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first() else { usage() };
    let rc = match cmd.as_str() {
        "bundle" => cmd_bundle(&argv[1..]),
        "compare" => cmd_compare(&argv[1..]),
        "placements" => cmd_placements(&argv[1..]),
        "assemble" => cmd_assemble(&argv[1..]),
        "atlas" => cmd_atlas(&argv[1..]),
        "simplify" => cmd_simplify(&argv[1..]),
        "generate" => cmd_generate(&argv[1..]),
        "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
        "-V" | "--version" => abgen::clihelp::print_version(BIN_NAME),
        other => {
            eprintln!("unknown subcommand {other:?}");
            usage();
        }
    };
    match rc {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

use abgen::lodgen::parse_parcel;

fn parse_parcels(s: &str) -> Result<Vec<(i32, i32)>> {
    let mut out = Vec::new();
    for tok in s.split(';') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        out.push(parse_parcel(tok)?);
    }
    if out.is_empty() {
        bail!("--parcels {s:?} has no parcels");
    }
    Ok(out)
}

type EntityGeometry = ((i32, i32), Vec<(i32, i32)>);

fn entity_geometry(client: &CatalystClient, entity_id: &str) -> Result<EntityGeometry> {
    let ent = client
        .fetch_entity(entity_id)
        .with_context(|| format!("fetch entity {entity_id}"))?;
    abgen::lodgen::scene_geometry(&ent)
}

fn cmd_bundle(argv: &[String]) -> Result<i32> {
    let mut src: Option<String> = None;
    let mut entity: Option<String> = None;
    let mut level: u32 = 1;
    let mut platform = "windows".to_string();
    let mut out = "lodgen-out".to_string();
    let mut catalyst = "https://peer.decentraland.org/content".to_string();
    let mut base: Option<String> = None;
    let mut parcels: Option<String> = None;
    let mut timestamp: Option<i64> = None;
    let mut vertical_clip: Option<f64> = None;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "--entity" => {
                entity = Some(need(i)?.clone());
                i += 1;
            }
            "--level" => {
                level = need(i)?.parse().context("--level")?;
                i += 1;
            }
            "--platform" => {
                platform = need(i)?.clone();
                i += 1;
            }
            "--out" => {
                out = need(i)?.clone();
                i += 1;
            }
            "--catalyst" => {
                catalyst = need(i)?.clone();
                i += 1;
            }
            "--base" => {
                base = Some(need(i)?.clone());
                i += 1;
            }
            "--parcels" => {
                parcels = Some(need(i)?.clone());
                i += 1;
            }
            "--timestamp" => {
                timestamp = Some(need(i)?.parse().context("--timestamp")?);
                i += 1;
            }
            "--vertical-clip" => {
                vertical_clip = Some(need(i)?.parse().context("--vertical-clip")?);
                i += 1;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other if other.starts_with("--") => {
                bail!("unknown flag {other:?}");
            }
            other => {
                if src.is_some() {
                    bail!("unexpected positional {other:?}");
                }
                src = Some(other.to_string());
            }
        }
        i += 1;
    }
    let src = src.ok_or_else(|| anyhow!("bundle needs a <src.glb> positional"))?;
    let entity = entity.ok_or_else(|| anyhow!("bundle needs --entity"))?;
    lods::validate_lod_platform(&platform)?;
    let sid = entity.to_lowercase();

    let client = CatalystClient::from_args(&catalyst, None);
    let (base_parcel, parcel_list) = match (&base, &parcels) {
        (Some(b), Some(p)) => (parse_parcel(b)?, parse_parcels(p)?),
        (None, None) => entity_geometry(&client, &entity)?,
        _ => bail!("--base and --parcels must be given together"),
    };
    println!(
        "entity {sid}: base={},{} parcels={}",
        base_parcel.0,
        base_parcel.1,
        parcel_list.len()
    );
    let plane = lods::plane_clipping(&parcel_list);
    let vertical = match vertical_clip {
        Some(h) => [0.0, h, 0.0, 0.0],
        None => lods::vertical_clipping(parcel_list.len()),
    };
    println!(
        "planeClipping=({},{},{},{}) verticalClipping=({},{},{},{}) rootPosition=({},{},{})",
        plane[0],
        plane[1],
        plane[2],
        plane[3],
        vertical[0],
        vertical[1],
        vertical[2],
        vertical[3],
        lods::root_position(base_parcel)[0],
        lods::root_position(base_parcel)[1],
        lods::root_position(base_parcel)[2]
    );

    let work = PathBuf::from(&out).join(".work");
    std::fs::create_dir_all(&work)?;
    let staged = work.join(format!("{sid}_{level}.glb"));
    std::fs::copy(&src, &staged).with_context(|| format!("copy {src} -> {}", staged.display()))?;

    let opts = lods::LodOptions {
        platform: platform.clone(),
        lod: Some(lods::LodGenMeta {
            parcels: parcel_list,
            base: base_parcel,
            timestamp,
            vertical_override: vertical_clip,
        }),
        ..Default::default()
    };
    let conv = lods::convert_lods(
        &client,
        &[staged.to_string_lossy().into_owned()],
        &out,
        &opts,
    )?;
    for r in &conv.results {
        println!(
            "built {}/{}/{} ({} bytes)",
            out, r.scene_id, r.rel_path, r.bytes
        );
    }
    for (loc, err) in &conv.skipped {
        eprintln!("SKIP {loc}: {err}");
    }
    Ok(if conv.skipped.is_empty() { 0 } else { 1 })
}

fn cmd_placements(argv: &[String]) -> Result<i32> {
    let mut coords: Option<String> = None;
    let mut scene: Option<String> = None;
    let mut iss = "auto".to_string();
    let mut catalyst = "https://peer.decentraland.org/content".to_string();
    let mut manifest_builder: Option<String> = None;
    let mut workdir: Option<String> = None;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "--coords" => {
                coords = Some(need(i)?.clone());
                i += 1;
            }
            "--scene" => {
                scene = Some(need(i)?.clone());
                i += 1;
            }
            "--iss" => {
                iss = need(i)?.clone();
                i += 1;
            }
            "--catalyst" => {
                catalyst = need(i)?.clone();
                i += 1;
            }
            "--manifest-builder" => {
                manifest_builder = Some(need(i)?.clone());
                i += 1;
            }
            "--workdir" => {
                workdir = Some(need(i)?.clone());
                i += 1;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other => bail!("unknown placements arg {other:?}"),
        }
        i += 1;
    }
    let target = match (&coords, &scene) {
        (Some(c), None) => {
            parse_parcel(c)?;
            c.clone()
        }
        (None, Some(s)) => s.clone(),
        _ => bail!("placements needs exactly one of --coords or --scene"),
    };

    let client = CatalystClient::from_args(&catalyst, None);
    let ent = client
        .resolve_scene(&target)
        .with_context(|| format!("resolve scene {target:?}"))?;
    eprintln!("scene entity: {}", ent.entity_id);

    let list = acquire_placements(&ent, coords.as_deref(), &iss, manifest_builder, workdir)?;
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(0)
}

fn acquire_placements(
    ent: &abgen::catalyst::Scene,
    coords: Option<&str>,
    iss: &str,
    manifest_builder: Option<String>,
    workdir: Option<String>,
) -> Result<Vec<placements::Placement>> {
    abgen::lodgen::acquire_placements(
        ent,
        coords,
        iss,
        manifest_builder.as_deref(),
        workdir.map(PathBuf::from).as_deref(),
    )
}

fn cmd_assemble(argv: &[String]) -> Result<i32> {
    let mut scene: Option<String> = None;
    let mut out: Option<String> = None;
    let mut iss = "auto".to_string();
    let mut catalyst = "https://peer.decentraland.org/content".to_string();
    let mut manifest_builder: Option<String> = None;
    let mut workdir: Option<String> = None;
    let mut cache: Option<String> = None;
    let mut level: u32 = 1;
    let mut no_crop = false;
    let mut no_atlas = false;
    let mut max_size: u32 = 512;
    let mut padding: u32 = 0;
    let mut atlas_fixed = false;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "--scene" => {
                scene = Some(need(i)?.clone());
                i += 1;
            }
            "-o" | "--out" => {
                out = Some(need(i)?.clone());
                i += 1;
            }
            "--iss" => {
                iss = need(i)?.clone();
                i += 1;
            }
            "--catalyst" => {
                catalyst = need(i)?.clone();
                i += 1;
            }
            "--manifest-builder" => {
                manifest_builder = Some(need(i)?.clone());
                i += 1;
            }
            "--workdir" => {
                workdir = Some(need(i)?.clone());
                i += 1;
            }
            "--cache" => {
                cache = Some(need(i)?.clone());
                i += 1;
            }
            "--level" => {
                level = need(i)?.parse().context("--level")?;
                i += 1;
            }
            "--no-crop" => {
                no_crop = true;
            }
            "--no-atlas" => {
                no_atlas = true;
            }
            "--max-size" => {
                max_size = need(i)?.parse().context("--max-size")?;
                i += 1;
            }
            "--padding" => {
                padding = need(i)?.parse().context("--padding")?;
                i += 1;
            }
            "--atlas-fixed" => {
                atlas_fixed = true;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other => bail!("unknown assemble arg {other:?}"),
        }
        i += 1;
    }
    let target = scene.ok_or_else(|| anyhow!("assemble needs --scene <entityId|X,Y>"))?;
    let out = out.ok_or_else(|| anyhow!("assemble needs -o <out.glb>"))?;
    let coords_hint = parse_parcel(&target).ok().map(|_| target.clone());

    let client = CatalystClient::from_args(&catalyst, None);
    let ent = client
        .resolve_scene(&target)
        .with_context(|| format!("resolve scene {target:?}"))?;
    eprintln!("scene entity: {}", ent.entity_id);

    let list = acquire_placements(
        &ent,
        coords_hint.as_deref(),
        &iss,
        manifest_builder,
        workdir,
    )?;
    eprintln!("placements: {}", list.len());

    let cache_dir = cache.as_deref().map(std::path::Path::new);
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    }
    let mut model = assemble::assemble(&client, &ent, &list, level, cache_dir)?;
    if !no_crop {
        let (base, parcels) = abgen::lodgen::scene_geometry(&ent)?;
        let rects = abgen::lodgen::crop::crop_rects_rh(base, &parcels);
        let report = abgen::lodgen::crop::crop(&mut model, &rects);
        eprintln!("crop: {}", report.summary());
    }
    let model = if no_atlas {
        model
    } else {
        let mode = if atlas_fixed {
            abgen::lodgen::atlas::AtlasMode::FullBleed
        } else {
            abgen::lodgen::atlas::AtlasMode::Native
        };
        abgen::lodgen::atlas::atlas_with(&model, max_size, padding, mode)?
    };
    for line in &model.log {
        eprintln!("{line}");
    }
    for line in model.log.iter().filter(|l| l.starts_with("atlas:")) {
        println!("{line}");
    }

    let glb = abgen::lodgen::emit::emit_glb(&model)?;
    if let Some(parent) = std::path::Path::new(&out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out, &glb).with_context(|| format!("write {out}"))?;

    let summary = model
        .log
        .iter()
        .rev()
        .find(|l| l.starts_with("summary:"))
        .cloned()
        .unwrap_or_default();
    println!("{summary}");
    println!(
        "tris={} materials={} images={} bytes={}",
        model.total_tris(),
        model.materials.len(),
        model.images.len(),
        glb.len()
    );
    let (mn, mx) = model.bounds();
    println!(
        "aabb_rh min=({},{},{}) max=({},{},{})",
        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
    );
    println!(
        "aabb_unity_local min=({},{},{}) max=({},{},{})",
        -mx[0], mn[1], mn[2], -mn[0], mx[1], mx[2]
    );
    if let Some(base) = ent
        .metadata
        .get("scene")
        .and_then(|s| s.get("base"))
        .and_then(|b| b.as_str())
        .and_then(|b| parse_parcel(b).ok())
    {
        let (bx, by) = (base.0 as f32 * 16.0, base.1 as f32 * 16.0);
        println!(
            "base={},{} aabb_unity_world min=({},{},{}) max=({},{},{})",
            base.0,
            base.1,
            -mx[0] + bx,
            mn[1],
            mn[2] + by,
            -mn[0] + bx,
            mx[1],
            mx[2] + by
        );
    }
    println!("wrote {out}");
    Ok(0)
}

fn cmd_atlas(argv: &[String]) -> Result<i32> {
    let mut input: Option<String> = None;
    let mut out: Option<String> = None;
    let mut max_size: u32 = 512;
    let mut padding: u32 = 0;
    let mut atlas_fixed = false;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "-i" | "--in" => {
                input = Some(need(i)?.clone());
                i += 1;
            }
            "-o" | "--out" => {
                out = Some(need(i)?.clone());
                i += 1;
            }
            "--max-size" => {
                max_size = need(i)?.parse().context("--max-size")?;
                i += 1;
            }
            "--padding" => {
                padding = need(i)?.parse().context("--padding")?;
                i += 1;
            }
            "--atlas-fixed" => {
                atlas_fixed = true;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other => bail!("unknown atlas arg {other:?}"),
        }
        i += 1;
    }
    let input = input.ok_or_else(|| anyhow!("atlas needs -i <in.glb>"))?;
    let out = out.ok_or_else(|| anyhow!("atlas needs -o <out.glb>"))?;

    let bytes = std::fs::read(&input).with_context(|| format!("read {input}"))?;
    let stem = std::path::Path::new(&input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lod")
        .to_string();
    let model = abgen::lodgen::model::from_glb_bytes(&bytes, &stem)
        .with_context(|| format!("parse {input}"))?;
    let mode = if atlas_fixed {
        abgen::lodgen::atlas::AtlasMode::FullBleed
    } else {
        abgen::lodgen::atlas::AtlasMode::Native
    };
    let atlased = abgen::lodgen::atlas::atlas_with(&model, max_size, padding, mode)?;
    for line in atlased.log.iter().filter(|l| l.starts_with("atlas:")) {
        println!("{line}");
    }
    let glb = abgen::lodgen::emit::emit_glb(&atlased)?;
    if let Some(parent) = std::path::Path::new(&out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out, &glb).with_context(|| format!("write {out}"))?;
    println!(
        "tris_in={} tris_out={} materials={} images={} bytes={}",
        model.total_tris(),
        atlased.total_tris(),
        atlased.materials.len(),
        atlased.images.len(),
        glb.len()
    );
    if atlased.total_tris() != model.total_tris() {
        bail!(
            "atlas changed triangle count: {} -> {}",
            model.total_tris(),
            atlased.total_tris()
        );
    }
    println!("wrote {out}");
    Ok(0)
}

fn cmd_simplify(argv: &[String]) -> Result<i32> {
    let mut input: Option<String> = None;
    let mut out: Option<String> = None;
    let mut ratio: f64 = 0.1;
    let mut tri_cap: Option<u64> = None;
    let mut backend = simplify::SimplifierBackend::from_env();
    let mut gltfpack: Option<String> = None;
    let mut allow_unsimplified = false;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "-i" | "--in" => {
                input = Some(need(i)?.clone());
                i += 1;
            }
            "-o" | "--out" => {
                out = Some(need(i)?.clone());
                i += 1;
            }
            "--ratio" => {
                ratio = need(i)?.parse().context("--ratio")?;
                i += 1;
            }
            "--tri-cap" => {
                tri_cap = Some(need(i)?.parse().context("--tri-cap")?);
                i += 1;
            }
            "--simplifier" => {
                backend = simplify::SimplifierBackend::parse(need(i)?)?;
                i += 1;
            }
            "--gltfpack" => {
                gltfpack = Some(need(i)?.clone());
                i += 1;
            }
            "--allow-unsimplified" => {
                allow_unsimplified = true;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other => bail!("unknown simplify arg {other:?}"),
        }
        i += 1;
    }
    let input = PathBuf::from(input.ok_or_else(|| anyhow!("simplify needs -i <in.glb>"))?);
    let out = PathBuf::from(out.ok_or_else(|| anyhow!("simplify needs -o <out.glb>"))?);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let report = match backend {
        simplify::SimplifierBackend::Meshopt => {
            eprintln!("simplifier: meshopt (in-crate meshoptimizer)");
            match abgen::lodgen::simplify_meshopt::simplify_file(&input, &out, ratio, tri_cap) {
                Ok(r) => r,
                Err(e) if allow_unsimplified => {
                    eprintln!(
                        "WARNING: meshopt simplify failed ({e:#}); --allow-unsimplified passthrough"
                    );
                    simplify::copy_unsimplified(&input, &out)?
                }
                Err(e) => return Err(e),
            }
        }
        simplify::SimplifierBackend::Gltfpack => {
            match simplify::resolve_gltfpack(gltfpack.as_deref().map(std::path::Path::new)) {
                Ok(bin) => {
                    eprintln!("gltfpack: {}", bin.display());
                    match simplify::simplify(&input, &out, ratio, tri_cap, &bin) {
                        Ok(r) => r,
                        Err(e) if allow_unsimplified => {
                            eprintln!(
                                "WARNING: gltfpack failed ({e:#}); --allow-unsimplified passthrough"
                            );
                            simplify::copy_unsimplified(&input, &out)?
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) if allow_unsimplified => {
                    eprintln!("WARNING: {e:#}; --allow-unsimplified passthrough");
                    simplify::copy_unsimplified(&input, &out)?
                }
                Err(e) => return Err(e),
            }
        }
    };
    println!("simplify: {}", report.summary());
    println!("wrote {}", out.display());
    Ok(0)
}

fn cmd_generate(argv: &[String]) -> Result<i32> {
    let mut params = abgen::lodgen::GenerateParams::default();
    let mut scene: Option<String> = None;
    let mut out: Option<String> = None;

    let mut i = 0usize;
    while i < argv.len() {
        let need = |i: usize| -> Result<&String> {
            argv.get(i + 1)
                .ok_or_else(|| anyhow!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "--scene" => {
                scene = Some(need(i)?.clone());
                i += 1;
            }
            "--out" => {
                out = Some(need(i)?.clone());
                i += 1;
            }
            "--platform" => {
                let mut list: Vec<String> = need(i)?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut seen = std::collections::HashSet::new();
                list.retain(|p| seen.insert(p.clone()));
                if list.is_empty() {
                    bail!("--platform needs at least one of windows|mac|linux");
                }
                for p in &list {
                    lods::validate_lod_platform(p)?;
                }
                params.platform = list[0].clone();
                params.platforms = list;
                i += 1;
            }
            "--level" => {
                let mut list: Vec<u32> = Vec::new();
                for tok in need(i)?.split(',') {
                    let tok = tok.trim();
                    if tok.is_empty() {
                        continue;
                    }
                    list.push(tok.parse().context("--level")?);
                }
                params.levels = abgen::lodgen::normalize_levels(&list)?;
                i += 1;
            }
            "--ratio" => {
                params.ratio = need(i)?.parse().context("--ratio")?;
                i += 1;
            }
            "--tri-cap" => {
                let v = need(i)?;
                match v.as_str() {
                    "auto" => {
                        params.tri_cap = None;
                        params.tri_cap_auto = true;
                    }
                    "off" => {
                        params.tri_cap = None;
                        params.tri_cap_auto = false;
                    }
                    _ => {
                        params.tri_cap = Some(v.parse().context("--tri-cap")?);
                        params.tri_cap_auto = false;
                    }
                }
                i += 1;
            }
            "--atlas-max" => {
                params.atlas_max = need(i)?.parse().context("--atlas-max")?;
                i += 1;
            }
            "--atlas-fixed" => {
                params.atlas_fixed = true;
            }
            "--no-crop" => {
                params.crop = false;
            }
            "--catalyst" => {
                params.catalyst = need(i)?.clone();
                i += 1;
            }
            "--iss" => {
                params.iss = need(i)?.clone();
                i += 1;
            }
            "--manifest-builder" => {
                params.manifest_builder = Some(need(i)?.clone());
                i += 1;
            }
            "--workdir" => {
                params.workdir = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--cache" => {
                params.cache = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--simplifier" => {
                params.simplifier = simplify::SimplifierBackend::parse(need(i)?)?;
                i += 1;
            }
            "--gltfpack" => {
                params.gltfpack = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--allow-unsimplified" => {
                params.allow_unsimplified = true;
            }
            "--keep-glb" => {
                params.keep_glb = true;
            }
            "--gpu" => {
                abgen::enable_gpu().map_err(|e| anyhow!("--gpu: {e}"))?;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            other => bail!("unknown generate arg {other:?}"),
        }
        i += 1;
    }
    params.scene = scene.ok_or_else(|| anyhow!("generate needs --scene <pointer|entityId>"))?;
    params.out_dir = out.ok_or_else(|| anyhow!("generate needs --out DIR"))?;

    let outcome = abgen::lodgen::generate(&params)?;
    for line in &outcome.log {
        eprintln!("{line}");
    }
    println!(
        "entity={} scene_id={} source_tris={}",
        outcome.entity_id, outcome.scene_id, outcome.source_tris
    );
    for lb in &outcome.levels {
        println!(
            "level={} final_tris={} bundle_bytes={} rel={}",
            lb.level, lb.simplify.tris_after, lb.bundle_bytes, lb.rel_path
        );
        println!("simplify[{}]: {}", lb.level, lb.simplify.summary());
        if let Some(glb) = &lb.glb_path {
            println!("kept glb[{}]: {}", lb.level, glb.display());
        }
        println!("bundle[{}]: {}", lb.level, lb.bundle_path.display());
    }
    for c in &outcome.gate {
        println!(
            "{} self-gate {}: {}",
            if c.ok { "PASS" } else { "FAIL" },
            c.label,
            c.detail
        );
    }
    let failures = abgen::lodgen::gate_failures(&outcome.gate);
    if failures == 0 {
        println!("SELF-GATE PASSED ({} checks)", outcome.gate.len());
        Ok(0)
    } else {
        println!(
            "SELF-GATE FAILED ({failures} of {} checks)",
            outcome.gate.len()
        );
        Ok(1)
    }
}
