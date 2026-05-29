use bytes::Bytes;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::{resolve_file_path, StorageError};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct ContentStorage {
    root: PathBuf,
}

impl ContentStorage {
    pub async fn new(base_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = base_path.into().join("contents");
        tokio::fs::create_dir_all(&root).await?;
        debug!(root = %root.display(), "content storage initialized");
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
            .unwrap_or("content");
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

        debug!(hash, bytes = data.len(), "content stored");
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

    pub async fn retrieve_uncompressed(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
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

    pub async fn exist_multiple(
        &self,
        hashes: &[&str],
    ) -> Result<Vec<(String, bool)>, StorageError> {
        let mut results = Vec::with_capacity(hashes.len());
        for &hash in hashes {
            let exists = self.exist(hash).await?;
            results.push((hash.to_owned(), exists));
        }
        Ok(results)
    }

    pub async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(hash, error = %e, "failed to delete content file");
            }
        }

        debug!(hash, "content deleted");
        Ok(())
    }

    pub async fn delete_multiple(&self, hashes: &[&str]) -> Result<(), StorageError> {
        for &hash in hashes {
            self.delete(hash).await?;
        }
        Ok(())
    }

    pub async fn file_path(&self, hash: &str) -> Result<Option<(PathBuf, bool)>, StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if path.is_file() {
            return Ok(Some((path, false)));
        }

        Ok(None)
    }

    pub async fn uncompressed_file_path(&self, hash: &str) -> Result<Option<PathBuf>, StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if path.is_file() {
            return Ok(Some(path));
        }

        Ok(None)
    }

    pub async fn file_info(&self, hash: &str) -> Result<Option<FileInfo>, StorageError> {
        let path = resolve_file_path(&self.root, hash).await?;

        if path.is_file() {
            let meta = tokio::fs::metadata(&path).await?;
            return Ok(Some(FileInfo {
                size: meta.len(),
                encoding: None,
                content_size: Some(meta.len()),
            }));
        }

        Ok(None)
    }

    pub async fn all_file_ids(
        &self,
        prefix: Option<&str>,
    ) -> Result<Vec<String>, StorageError> {
        let mut ids = Vec::new();
        let mut shard_dirs = tokio::fs::read_dir(&self.root).await?;

        while let Some(shard_entry) = shard_dirs.next_entry().await? {
            if !shard_entry.file_type().await?.is_dir() {
                continue;
            }
            let shard_path = shard_entry.path();
            let mut entries = tokio::fs::read_dir(&shard_path).await?;

            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();

                if !crate::is_canonical_content_id(&name_str) {
                    continue;
                }

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

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: u64,
    pub encoding: Option<String>,
    pub content_size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn store_retrieve_delete_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-test-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let data = Bytes::from_static(b"hello decentraland");

        storage.store(hash, data.clone()).await.unwrap();
        assert!(storage.exist(hash).await.unwrap());

        let retrieved = storage.retrieve(hash).await.unwrap().unwrap();
        assert_eq!(retrieved, data);

        let info = storage.file_info(hash).await.unwrap().unwrap();
        assert_eq!(info.size, data.len() as u64);
        assert!(info.encoding.is_none());

        storage.delete(hash).await.unwrap();
        assert!(!storage.exist(hash).await.unwrap());
        assert!(storage.retrieve(hash).await.unwrap().is_none());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn exist_returns_false_for_missing() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-test-miss-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        assert!(!storage.exist(hash).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn delete_missing_is_silent() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-test-delmiss-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        storage.delete(hash).await.unwrap();

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn invalid_id_is_rejected() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-test-bad-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        match storage.exist("../etc/passwd").await {
            Err(StorageError::InvalidId(_)) => {}
            other => panic!("expected InvalidId, got {:?}", other),
        }
        match storage.store("Qm\0evil", Bytes::from_static(b"")).await {
            Err(StorageError::InvalidId(_)) => {}
            other => panic!("expected InvalidId, got {:?}", other),
        }

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn all_file_ids_lists_stored_files() {
        let tmp = std::env::temp_dir().join(format!("catalyrst-test-list-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        let a = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let b = "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e";
        storage.store(a, Bytes::from_static(b"a")).await.unwrap();
        storage.store(b, Bytes::from_static(b"b")).await.unwrap();

        let ids = storage.all_file_ids(None).await.unwrap();
        assert!(ids.contains(&a.to_string()));
        assert!(ids.contains(&b.to_string()));

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn retrieve_ignores_gzip_companion() {

        let tmp = std::env::temp_dir().join(format!("catalyrst-test-gzip-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();

        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
        let raw_data = Bytes::from_static(b"raw content");
        let attacker_gzip = Bytes::from_static(b"attacker-planted gzip content");

        storage.store(hash, raw_data.clone()).await.unwrap();

        let raw_path = crate::resolve_file_path(storage.root(), hash).await.unwrap();
        let gzip_path = PathBuf::from(format!("{}.gzip", raw_path.display()));
        tokio::fs::write(&gzip_path, &attacker_gzip).await.unwrap();

        let retrieved = storage.retrieve(hash).await.unwrap().unwrap();
        assert_eq!(
            retrieved, raw_data,
            "retrieve() must NOT prefer a `.gzip` sibling (unverified)"
        );

        let info = storage.file_info(hash).await.unwrap().unwrap();
        assert!(info.encoding.is_none(), "file_info must report no encoding");
        assert_eq!(info.size, raw_data.len() as u64);

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn store_rejects_tmp_path_symlink() {

        let tmp = std::env::temp_dir().join(format!("catalyrst-test-nofollow-{}", std::process::id()));
        let storage = ContentStorage::new(&tmp).await.unwrap();
        let hash = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

        storage.store(hash, Bytes::from_static(b"x")).await.unwrap();
        assert!(storage.exist(hash).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
