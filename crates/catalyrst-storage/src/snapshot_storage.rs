use bytes::Bytes;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::{
    create_staging_file, ensure_file_path, resolve_file_path, staging_path, stat_for_read, FileIds,
    KnownShards, StorageError,
};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct SnapshotStorage {
    root: PathBuf,
    known_shards: KnownShards,
}

impl SnapshotStorage {
    pub async fn new(base_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = base_path.into().join("snapshots");
        tokio::fs::create_dir_all(&root).await?;
        // Staging files whose writer died before the rename have no other reaper.
        // Backgrounded for the reason given in ContentStorage::new: it is a
        // reaper, not a precondition for serving, and its 1h age threshold is
        // what already made it safe against a live store.
        let sweep_root = root.clone();
        tokio::spawn(async move {
            crate::sweep_stale_staging(&sweep_root, "snapshot").await;
        });
        debug!(root = %root.display(), "snapshot storage initialized");
        Ok(Self {
            known_shards: KnownShards::new(root.clone()),
            root,
        })
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub async fn store(&self, hash: &str, data: Bytes) -> Result<(), StorageError> {
        use tokio::io::AsyncWriteExt;

        // The one path that creates the shard directory.
        let path = ensure_file_path(&self.root, hash, &self.known_shards).await?;

        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        // The guard arrives with the file, so from the instant the staging file exists every exit
        // -- an error, a `?`, or this future being dropped mid-write -- removes it.
        let (mut file, mut staging) =
            create_staging_file(staging_path(&path, "snapshot", seq)).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;
        drop(file);

        tokio::fs::rename(staging.path(), &path).await?;
        staging.disarm();

        if let Some(parent) = path.parent() {
            if let Ok(dir) = tokio::fs::File::open(parent).await {
                let _ = dir.sync_all().await;
            }
        }

        debug!(hash, bytes = data.len(), "snapshot stored");
        Ok(())
    }

    pub async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if stat_for_read(&self.known_shards, &path).await?.is_some() {
            let data = tokio::fs::read(&path).await?;
            return Ok(Some(Bytes::from(data)));
        }

        Ok(None)
    }

    pub async fn exist(&self, hash: &str) -> Result<bool, StorageError> {
        let path = resolve_file_path(&self.root, hash)?;
        Ok(stat_for_read(&self.known_shards, &path).await?.is_some())
    }

