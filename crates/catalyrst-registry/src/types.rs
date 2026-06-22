use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistryStatus {
    Complete,
    Fallback,
    Pending,
    Failed,
}

impl RegistryStatus {
    pub fn is_servable(self) -> bool {
        matches!(self, RegistryStatus::Complete | RegistryStatus::Fallback)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlatformVersion {
    pub version: String,
    #[serde(rename = "buildDate")]
    pub build_date: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Versions {
    pub assets: PlatformVersions,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlatformVersions {
    pub windows: PlatformVersion,
    pub mac: PlatformVersion,
    pub webgl: PlatformVersion,
    pub linux: PlatformVersion,
}

impl Versions {
    pub fn is_empty(&self) -> bool {
        self.assets.windows.version.is_empty()
            && self.assets.mac.version.is_empty()
            && self.assets.webgl.version.is_empty()
            && self.assets.linux.version.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Bundles {
    pub assets: BundlePlatformStatuses,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lods: Option<BundlePlatformStatuses>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundlePlatformStatuses {
    pub windows: BuildStatus,
    pub mac: BuildStatus,
    pub webgl: BuildStatus,
    pub linux: BuildStatus,
}

impl Default for BundlePlatformStatuses {
    fn default() -> Self {
        Self {
            windows: BuildStatus::Pending,
            mac: BuildStatus::Pending,
            webgl: BuildStatus::Pending,
            linux: BuildStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentFile {
    pub file: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbEntity {
    pub id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub timestamp: i64,
    pub pointers: Vec<String>,
    pub content: Vec<ContentFile>,
    pub metadata: Value,
    pub deployer: String,
    pub status: RegistryStatus,
    pub bundles: Bundles,
    pub versions: Versions,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityVersions {
    pub pointers: Vec<String>,
    pub versions: Versions,
    pub bundles: Bundles,
    pub status: RegistryStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityStatus {
    #[serde(rename = "entityId")]
    pub entity_id: String,
    pub catalyst: BuildStatus,
    pub complete: bool,
    #[serde(rename = "assetBundles")]
    pub asset_bundles: PlatformStatuses,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lods: Option<PlatformStatuses>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformStatuses {
    pub mac: BuildStatus,
    pub windows: BuildStatus,
    pub linux: BuildStatus,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct QueuesStatus {
    #[serde(rename = "windowsPendingJobs")]
    pub windows_pending_jobs: Vec<String>,
    #[serde(rename = "macPendingJobs")]
    pub mac_pending_jobs: Vec<String>,
    #[serde(rename = "webglPendingJobs")]
    pub webgl_pending_jobs: Vec<String>,
    #[serde(rename = "linuxPendingJobs")]
    pub linux_pending_jobs: Vec<String>,
    /// Operator queue-pause flag. Only populated for authenticated admin
    /// callers; omitted from the public/scene-facing response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldManifest {
    pub occupied: Vec<String>,
    pub spawn_coordinate: Coordinate,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Coordinate {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactProfile {
    pub pointer: String,
    #[serde(rename = "hasClaimedName")]
    pub has_claimed_name: bool,
    pub name: String,
    #[serde(rename = "nameColor", skip_serializing_if = "Option::is_none")]
    pub name_color: Option<Value>,
    #[serde(rename = "thumbnailUrl")]
    pub thumbnail_url: String,
}

#[derive(Debug, Deserialize)]
pub struct PointersBody {
    #[serde(default)]
    pub pointers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct IdsBody {
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorldNameQuery {
    pub world_name: Option<String>,
}

pub const MAX_POINTERS: usize = 200;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn populated_versions() -> Versions {
        let leaf = |v: &str, d: &str| PlatformVersion {
            version: v.to_string(),
            build_date: d.to_string(),
        };
        Versions {
            assets: PlatformVersions {
                windows: leaf("v41", "2024-03-15T12:34:56.789Z"),
                mac: leaf("v41", "2024-03-15T12:34:56.789Z"),
                webgl: leaf("v41", "2024-03-15T12:34:56.789Z"),
                linux: leaf("v41", "2024-03-15T12:34:56.789Z"),
            },
        }
    }

    // The exact `versions` envelope upstream's `createRegistryEntity` fixture
    // emits (`test/utils.ts:174-180`): the Unity client deserializes it as
    // `versions.assets.<platform>.{version, buildDate}` (camelCase). A flat
    // shape here drops the AB version pin and breaks the asset-bundle path.
    #[test]
    fn versions_serializes_nested_per_platform() {
        let got = serde_json::to_value(populated_versions()).unwrap();
        let leaf = json!({ "version": "v41", "buildDate": "2024-03-15T12:34:56.789Z" });
        assert_eq!(
            got,
            json!({ "assets": { "windows": leaf, "mac": leaf, "webgl": leaf, "linux": leaf } })
        );
    }

    // Default (unbuilt) entity: empty leaf strings, still fully nested — matches
    // upstream `createRegistryEntity` (`{ version: '', buildDate: '' }`).
    #[test]
    fn default_versions_keeps_nested_envelope() {
        let got = serde_json::to_value(Versions::default()).unwrap();
        let empty = json!({ "version": "", "buildDate": "" });
        assert_eq!(
            got,
            json!({ "assets": { "windows": empty, "mac": empty, "webgl": empty, "linux": empty } })
        );
    }

    // `/entities/versions` response item. Upstream returns each entity as
    // `{ pointers, versions, bundles, status }` (`get-entity-versions.ts:30-36`,
    // wire-checked in `entities-endpoints.spec.ts:27-32`).
    #[test]
    fn entity_versions_wire_shape_matches_upstream() {
        let item = EntityVersions {
            pointers: vec!["1000,1000".to_string()],
            versions: populated_versions(),
            bundles: Bundles {
                assets: BundlePlatformStatuses {
                    windows: BuildStatus::Complete,
                    mac: BuildStatus::Complete,
                    webgl: BuildStatus::Complete,
                    linux: BuildStatus::Complete,
                },
                lods: None,
            },
            status: RegistryStatus::Complete,
        };
        let got = serde_json::to_value(item).unwrap();
        let leaf = json!({ "version": "v41", "buildDate": "2024-03-15T12:34:56.789Z" });
        assert_eq!(
            got,
            json!({
                "pointers": ["1000,1000"],
                "versions": { "assets": { "windows": leaf, "mac": leaf, "webgl": leaf, "linux": leaf } },
                "bundles": { "assets": { "windows": "complete", "mac": "complete", "webgl": "complete", "linux": "complete" } },
                "status": "complete"
            })
        );
    }

    // `/entities/active` response item carries the same nested `versions`
    // envelope as a sibling of `bundles` (`Registry.DbEntity`, `types/types.ts:73-77`).
    #[test]
    fn db_entity_carries_nested_versions() {
        let entity = DbEntity {
            id: "bafkrei".to_string(),
            entity_type: "scene".to_string(),
            timestamp: 0,
            pointers: vec!["1000,1000".to_string()],
            content: vec![],
            metadata: json!({}),
            deployer: "0xabc".to_string(),
            status: RegistryStatus::Complete,
            bundles: Bundles::default(),
            versions: populated_versions(),
        };
        let got = serde_json::to_value(entity).unwrap();
        let leaf = json!({ "version": "v41", "buildDate": "2024-03-15T12:34:56.789Z" });
        assert_eq!(
            got["versions"],
            json!({ "assets": { "windows": leaf, "mac": leaf, "webgl": leaf, "linux": leaf } })
        );
        assert_eq!(got["type"], json!("scene"));
    }
}
