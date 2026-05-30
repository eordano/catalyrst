use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_BODY_BYTES: usize = 256 * 1024;
pub const HASH_HEX_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("body exceeds {max} bytes")]
    TooLarge { max: usize },

    #[error("invalid content hash: {0}")]
    InvalidHash(String),

    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub struct ContentStore {
    base: PathBuf,
}

impl ContentStore {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub async fn init(&self) -> Result<(), ContentError> {
        tokio::fs::create_dir_all(&self.base).await?;
        Ok(())
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        let mut p = self.base.clone();
        p.push(&hash[0..2]);
        p.push(&hash[0..4]);
        p.push(hash);
        p
    }

    pub fn exists(&self, hash: &str) -> bool {
        if !is_valid_hash(hash) {
            return false;
        }
        self.path_for(hash).is_file()
    }

    pub async fn put(&self, body: &[u8]) -> Result<String, ContentError> {
        if body.len() > MAX_BODY_BYTES {
            return Err(ContentError::TooLarge { max: MAX_BODY_BYTES });
        }
        let mut hasher = Sha256::new();
        hasher.update(body);
        let hash = hex::encode(hasher.finalize());

        let final_path = self.path_for(&hash);
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::metadata(&final_path).await.is_ok() {
            return Ok(hash);
        }

        let tmp_path = {
            let mut p = final_path.clone();
            let fname = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            p.set_file_name(format!(".{}.tmp.{}", fname, std::process::id()));
            p
        };

        tokio::fs::write(&tmp_path, body).await?;
        match tokio::fs::rename(&tmp_path, &final_path).await {
            Ok(()) => Ok(hash),
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                if tokio::fs::metadata(&final_path).await.is_ok() {
                    return Ok(hash);
                }
                Err(ContentError::Io(e))
            }
        }
    }

    pub async fn put_expecting(
        &self,
        body: &[u8],
        expected: &str,
    ) -> Result<String, ContentError> {
        if !is_valid_hash(expected) {
            return Err(ContentError::InvalidHash(expected.to_string()));
        }
        let actual = {
            let mut h = Sha256::new();
            h.update(body);
            hex::encode(h.finalize())
        };
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(ContentError::HashMismatch {
                expected: expected.to_ascii_lowercase(),
                actual,
            });
        }
        self.put(body).await
    }

    pub async fn get(&self, hash: &str) -> Result<Option<Vec<u8>>, ContentError> {
        if !is_valid_hash(hash) {
            return Err(ContentError::InvalidHash(hash.to_string()));
        }
        let path = self.path_for(hash);
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ContentError::Io(e)),
        }
    }

    pub async fn gc(&self, referenced: &HashSet<String>) -> Result<GcStats, ContentError> {
        let mut stats = GcStats::default();
        if !self.base.exists() {
            return Ok(stats);
        }
        let mut outer = tokio::fs::read_dir(&self.base).await?;
        while let Some(level1) = outer.next_entry().await? {
            if !level1.file_type().await?.is_dir() {
                continue;
            }
            let mut mid = tokio::fs::read_dir(level1.path()).await?;
            while let Some(level2) = mid.next_entry().await? {
                if !level2.file_type().await?.is_dir() {
                    continue;
                }
                let mut inner = tokio::fs::read_dir(level2.path()).await?;
                while let Some(file) = inner.next_entry().await? {
                    let fname = file.file_name();
                    let fname_s = fname.to_string_lossy();
                    if fname_s.starts_with('.') {
                        continue;
                    }
                    if !is_valid_hash(&fname_s) {
                        continue;
                    }
                    let ftype = file.file_type().await?;
                    if !ftype.is_file() {
                        continue;
                    }
                    stats.scanned += 1;
                    if !referenced.contains(fname_s.as_ref()) {
                        if let Err(e) = tokio::fs::remove_file(file.path()).await {
                            tracing::warn!(error = %e, path = ?file.path(), "gc: failed to remove orphan");
                            continue;
                        }
                        stats.removed += 1;
                    } else {
                        stats.kept += 1;
                    }
                }
            }
        }
        Ok(stats)
    }
}

#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct GcStats {
    pub scanned: u64,
    pub kept: u64,
    pub removed: u64,
}

pub fn is_valid_hash(s: &str) -> bool {
    s.len() == HASH_HEX_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn tmpdir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let mut rnd = [0u8; 8];
        getrandom_bytes(&mut rnd);
        p.push(format!("cmm-content-{}-{}", tag, hex::encode(rnd)));
        p
    }

    fn getrandom_bytes(buf: &mut [u8]) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let mut x: u64 = (nanos as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(pid as u64);
        for b in buf.iter_mut() {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *b = (x >> 33) as u8;
        }
    }

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let dir = tmpdir("rt");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = b"hello content addressed world";
        let h = store.put(body).await.unwrap();
        assert_eq!(h.len(), HASH_HEX_LEN);
        assert!(store.exists(&h));
        let got = store.get(&h).await.unwrap().unwrap();
        assert_eq!(got, body);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn put_is_idempotent() {
        let dir = tmpdir("idem");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = b"the same body";
        let h1 = store.put(body).await.unwrap();
        let h2 = store.put(body).await.unwrap();
        assert_eq!(h1, h2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn put_too_large_rejected() {
        let dir = tmpdir("big");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = vec![0u8; MAX_BODY_BYTES + 1];
        let err = store.put(&body).await.unwrap_err();
        assert!(matches!(err, ContentError::TooLarge { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let dir = tmpdir("miss");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let h = "0".repeat(64);
        let got = store.get(&h).await.unwrap();
        assert!(got.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_hash_rejected() {
        let dir = tmpdir("badhash");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let bad = "zzzz";
        assert!(!store.exists(bad));
        let got = store.get(bad).await;
        assert!(matches!(got, Err(ContentError::InvalidHash(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn put_expecting_mismatch_rejected() {
        let dir = tmpdir("expect");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = b"abc";
        let wrong = "0".repeat(64);
        let err = store.put_expecting(body, &wrong).await.unwrap_err();
        assert!(matches!(err, ContentError::HashMismatch { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn gc_removes_unreferenced() {
        let dir = tmpdir("gc");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let keep_hash = store.put(b"keepme").await.unwrap();
        let drop_hash = store.put(b"dropme").await.unwrap();
        assert!(store.exists(&keep_hash));
        assert!(store.exists(&drop_hash));
        let mut keep: HashSet<String> = HashSet::new();
        keep.insert(keep_hash.clone());
        let stats = store.gc(&keep).await.unwrap();
        assert_eq!(stats.kept, 1);
        assert_eq!(stats.removed, 1);
        assert!(store.exists(&keep_hash));
        assert!(!store.exists(&drop_hash));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
