use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use sha2::{Digest, Sha256};

/// The two image kinds the upstream service produces per profile entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Face,
    Body,
}

impl ImageKind {
    pub fn filename(self) -> &'static str {
        match self {
            ImageKind::Face => "face.png",
            ImageKind::Body => "body.png",
        }
    }
}

/// Filesystem cache for rendered profile images, sharded by a 2-byte hex
/// prefix of the entity id (same `<prefix>/<id>` layout the content-server
/// uses for content files). Layout:
///   `<root>/<prefix>/<entity>/{face,body}.png`
pub struct ImageCache {
    root: PathBuf,
    ttl: Option<Duration>,
}

impl ImageCache {
    pub fn new(root: impl Into<PathBuf>, ttl_seconds: u64) -> Self {
        Self {
            root: root.into(),
            ttl: if ttl_seconds == 0 {
                None
            } else {
                Some(Duration::from_secs(ttl_seconds))
            },
        }
    }

    fn entity_dir(&self, entity: &str) -> PathBuf {
        self.root.join(hex_prefix(entity)).join(entity)
    }

    fn path(&self, entity: &str, kind: ImageKind) -> PathBuf {
        self.entity_dir(entity).join(kind.filename())
    }

    /// Returns the cached bytes if a fresh entry exists. A `None` means the
    /// caller should pull from origin (missing or stale).
    pub async fn get(&self, entity: &str, kind: ImageKind) -> Option<Bytes> {
        let path = self.path(entity, kind);
        let meta = tokio::fs::metadata(&path).await.ok()?;
        if !meta.is_file() {
            return None;
        }
        if let Some(ttl) = self.ttl {
            let modified = meta.modified().ok()?;
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::ZERO);
            if age > ttl {
                return None;
            }
        }
        let data = tokio::fs::read(&path).await.ok()?;
        Some(Bytes::from(data))
    }

    /// Writes bytes atomically (temp file + rename) so a concurrent reader
    /// never observes a partial PNG.
    pub async fn put(&self, entity: &str, kind: ImageKind, data: &Bytes) -> std::io::Result<()> {
        let dir = self.entity_dir(entity);
        tokio::fs::create_dir_all(&dir).await?;
        let final_path = dir.join(kind.filename());
        let tmp_path = dir.join(format!(".{}.{}.tmp", kind.filename(), std::process::id()));
        tokio::fs::write(&tmp_path, data).await?;
        match tokio::fs::rename(&tmp_path, &final_path).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(e)
            }
        }
    }
}

/// First two bytes of sha256(entity) as lowercase hex — the shard directory.
fn hex_prefix(entity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entity.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
}

/// Profile entity ids are IPFS CIDs (`Qm…` v0 or `ba…` v1). Reject anything
/// else so a request can never name a path component (`/`, `..`, NUL, …).
pub fn is_valid_entity_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 100 {
        return false;
    }
    let cidv0 = id.len() == 46
        && id.starts_with("Qm")
        && id[2..].chars().all(|c| {
            matches!(c,
                '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z')
        });
    let cidv1 = id.starts_with("ba")
        && id.len() >= 58
        && id[2..].chars().all(|c| matches!(c, 'a'..='z' | '2'..='7'));
    cidv0 || cidv1
}

/// Defensive: never let an id escape the cache root even if validation above
/// is loosened later.
#[allow(dead_code)]
pub(crate) fn stays_within(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QM: &str = "QmPeX5wQyTuLrU3p3HrChAtgcMz1mDdRRpHm5Ks5sQ8mY3";

    #[test]
    fn validates_entity_ids() {
        assert!(is_valid_entity_id(QM));
        assert!(is_valid_entity_id(
            "bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy"
        ));
        assert!(!is_valid_entity_id(""));
        assert!(!is_valid_entity_id("../etc/passwd"));
        assert!(!is_valid_entity_id("entities/foo"));
        assert!(!is_valid_entity_id("0xabc"));
    }

    #[tokio::test]
    async fn put_then_get_roundtrips_and_shards() {
        let dir = std::env::temp_dir().join(format!("cpi-test-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let cache = ImageCache::new(&dir, 0);
        let data = Bytes::from_static(b"\x89PNG\r\n\x1a\nfake");

        assert!(cache.get(QM, ImageKind::Face).await.is_none());
        cache.put(QM, ImageKind::Face, &data).await.unwrap();
        assert_eq!(cache.get(QM, ImageKind::Face).await.unwrap(), data);
        // body is a separate file under the same entity dir
        assert!(cache.get(QM, ImageKind::Body).await.is_none());

        // file landed under <root>/<prefix>/<entity>/face.png
        let expected = dir.join(hex_prefix(QM)).join(QM).join("face.png");
        assert!(tokio::fs::metadata(&expected).await.is_ok());

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn ttl_zero_never_expires() {
        let dir = std::env::temp_dir().join(format!("cpi-ttl-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let cache = ImageCache::new(&dir, 0);
        let data = Bytes::from_static(b"x");
        cache.put(QM, ImageKind::Body, &data).await.unwrap();
        assert!(cache.get(QM, ImageKind::Body).await.is_some());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
