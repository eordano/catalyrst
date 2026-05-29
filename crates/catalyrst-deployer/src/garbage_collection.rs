use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use tracing::{error, info};

use crate::{BackendError, ContentHash, DatabaseBackend, Pointer, StorageBackend};

#[derive(Debug, Clone)]
pub struct GarbageCollectionConfig {
    pub enabled: bool,
    pub sweep_interval: Duration,
    pub profile_max_age: Duration,
}

impl Default for GarbageCollectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sweep_interval: Duration::from_secs(6 * 60 * 60),
            profile_max_age: Duration::from_secs(3650 * 24 * 60 * 60),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GcStaleProfilesResult {
    pub deleted_hashes: Vec<ContentHash>,
    pub deleted_deployment_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct SweepResult {
    pub gc_profile_active_entities: Vec<Pointer>,
    pub gc_unused_hashes: Vec<ContentHash>,
    pub gc_stale_profiles: Option<GcStaleProfilesResult>,
}

pub struct GarbageCollectionManager {
    database: Arc<dyn DatabaseBackend>,
    storage: Arc<dyn StorageBackend>,
    config: GarbageCollectionConfig,
    last_gc_time: Mutex<i64>,
    last_sweep_result: Mutex<Option<SweepResult>>,
}

impl GarbageCollectionManager {
    pub fn new(
        database: Arc<dyn DatabaseBackend>,
        storage: Arc<dyn StorageBackend>,
        config: GarbageCollectionConfig,
    ) -> Self {
        Self {
            database,
            storage,
            config,
            last_gc_time: Mutex::new(0),
            last_sweep_result: Mutex::new(None),
        }
    }

    pub async fn start(&self) -> Result<(), BackendError> {
        let stored = self.database.get_last_gc_time().await?.unwrap_or(0);
        *self.last_gc_time.lock() = stored;

        self.perform_sweep().await;
        Ok(())
    }

    pub async fn perform_sweep(&self) {
        let old_profile_since = Utc::now() - self.config.profile_max_age;
        let mut result = SweepResult::default();

        match self.gc_profile_active_entities(old_profile_since).await {
            Ok(pointers) => result.gc_profile_active_entities = pointers,
            Err(e) => {
                error!(error = %e, "failed to clean up stale profile active pointers");
                *self.last_sweep_result.lock() = Some(result);
                return;
            }
        }

        if !self.config.enabled {
            *self.last_sweep_result.lock() = Some(result);
            return;
        }

        let new_gc_time = Utc::now().timestamp_millis();

        match self.gc_unused_hashes().await {
            Ok(hashes) => result.gc_unused_hashes = hashes,
            Err(e) => {
                error!(error = %e, "failed to GC unused hashes");
            }
        }

        match self.gc_stale_profiles(old_profile_since).await {
            Ok(r) => result.gc_stale_profiles = Some(r),
            Err(e) => {
                error!(error = %e, "failed to GC stale profiles");
            }
        }

        if let Err(e) = self.database.set_last_gc_time(new_gc_time).await {
            error!(error = %e, "failed to persist GC timestamp");
        } else {
            *self.last_gc_time.lock() = new_gc_time;
        }

        *self.last_sweep_result.lock() = Some(result);
    }

    pub fn last_sweep_result(&self) -> Option<SweepResult> {
        self.last_sweep_result.lock().clone()
    }

    async fn gc_profile_active_entities(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<Vec<Pointer>, BackendError> {
        info!("running stale profile active-pointer cleanup");
        let pointers = self.database.gc_profile_active_pointers(older_than).await?;
        info!(count = pointers.len(), "stale profile active pointers cleared");
        Ok(pointers)
    }

    async fn gc_unused_hashes(&self) -> Result<Vec<ContentHash>, BackendError> {
        let since = {
            let ts = *self.last_gc_time.lock();
            DateTime::from_timestamp_millis(ts).unwrap_or(DateTime::<Utc>::MIN_UTC)
        };

        let hashes = self
            .database
            .find_unreferenced_content_hashes(since)
            .await?;

        if !hashes.is_empty() {
            info!(count = hashes.len(), "deleting unreferenced content hashes");
            self.storage.delete(&hashes).await?;
        }

        Ok(hashes)
    }

    async fn gc_stale_profiles(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<GcStaleProfilesResult, BackendError> {
        let result = self.database.gc_stale_profiles(older_than).await?;

        if !result.deleted_hashes.is_empty() {
            info!(
                count = result.deleted_hashes.len(),
                "deleting content files for stale profiles"
            );
            self.storage.delete(&result.deleted_hashes).await?;
        }

        Ok(GcStaleProfilesResult {
            deleted_hashes: result.deleted_hashes,
            deleted_deployment_ids: result.deleted_deployment_ids,
        })
    }
}

pub async fn delete_unreferenced_files(
    database: &dyn DatabaseBackend,
    storage: &dyn StorageBackend,
) -> Result<usize, BackendError> {
    info!("building set of all referenced hashes");

    let all_files = storage.all_file_ids().await?;

    let since = DateTime::<Utc>::MIN_UTC;
    let unreferenced = database
        .find_unreferenced_content_hashes(since)
        .await?;

    let unreferenced_set: std::collections::HashSet<&str> =
        unreferenced.iter().map(|h| h.as_str()).collect();

    let to_delete: Vec<ContentHash> = all_files
        .into_iter()
        .filter(|f| unreferenced_set.contains(f.as_str()))
        .collect();

    let count = to_delete.len();
    if count > 0 {
        info!(count, "deleting unreferenced files");
        storage.delete(&to_delete).await?;
    }

    info!(count, "unreferenced files deleted");
    Ok(count)
}
