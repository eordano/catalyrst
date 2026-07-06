use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::resolver;

struct PlatformAb {
    version: String,
    build_date: String,
}

pub fn entity_versions(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
) -> serde_json::Value {
    let mut assets = serde_json::Map::new();
    for (_, platform) in resolver::PLATFORMS {
        let obj = match servable_platform(out_root, bundle_index, entity_id, platform) {
            Some(p) => serde_json::json!({ "version": p.version, "buildDate": p.build_date }),
            None => serde_json::json!({ "version": "", "buildDate": "" }),
        };
        assets.insert(platform.to_string(), obj);
    }
    serde_json::json!({ "assets": serde_json::Value::Object(assets) })
}

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

const CONVERTIBLE_EXTS: [&str; 5] = [".glb", ".gltf", ".png", ".jpg", ".jpeg"];

pub fn is_convertible(file: &str) -> bool {
    let f = file.to_ascii_lowercase();
    CONVERTIBLE_EXTS.iter().any(|e| f.ends_with(e))
}

pub fn is_buildable(content: &[crate::catalyst::ContentEntry]) -> bool {
    content.iter().any(|c| is_convertible(&c.file))
}

pub fn entity_versions_jit(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    buildable: bool,
    ab_version: &str,
    ab_date: &str,
) -> serde_json::Value {
    let mut assets = serde_json::Map::new();
    for (_, platform) in resolver::PLATFORMS {
        let obj = match servable_platform(out_root, bundle_index, entity_id, platform) {
            Some(p) => serde_json::json!({ "version": p.version, "buildDate": p.build_date }),

            None if buildable => serde_json::json!({
                "version": ab_version,
                "buildDate": ab_date,
            }),

            None => serde_json::json!({ "version": "", "buildDate": "" }),
        };
        assets.insert(platform.to_string(), obj);
    }
    serde_json::json!({ "assets": serde_json::Value::Object(assets) })
}

pub fn entity_ab_record(
    out_root: &Path,
    bundle_index: &HashMap<String, PathBuf>,
    entity_id: &str,
    buildable: bool,
    ab_version: &str,
    ab_date: &str,
) -> Option<(serde_json::Value, serde_json::Value, &'static str)> {
    let versions = entity_versions_jit(
        out_root,
        bundle_index,
        entity_id,
        buildable,
        ab_version,
        ab_date,
    );
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

    let status = if available("windows") && available("mac") && available("linux") {
        "complete"
    } else {
        "fallback"
    };
    Some((versions, bundles, status))
}

pub fn versions_empty(versions: &serde_json::Value) -> bool {
    let Some(assets) = versions.get("assets").and_then(|a| a.as_object()) else {
        return true;
    };
    assets.values().all(|p| {
        p.get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalyst::{ContentEntry, Scene};

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

        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v41","files":[bundle,"dcl"],"date":"D"}).to_string(),
        );
        write(&root.join(entity).join("windows").join(bundle), "AB");

        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["version"], "v41");

        let bd = v["assets"]["windows"]["buildDate"].as_str().unwrap();
        assert!(
            bd.ends_with('Z') && bd.contains('T'),
            "mtime ISO, got {bd:?}"
        );

        assert_eq!(v["assets"]["mac"]["version"], "");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_date_rfc3339_forwarded_verbatim() {
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
        let root = tmp("missing");
        let _ = std::fs::remove_dir_all(&root);
        let entity = "bafkEntity";
        write(
            &root.join(entity).join("windows.manifest.json"),
            &serde_json::json!({"version":"v41","files":["QmHash_windows","dcl"],"date":"D"})
                .to_string(),
        );

        let v = entity_versions(&root, &HashMap::new(), entity);
        assert_eq!(v["assets"]["windows"]["version"], "");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn flat_deps_hash_bundle_resolves() {
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

    const AB_DATE: &str = "2026-03-04T05:06:07.000Z";

    #[test]
    fn jit_buildable_advertises_all_platforms_even_with_nothing_on_disk() {
        let root = tmp("jit");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkScene", &["model.glb", "scene.json"]);
        let v = entity_versions_jit(
            &root,
            &HashMap::new(),
            &s.entity_id,
            is_buildable(&s.content),
            "v41",
            AB_DATE,
        );
        assert_eq!(v["assets"]["windows"]["version"], "v41");
        assert_eq!(v["assets"]["mac"]["version"], "v41");

        assert_eq!(v["assets"]["webgl"]["buildDate"], AB_DATE);
        assert_ne!(
            v["assets"]["webgl"]["buildDate"],
            crate::manifest::provenance("bafkScene")
        );
        assert!(!versions_empty(&v));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn jit_corpus_hit_keeps_real_version_others_jit() {
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
        let v = entity_versions_jit(
            &root,
            &HashMap::new(),
            &s.entity_id,
            is_buildable(&s.content),
            "v41",
            AB_DATE,
        );
        assert_eq!(v["assets"]["windows"]["version"], "v40-real");
        assert_eq!(v["assets"]["mac"]["version"], "v41");
        assert_eq!(v["assets"]["mac"]["buildDate"], AB_DATE);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_buildable_entity_is_empty() {
        let root = tmp("nob");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkData", &["scene.json", "main.crdt"]);
        assert!(!is_buildable(&s.content));
        let v = entity_versions_jit(
            &root,
            &HashMap::new(),
            &s.entity_id,
            is_buildable(&s.content),
            "v41",
            AB_DATE,
        );
        assert!(versions_empty(&v));
    }

    #[test]
    fn ab_record_full_wire_shape() {
        let root = tmp("rec");
        let _ = std::fs::remove_dir_all(&root);
        let s = scene("bafkScene", &["model.glb"]);
        let rec = entity_ab_record(
            &root,
            &HashMap::new(),
            &s.entity_id,
            is_buildable(&s.content),
            "v41",
            AB_DATE,
        );
        let (versions, bundles, status) = rec.expect("buildable -> Some");
        assert_eq!(versions["assets"]["windows"]["version"], "v41");
        assert_eq!(versions["assets"]["windows"]["buildDate"], AB_DATE);
        assert_eq!(bundles["assets"]["windows"], "complete");
        assert_eq!(bundles["assets"]["linux"], "complete");
        assert_eq!(status, "complete");

        let d = scene("bafkData", &["scene.json"]);
        assert!(entity_ab_record(
            &root,
            &HashMap::new(),
            &d.entity_id,
            is_buildable(&d.content),
            "v41",
            AB_DATE
        )
        .is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
