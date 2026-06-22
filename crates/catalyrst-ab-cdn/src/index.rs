//! AB-availability index — derived from the SAME corpus the serve path uses.
//!
//! This folds catalyrst-registry's manifest-store derivation into the unified
//! server, with two consistency guarantees the old split index lacked (the split
//! is what let the index advertise ABs the server then 404s — the non-GP scene
//! bug):
//!
//! 1. **Same corpus.** It reads `out_root` — the very directory this server
//!    serves from — never a second `ABGEN_OUT_ROOT`. The index physically cannot
//!    point at a different corpus than the bytes it indexes.
//! 2. **Servability-derived.** A platform is reported available ONLY when its
//!    per-entity manifest exists AND every bundle the manifest lists resolves
//!    through the same `resolver` path the GET handler uses. So the index can
//!    never advertise an asset the server would 404; availability == servability.
//!
//! Wire shape mirrors the registry: `{ "assets": { "<platform>": { "version",
//! "buildDate" } } }`, empty strings for an unavailable platform.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::resolver;

/// Platforms the registry reports, matching the corpus manifest layout.
const PLATFORMS: [&str; 4] = ["windows", "mac", "webgl", "linux"];

struct PlatformAb {
    version: String,
    build_date: String,
}

/// Derive `versions.assets.<platform>` for `entity_id` from the served corpus
/// (`out_root` + `bundle_index`). Synchronous (filesystem stat-bound); callers
/// should wrap in `spawn_blocking` and cache (as the route layer will).
pub fn entity_versions(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
) -> serde_json::Value {
    let mut assets = serde_json::Map::new();
    for platform in PLATFORMS {
        let obj = match servable_platform(out_root, bundle_index, entity_id, platform) {
            Some(p) => serde_json::json!({ "version": p.version, "buildDate": p.build_date }),
            None => serde_json::json!({ "version": "", "buildDate": "" }),
        };
        assets.insert(platform.to_string(), obj);
    }
    serde_json::json!({ "assets": serde_json::Value::Object(assets) })
}

/// `Some` only when the per-entity `<platform>.manifest.json` exists AND every
/// bundle it lists is servable; otherwise `None` (so the index won't advertise a
/// platform the server can't fully serve).
fn servable_platform(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    platform: &str,
) -> Option<PlatformAb> {
    let manifest_path = out_root
        .join(entity_id)
        .join(format!("{platform}.manifest.json"));
    let text = std::fs::read_to_string(&manifest_path).ok()?;
    let m: serde_json::Value = serde_json::from_str(&text).ok()?;

    let files = m.get("files").and_then(|f| f.as_array())?;
    for f in files {
        let Some(name) = f.as_str() else { continue };
        // "dcl" is a manifest sentinel, not a fetched bundle file.
        if name == "dcl" {
            continue;
        }
        if !bundle_resolves(out_root, bundle_index, entity_id, name) {
            return None;
        }
    }

    Some(PlatformAb {
        version: m
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        build_date: normalize_build_date(m.get("date").and_then(|v| v.as_str()), &manifest_path),
    })
}

/// Match the registry's `buildDate`: forward verbatim when the manifest `date` is
/// already a real RFC3339 instant, otherwise the manifest file's mtime as RFC3339
/// (millis, `Z`). The abgen converter writes a non-instant provenance string, so
/// this resolves to the file mtime — identical to ab-registry reading the same file.
fn normalize_build_date(raw: Option<&str>, manifest_path: &Path) -> String {
    if let Some(date) = raw {
        if chrono::DateTime::parse_from_rfc3339(date.trim()).is_ok() {
            return date.to_string();
        }
    }
    file_mtime_iso(manifest_path).unwrap_or_default()
}

fn file_mtime_iso(path: &Path) -> Option<String> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// True iff `name` resolves to an on-disk bundle via the exact resolution the GET
/// handler uses: the no-deps index, then `binary_path` (flat or
/// `<entity>/<platform>/<file>`), with the case-insensitive fallback.
fn bundle_resolves(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    name: &str,
) -> bool {
    if let Some(p) = bundle_index.get(&name.to_ascii_lowercase()) {
        if p.is_file() {
            return true;
        }
    }
    match resolver::binary_path(out_root, entity_id, name) {
        Some(exact) => resolver::resolve_with_casing(&exact).is_some(),
        None => false,
    }
}

/// Convertible content extensions — an entity with any of these is JIT-buildable.
/// Mirrors `abgen::live`'s CONVERTIBLE_EXTS (private there).
const CONVERTIBLE_EXTS: [&str; 5] = [".glb", ".gltf", ".png", ".jpg", ".jpeg"];

