use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

    pub fn bundles(&self, is_world: bool) -> Bundles {
        let lods = if is_world {
            None
        } else if self.lods.mac.is_some() || self.lods.windows.is_some() {
            Some(BundlePlatformStatuses {
                windows: self.lods.windows.unwrap_or(BuildStatus::Pending),
                mac: self.lods.mac.unwrap_or(BuildStatus::Pending),
                webgl: self.lods.mac.or(self.lods.windows).unwrap_or(BuildStatus::Pending),
            })
        } else {
            None
        };
        Bundles {
            assets: BundlePlatformStatuses {
                windows: self.windows_status(),
                mac: self.mac_status(),
                webgl: self.webgl_status(),
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
            },
        }
    }

    pub fn registry_status(&self, has_content: bool) -> RegistryStatus {
        let w = self.windows_status();
        let m = self.mac_status();

        let required_complete = matches!(w, BuildStatus::Complete) && matches!(m, BuildStatus::Complete);
        if required_complete {
            return RegistryStatus::Complete;
        }

        let any_failed = matches!(w, BuildStatus::Failed) || matches!(m, BuildStatus::Failed);
        let none_present = self.windows.is_none() && self.mac.is_none() && self.webgl.is_none();

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
                build_date: raw.date,
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
