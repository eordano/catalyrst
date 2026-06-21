use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::snapshots_repository;
use sqlx::PgPool;

#[derive(Clone)]
pub struct ProcessedSnapshotStorage {
    pool: PgPool,
    cache: Arc<RwLock<HashSet<String>>>,
}

impl ProcessedSnapshotStorage {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn filter_processed_snapshots_from(
        &self,
        snapshot_hashes: &[String],
    ) -> Result<HashSet<String>, sqlx::Error> {
        {
            let cache = self.cache.read().await;
            let all_cached = snapshot_hashes.iter().all(|h| cache.contains(h));
            if all_cached {
                return Ok(snapshot_hashes.iter().cloned().collect());
            }
        }

        let from_repo =
            snapshots_repository::get_processed_snapshots(&self.pool, snapshot_hashes).await?;

        {
            let mut cache = self.cache.write().await;
            for hash in &from_repo {
                cache.insert(hash.clone());
            }
        }

        Ok(from_repo)
    }

    pub async fn mark_snapshot_as_processed(&self, snapshot_hash: &str) -> Result<(), sqlx::Error> {
        let now_ms = chrono::Utc::now().timestamp_millis() as f64;
        snapshots_repository::save_processed_snapshot(&self.pool, snapshot_hash, now_ms).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(snapshot_hash.to_string());
        }

        info!(snapshot_hash, "Processed snapshot saved");
        Ok(())
    }

    pub async fn reset(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}
