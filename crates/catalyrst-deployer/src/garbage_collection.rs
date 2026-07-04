use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use tracing::{error, info};

use crate::{BackendError, ContentHash, DatabaseBackend, Pointer, StorageBackend};

const GC_DELETE_BATCH_SIZE: usize = 1000;

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
            profile_max_age: Duration::from_secs(365 * 24 * 60 * 60),
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
        info!(
            count = pointers.len(),
            "stale profile active pointers cleared"
        );
        Ok(pointers)
    }

    async fn gc_unused_hashes(&self) -> Result<Vec<ContentHash>, BackendError> {
        let since = {
            let ts = *self.last_gc_time.lock();
            DateTime::from_timestamp_millis(ts).unwrap_or(DateTime::<Utc>::MIN_UTC)
        };

        let mut deleted: Vec<ContentHash> = Vec::new();
        let mut after: Option<ContentHash> = None;

        loop {
            let batch = self
                .database
                .find_unreferenced_content_hashes_batch(
                    since,
                    after.as_deref(),
                    GC_DELETE_BATCH_SIZE,
                )
                .await?;

            if batch.is_empty() {
                break;
            }

            info!(count = batch.len(), "deleting unreferenced content hashes");
            self.storage.delete(&batch).await?;

            after = batch.last().cloned();
            let was_full_batch = batch.len() == GC_DELETE_BATCH_SIZE;
            deleted.extend(batch);

            if !was_full_batch {
                break;
            }
        }

        Ok(deleted)
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
    let unreferenced = database.find_unreferenced_content_hashes(since).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    struct FakeDb {
        unreferenced: Vec<ContentHash>,
    }

    #[async_trait::async_trait]
    impl DatabaseBackend for FakeDb {
        async fn find_unreferenced_content_hashes(
            &self,
            _since: DateTime<Utc>,
        ) -> Result<Vec<ContentHash>, BackendError> {
            Ok(self.unreferenced.clone())
        }

        async fn begin_transaction(
            &self,
        ) -> Result<Box<dyn crate::TransactionHandle>, BackendError> {
            unimplemented!()
        }
        async fn deployment_exists(&self, _: &str) -> Result<bool, BackendError> {
            unimplemented!()
        }
        async fn get_deployment_by_entity_id(
            &self,
            _: &str,
        ) -> Result<Option<crate::Deployment>, BackendError> {
            unimplemented!()
        }
        async fn save_deployment(
            &self,
            _: &dyn crate::TransactionHandle,
            _: &crate::Entity,
            _: &crate::AuditInfo,
            _: Option<crate::DeploymentId>,
        ) -> Result<crate::DeploymentId, BackendError> {
            unimplemented!()
        }
        async fn save_content_files(
            &self,
            _: &dyn crate::TransactionHandle,
            _: crate::DeploymentId,
            _: &[crate::EntityContentEntry],
        ) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn calculate_overwrote(
            &self,
            _: &crate::Entity,
        ) -> Result<Vec<crate::DeploymentId>, BackendError> {
            unimplemented!()
        }
        async fn calculate_overwritten_by(
            &self,
            _: &crate::Entity,
        ) -> Result<Option<crate::DeploymentId>, BackendError> {
            unimplemented!()
        }
        async fn set_entities_as_overwritten(
            &self,
            _: &dyn crate::TransactionHandle,
            _: &[crate::DeploymentId],
            _: crate::DeploymentId,
        ) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn update_active_deployments(
            &self,
            _: &dyn crate::TransactionHandle,
            _: &[Pointer],
            _: &str,
        ) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn remove_active_deployments(
            &self,
            _: &dyn crate::TransactionHandle,
            _: &[Pointer],
        ) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn get_historical_deployments(
            &self,
            _: &crate::DeploymentOptions,
        ) -> Result<crate::PartialDeploymentHistory, BackendError> {
            unimplemented!()
        }
        async fn get_active_deployments(
            &self,
            _: Option<&[String]>,
            _: Option<&[Pointer]>,
        ) -> Result<Vec<crate::Deployment>, BackendError> {
            unimplemented!()
        }
        async fn get_deployments_by_ids(
            &self,
            _: &[crate::DeploymentId],
        ) -> Result<Vec<crate::DeploymentIdWithPointers>, BackendError> {
            unimplemented!()
        }
        async fn gc_stale_profiles(
            &self,
            _: DateTime<Utc>,
        ) -> Result<crate::GcStaleProfilesResult, BackendError> {
            unimplemented!()
        }
        async fn gc_profile_active_pointers(
            &self,
            _: DateTime<Utc>,
        ) -> Result<Vec<Pointer>, BackendError> {
            unimplemented!()
        }
        async fn save_failed_deployment(
            &self,
            _: &crate::FailedDeployment,
        ) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn delete_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn find_failed_deployment(
            &self,
            _: &str,
        ) -> Result<Option<crate::FailedDeployment>, BackendError> {
            unimplemented!()
        }
        async fn get_all_failed_deployments(
            &self,
        ) -> Result<Vec<crate::FailedDeployment>, BackendError> {
            unimplemented!()
        }
        async fn remove_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn get_last_gc_time(&self) -> Result<Option<i64>, BackendError> {
            unimplemented!()
        }
        async fn set_last_gc_time(&self, _: i64) -> Result<(), BackendError> {
            unimplemented!()
        }
    }

    struct RecordingStorage {
        batches: StdMutex<Vec<usize>>,
    }

    #[async_trait::async_trait]
    impl StorageBackend for RecordingStorage {
        async fn delete(&self, hashes: &[ContentHash]) -> Result<(), BackendError> {
            self.batches.lock().unwrap().push(hashes.len());
            Ok(())
        }
        async fn exist_multiple(
            &self,
            _: &[ContentHash],
        ) -> Result<std::collections::HashMap<ContentHash, bool>, BackendError> {
            unimplemented!()
        }
        async fn store(&self, _: &ContentHash, _: &[u8]) -> Result<(), BackendError> {
            unimplemented!()
        }
        async fn all_file_ids(&self) -> Result<Vec<ContentHash>, BackendError> {
            unimplemented!()
        }
    }

    async fn sweep_batches(n: usize) -> (usize, Vec<usize>) {
        let hashes: Vec<ContentHash> = (0..n).map(|i| format!("hash-{i:06}")).collect();
        let db: Arc<dyn DatabaseBackend> = Arc::new(FakeDb {
            unreferenced: hashes,
        });
        let storage = Arc::new(RecordingStorage {
            batches: StdMutex::new(Vec::new()),
        });
        let manager =
            GarbageCollectionManager::new(db, storage.clone(), GarbageCollectionConfig::default());

        let deleted = manager
            .gc_unused_hashes()
            .await
            .expect("gc_unused_hashes should succeed");
        let batches = storage.batches.lock().unwrap().clone();
        (deleted.len(), batches)
    }

    #[tokio::test]
    async fn gc_unused_hashes_splits_into_1000_sized_batches() {
        assert_eq!(GC_DELETE_BATCH_SIZE, 1000);

        let (deleted, batches) = sweep_batches(0).await;
        assert_eq!(deleted, 0);
        assert!(batches.is_empty(), "empty set must issue no delete");

        let (deleted, batches) = sweep_batches(1).await;
        assert_eq!(deleted, 1);
        assert_eq!(batches, vec![1]);

        let (deleted, batches) = sweep_batches(1000).await;
        assert_eq!(deleted, 1000);
        assert_eq!(batches, vec![1000]);

        let (deleted, batches) = sweep_batches(1001).await;
        assert_eq!(deleted, 1001);
        assert_eq!(batches, vec![1000, 1]);

        let (deleted, batches) = sweep_batches(2500).await;
        assert_eq!(deleted, 2500);
        assert_eq!(batches, vec![1000, 1000, 500]);
    }
}
