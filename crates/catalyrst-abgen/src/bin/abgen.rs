#![cfg_attr(target_arch = "wasm32", no_main)]
#![cfg(not(target_arch = "wasm32"))]

use abgen::builder::{build_bundle, BuildOpts};
use abgen::local_store::LocalContentStore;
use abgen::naming;
use abgen::Result;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    match std::panic::catch_unwind(run) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            eprintln!("error: internal panic: {msg}");
            std::process::exit(1);
        }
    }
}

fn template_preflight() {
    let missing = abgen::builder::templates_missing();
    if !missing.is_empty() {
        eprintln!(
            "WARNING: missing bundle templates in {}: {} — set ABGEN_ROOT to the directory containing template/ (e.g. ABGEN_ROOT=/path/to/catalyrst-abgen)",
            abgen::builder::template_dir().display(),
            missing.join(", ")
        );
    }
}

const BIN_NAME: &str = "abgen";

fn usage_text() -> &'static str {
    "usage: abgen <glb-path> <bundle-name> <root-hash> <output-path> \\\n\
        \x20         [--source-file PATH] [--entity-type TYPE] \\\n\
        \x20         [--content-map JSON] [--content-dir DIR] \\\n\
        \x20         [--metadata-dep NAME]…  [--metadata-deps-file PATH] \\\n\
        \x20         [--model-referenced] [--expect-hash HEX | --expect-hash-file PATH] \\\n\
        \x20         [--magenta-missing] [--gpu]\n\
        \n\
        --source-file PATH            virtual in-entity path of the input, used\n\
        \x20                          for extension sniffing (`.glb` vs `.gltf`),\n\
        \x20                          the `_emote.glb` suffix gate, and sibling\n\
        \x20                          URI resolution. Defaults to <glb-path>.\n\
        --entity-type TYPE            catalyst entity type (`emote`, `wearable`,\n\
        \x20                          `scene`, …). `emote` routes the build to\n\
        \x20                          the Mecanim AnimatorController+Animator\n\
        \x20                          emission path even when the glb filename\n\
        \x20                          doesn't end with `_emote.glb` (emote\n\
        \x20                          representations frequently use names like\n\
        \x20                          `male/<motion>.glb`). Without this flag,\n\
        \x20                          the legacy filename heuristic is the only\n\
        \x20                          signal.\n\
        --content-map JSON            path to a JSON file containing the entity's\n\
        \x20                          content array (`[{\"file\":..,\"hash\":..}]`).\n\
        \x20                          Used with --source-file to resolve sibling URIs\n\
        \x20                          to content hashes (cross-bundle PPtrs).\n\
        --content-dir DIR             root of the content store; files looked up at\n\
        \x20                          `DIR/<sha1(cid)[:4]>/<cid>`. Together with\n\
        \x20                          --content-map enables byte resolution for .gltf\n\
        \x20                          external .bin/.png URIs.\n\
        --metadata-dep NAME           sibling-bundle filename (e.g. \"<hash>_<platform>\",\n\
        \x20                          where <platform> is one of linux/windows/mac/webgl)\n\
        \x20                          written into `metadata.json.dependencies`.\n\
        \x20                          Repeat for multiple. Order is preserved.\n\
        --metadata-deps-file PATH     read deps from a file, one name per line\n\
        \x20                          (blank lines / `#` comments skipped). Combined\n\
        \x20                          with any --metadata-dep flags (file first).\n\
        --model-referenced            standalone-texture streaming gate (off by\n\
        \x20                          default — matches ab-generate Phase 2 when\n\
        \x20                          no sibling glb references the standalone CID).\n\
        --expect-hash HEX             opt-in emit-and-verify: hex-encoded SHA-256 of\n\
        \x20                          the *prod* bundle this build reproduces. On the\n\
        \x20                          default shader-slot position miss, ab-build\n\
        \x20                          rebuilds with the opposite slot and writes that\n\
        \x20                          (whichever matches). Closes the AssetBundle\n\
        \x20                          shader-slot residual for parity-replay\n\
        \x20                          pipelines that have prod hashes on hand.\n\
        --expect-hash-file PATH       same as --expect-hash but reads the hex from a\n\
        \x20                          file (first whitespace-delimited token).\n\
        --magenta-missing             replace textures that fail to resolve or decode\n\
        \x20                          with a 256x256 magenta MISSING placeholder\n\
        \x20                          instead of failing/dropping them (also via\n\
        \x20                          ABGEN_MAGENTA_MISSING).\n\
        --gpu                         enable the GPU BC7/BC5 encode path (needs a\n\
        \x20                          binary built with --features gpu; exits 2\n\
        \x20                          otherwise).\n\
        --help, -h                    print this help. --version, -V print the version."
}