pub fn is_convertible(file: &str) -> bool {
    let f = file.to_ascii_lowercase();
    CONVERTIBLE_EXTS.iter().any(|e| f.ends_with(e))
}

/// Whether an entity's content makes it JIT-buildable (has any convertible asset).
pub fn is_buildable(content: &[abgen::catalyst::ContentEntry]) -> bool {
    content.iter().any(|c| is_convertible(&c.file))
}

/// JIT-aware `versions.assets`. The durable model: the server serves an AB via
/// corpus-hit OR JIT-on-miss, so a platform is available when it's on disk (→ the
/// real manifest version) OR the entity is JIT-buildable (→ the configured
/// `ab_version`, since a fetch will JIT-convert it). `buildDate` is the
/// deterministic `provenance(entity)` the manifest carries either way, so the
/// reported version is stable across the build.
pub fn entity_versions_jit(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    buildable: bool,
    ab_version: &str,
) -> serde_json::Value {
    let mut assets = serde_json::Map::new();
    for platform in PLATFORMS {
        let obj = match servable_platform(out_root, bundle_index, entity_id, platform) {
            // Corpus hit: forward the real on-disk manifest version/date.
            Some(p) => serde_json::json!({ "version": p.version, "buildDate": p.build_date }),
            // Miss but buildable: the server JIT-converts on fetch, so advertise it
            // with the version/date the JIT write-back will stamp.
            None if buildable => serde_json::json!({
                "version": ab_version,
                "buildDate": abgen::manifest::provenance(entity_id),
            }),
            // Not AB-able: not advertised.
            None => serde_json::json!({ "version": "", "buildDate": "" }),
        };
        assets.insert(platform.to_string(), obj);
    }
    serde_json::json!({ "assets": serde_json::Value::Object(assets) })
}

/// Full registry record for an entity, wire-compatible with ab-registry's
/// `EntityVersions`/`DbEntity` AB fields: `(versions, bundles, status)`. `None`
/// when the entity is neither on disk nor buildable (the registry skips
/// non-servable entities). `bundles.assets.<platform>` mirrors per-platform
/// availability; `status` is the `RegistryStatus` (complete when the required
/// windows+mac+linux are all available, else fallback). All strings serialize as
/// the registry's lowercase enums.
pub fn entity_ab_record(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    buildable: bool,
    ab_version: &str,
) -> Option<(serde_json::Value, serde_json::Value, &'static str)> {
    let versions = entity_versions_jit(out_root, bundle_index, entity_id, buildable, ab_version);
    if versions_empty(&versions) {
        return None;
    }
    let available = |p: &str| {
        !versions["assets"][p]["version"]
            .as_str()
            .unwrap_or("")
            .is_empty()
    };
    let bstat = |p: &str| if available(p) { "complete" } else { "pending" };
    let bundles = serde_json::json!({ "assets": {
        "windows": bstat("windows"),
        "mac": bstat("mac"),
        "webgl": bstat("webgl"),
        "linux": bstat("linux"),
    }});
    // RegistryStatus: complete iff the required platforms (windows/mac/linux) are
    // all available; otherwise fallback (servable but incomplete).
    let status = if available("windows") && available("mac") && available("linux") {
        "complete"
    } else {
        "fallback"
    };
    Some((versions, bundles, status))
}

