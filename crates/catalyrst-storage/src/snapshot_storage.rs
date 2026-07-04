use bytes::Bytes;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::{resolve_file_path, StorageError};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct SnapshotStorage {
    root: PathBuf,
}

impl SnapshotStorage {
    pub async fn new(base_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = base_path.into().join("snapshots");
        tokio::fs::create_dir_all(&root).await?;
        debug!(root = %root.display(), "snapshot storage initialized");
        Ok(Self { root })
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub async fn store(&self, hash: &str, data: Bytes) -> Result<(), StorageError> {
        use tokio::io::AsyncWriteExt;

        let path = resolve_file_path(&self.root, hash).await?;

        let base = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("snapshot");
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp_path = path.with_file_name(format!("{}.{}.{}.tmp", base, std::process::id(), seq));

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp_path)
            .await?;
        if let Err(e) = file.write_all(&data).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e.into());
        }
        file.sync_all().await?;
        drop(file);

        tokio::fs::rename(&tmp_path, &path).await?;

        if let Some(parent) = path.parent() {
            if let Ok(dir) = tokio::fs::File::open(parent).await {
                let _ = dir.sync_all().await;
            }
        }

        debug!(hash, bytes = data.len(), "snapshot stored");
        Ok(())
    }

    pub async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if path.is_file() {
            let data = tokio::fs::read(&path).await?;
            return Ok(Some(Bytes::from(data)));
        }

        Ok(None)
    }

    pub async fn exist(&self, hash: &str) -> Result<bool, StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;
        Ok(path.is_file())
    }

    pub async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(hash, error = %e, "failed to delete snapshot file");
            }
        }

        debug!(hash, "snapshot deleted");
        Ok(())
    }

    pub async fn all_file_ids(&self, prefix: Option<&str>) -> Result<Vec<String>, StorageError> {
        let mut ids = Vec::new();
        let mut shard_dirs = tokio::fs::read_dir(&self.root).await?;

        while let Some(shard_entry) = shard_dirs.next_entry().await? {
            if !shard_entry.file_type().await?.is_dir() {
                continue;
            }
            let mut entries = tokio::fs::read_dir(shard_entry.path()).await?;

            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();

                if let Some(pfx) = prefix {
                    if !name_str.starts_with(pfx) {
                        continue;
                    }
                }

                ids.push(name_str);
            }
        }

        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

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

        let all = storage.all_file_ids(None).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&a.to_string()));
        assert!(all.contains(&b.to_string()));

        let filtered = storage.all_file_ids(Some("bafkreihdw")).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains(&a.to_string()));

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