    pub async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        let path = resolve_file_path(&self.root, hash)?;

        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(hash, error = %e, "failed to delete snapshot file");
            }
        }

        debug!(hash, "snapshot deleted");
        Ok(())
    }

    /// Every snapshot id this store actually holds, pulled one at a time.
    ///
    /// Same walk and same contract as
    /// [`ContentStorage::all_file_ids`](crate::ContentStorage::all_file_ids) -- see [`FileIds`]. The
    /// round-trip filter earns its keep here too: a staging file left by a cancelled store is named
    /// `<id>.<pid>.<seq>.tmp`, which no read can ever resolve, and offering it as an id makes a
    /// consumer that syncs from this list act on content that does not exist.
    pub fn all_file_ids<'a>(&'a self, prefix: Option<&'a str>) -> FileIds<'a> {
        FileIds::new(&self.root, &self.known_shards, "snapshot", prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    /// Drains the walk into a `Vec`, which is what a test wants and what the walk itself refuses to
    /// decide for its callers.
    async fn collect_ids(storage: &SnapshotStorage, prefix: Option<&str>) -> Vec<String> {
        let mut walk = storage.all_file_ids(prefix);
        let mut ids = Vec::new();
        while let Some(id) = walk.next().await.unwrap() {
            ids.push(id);
        }
        ids
    }

    #[tokio::test]
    async fn snapshot_store_retrieve_delete() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-snap-{}", std::process::id()));
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let hash = "bafkreiaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let data = Bytes::from_static(b"snapshot payload");

        storage.store(hash, data.clone()).await.unwrap();
        assert!(storage.exist(hash).await.unwrap());

        let retrieved = storage.retrieve(hash).await.unwrap().unwrap();
        assert_eq!(retrieved, data);

        storage.delete(hash).await.unwrap();
        assert!(!storage.exist(hash).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn snapshot_all_file_ids() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-snap-list-{}", std::process::id()));
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let a = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let b = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";

        storage.store(a, Bytes::from_static(b"a")).await.unwrap();
        storage.store(b, Bytes::from_static(b"b")).await.unwrap();

        let all = collect_ids(&storage, None).await;
        assert_eq!(all.len(), 2);
        assert!(all.contains(&a.to_string()));
        assert!(all.contains(&b.to_string()));

        let filtered = collect_ids(&storage, Some("bafkreihdw")).await;
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains(&a.to_string()));

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    fn shard_dir_of(storage: &SnapshotStorage, hash: &str) -> PathBuf {
        crate::resolve_file_path(storage.root(), hash)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// A shard nothing was ever stored in is an ordinary miss, and probing it creates nothing.
    #[tokio::test]
    async fn snapshot_read_of_never_created_shard_is_a_plain_miss() {
        let tmp =
            std::env::temp_dir().join(format!("catalyrst-snap-virgin-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let shard_dir = shard_dir_of(&storage, hash);

        assert!(!storage.exist(hash).await.unwrap());
        assert!(storage.retrieve(hash).await.unwrap().is_none());
        storage.delete(hash).await.unwrap();

        assert!(
            !shard_dir.exists(),
            "a read must not create {}",
            shard_dir.display()
        );
        let mut entries = tokio::fs::read_dir(storage.root()).await.unwrap();
        assert!(
            entries.next_entry().await.unwrap().is_none(),
            "reads must leave the snapshot root empty"
        );

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    /// A shard destroyed underneath us is damage, not an empty node.
    #[tokio::test]
    async fn snapshot_destroyed_shard_is_a_fault_not_a_miss() {
        let tmp =
            std::env::temp_dir().join(format!("catalyrst-snap-destroyed-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        storage.store(hash, Bytes::from_static(b"x")).await.unwrap();

        let shard_dir = shard_dir_of(&storage, hash);
        tokio::fs::remove_dir_all(&shard_dir).await.unwrap();

        assert!(matches!(
            storage.exist(hash).await,
            Err(StorageError::Io(_))
        ));
        assert!(
            matches!(storage.exist(hash).await, Err(StorageError::Io(_))),
            "the report lasts as long as the damage: nothing is consumed to produce it"
        );
        assert!(matches!(
            storage.retrieve(hash).await,
            Err(StorageError::Io(_))
        ));
        assert!(
            !shard_dir.exists(),
            "the faulting read must not have healed the shard"
        );

        storage.store(hash, Bytes::from_static(b"y")).await.unwrap();
        assert!(storage.exist(hash).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    /// Enumeration yields ids, not whatever happens to be lying in a shard. A staging file left by
    /// a cancelled store is named `<id>.<pid>.<seq>.tmp`, which no read can resolve; offering it as
    /// an id makes a consumer that syncs or GCs from this list act on content that does not exist.
    #[tokio::test]
    async fn snapshot_all_file_ids_yields_only_real_ids() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-snap-junk-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let stored = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let elsewhere = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";
        storage
            .store(stored, Bytes::from_static(b"a"))
            .await
            .unwrap();

        let shard = shard_dir_of(&storage, stored);
        tokio::fs::write(shard.join(format!("{stored}.4242.0.tmp")), b"leaked")
            .await
            .unwrap();
        tokio::fs::create_dir(shard.join("junkdir")).await.unwrap();
        tokio::fs::write(shard.join(elsewhere), b"misplaced")
            .await
            .unwrap();

        assert_eq!(collect_ids(&storage, None).await, vec![stored]);
        assert_eq!(
            collect_ids(&storage, Some("bafkreihdw")).await,
            vec![stored],
            "the prefix filter applies to ids, after the junk is gone"
        );

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    /// A cancelled store leaves nothing behind for enumeration to trip over.
    #[tokio::test]
    async fn snapshot_cancelled_store_leaves_no_staging_file() {
        let tmp =
            std::env::temp_dir().join(format!("catalyrst-snap-cancel-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let payload = Bytes::from(vec![3u8; 64 * 1024 * 1024]);
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(1),
            storage.store(hash, payload),
        )
        .await;

        // Cleanup is eventually-consistent by construction -- the guard rides back through a oneshot
        // owned by a detached task, so the unlink lands when that task next runs, not the instant
        // the caller is cancelled. Sampling immediately would be flaky in both directions.
        let leaked = crate::wait_for_staging_cleanup(&shard_dir_of(&storage, hash)).await;
        assert!(
            leaked.is_empty(),
            "cancellation must not leak staging files, found {leaked:?}"
        );

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn snapshot_invalid_id_rejected() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-snap-bad-{}", std::process::id()));
        let storage = SnapshotStorage::new(&tmp).await.unwrap();

        match storage
            .store("../etc/passwd", Bytes::from_static(b""))
            .await
        {
            Err(StorageError::InvalidId(_)) => {}
            other => panic!("expected InvalidId, got {:?}", other),
        }

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
