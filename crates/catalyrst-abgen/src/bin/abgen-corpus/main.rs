#![allow(clippy::too_many_arguments)]
#![cfg_attr(target_arch = "wasm32", no_main)]
#![cfg(not(target_arch = "wasm32"))]

use abgen::builder::BuildOpts;
use abgen::local_store::{LocalContentStore, ABGEN_CONTENT_ROOT_ENV, DEFAULT_CONTENT_ROOT};
use abgen::Result;
use anyhow::{anyhow, Context};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

mod build;
mod dedup;
mod sources;

use build::{build_bundle_at, run_fused_entity_ids, write_cdn_manifest, BuildCounters};
use sources::{
    fetch_ids_into_store, from_collection_urn, from_entity_ids, from_live_reference,
    from_reference, manifest_from_ids,
};

const DEFAULT_LAMBDAS_URL: &str = "http://localhost:5141/lambdas";

const DEFAULT_CDN_AB_VERSION: &str = "v41";
const DEFAULT_CONTENT_SERVER_URL: &str = "https://peer.decentraland.org/content";

const DEFAULT_PER_VINTAGE: usize = 50;

#[derive(Deserialize)]
pub(crate) struct Manifest {
    pub(crate) content_dir: String,
    pub(crate) entities: Vec<EntityEntry>,
}

#[derive(Deserialize)]
pub(crate) struct EntityEntry {
    pub(crate) entity_id: String,
    pub(crate) content: Vec<ContentItem>,
    pub(crate) bundles: Vec<BundleSpec>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct ContentItem {
    pub(crate) file: String,
    pub(crate) hash: String,
}

#[derive(Deserialize, Clone)]
pub(crate) struct BundleSpec {
    pub(crate) cid: String,
    pub(crate) bundle_name: String,
    #[serde(default)]
    pub(crate) source_file: Option<String>,
    #[serde(default)]
    pub(crate) entity_type: Option<String>,
    #[serde(default)]
    pub(crate) metadata_deps: Vec<String>,
    #[serde(default)]
    pub(crate) model_referenced: bool,
    #[serde(default)]
    pub(crate) expect_hash: Option<String>,
    #[serde(default)]
    pub(crate) standalone_color_space: Option<i64>,

    #[serde(default)]
    pub(crate) standalone_normal: bool,

