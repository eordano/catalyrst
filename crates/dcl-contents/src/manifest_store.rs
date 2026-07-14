use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

fn required_platforms() -> &'static [String] {
    static REQ: OnceLock<Vec<String>> = OnceLock::new();
    REQ.get_or_init(|| {
        std::env::var("AB_REGISTRY_REQUIRED_PLATFORMS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_lowercase())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| vec!["windows".to_string(), "mac".to_string()])
    })
}

use chrono::{DateTime, SecondsFormat, Utc};
use moka::future::Cache;
use serde::Deserialize;

use crate::types::{
    BuildStatus, BundlePlatformStatuses, Bundles, PlatformVersion, PlatformVersions,
    RegistryStatus, Versions,
};

#[derive(Debug, Clone, Deserialize)]
struct RawManifest {
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "exitCode", default)]
    exit_code: i32,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AbManifests {
    pub windows: Option<PlatformManifest>,
    pub mac: Option<PlatformManifest>,
    pub webgl: Option<PlatformManifest>,
    pub linux: Option<PlatformManifest>,
    pub lods: PlatformLods,
}

#[derive(Debug, Clone, Default)]
pub struct PlatformLods {
    pub mac: Option<BuildStatus>,
    pub windows: Option<BuildStatus>,
}

#[derive(Debug, Clone)]
pub struct PlatformManifest {
    pub version: Option<String>,
    pub build_date: Option<String>,
    pub exit_code: i32,
}

impl PlatformManifest {
    pub fn status(&self) -> BuildStatus {
        if self.exit_code == 0 {
            BuildStatus::Complete
        } else {
            BuildStatus::Failed
        }
    }
}

impl AbManifests {
    fn platform_status(m: &Option<PlatformManifest>) -> BuildStatus {
        match m {
            None => BuildStatus::Pending,
            Some(m) => m.status(),
        }
    }

    pub fn windows_status(&self) -> BuildStatus {
        Self::platform_status(&self.windows)
    }
    pub fn mac_status(&self) -> BuildStatus {
        Self::platform_status(&self.mac)
    }
    pub fn webgl_status(&self) -> BuildStatus {
        Self::platform_status(&self.webgl)
    }
    pub fn linux_status(&self) -> BuildStatus {
        Self::platform_status(&self.linux)
    }

    pub fn bundles(&self, is_world: bool) -> Bundles {
        let lods = if is_world {
            None
        } else if self.lods.mac.is_some() || self.lods.windows.is_some() {
            Some(BundlePlatformStatuses {
                windows: self.lods.windows.unwrap_or(BuildStatus::Pending),
                mac: self.lods.mac.unwrap_or(BuildStatus::Pending),
                webgl: self
                    .lods
                    .mac
                    .or(self.lods.windows)
                    .unwrap_or(BuildStatus::Pending),
                linux: self
                    .lods
                    .windows
                    .or(self.lods.mac)
                    .unwrap_or(BuildStatus::Pending),
            })
        } else {
            None
        };
        Bundles {
            assets: BundlePlatformStatuses {
                windows: self.windows_status(),
                mac: self.mac_status(),
                webgl: self.webgl_status(),
                linux: self.linux_status(),
            },
            lods,
        }
    }

    pub fn versions(&self) -> Versions {
        let pv = |m: &Option<PlatformManifest>| match m {
            Some(p) => PlatformVersion {
                version: p.version.clone().unwrap_or_default(),
                build_date: p.build_date.clone().unwrap_or_default(),
            },
            None => PlatformVersion::default(),
        };
        Versions {
            assets: PlatformVersions {
                windows: pv(&self.windows),
                mac: pv(&self.mac),
                webgl: pv(&self.webgl),
                linux: pv(&self.linux),
            },
        }
    }

    pub fn registry_status(&self, has_content: bool) -> RegistryStatus {
        let w = self.windows_status();
        let m = self.mac_status();
        let l = self.linux_status();

        let req = required_platforms();
        let ok = |name: &str, s: &BuildStatus| {
            !req.iter().any(|p| p == name) || matches!(s, BuildStatus::Complete)
        };
        let required_complete = ok("windows", &w) && ok("mac", &m) && ok("linux", &l);
        if required_complete {
            return RegistryStatus::Complete;
        }

        let any_failed = matches!(w, BuildStatus::Failed)
            || matches!(m, BuildStatus::Failed)
            || matches!(l, BuildStatus::Failed);
        let none_present = self.windows.is_none()
            && self.mac.is_none()
            && self.webgl.is_none()
            && self.linux.is_none();

        if has_content {
            if any_failed && none_present {
                RegistryStatus::Failed
            } else {
                RegistryStatus::Fallback
            }
        } else if none_present {
            RegistryStatus::Pending
        } else if any_failed {
            RegistryStatus::Failed
        } else {
            RegistryStatus::Pending
        }
    }
}