/// True when no platform is available (all versions empty) — caller skips it, as
/// the registry skips non-servable entities.
pub fn versions_empty(versions: &serde_json::Value) -> bool {
    let Some(assets) = versions.get("assets").and_then(|a| a.as_object()) else {
        return true;
    };
    assets
        .values()
        .all(|p| p.get("version").and_then(|v| v.as_str()).unwrap_or("").is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use abgen::catalyst::{ContentEntry, Scene};

    fn scene(entity: &str, files: &[&str]) -> Scene {
        Scene {
            entity_id: entity.to_string(),
            entity_type: "scene".to_string(),
            pointers: vec!["0,0".to_string()],
            content: files
                .iter()
                .map(|f| ContentEntry {
                    file: f.to_string(),
                    hash: format!("Qm{f}"),
                })
                .collect(),
            metadata: serde_json::Value::Null,
        }
    }

    fn tmp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("abcdn_index_{label}_{}", std::process::id()))
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn available_only_when_manifest_and_bundles_present() {
        let root = tmp("ok");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkEntity";
        let bundle = "QmHash_windows";
        // manifest + the bundle file it lists (entity/platform subdir layout).
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v41","files":[bundle,"dcl"],"date":"D"}).to_string(),
        );
        write(&root.join(entity).join("windows").join(bundle), "AB");

        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["version"], "v41");
        // "D" is not an RFC3339 instant -> buildDate normalizes to the manifest
        // file mtime (matches ab-registry reading the same file).
        let bd = v["assets"]["windows"]["buildDate"].as_str().unwrap();
        assert!(bd.ends_with('Z') && bd.contains('T'), "mtime ISO, got {bd:?}");
        // other platforms: no manifest -> unavailable
        assert_eq!(v["assets"]["mac"]["version"], "");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_date_rfc3339_forwarded_verbatim() {
        // A manifest carrying a real instant is forwarded byte-for-byte.
        let root = tmp("bd");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkEntity";
        let bundle = "QmHash_windows";
        let instant = "2024-01-02T03:04:05.678Z";
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v41","files":[bundle,"dcl"],"date":instant}).to_string(),
        );
        write(&root.join(entity).join("windows").join(bundle), "AB");
        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["buildDate"], instant);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_present_but_bundle_missing_is_unavailable() {
        // The core invariant: a manifest with a missing bundle must NOT be
        // advertised (this is exactly the old split-index 404).
        let root = tmp("missing");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkEntity";
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v41","files":["QmHash_windows","dcl"],"date":"D"})
                .to_string(),
        );
        // (no bundle file written)
        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["version"], "");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn flat_deps_hash_bundle_resolves() {
        // v0-abgen layout: manifest lists the full deps-hash name, file is flat.
        let root = tmp("flat");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkEntity";
        let flat = "QmHash_4f53cda18c2baa0c0354bb5f9a3ecbe5_windows";
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v0-abgen","files":[flat,"dcl"],"date":"D"}).to_string(),
        );
        write(&root.join(flat), "AB");
        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["version"], "v0-abgen");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_manifest_is_unavailable() {
        let root = tmp("none");
        let _ = std::fs::remove_dir_all(&root);
        let v = entity_versions(&root, &HashMap::new(), "missing");
        assert_eq!(v["assets"]["windows"]["version"], "");
        assert_eq!(v["assets"]["linux"]["version"], "");
    }

    #[test]
    fn jit_buildable_advertises_all_platforms_even_with_nothing_on_disk() {
        // The durable model: an un-generated but buildable scene is advertised at
        // the JIT version on every platform, so a fetch triggers JIT-on-miss
        // instead of the client never asking.
        let root = tmp("jit");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkScene", &["model.glb", "scene.json"]);
        let v = entity_versions_jit(&root, &HashMap::new(), &s.entity_id, is_buildable(&s.content), "v41");
        assert_eq!(v["assets"]["windows"]["version"], "v41");
        assert_eq!(v["assets"]["mac"]["version"], "v41");
        // buildDate is the deterministic provenance, stable across the JIT build.
        assert_eq!(
            v["assets"]["webgl"]["buildDate"],
            abgen::manifest::provenance("bafkScene")
        );
        assert!(!versions_empty(&v));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn jit_corpus_hit_keeps_real_version_others_jit() {
        // On-disk windows -> its real manifest version; the rest -> JIT version.
        let root = tmp("jithit");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkScene";
        let bundle = "QmModel.glb_windows";
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v40-real","files":[bundle,"dcl"],"date":"D"})
                .to_string(),
        );
        write(&root.join(entity).join("windows").join(bundle), "AB");
        let s = scene(entity, &["model.glb"]);
        let v = entity_versions_jit(&root, &HashMap::new(), &s.entity_id, is_buildable(&s.content), "v41");
        assert_eq!(v["assets"]["windows"]["version"], "v40-real"); // corpus hit
        assert_eq!(v["assets"]["mac"]["version"], "v41"); // JIT on miss
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_buildable_entity_is_empty() {
        let root = tmp("nob");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkData", &["scene.json", "main.crdt"]);
        assert!(!is_buildable(&s.content));
        let v = entity_versions_jit(&root, &HashMap::new(), &s.entity_id, is_buildable(&s.content), "v41");
        assert!(versions_empty(&v));
    }

    #[test]
    fn ab_record_full_wire_shape() {
        // EntityVersions parity: buildable entity -> versions + bundles + status,
        // all platforms complete (servable via hit-or-JIT).
        let root = tmp("rec");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkScene", &["model.glb"]);
        let rec = entity_ab_record(&root, &HashMap::new(), &s.entity_id, is_buildable(&s.content), "v41");
        let (versions, bundles, status) = rec.expect("buildable -> Some");
        assert_eq!(versions["assets"]["windows"]["version"], "v41");
        assert_eq!(bundles["assets"]["windows"], "complete");
        assert_eq!(bundles["assets"]["linux"], "complete");
        assert_eq!(status, "complete");
        // non-buildable -> None (skipped, like the registry).
        let d = scene("bafkData", &["scene.json"]);
        assert!(entity_ab_record(&root, &HashMap::new(), &d.entity_id, is_buildable(&d.content), "v41").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