    #[serde(default)]
    pub(crate) force_default_material: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct EffectiveToggles {
    pub(crate) collection_mode: bool,
    pub(crate) real_textures: bool,
    pub(crate) v38_compat: bool,
    pub(crate) v38_timestamp: i64,
    pub(crate) magenta_missing: bool,
}

const BIN_NAME: &str = "abgen-corpus";

fn usage() -> ! {
    abgen::clihelp::usage_error(usage_text());
}

fn usage_text() -> &'static str {
    "usage:\n  \
         abgen-corpus <manifest.json> <out-dir> [-j JOBS]\n  \
         abgen-corpus --from-reference <ref-dir> <out-dir> \\\n               \
             [--platform windows|mac] [--content-dir <dir>] [--entities <entities.json>] \\\n               \
             [--collection-mode] [--flat] [--expect-hash] [-j JOBS]\n  \
         abgen-corpus --entity-ids <ids.txt> <out-dir> \\\n               \
             [--platform windows|mac] [--content-dir <dir>] [--cdn-layout] \\\n               \
             [--fetch-missing] [--content-server-url <url>] [-j JOBS]\n  \
         abgen-corpus --collection-urn <urn> <out-dir> \\\n               \
             [--lambdas-url <url>] [--platform windows|mac] [--content-dir <dir>] \\\n               \
             [--fetch-missing] [-j JOBS]\n  \
         abgen-corpus --world <name>[,<name>...] <out-dir> \\\n               \
             [--worlds-url <url>] [--platform windows|mac] [--content-dir <dir>] [--cdn-layout] [-j JOBS]\n               \
             (resolves via --worlds-url, else ABGEN_WORLDS_URL, else the public\n               \
              worlds-content-server;\n               \
              fetches scene entities + content into the store, then converts)\n  \
         abgen-corpus --live-mode <ab-cdn-reference-dir> <out-dir> \\\n               \
             [--platform windows|mac] [--per-vintage N] [--content-dir <dir>] [-j JOBS]\n\
         \n\
         --live-mode (D6 validation loop): build with the EXACT live-serving\n  \
         flag set (live.rs Proxy::new enables real textures + v38 compat\n  \
         when parity is off — same as --client), sampling entities per converter\n  \
         vintage from an ab-cdn-reference tree. The tree is scanned for\n  \
         <entity>/<platform>.manifest.json (one optional prefix-dir level of\n  \
         nesting is handled); manifests with a non-zero exitCode are skipped;\n  \
         entities are grouped by manifest \"version\" (v15/v35/v38/v41/v49/...)\n  \
         and --per-vintage (default 50) are sampled deterministically (sorted\n  \
         by entity id, stride-sampled). Only bundles the upstream manifest\n  \
         actually lists for that entity+platform are built, into\n  \
         <out>/<entity>/<bundle_name>, plus <out>/live-mode-sample.json\n  \
         (the vintage->entity sample record). Verify with:\n               \
             abgen-verify --tolerant <out-dir> <ab-cdn-reference-dir>\n  \
         Conflicts with --parity/--from-reference/--collection-urn/--entity-ids/\n  \
         --world/--cdn-layout/--flat.\n\
         \n\
         --entity-ids: build from a file of entity ids (one per line, blank\n  \
         lines and # comments ignored), deriving each entity's bundles from\n  \
         the local content store; ids whose entity json or content is absent\n  \
         from the store are counted and skipped.\n\
         \n\
         --fetch-missing: before deriving, download each listed entity + its\n  \
         content files into the store from the catalyst content server at\n  \
         --content-server-url (default https://peer.decentraland.org/content;\n  \
         already-present files are kept) — the same\n  \
         fetch path --world uses, so --entity-ids works against a remote\n  \
         catalyst without a pre-synced store. With --collection-urn, missing\n  \
         content is downloaded from the URLs in the lambdas response.\n\
         \n\
         --collection-urn resolves a wearables collection URN via a catalyst\n  \
         lambdas server (default http://localhost:5141/lambdas, a local catalyst) and builds every glb +\n  \
         image in the collection as a flat <out>/<content-hash>_<platform>,\n  \
         matching the converter's ConvertWearablesCollection output (implies\n  \
         --collection-mode + --flat).\n\
         \n\
         manifest format (mode 1):\n  \
         { \"content_dir\": \"/path\",\n    \
           \"entities\": [{ \"entity_id\": \"<cid>\",\n                    \
                          \"content\": [{\"file\":\"foo.glb\",\"hash\":\"<cid>\"}, ...],\n                    \
                          \"bundles\": [{\"cid\":\"<cid>\",\"bundle_name\":\"<cid>_windows\",\n                                       \
                                       \"source_file\":\"foo.glb\",\"entity_type\":\"scene\",\n                                       \
                                       \"metadata_deps\":[...],\"model_referenced\":false}] }] }\n\
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
         v41) and --content-server-url <url>. Only valid with --entity-ids\n  \
         or --world.\n  \
         With --entity-ids, --platform also accepts the comma pair\n  \
         windows,mac (either order): one fused pass derives each entity and\n  \
         parses+encodes each bundle ONCE, then serializes+compresses per\n  \
         platform (their encode parameters are identical; .resS payloads\n  \
         come out byte-identical across the pair by construction). Other\n  \
         combinations are rejected — linux/webgl change the encode itself.\n  \
         Client mode (--real-textures + --v38-compat) is the DEFAULT for\n  \
         --cdn-layout — fork-faithful stub bytes render unlit white/gray in\n  \
         the client. Pass --parity to opt back into stub mode, or --client\n  \
         to make client mode explicit.\n\
         \n\
         --client: shorthand for --real-textures + --v38-compat.\n  \
         --parity (alias --fork-faithful): fork byte-parity stub mode;\n  \
         conflicts with --real-textures/--v38-compat/--client.\n\
         \n\
         --real-textures: oversized standalone textures are downscaled and\n  \
         BC7-encoded for real (production-like) instead of the fork-faithful\n  \
         mean-color stub. Correct/leaner for serving; diverges from fork\n  \
         byte-parity (val300 windows 2652 -> ~2075). Default OFF (stub)\n  \
         except under --cdn-layout (see above).\n\
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
         rely on that).\n\
         \n\
         --gpu: enable the GPU BC7/BC5 encode path (needs a binary built with\n  \
         --features gpu; exits 2 otherwise).\n\
         \n\
         --help/-h prints this help; --version/-V prints the version."
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn resolve_texture_mode(
    cdn_layout: bool,
    client: bool,
    parity: bool,
    real_textures: bool,
    v38_compat: bool,
) -> std::result::Result<(bool, bool), String> {
    if client && parity {
        return Err("--client and --parity are mutually exclusive".to_string());
    }
    if parity && (real_textures || v38_compat) {
        return Err(
            "--parity (fork-faithful stub mode) conflicts with --real-textures/--v38-compat; \
             pass one or the other"
                .to_string(),
        );
    }
    if client {
        return Ok((true, true));
    }
    if parity {
        return Ok((false, false));
    }
    if cdn_layout {
        return Ok((true, true));
    }
    Ok((real_textures, v38_compat))
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
    let mut worlds: Vec<String> = Vec::new();
    let mut worlds_url_flag: Option<String> = None;
    let mut lambdas_url = DEFAULT_LAMBDAS_URL.to_string();
    let mut collection_mode = false;
    let mut flat_output = false;
    let mut cdn_layout = false;
    let mut real_textures = false;
    let mut v38_compat = false;
    let mut client_mode = false;
    let mut parity_mode = false;
    let mut live_mode: Option<String> = None;
    let mut per_vintage: usize = DEFAULT_PER_VINTAGE;
    let mut ab_version = DEFAULT_CDN_AB_VERSION.to_string();
    let mut content_server_url = DEFAULT_CONTENT_SERVER_URL.to_string();
    let mut skip_existing = false;
    let mut force = false;
    let mut fetch_missing = false;
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
                real_textures = true;
            }
            "--v38-compat" => {
                v38_compat = true;
            }
            "--client" => {
                client_mode = true;
            }
            "--parity" | "--fork-faithful" => {
                parity_mode = true;
            }
            "--live-mode" => {
                i += 1;
                live_mode = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--per-vintage" => {
                i += 1;
                per_vintage = argv
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--flat" => {
                flat_output = true;
            }
            "--gpu" => {
                if let Err(e) = abgen::enable_gpu() {
                    eprintln!("error: --gpu: {e}");
                    std::process::exit(2);
                }
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
            "--worlds-url" => {
                i += 1;
                worlds_url_flag = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--world" => {
                i += 1;
                let v = argv.get(i).cloned().unwrap_or_else(|| usage());
                worlds.extend(
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
            "--expect-hash" => {
                expect_hash_enable = true;
            }
            "--skip-existing" => {
                skip_existing = true;
            }
            "--fetch-missing" => {
                fetch_missing = true;
            }
            "--force" => {
                force = true;
            }
            "-h" | "--help" => abgen::clihelp::print_help(usage_text()),
            "-V" | "--version" => abgen::clihelp::print_version(BIN_NAME),
            other if other.starts_with("--") => {
                abgen::clihelp::bad_flag(other, usage_text());
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if live_mode.is_some() {
        if from_ref.is_some()
            || collection_urn.is_some()
            || entity_ids_path.is_some()
            || !worlds.is_empty()
            || cdn_layout
            || flat_output
        {
            eprintln!(
                "error: --live-mode conflicts with --from-reference/--collection-urn/\
                 --entity-ids/--world/--cdn-layout/--flat"
            );
            usage();
        }
        client_mode = true;
    }
    if collection_urn.is_some() {
        collection_mode = true;
        flat_output = true;
    }
    if cdn_layout && flat_output {
        eprintln!("error: --cdn-layout is incompatible with --flat / --collection-urn");
        usage();
    }
    if cdn_layout && entity_ids_path.is_none() && worlds.is_empty() {
        eprintln!("error: --cdn-layout currently requires --entity-ids or --world");
        usage();
    }
    if fetch_missing && entity_ids_path.is_none() && collection_urn.is_none() {
        eprintln!(
            "error: --fetch-missing requires --entity-ids or --collection-urn \
             (--world always fetches)"
        );
        usage();
    }
    let platforms: Vec<String> = platform
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if platforms.is_empty() {
        eprintln!("error: --platform requires at least one platform");
        usage();
    }
    let platform = platforms[0].clone();
    if platforms.len() > 1 {
        let pair_ok = platforms.len() == 2
            && platforms[0] != platforms[1]
            && platforms.iter().all(|p| p == "windows" || p == "mac");
        if !pair_ok {
            eprintln!(
                "error: multi-platform --platform supports exactly windows,mac (either order); \
                 linux/webgl change texture encode and cannot share a fused pass"
            );
            usage();
        }
        if !(cdn_layout && entity_ids_path.is_some()) {
            eprintln!(
                "error: --platform with a comma list requires --entity-ids --cdn-layout \
                 (the fused encode-once pass)"
            );
            usage();
        }
    }
    if !worlds.is_empty()
        && (entity_ids_path.is_some() || from_ref.is_some() || collection_urn.is_some())
    {
        eprintln!("error: --world conflicts with --entity-ids/--from-reference/--collection-urn");
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
    let toggles = match resolve_texture_mode(
        cdn_layout,
        client_mode,
        parity_mode,
        real_textures,
        v38_compat,
    ) {
        Ok((set_real, set_v38)) => {
            if cdn_layout && !client_mode && !parity_mode {
                eprintln!(
                    "cdn-layout: client mode is the default (real textures + v38 compat); \
                     pass --parity for fork-faithful stub bytes"
                );
            }
            EffectiveToggles {
                collection_mode: collection_mode || BuildOpts::env_collection_mode(),
                real_textures: set_real || (!parity_mode && BuildOpts::env_real_textures()),
                v38_compat: set_v38 || (!parity_mode && BuildOpts::env_v38_compat()),
                v38_timestamp: BuildOpts::env_v38_timestamp(),
                magenta_missing: BuildOpts::env_magenta_missing(),
            }
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            usage();
        }
    };

    let missing = abgen::builder::templates_missing();
    if !missing.is_empty() {
        eprintln!(
            "WARNING: missing bundle templates in {}: {} — set ABGEN_ROOT to the directory containing template/ (e.g. ABGEN_ROOT=/path/to/catalyrst-abgen)",
            abgen::builder::template_dir().display(),
            missing.join(", ")
        );
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let mut live_sample_summary: Option<serde_json::Value> = None;
    let (manifest, out_root) = if let Some(ref live_ref) = live_mode {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let (m, summary) = from_live_reference(Path::new(live_ref), &cdir, &platform, per_vintage)?;
        live_sample_summary = Some(summary);
        (m, out_root)
    } else if let Some(ids_path) = entity_ids_path {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        if cdn_layout {
            let raw =
                std::fs::read_to_string(&ids_path).with_context(|| format!("read {ids_path}"))?;
            let ids: Vec<String> = raw
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.to_string())
                .collect();
            let store = LocalContentStore::new(&cdir);
            if fetch_missing {
                fetch_ids_into_store(&store, &content_server_url, &ids);
            }
            std::fs::create_dir_all(&out_root)?;
            eprintln!(
                "fused derive+build: {} entity ids, platform {}, -j (global pool)",
                ids.len(),
                platforms.join("+")
            );
            let o = run_fused_entity_ids(
                &ids,
                &store,
                &out_root,
                &platforms,
                toggles,
                &ab_version,
                &content_server_url,
                skip_existing,
                force,
            );
            eprintln!(
                "cdn-layout: streamed per-entity manifests ({} errors, {} incomplete)",
                o.manifest_errs, o.manifest_incomplete
            );
            eprintln!(
                "reconcile: divergent={} rebuilt={} relinked={} errs={}",
                o.reconcile.divergent, o.reconcile.rebuilt, o.reconcile.relinked, o.reconcile.errs
            );
            let n_errs = o.errs + o.manifest_errs + o.reconcile.errs;
            let total = o.built + o.skipped + o.errs;
            println!(
                "DONE built={} skipped={} errs={n_errs} total={total}",
                o.built, o.skipped
            );
            if n_errs > 0 {
                std::process::exit(1);
            }
            return Ok(());
        }
        let m = from_entity_ids(
            &ids_path,
            &cdir,
            &platform,
            cdn_layout,
            fetch_missing.then_some(content_server_url.as_str()),
        )?;
        (m, out_root)
    } else if !worlds.is_empty() {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let worlds_url = worlds_url_flag
            .clone()
            .unwrap_or_else(abgen::worlds::worlds_url_from_env);
        let store = LocalContentStore::new(cdir.clone());
        let mut ids: Vec<String> = Vec::new();
        for w in &worlds {
            let scenes = abgen::worlds::resolve_world(&worlds_url, w)?;
            if scenes.is_empty() {
                eprintln!("{w}: no scenes deployed, skipping");
            }
            for s in scenes {
                let (fetched, total) = abgen::worlds::fetch_scene_into_store(&store, &s)
                    .with_context(|| format!("world {w}"))?;
                eprintln!(
                    "{w}: entity {} ({fetched}/{total} content files fetched)",
                    s.entity_id
                );
                ids.push(s.entity_id);
            }
        }
        if ids.is_empty() {
            return Err(anyhow!("--world resolved no scene entities"));
        }
        let m = manifest_from_ids(&ids, &cdir, &platform, cdn_layout)?;
        (m, out_root)
    } else if let Some(urn) = collection_urn {
        if positional.len() != 1 {
            usage();
        }
        let out_root = PathBuf::from(&positional[0]);
        let cdir = content_dir
            .or_else(|| std::env::var(ABGEN_CONTENT_ROOT_ENV).ok())
            .unwrap_or_else(|| DEFAULT_CONTENT_ROOT.to_string());
        let m = from_collection_urn(&urn, &lambdas_url, &cdir, &platform, fetch_missing)?;
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
    if let Some(summary) = &live_sample_summary {
        let p = out_root.join("live-mode-sample.json");
        std::fs::write(&p, serde_json::to_vec_pretty(summary)?)
            .with_context(|| format!("write {}", p.display()))?;
        eprintln!("live-mode: sample record at {}", p.display());
    }

    let total: usize = manifest.entities.iter().map(|e| e.bundles.len()).sum();
    let built = AtomicUsize::new(0);
    let errs = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);

    let first_written: dedup::FirstWritten = Mutex::new(HashMap::new());
    let counters = BuildCounters {
        built: &built,
        errs: &errs,
        skipped: &skipped,
    };

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
            build_bundle_at(
                &store,
                &content_by_file,
                spec,
                &ent_out,
                &ent.entity_id,
                toggles,
                skip_existing,
                force,
                if cdn_layout {
                    Some(&first_written)
                } else {
                    None
                },
                &counters,
            );
        });

    let mut reconcile_errs = 0usize;
    if cdn_layout {
        let rs = dedup::reconcile_divergent(&store, &out_root, &platform, toggles, first_written);
        eprintln!(
            "reconcile: divergent={} rebuilt={} relinked={} errs={}",
            rs.divergent, rs.rebuilt, rs.relinked, rs.errs
        );
        reconcile_errs = rs.errs;
    }

    let mut manifest_errs = 0usize;
    let mut manifest_incomplete = 0usize;
    if cdn_layout {
        let build_date = abgen::live::build_scoped_date();
        for ent in &manifest.entities {
            match write_cdn_manifest(
                &out_root,
                &ent.entity_id,
                &platform,
                &ent.bundles,
                &ab_version,
                &content_server_url,
                &build_date,
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
    let n_errs = errs.load(Ordering::Relaxed) + manifest_errs + reconcile_errs;
    let n_skipped = skipped.load(Ordering::Relaxed);
    println!("DONE built={n_built} skipped={n_skipped} errs={n_errs} total={total}");
    if n_errs > 0 {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_texture_mode;
    use super::sources::{contents_base_url, parse_live_manifest, sample_stride};

    #[test]
    fn contents_base_url_normalizes_trailing_slash() {
        for csu in [
            "https://peer.decentraland.org/content",
            "https://peer.decentraland.org/content/",
        ] {
            assert_eq!(
                contents_base_url(csu),
                "https://peer.decentraland.org/content/contents/"
            );
        }
    }

    #[test]
    fn cdn_layout_defaults_to_client_mode() {
        assert_eq!(
            resolve_texture_mode(true, false, false, false, false),
            Ok((true, true))
        );
        assert_eq!(
            resolve_texture_mode(true, false, false, true, false),
            Ok((true, true))
        );
        assert_eq!(
            resolve_texture_mode(true, false, false, false, true),
            Ok((true, true))
        );
    }

    #[test]
    fn parity_opts_out_of_cdn_layout_client_default() {
        assert_eq!(
            resolve_texture_mode(true, false, true, false, false),
            Ok((false, false))
        );
        assert_eq!(
            resolve_texture_mode(false, false, true, false, false),
            Ok((false, false))
        );
    }

    #[test]
    fn client_flag_is_explicit_everywhere() {
        assert_eq!(
            resolve_texture_mode(true, true, false, false, false),
            Ok((true, true))
        );
        assert_eq!(
            resolve_texture_mode(false, true, false, false, false),
            Ok((true, true))
        );
    }

    #[test]
    fn non_cdn_layout_keeps_fork_faithful_default() {
        assert_eq!(
            resolve_texture_mode(false, false, false, false, false),
            Ok((false, false))
        );
        assert_eq!(
            resolve_texture_mode(false, false, false, true, false),
            Ok((true, false))
        );
        assert_eq!(
            resolve_texture_mode(false, false, false, false, true),
            Ok((false, true))
        );
        assert_eq!(
            resolve_texture_mode(false, false, false, true, true),
            Ok((true, true))
        );
    }

    #[test]
    fn ambiguous_combinations_are_refused() {
        assert!(resolve_texture_mode(true, true, true, false, false).is_err());
        assert!(resolve_texture_mode(false, true, true, false, false).is_err());
        assert!(resolve_texture_mode(true, false, true, true, false).is_err());
        assert!(resolve_texture_mode(true, false, true, false, true).is_err());
        assert!(resolve_texture_mode(false, false, true, true, true).is_err());
    }

    #[test]
    fn live_mode_resolves_to_live_flag_set() {
        assert_eq!(
            resolve_texture_mode(false, true, false, false, false),
            Ok((true, true))
        );
        assert!(resolve_texture_mode(false, true, true, false, false).is_err());
    }

    #[test]
    fn stride_sampling_is_deterministic_and_spread() {
        assert_eq!(sample_stride(0, 50), Vec::<usize>::new());
        assert_eq!(sample_stride(10, 0), Vec::<usize>::new());
        assert_eq!(sample_stride(3, 50), vec![0, 1, 2]);
        assert_eq!(sample_stride(4, 4), vec![0, 1, 2, 3]);
        let s = sample_stride(1000, 50);
        assert_eq!(s.len(), 50);
        assert_eq!(s[0], 0);
        assert!(s.windows(2).all(|w| w[0] < w[1]));
        assert!(*s.last().unwrap() < 1000);
        assert_eq!(s, sample_stride(1000, 50));
    }

    #[test]
    fn live_manifest_parsing_filters_platform_and_failures() {
        let ok = br#"{
            "version": "v41",
            "files": ["bafkabc_windows", "bafkdef_windows", "buildlogtep.json", "dcl"],
            "exitCode": 0,
            "contentServerUrl": "https://peer.decentraland.org/content",
            "date": "2025-11-14T21:46:26.102Z"
        }"#;
        let (vintage, files) = parse_live_manifest(ok, "windows").expect("parses");
        assert_eq!(vintage, "v41");
        assert_eq!(files.len(), 2);
        assert!(files.contains("bafkabc_windows"));
        assert!(files.contains("bafkdef_windows"));
        assert!(!files.contains("dcl"));

        let fail = br#"{"version":"v41","files":[],"exitCode":11}"#;
        assert!(parse_live_manifest(fail, "windows").is_none());
        let fail2 = br#"{"version":"v41","files":["bafkabc_windows"],"exitCode":1}"#;
        assert!(parse_live_manifest(fail2, "windows").is_none());

        assert!(parse_live_manifest(ok, "mac").is_none());
        let nover = br#"{"files":["bafkabc_windows"],"exitCode":0}"#;
        assert!(parse_live_manifest(nover, "windows").is_none());
    }
}
