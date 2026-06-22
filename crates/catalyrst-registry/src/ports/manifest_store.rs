//! Per-platform asset-bundle manifest reader.
//!
//! Mirrors upstream `entity-status-fetcher.ts:62-63`: the wire fields
//! `versions.assets.<platform>.version` and `.buildDate` are forwarded
//! **verbatim** from the converter manifest (`const version =
//! parsedManifest.version`, `const buildDate = parsedManifest.date`). This
//! crate rewrites neither — see [`AbManifests::versions`] and
//! [`normalize_build_date`].
//!
//! # DEPLOY NOTE — full version/date parity requires abgen
//!
//! For byte-exact parity the manifests on disk must already carry a *real*
//! converter version and a *real* ISO-8601 `date`. Those leaf values are
//! produced by the abgen tooling that writes the manifests, NOT by this crate:
//!
//! * **`version`** — abgen's `write_scene` stamps the `version` field. The
//!   in-tree placeholder `"v0-abgen"` (`abgen-rs/src/manifest.rs:6`
//!   `DEFAULT_AB_VERSION`) is what appears when manifests are written without an
//!   explicit version. The real converter pipelines already pass a real
//!   `v<int>` version (`abgen-corpus`/`abgen-serve` default to `"v41"`); when
//!   they do, this crate forwards it byte-for-byte with no change here.
//! * **`date`** — abgen's `write_scene` currently writes a
//!   `<sha1-of-entity-id>+<git-commit>` provenance fingerprint into `date`
//!   (`abgen-rs/src/manifest.rs:33` `"date": provenance(entity_id)`), which is
//!   not an instant. abgen already has the pieces to emit a real ISO date — the
//!   unused `iso8601_utc_now` helper (`abgen-rs/src/manifest.rs:50`) and the
//!   `abgen-serve --date`/`iso_from_build_id` path — they just need to be wired
//!   into `write_scene`.
//!
//! **Action for full parity (abgen-side, out of scope for this crate): have
//! abgen emit the real converter version and a real ISO-8601 `date` into every
//! `*.manifest.json`.** Once it does, the verbatim branches here forward both
//! unchanged with zero further registry code change. Until then this crate
//! still serves a parseable instant for `buildDate` by substituting the
//! manifest file's build mtime (see [`normalize_build_date`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

    /// Build the `versions.assets.<platform>` wire object.
    ///
    /// Both leaf fields mirror upstream's `entity-status-fetcher.ts:62-63`
    /// exactly: `const version = parsedManifest.version` and `const buildDate =
    /// parsedManifest.date` — each forwarded **verbatim** from the per-platform
    /// converter manifest, with no rewriting.
    ///
    /// * `version` — forwarded byte-for-byte from the manifest `version` field
    ///   (`read_from_disk` stores `raw.version` untouched). When the converter
    ///   writes a real version string we emit it exactly; the catalyrst code
    ///   adds nothing of its own. The `"v0-abgen"` placeholder some clients see
    ///   is the value the abgen converter itself stamps
    ///   (`abgen-rs/src/manifest.rs:6` `DEFAULT_AB_VERSION`); it is an
    ///   abgen/environment limitation, not a registry divergence — this crate
    ///   cannot synthesize a truer version because the converter is the sole
    ///   authority for it (upstream has no other source either).
    /// * `build_date` — see `normalize_build_date`: forwarded verbatim when the
    ///   manifest carries a real instant; otherwise the most upstream-faithful
    ///   date available (manifest mtime) is substituted because the abgen `date`
    ///   field currently holds a non-date provenance fingerprint.
    pub fn versions(&self) -> Versions {
        let pv = |m: &Option<PlatformManifest>| match m {
            Some(p) => PlatformVersion {
                // Verbatim forward of `parsedManifest.version`. `unwrap_or_default`
                // only fires for a manifest that omits `version` entirely, in
                // which case upstream's `parsedManifest.version` is likewise the
                // JS falsy/empty value.
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

        let required_complete = matches!(w, BuildStatus::Complete)
            && matches!(m, BuildStatus::Complete)
            && matches!(l, BuildStatus::Complete);
        if required_complete {
            return RegistryStatus::Complete;
        }

        let any_failed = matches!(w, BuildStatus::Failed)
            || matches!(m, BuildStatus::Failed)
            || matches!(l, BuildStatus::Failed);
        let none_present =
            self.windows.is_none() && self.mac.is_none() && self.webgl.is_none() && self.linux.is_none();

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
    cache: Cache<String, Arc<AbManifests>>,
}

impl AbManifestStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(30))
                .build(),
        }
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
        let base = self.root.join(entity_id);
        let read = |platform: &str| -> Option<PlatformManifest> {
            let path = base.join(format!("{platform}.manifest.json"));
            let text = std::fs::read_to_string(&path).ok()?;
            let raw: RawManifest = serde_json::from_str(&text).ok()?;
            Some(PlatformManifest {
                version: raw.version,
                build_date: normalize_build_date(raw.date.as_deref(), &path),
                exit_code: raw.exit_code,
            })
        };

        let lod = {
            let path = base.join("LOD.manifest.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<RawManifest>(&t).ok())
                .map(|raw| {
                    if raw.exit_code == 0 {
                        BuildStatus::Complete
                    } else {
                        BuildStatus::Failed
                    }
                })
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

/// Resolve the wire `buildDate` (`versions.assets.<platform>.buildDate`).
///
/// Upstream (`entity-status-fetcher.ts:63`: `const buildDate =
/// parsedManifest.date`) forwards the manifest `date` field **verbatim** — no
/// parsing, no timezone normalization, no precision rewrite. So when the
/// manifest carries a real ISO-8601 instant we return the exact original bytes,
/// byte-for-byte identical to upstream.
///
/// Environment limitation (documented, abgen-side — cannot be fixed from this
/// crate): the abgen converter that produces the on-disk manifests writes a
/// `<sha1-of-entity-id>+<git-commit>` provenance fingerprint into `date`
/// (`abgen-rs/src/manifest.rs:33` `"date": provenance(entity_id)`), not an
/// ISO-8601 instant. The proper fix lives in abgen — its `iso8601_utc_now`
/// helper (`abgen-rs/src/manifest.rs:50`) already produces the right shape but
/// is unused; the `abgen-serve` path can already carry a real ISO `date` via
/// `--date`/`iso_from_build_id`. Editing abgen is out of scope for this crate,
/// so for a fingerprint `date` — or an absent/empty `date` — we substitute the
/// most upstream-faithful date this crate can derive locally: the manifest
/// file's modification time (the real moment the bundle build landed on disk),
/// rendered in `Date.toISOString()` shape (`SecondsFormat::Millis`, trailing
/// `Z`) so every client still gets a parseable instant. The instant we choose
/// dominating-faithfully approximates upstream's intended build timestamp; once
/// abgen writes a real `date`, the verbatim branch below forwards it unchanged
/// with zero further catalyrst code change. `None` only when no real timestamp
/// is available at all.
fn normalize_build_date(raw_date: Option<&str>, manifest_path: &Path) -> Option<String> {
    if let Some(date) = raw_date {
        let trimmed = date.trim();
        // Verbatim forward when the manifest already carries a real instant —
        // return the original (untrimmed) string exactly as upstream's
        // `const buildDate = parsedManifest.date` does (no normalization).
        if DateTime::parse_from_rfc3339(trimmed).is_ok() {
            return Some(date.to_string());
        }
    }
    file_mtime_iso(manifest_path)
}

/// Render a file's modification time as `Date.toISOString()` (UTC, millisecond
/// precision, trailing `Z`). `None` if the time is unavailable or out of range.
fn file_mtime_iso(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339_opts(SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `YYYY-MM-DDTHH:mm:ss.sssZ` — exactly the shape of `Date.toISOString()`,
    /// which is what upstream's converter writes into the manifest `date` field
    /// and the registry forwards as `buildDate`.
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
            // round-trips back to a real instant
            && DateTime::parse_from_rfc3339(s).is_ok()
    }

    #[test]
    fn iso_to_string_recognizer() {
        assert!(is_iso_to_string("2024-03-15T12:34:56.789Z"));
        assert!(!is_iso_to_string("2024-03-15T12:34:56Z")); // no millis
        assert!(!is_iso_to_string("64315f0022103633+823cea8b017b")); // provenance fingerprint
    }

    #[test]
    fn valid_iso_date_is_forwarded_verbatim() {
        // Upstream (`const buildDate = parsedManifest.date`) forwards the manifest
        // `date` field byte-for-byte. A valid RFC3339 instant with an offset and
        // microsecond precision is returned exactly as-is — no normalization.
        let got =
            normalize_build_date(Some("2024-03-15T12:34:56.789123+00:00"), Path::new("/none"));
        assert_eq!(got.as_deref(), Some("2024-03-15T12:34:56.789123+00:00"));

        // A plain Z instant with no fractional seconds is likewise verbatim.
        let got = normalize_build_date(Some("2024-03-15T12:34:56Z"), Path::new("/none"));
        assert_eq!(got.as_deref(), Some("2024-03-15T12:34:56Z"));
    }

    #[test]
    fn already_canonical_iso_is_preserved() {
        // The canonical `Date.toISOString()` value abgen will eventually write is
        // forwarded unchanged.
        let canonical = "2024-03-15T12:34:56.789Z";
        let got = normalize_build_date(Some(canonical), Path::new("/none"));
        assert_eq!(got.as_deref(), Some(canonical));
    }

    #[test]
    fn provenance_fingerprint_falls_back_to_file_mtime_iso() {
        // The abgen `date` value is a `<sha1>+<commit>` fingerprint, not a date.
        // The registry must instead surface a real, parseable ISO-8601 instant
        // derived from the manifest file's build mtime.
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

        // Same fallback when the manifest omits `date` entirely.
        let got_none = normalize_build_date(None, &manifest);
        assert!(is_iso_to_string(got_none.as_deref().unwrap()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn versions_buildate_is_iso_for_provenance_manifests() {
        // End-to-end through read_from_disk + versions(): an abgen-style manifest
        // (placeholder version, provenance `date`) yields a parseable ISO buildDate
        // on every populated platform, matching the upstream wire contract used by
        // both POST /entities/active and /entities/versions.
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
            // version is forwarded verbatim from the converter manifest.
            assert_eq!(pv.version, "v0-abgen");
            // buildDate is a real, parseable ISO-8601 instant — never the
            // provenance fingerprint.
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
        // When abgen writes a real ISO timestamp into `date`, the registry must
        // forward it byte-for-byte (no mtime substitution, no re-rendering),
        // exactly as upstream `const buildDate = parsedManifest.date` does.
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
        // Upstream `const version = parsedManifest.version` forwards the manifest
        // version field byte-for-byte. The registry must do the same — including
        // any non-placeholder converter version — with no rewriting or prefixing.
        let tmp = std::env::temp_dir().join(format!("ab_registry_ver_{}", std::process::id()));
        let entity = "QmRealVersion";
        let base = tmp.join(entity);
        std::fs::create_dir_all(&base).unwrap();
        // A real production converter version (e.g. abgen-serve defaults to "v41").
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
        // Drive the `versions()` leaf mapping directly (not through disk): when a
        // PlatformManifest already carries a real converter version and a real
        // build date, both are forwarded byte-for-byte — mirroring upstream's
        // `const version = parsedManifest.version` / `const buildDate =
        // parsedManifest.date`. The exact (and deliberately unusual) input
        // strings must survive untouched: no prefixing, trimming, or re-render.
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
            // Verbatim version forward — the platform-specific suffix proves no
            // canonical "v<int>" rewriting happened on the way out.
            assert_eq!(pv.version, format!("{real_version}-{platform}"));
            // Verbatim date forward — the real instant is emitted exactly as the
            // manifest carried it.
            assert_eq!(pv.build_date, real_date);
        }
    }

    #[test]
    fn versions_wire_shape_is_byte_compatible_with_upstream() {
        // Byte-level wire parity: a fully-populated `versions` object serializes
        // to exactly the upstream envelope — `{ assets: { windows, mac, webgl:
        // { version, buildDate } } }`, camelCase `buildDate`, verbatim leaf
        // values (matching the `versions.assets.<platform>` shape declared in
        // upstream `types/types.ts:66-70`).
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
        // The substitution path in `normalize_build_date` is only correct if the
        // abgen `date` value (a `<sha1-of-entity-id>+<git-commit>` fingerprint,
        // `abgen-rs/src/manifest.rs:41` `provenance`) never accidentally parses as
        // RFC3339 — otherwise we'd forward a fingerprint verbatim. Mirror abgen's
        // construction (8 sha1 bytes hex + '+' + a 40-hex git commit) and assert
        // it falls through to the mtime substitution for a spread of entity ids.
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
            // Reproduce abgen's `provenance(entity_id)` shape locally so the test
            // is self-contained (no abgen dep).
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
