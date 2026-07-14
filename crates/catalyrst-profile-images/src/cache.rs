use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use sha2::{Digest, Sha256};

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

/// Bytes written since the last sweep that trigger the next one. A sweep walks
/// the whole cache, so it must not run per-request; amortising it over this
/// much new data keeps it rare while bounding the overshoot to roughly one
/// interval above the budget.
const SWEEP_AFTER_BYTES: u64 = 64 * 1024 * 1024;

/// Evict down to this fraction of the budget, so a sweep reclaims a useful
/// amount instead of one file per subsequent write.
const LOW_WATER: f64 = 0.9;

pub struct ImageCache {
    root: PathBuf,
    ttl: Option<Duration>,
    /// `None` means unbounded, which is the old behaviour: a cache of derivable
    /// renders that only ever grew.
    max_bytes: Option<u64>,
    written_since_sweep: Arc<AtomicU64>,
    sweeping: Arc<AtomicBool>,
}

impl ImageCache {
    pub fn new(root: impl Into<PathBuf>, ttl_seconds: u64) -> Self {
        Self::with_budget(root, ttl_seconds, None)
    }

    pub fn with_budget(root: impl Into<PathBuf>, ttl_seconds: u64, max_bytes: Option<u64>) -> Self {
        Self {
            root: root.into(),
            ttl: if ttl_seconds == 0 {
                None
            } else {
                Some(Duration::from_secs(ttl_seconds))
            },
            max_bytes,
            written_since_sweep: Arc::new(AtomicU64::new(0)),
            sweeping: Arc::new(AtomicBool::new(false)),
        }
    }

    fn entity_dir(&self, entity: &str) -> PathBuf {
        self.root.join(hex_prefix(entity)).join(entity)
    }

    fn path(&self, entity: &str, kind: ImageKind) -> PathBuf {
        self.entity_dir(entity).join(kind.filename())
    }

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

    pub async fn put(&self, entity: &str, kind: ImageKind, data: &Bytes) -> std::io::Result<()> {
        let dir = self.entity_dir(entity);
        tokio::fs::create_dir_all(&dir).await?;
        let final_path = dir.join(kind.filename());
        let tmp_path = dir.join(format!(".{}.{}.tmp", kind.filename(), std::process::id()));
        tokio::fs::write(&tmp_path, data).await?;
        match tokio::fs::rename(&tmp_path, &final_path).await {
            Ok(()) => {
                self.note_written(data.len() as u64);
                Ok(())
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(e)
            }
        }
    }

    /// Counts a write and, once enough has accumulated, sweeps in the
    /// background. Detached on purpose: a render already costs seconds, and
    /// making the caller wait on a directory walk would add the eviction cost
    /// to a request that has nothing to do with it.
    fn note_written(&self, bytes: u64) {
        let Some(max) = self.max_bytes else {
            return;
        };
        let prior = self.written_since_sweep.fetch_add(bytes, Ordering::Relaxed);
        if prior + bytes < SWEEP_AFTER_BYTES {
            return;
        }
        // One sweep at a time; a second trigger while one runs is a no-op
        // rather than a second walk over the same tree.
        if self
            .sweeping
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.written_since_sweep.store(0, Ordering::Relaxed);
        let root = self.root.clone();
        let sweeping = self.sweeping.clone();
        tokio::spawn(async move {
            match sweep_to_budget(&root, max).await {
                Ok(freed) if freed > 0 => {
                    tracing::info!(
                        freed_bytes = freed,
                        budget = max,
                        "profile image cache evicted"
                    )
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "profile image cache sweep failed"),
            }
            sweeping.store(false, Ordering::Release);
        });
    }
}

