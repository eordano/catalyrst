use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_POSTER_BYTES: usize = 500 * 1024;
pub const HASH_HEX_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("body exceeds {max} bytes")]
    TooLarge { max: usize },

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

    pub async fn put(&self, body: &[u8]) -> Result<String, ContentError> {
        if body.len() > MAX_POSTER_BYTES {
            return Err(ContentError::TooLarge {
                max: MAX_POSTER_BYTES,
            });
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

    pub fn exists(&self, hash: &str) -> bool {
        is_valid_hash(hash) && self.path_for(hash).is_file()
    }
}

pub fn is_valid_hash(s: &str) -> bool {
    s.len() == HASH_HEX_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!(
            "evt-content-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        p
    }

    #[tokio::test]
    async fn put_is_idempotent_and_addressable() {
        let dir = tmpdir("idem");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = b"poster bytes";
        let h1 = store.put(body).await.unwrap();
        let h2 = store.put(body).await.unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), HASH_HEX_LEN);
        assert!(store.exists(&h1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn put_too_large_rejected() {
        let dir = tmpdir("big");
        let store = ContentStore::new(&dir);
        store.init().await.unwrap();
        let body = vec![0u8; MAX_POSTER_BYTES + 1];
        let err = store.put(&body).await.unwrap_err();
        assert!(matches!(err, ContentError::TooLarge { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