fn run() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut metadata_deps: Vec<String> = Vec::new();
    let mut deps_file: Option<String> = None;
    let mut source_file: Option<String> = None;
    let mut entity_type: Option<String> = None;
    let mut content_map_path: Option<String> = None;
    let mut content_dir: Option<String> = None;
    let mut model_referenced = false;
    let mut magenta_missing = false;
    let mut expect_hash: Option<String> = None;
    let mut expect_hash_file: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--metadata-dep" => {
                i += 1;
                let v = argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--metadata-dep needs a value");
                    std::process::exit(2);
                });
                metadata_deps.push(v);
            }
            "--metadata-deps-file" => {
                i += 1;
                deps_file = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--metadata-deps-file needs a value");
                    std::process::exit(2);
                }));
            }
            "--source-file" => {
                i += 1;
                source_file = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--source-file needs a value");
                    std::process::exit(2);
                }));
            }
            "--entity-type" => {
                i += 1;
                entity_type = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--entity-type needs a value");
                    std::process::exit(2);
                }));
            }
            "--content-map" => {
                i += 1;
                content_map_path = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--content-map needs a value");
                    std::process::exit(2);
                }));
            }
            "--content-dir" => {
                i += 1;
                content_dir = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--content-dir needs a value");
                    std::process::exit(2);
                }));
            }
            "--model-referenced" => model_referenced = true,
            "--magenta-missing" => magenta_missing = true,
            "--expect-hash" => {
                i += 1;
                expect_hash = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--expect-hash needs a value");
                    std::process::exit(2);
                }));
            }
            "--expect-hash-file" => {
                i += 1;
                expect_hash_file = Some(argv.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("--expect-hash-file needs a value");
                    std::process::exit(2);
                }));
            }
            "--gpu" => {
                if let Err(e) = abgen::enable_gpu() {
                    eprintln!("error: --gpu: {e}");
                    std::process::exit(2);
                }
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

    if positional.len() != 4 {
        abgen::clihelp::usage_error(usage_text());
    }
    template_preflight();
    let glb_path = &positional[0];
    let bundle_name = &positional[1];
    let root_hash = &positional[2];
    let out_path = PathBuf::from(&positional[3]);
    let effective_source: String = source_file.clone().unwrap_or_else(|| glb_path.clone());

    if let Some(path) = &deps_file {
        let content = std::fs::read_to_string(path)?;
        let mut from_file: Vec<String> = Vec::new();
        for line in content.lines() {
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            from_file.push(s.to_string());
        }
        let mut combined = from_file;
        combined.extend(metadata_deps);
        metadata_deps = combined;
    }

    let content_by_file: HashMap<String, String> = match &content_map_path {
        Some(p) => {
            let raw = std::fs::read_to_string(p)?;
            let entries: serde_json::Value = serde_json::from_str(&raw)?;
            let mut m = HashMap::new();
            if let Some(arr) = entries.as_array() {
                for e in arr {
                    if let (Some(f), Some(h)) = (
                        e.get("file").and_then(|v| v.as_str()),
                        e.get("hash").and_then(|v| v.as_str()),
                    ) {
                        m.insert(f.to_lowercase(), h.to_string());
                    }
                }
            }
            m
        }
        None => HashMap::new(),
    };

    let glb = abgen::local_store::mmap_file(std::path::Path::new(glb_path))?;

    let store: Option<LocalContentStore> = content_dir.as_deref().map(LocalContentStore::new);
    let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
        let s = store.as_ref()?;
        let key = naming::resolve_uri_to_content_file(uri, &effective_source)
            .ok()?
            .to_lowercase();
        let h = content_by_file.get(&key)?;
        s.fetch(h).ok()
    };
    let resolve: abgen::gltf::Resolve = if store.is_some() && !content_by_file.is_empty() {
        Some(&resolve_fn)
    } else {
        None
    };

    let resolve_hash_fn = |uri: &str| -> Option<String> {
        let key = naming::resolve_uri_to_content_file(uri, &effective_source)
            .ok()?
            .to_lowercase();
        content_by_file.get(&key).cloned()
    };
    let resolve_hash: Option<abgen::builder::ResolveHash> =
        if !content_by_file.is_empty() && source_file.is_some() {
            Some(&resolve_hash_fn)
        } else {
            None
        };

    let expect_hash_owned: Option<String> = match (expect_hash, expect_hash_file) {
        (Some(_), Some(_)) => {
            eprintln!("--expect-hash and --expect-hash-file are mutually exclusive");
            std::process::exit(2);
        }
        (Some(h), None) => Some(h.trim().to_string()),
        (None, Some(p)) => {
            let raw = std::fs::read_to_string(&p)?;
            let token = raw.split_whitespace().next().unwrap_or("").to_string();
            if token.is_empty() {
                eprintln!("--expect-hash-file {p} is empty");
                std::process::exit(2);
            }
            Some(token)
        }
        (None, None) => None,
    };

    let opts = BuildOpts {
        keep_forward_plus: true,
        source_file: Some(&effective_source),
        entity_type: entity_type.as_deref(),
        resolve,
        resolve_hash,
        model_referenced,
        metadata_dependencies: &metadata_deps,
        expect_hash: expect_hash_owned.as_deref(),
        standalone_color_space: None,
        standalone_normal: false,
        magenta_missing,
        collection_mode: BuildOpts::env_collection_mode(),
        real_textures: BuildOpts::env_real_textures(),
        v38_compat: BuildOpts::env_v38_compat(),
        v38_timestamp: BuildOpts::env_v38_timestamp(),
        ..Default::default()
    };
    let artifact = build_bundle(&glb[..], bundle_name, root_hash, &opts)?;
    std::fs::write(&out_path, &artifact.data)?;
    println!("{} bytes -> {}", artifact.data.len(), out_path.display());
    Ok(())
}