/// Evicts oldest-first until the tree fits `LOW_WATER * max`.
///
/// Ordering is by mtime, i.e. by when the image was rendered, not by when it
/// was last served. True LRU would need either atime -- which `relatime` and
/// `noatime` make unreliable, and which is exactly the mount option a busy
/// cache directory tends to get -- or a side index to maintain and keep
/// consistent with the disk. Neither is worth it here, because every entry is
/// re-derivable: evicting a hot image costs one re-render, not a loss. The
/// budget is the property that matters; the ordering is a heuristic.
async fn sweep_to_budget(root: &std::path::Path, max: u64) -> std::io::Result<u64> {
    let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total: u64 = 0;

    let mut shards = match tokio::fs::read_dir(root).await {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };
    while let Ok(Some(shard)) = shards.next_entry().await {
        if !matches!(shard.file_type().await, Ok(ft) if ft.is_dir()) {
            continue;
        }
        let Ok(mut ents) = tokio::fs::read_dir(shard.path()).await else {
            continue;
        };
        while let Ok(Some(entity)) = ents.next_entry().await {
            if !matches!(entity.file_type().await, Ok(ft) if ft.is_dir()) {
                continue;
            }
            let Ok(mut files) = tokio::fs::read_dir(entity.path()).await else {
                continue;
            };
            while let Ok(Some(f)) = files.next_entry().await {
                let Ok(meta) = f.metadata().await else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let name = f.file_name();
                // Leave staging files to their writer; they are not cache
                // entries and removing one mid-write would corrupt a render.
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                total += meta.len();
                entries.push((mtime, meta.len(), f.path()));
            }
        }
    }

    if total <= max {
        return Ok(0);
    }

    entries.sort_by_key(|(mtime, _, _)| *mtime);
    let target = (max as f64 * LOW_WATER) as u64;
    let mut freed = 0u64;
    for (_, size, path) in entries {
        if total.saturating_sub(freed) <= target {
            break;
        }
        if tokio::fs::remove_file(&path).await.is_ok() {
            freed += size;
        }
    }
    Ok(freed)
}

fn hex_prefix(entity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entity.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
}

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

        assert!(cache.get(QM, ImageKind::Body).await.is_none());

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

    async fn tree_bytes(root: &Path) -> u64 {
        let mut total = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(p) = stack.pop() {
            let Ok(mut rd) = tokio::fs::read_dir(&p).await else {
                continue;
            };
            while let Ok(Some(e)) = rd.next_entry().await {
                let Ok(m) = e.metadata().await else { continue };
                if m.is_dir() {
                    stack.push(e.path());
                } else {
                    total += m.len();
                }
            }
        }
        total
    }

    /// The budget is the property that matters, so assert on bytes rather than
    /// on which entries survived -- the ordering is an explicitly-stated
    /// heuristic and pinning it would pin the heuristic, not the contract.
    #[tokio::test]
    async fn sweep_brings_an_over_budget_tree_under_the_limit() {
        let dir = std::env::temp_dir().join(format!("cpi-eviction-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let cache = ImageCache::new(&dir, 0);
        let blob = Bytes::from(vec![7u8; 4096]);

        // 40 entities x 2 images x 4 KiB = 320 KiB written.
        for i in 0..40u32 {
            let entity = format!("Qm{:0>44}", i);
            cache.put(&entity, ImageKind::Face, &blob).await.unwrap();
            cache.put(&entity, ImageKind::Body, &blob).await.unwrap();
        }
        let before = tree_bytes(&dir).await;
        assert!(before >= 320 * 1024, "expected a full tree, got {before}");

        let budget = 100 * 1024;
        let freed = sweep_to_budget(&dir, budget).await.unwrap();
        let after = tree_bytes(&dir).await;

        assert!(freed > 0, "sweep freed nothing from an over-budget tree");
        assert!(
            after <= budget,
            "tree still over budget after sweep: {after} > {budget}"
        );
        assert!(
            after <= (budget as f64 * LOW_WATER) as u64 + 4096,
            "sweep should reach the low-water mark, landed at {after}"
        );

        // A tree already under budget must not lose anything.
        let freed_again = sweep_to_budget(&dir, budget).await.unwrap();
        assert_eq!(freed_again, 0, "sweep evicted from an under-budget tree");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn sweep_leaves_staging_files_to_their_writer() {
        let dir = std::env::temp_dir().join(format!("cpi-staging-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let cache = ImageCache::new(&dir, 0);
        let blob = Bytes::from(vec![1u8; 8192]);
        for i in 0..8u32 {
            cache
                .put(&format!("Qm{:0>44}", i), ImageKind::Face, &blob)
                .await
                .unwrap();
        }
        let staging = cache
            .entity_dir(&format!("Qm{:0>44}", 0))
            .join(".face.png.999.tmp");
        tokio::fs::write(&staging, vec![2u8; 8192]).await.unwrap();

        sweep_to_budget(&dir, 1024).await.unwrap();
        assert!(
            tokio::fs::metadata(&staging).await.is_ok(),
            "sweep removed an in-flight staging file"
        );
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