#[derive(Clone)]
pub struct AbManifestStore {
    root: PathBuf,
    fallback: Option<PathBuf>,
    cache: Cache<String, Arc<AbManifests>>,
}

impl AbManifestStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            fallback: None,
            cache: Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(30))
                .build(),
        }
    }

    pub fn with_fallback_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.fallback = Some(root.into());
        self
    }

    fn roots(&self) -> impl Iterator<Item = &PathBuf> {
        std::iter::once(&self.root).chain(self.fallback.iter())
    }

    pub async fn get(&self, entity_id: &str) -> Arc<AbManifests> {
        if let Some(hit) = self.cache.get(entity_id).await {
            return hit;
        }
        let m = Arc::new(self.read_from_disk(entity_id));
        self.cache.insert(entity_id.to_string(), m.clone()).await;
        m
    }

    fn read_from_disk(&self, entity_id: &str) -> AbManifests {
        if !valid_entity_id(entity_id) {
            return AbManifests::default();
        }
        let read = |platform: &str| -> Option<PlatformManifest> {
            let mut first: Option<PlatformManifest> = None;
            for root in self.roots() {
                let path = root
                    .join(entity_id)
                    .join(format!("{platform}.manifest.json"));
                let Some(raw) = parse_raw_manifest(&path) else {
                    continue;
                };
                let m = PlatformManifest {
                    version: raw.version,
                    build_date: normalize_build_date(raw.date.as_deref(), &path),
                    exit_code: raw.exit_code,
                };
                if m.exit_code == 0 {
                    return Some(m);
                }
                first.get_or_insert(m);
            }
            first
        };

        let lod = {
            let mut first: Option<BuildStatus> = None;
            for root in self.roots() {
                let path = root.join(entity_id).join("LOD.manifest.json");
                let Some(raw) = parse_raw_manifest(&path) else {
                    continue;
                };
                if raw.exit_code == 0 {
                    first = Some(BuildStatus::Complete);
                    break;
                }
                first.get_or_insert(BuildStatus::Failed);
            }
            first
        };

        AbManifests {
            windows: read("windows"),
            mac: read("mac"),
            webgl: read("webgl"),
            linux: read("linux"),
            lods: PlatformLods {
                mac: lod,
                windows: lod,
            },
        }
    }

    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    pub async fn invalidate(&self, entity_id: &str) {
        self.cache.invalidate(entity_id).await;
    }
}

