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
}

impl Versions {
    pub fn is_empty(&self) -> bool {
        self.assets.windows.version.is_empty()
            && self.assets.mac.version.is_empty()
            && self.assets.webgl.version.is_empty()
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
}

impl Default for BundlePlatformStatuses {
    fn default() -> Self {
        Self {
            windows: BuildStatus::Pending,
            mac: BuildStatus::Pending,
            webgl: BuildStatus::Pending,
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
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct QueuesStatus {
    #[serde(rename = "windowsPendingJobs")]
    pub windows_pending_jobs: Vec<String>,
    #[serde(rename = "macPendingJobs")]
    pub mac_pending_jobs: Vec<String>,
    #[serde(rename = "webglPendingJobs")]
    pub webgl_pending_jobs: Vec<String>,
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