fn valid_entity_id(id: &str) -> bool {
    (10..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn parse_raw_manifest(path: &Path) -> Option<RawManifest> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn normalize_build_date(raw_date: Option<&str>, manifest_path: &Path) -> Option<String> {
    if let Some(date) = raw_date {
        let trimmed = date.trim();

        if DateTime::parse_from_rfc3339(trimmed).is_ok() {
            return Some(date.to_string());
        }
    }
    file_mtime_iso(manifest_path)
}

fn file_mtime_iso(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339_opts(SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_iso_to_string(s: &str) -> bool {
        let b = s.as_bytes();
        if b.len() != 24 {
            return false;
        }
        let digit = |i: usize| b[i].is_ascii_digit();
        (0..4).all(digit)
            && b[4] == b'-'
            && digit(5)
            && digit(6)
            && b[7] == b'-'
            && digit(8)
            && digit(9)
            && b[10] == b'T'
            && digit(11)
            && digit(12)
            && b[13] == b':'
            && digit(14)
            && digit(15)
            && b[16] == b':'
            && digit(17)
            && digit(18)
            && b[19] == b'.'
            && digit(20)
            && digit(21)
            && digit(22)
            && b[23] == b'Z'
            && DateTime::parse_from_rfc3339(s).is_ok()
    }

    fn write_ok_manifest(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("windows.manifest.json"),
            serde_json::to_string(&serde_json::json!({
                "version": "v41",
                "exitCode": 0,
                "date": "2024-03-15T12:34:56.789Z",
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_platform_manifest(dir: &Path, platform: &str, exit_code: i32) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(format!("{platform}.manifest.json")),
            serde_json::to_string(&serde_json::json!({
                "version": "v41",
                "exitCode": exit_code,
                "date": "2024-03-15T12:34:56.789Z",
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn fallback_root_reports_jit_only_entity_complete() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_fbjit_{}", std::process::id()));
        let out_root = tmp.join("out");
        let jit_root = tmp.join("jit");
        let entity = "QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD";
        for platform in ["windows", "mac", "linux"] {
            write_platform_manifest(&jit_root.join(entity), platform, 0);
        }

        let store = AbManifestStore::new(&out_root).with_fallback_root(&jit_root);
        let m = store.read_from_disk(entity);
        assert!(matches!(m.windows_status(), BuildStatus::Complete));
        assert!(matches!(m.mac_status(), BuildStatus::Complete));
        assert!(matches!(m.linux_status(), BuildStatus::Complete));

        let single = AbManifestStore::new(&out_root);
        let sm = single.read_from_disk(entity);
        assert!(
            matches!(sm.windows_status(), BuildStatus::Pending),
            "a single-root store must not see jit_root manifests"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fallback_root_unions_platforms_across_both_roots() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_fbunion_{}", std::process::id()));
        let out_root = tmp.join("out");
        let jit_root = tmp.join("jit");
        let entity = "QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD";
        write_platform_manifest(&out_root.join(entity), "windows", 0);
        write_platform_manifest(&jit_root.join(entity), "mac", 0);
        write_platform_manifest(&jit_root.join(entity), "linux", 0);

        let store = AbManifestStore::new(&out_root).with_fallback_root(&jit_root);
        let m = store.read_from_disk(entity);
        assert!(matches!(m.windows_status(), BuildStatus::Complete));
        assert!(matches!(m.mac_status(), BuildStatus::Complete));
        assert!(matches!(m.linux_status(), BuildStatus::Complete));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fallback_root_out_root_wins_per_platform() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_fbwin_{}", std::process::id()));
        let out_root = tmp.join("out");
        let jit_root = tmp.join("jit");
        let entity = "QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD";
        write_platform_manifest(&out_root.join(entity), "windows", 0);
        write_platform_manifest(&jit_root.join(entity), "windows", 1);

        let store = AbManifestStore::new(&out_root).with_fallback_root(&jit_root);
        let m = store.read_from_disk(entity);
        assert!(
            matches!(m.windows_status(), BuildStatus::Complete),
            "out_root must win over jit_root for the same platform"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prefer_complete_across_roots_over_stale_out_root_failure() {
        let tmp =
            std::env::temp_dir().join(format!("ab_registry_prefercplt_{}", std::process::id()));
        let out_root = tmp.join("out");
        let jit_root = tmp.join("jit");
        let entity = "QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD";
        write_platform_manifest(&out_root.join(entity), "windows", 12);
        write_platform_manifest(&jit_root.join(entity), "windows", 0);

        let store = AbManifestStore::new(&out_root).with_fallback_root(&jit_root);
        let m = store.read_from_disk(entity);
        assert!(
            matches!(m.windows_status(), BuildStatus::Complete),
            "a Complete jit_root rebuild must win over a stale Failed out_root manifest"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn traversal_shaped_entity_ids_are_rejected_at_the_store_boundary() {
        let tmp =
            std::env::temp_dir().join(format!("ab_registry_traversal_{}", std::process::id()));
        let root = tmp.join("root");
        std::fs::create_dir_all(&root).unwrap();
        write_ok_manifest(&tmp.join("escape"));
        write_ok_manifest(&root.join("QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD"));

        let store = AbManifestStore::new(&root);
        let overlong = "a".repeat(129);
        let bad: Vec<&str> = vec![
            "../escape",
            "..",
            ".",
            "a/../../escape",
            "/etc/passwd",
            "foo/bar",
            "foo\\bar",
            "%2e%2e%2fescape",
            "%2e%2e/escape",
            "..%2fescape",
            "",
            "short",
            "Qm123 456789",
            "Qm1234567.",
            "urn:decentraland:entity:Qm123",
            &overlong,
        ];
        for id in bad {
            let m = store.read_from_disk(id);
            assert!(
                m.windows.is_none()
                    && m.mac.is_none()
                    && m.webgl.is_none()
                    && m.linux.is_none()
                    && m.lods.windows.is_none(),
                "id {id:?} must be rejected with an empty manifest set"
            );
            assert!(matches!(m.windows_status(), BuildStatus::Pending));
        }

        let ok = store.read_from_disk("QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD");
        assert!(
            matches!(ok.windows_status(), BuildStatus::Complete),
            "control: a valid CID under root must still resolve"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_cid_shapes_pass_the_entity_id_allowlist() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_cids_{}", std::process::id()));
        let ids = [
            "QmPAyzWU7gtdVRr9DGohiRzrSXL67NQdRuMwpfecoireUD",
            "bafkreiahsvnr4x4rnskhkwfbnbplkbqhzb3xagdwpyfy44lgcndmhyizde",
            "Qm12345678",
        ];
        for id in ids {
            assert!(valid_entity_id(id), "{id:?} must pass the allowlist");
            write_ok_manifest(&tmp.join(id));
        }
        assert!(valid_entity_id(&"a".repeat(128)));
        assert!(!valid_entity_id(&"a".repeat(129)));
        assert!(!valid_entity_id("Qm1234567"));

        let store = AbManifestStore::new(&tmp);
        for id in ids {
            let m = store.read_from_disk(id);
            assert!(
                matches!(m.windows_status(), BuildStatus::Complete),
                "{id:?} must read its manifest through the boundary"
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn iso_to_string_recognizer() {
        assert!(is_iso_to_string("2024-03-15T12:34:56.789Z"));
        assert!(!is_iso_to_string("2024-03-15T12:34:56Z"));
        assert!(!is_iso_to_string("64315f0022103633+823cea8b017b"));
    }

    #[test]
    fn valid_iso_date_is_forwarded_verbatim() {
        let got =
            normalize_build_date(Some("2024-03-15T12:34:56.789123+00:00"), Path::new("/none"));
        assert_eq!(got.as_deref(), Some("2024-03-15T12:34:56.789123+00:00"));

        let got = normalize_build_date(Some("2024-03-15T12:34:56Z"), Path::new("/none"));
        assert_eq!(got.as_deref(), Some("2024-03-15T12:34:56Z"));
    }

    #[test]
    fn already_canonical_iso_is_preserved() {
        let canonical = "2024-03-15T12:34:56.789Z";
        let got = normalize_build_date(Some(canonical), Path::new("/none"));
        assert_eq!(got.as_deref(), Some(canonical));
    }

    #[test]
    fn provenance_fingerprint_falls_back_to_file_mtime_iso() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_buildate_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let manifest = tmp.join("windows.manifest.json");
        std::fs::write(&manifest, b"{}").unwrap();

        let got = normalize_build_date(Some("64315f0022103633+823cea8b017b"), &manifest);
        let got = got.expect("mtime-derived buildDate");
        assert!(
            is_iso_to_string(&got),
            "buildDate must be a toISOString instant, got {got:?}"
        );

        let got_none = normalize_build_date(None, &manifest);
        assert!(is_iso_to_string(got_none.as_deref().unwrap()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn versions_buildate_is_iso_for_provenance_manifests() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_versions_{}", std::process::id()));
        let entity = "QmTestEntity";
        let base = tmp.join(entity);
        std::fs::create_dir_all(&base).unwrap();
        for platform in ["windows", "mac", "webgl", "linux"] {
            let body = serde_json::json!({
                "version": "v0-abgen",
                "files": ["a", "b"],
                "exitCode": 0,
                "date": "64315f0022103633+823cea8b017b",
            });
            std::fs::write(
                base.join(format!("{platform}.manifest.json")),
                serde_json::to_string(&body).unwrap(),
            )
            .unwrap();
        }

        let store = AbManifestStore::new(&tmp);
        let manifests = store.read_from_disk(entity);
        let versions = manifests.versions();

        for pv in [
            &versions.assets.windows,
            &versions.assets.mac,
            &versions.assets.webgl,
            &versions.assets.linux,
        ] {
            assert_eq!(pv.version, "v0-abgen");

            assert!(
                is_iso_to_string(&pv.build_date),
                "buildDate must be ISO-8601, got {:?}",
                pv.build_date
            );
            assert_ne!(pv.build_date, "64315f0022103633+823cea8b017b");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn versions_buildate_is_forwarded_verbatim_when_manifest_carries_iso() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_verbatim_{}", std::process::id()));
        let entity = "QmIsoEntity";
        let base = tmp.join(entity);
        std::fs::create_dir_all(&base).unwrap();
        let iso = "2024-03-15T12:34:56.789Z";
        for platform in ["windows", "mac", "webgl", "linux"] {
            let body = serde_json::json!({
                "version": "v7",
                "exitCode": 0,
                "date": iso,
            });
            std::fs::write(
                base.join(format!("{platform}.manifest.json")),
                serde_json::to_string(&body).unwrap(),
            )
            .unwrap();
        }

        let store = AbManifestStore::new(&tmp);
        let versions = store.read_from_disk(entity).versions();
        for pv in [
            &versions.assets.windows,
            &versions.assets.mac,
            &versions.assets.webgl,
            &versions.assets.linux,
        ] {
            assert_eq!(pv.build_date, iso, "buildDate must be forwarded verbatim");
            assert_eq!(pv.version, "v7");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn version_is_forwarded_verbatim_for_real_converter_version() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_ver_{}", std::process::id()));
        let entity = "QmRealVersion";
        let base = tmp.join(entity);
        std::fs::create_dir_all(&base).unwrap();

        let real_version = "v41";
        for platform in ["windows", "mac", "webgl", "linux"] {
            let body = serde_json::json!({
                "version": real_version,
                "exitCode": 0,
                "date": "2025-01-02T03:04:05.678Z",
            });
            std::fs::write(
                base.join(format!("{platform}.manifest.json")),
                serde_json::to_string(&body).unwrap(),
            )
            .unwrap();
        }

        let versions = AbManifestStore::new(&tmp).read_from_disk(entity).versions();
        for pv in [
            &versions.assets.windows,
            &versions.assets.mac,
            &versions.assets.webgl,
            &versions.assets.linux,
        ] {
            assert_eq!(pv.version, real_version, "version forwarded verbatim");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn versions_mapping_forwards_real_version_and_date_verbatim() {
        let real_version = "v41-converter.20250102";
        let real_date = "2025-01-02T03:04:05.678Z";
        let manifest = |platform: &str| {
            Some(PlatformManifest {
                version: Some(format!("{real_version}-{platform}")),
                build_date: Some(real_date.to_string()),
                exit_code: 0,
            })
        };
        let manifests = AbManifests {
            windows: manifest("windows"),
            mac: manifest("mac"),
            webgl: manifest("webgl"),
            linux: manifest("linux"),
            lods: PlatformLods::default(),
        };

        let v = manifests.versions();
        for (pv, platform) in [
            (&v.assets.windows, "windows"),
            (&v.assets.mac, "mac"),
            (&v.assets.webgl, "webgl"),
            (&v.assets.linux, "linux"),
        ] {
            assert_eq!(pv.version, format!("{real_version}-{platform}"));

            assert_eq!(pv.build_date, real_date);
        }
    }

    #[test]
    fn versions_wire_shape_is_byte_compatible_with_upstream() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_wire_{}", std::process::id()));
        let entity = "QmWireEntity";
        let base = tmp.join(entity);
        std::fs::create_dir_all(&base).unwrap();
        let iso = "2024-03-15T12:34:56.789Z";
        let version = "v41";
        for platform in ["windows", "mac", "webgl", "linux"] {
            let body = serde_json::json!({
                "version": version,
                "exitCode": 0,
                "date": iso,
            });
            std::fs::write(
                base.join(format!("{platform}.manifest.json")),
                serde_json::to_string(&body).unwrap(),
            )
            .unwrap();
        }

        let versions = AbManifestStore::new(&tmp).read_from_disk(entity).versions();
        let got: serde_json::Value = serde_json::to_value(&versions).unwrap();

        let leaf = serde_json::json!({ "version": version, "buildDate": iso });
        let expected = serde_json::json!({
            "assets": {
                "windows": leaf,
                "mac": leaf,
                "webgl": leaf,
                "linux": leaf,
            }
        });
        assert_eq!(
            got, expected,
            "versions wire shape must match upstream byte-for-byte"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn abgen_provenance_fingerprint_never_parses_as_a_date() {
        let tmp = std::env::temp_dir().join(format!("ab_registry_prov_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let manifest = tmp.join("windows.manifest.json");
        std::fs::write(&manifest, b"{}").unwrap();

        for entity in [
            "QmA",
            "scene-0-0",
            "0xdeadbeef",
            "urn:decentraland:entity:Qm123",
        ] {
            let h = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut s = DefaultHasher::new();
                entity.hash(&mut s);
                s.finish()
            };
            let fingerprint = format!("{h:016x}+0123456789abcdef0123456789abcdef01234567");
            assert!(
                DateTime::parse_from_rfc3339(&fingerprint).is_err(),
                "provenance fingerprint {fingerprint:?} must not parse as a date"
            );
            let got = normalize_build_date(Some(&fingerprint), &manifest);
            let got = got.expect("mtime substitution");
            assert!(
                is_iso_to_string(&got),
                "fingerprint must fall through to ISO mtime, got {got:?}"
            );
            assert_ne!(
                got, fingerprint,
                "fingerprint must never be forwarded verbatim"
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
