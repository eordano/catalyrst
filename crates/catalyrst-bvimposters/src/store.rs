use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::key::{ImposterKey, MAX_LEVEL};

const EVICT_MIN_AGE: Duration = Duration::from_secs(3600);
const SWEEP_MAX_AGE: Duration = Duration::from_secs(86400);

pub struct Store {
    root: PathBuf,
    max_bytes: u64,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct StoreUsage {
    pub bytes: u64,
    pub entries: u64,
}

impl Store {
    pub fn new(root: PathBuf, max_bytes: u64) -> Self {
        Self { root, max_bytes }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn store_dir(&self) -> PathBuf {
        self.root.join("store")
    }

    pub fn level_dir(&self, level: u8) -> PathBuf {
        self.store_dir().join(level.to_string())
    }

    pub fn zip_path(&self, key: &ImposterKey) -> PathBuf {
        self.level_dir(key.tile.level).join(key.zip_name())
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn staging_root(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn evicted_dir(&self) -> PathBuf {
        self.root.join("evicted")
    }

    pub fn quarantine_path(&self) -> PathBuf {
        self.root.join("quarantine.json")
    }

    pub fn quarantined_dir(&self) -> PathBuf {
        self.root.join("quarantined")
    }

    pub fn quarantine_entry(&self, key: &ImposterKey) -> std::io::Result<bool> {
        let src = self.zip_path(key);
        let dir = self.quarantined_dir().join(key.tile.level.to_string());
        std::fs::create_dir_all(&dir)?;
        match std::fs::rename(&src, dir.join(key.zip_name())) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn init(&self) -> Result<()> {
        for dir in [
            self.store_dir(),
            self.tmp_dir(),
            self.staging_root(),
            self.evicted_dir(),
        ] {
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    pub async fn read_hit(&self, key: &ImposterKey) -> Option<Vec<u8>> {
        let path = self.zip_path(key);
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                touch(&path);
                Some(bytes)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "store entry unreadable, quarantining");
                let _ = self.evict_file(&path);
                None
            }
        }
    }

    pub async fn land_tmp(&self, tmp: &Path, key: &ImposterKey) -> Result<()> {
        let dir = self.level_dir(key.tile.level);
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("creating {}", dir.display()))?;
        let target = self.zip_path(key);
        tokio::fs::rename(tmp, &target)
            .await
            .with_context(|| format!("landing {}", target.display()))?;
        Ok(())
    }

    pub fn evict_file(&self, path: &Path) -> std::io::Result<()> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = self
            .evicted_dir()
            .join(format!("{}-{}", uuid::Uuid::new_v4(), name));
        std::fs::rename(path, &target)?;
        let _ = std::fs::remove_file(&target);
        Ok(())
    }

    pub fn usage(&self) -> StoreUsage {
        let mut usage = StoreUsage::default();
        for (_, _, len) in self.scan() {
            usage.bytes += len;
            usage.entries += 1;
        }
        usage
    }

    fn scan(&self) -> Vec<(PathBuf, SystemTime, u64)> {
        let mut entries = Vec::new();
        for level in 0..=MAX_LEVEL {
            let Ok(rd) = std::fs::read_dir(self.level_dir(level)) else {
                continue;
            };
            for entry in rd.flatten() {
                let Ok(md) = entry.metadata() else { continue };
                if !md.is_file() {
                    continue;
                }
                entries.push((entry.path(), md.modified().unwrap_or(UNIX_EPOCH), md.len()));
            }
        }
        entries
    }

    pub fn evict_pass(&self) -> Result<u64> {
        let mut entries = self.scan();
        let mut total: u64 = entries.iter().map(|e| e.2).sum();
        if total > self.max_bytes {
            entries.sort_by_key(|e| e.1);
            let now = SystemTime::now();
            for (path, mtime, len) in entries {
                if total <= self.max_bytes {
                    break;
                }
                let age = now.duration_since(mtime).unwrap_or_default();
                if age < EVICT_MIN_AGE {
                    continue;
                }
                if self.evict_file(&path).is_ok() {
                    tracing::info!(path = %path.display(), len, "evicted");
                    total -= len;
                }
            }
        }
        self.drain_evicted();
        Ok(total)
    }

    fn drain_evicted(&self) {
        let Ok(rd) = std::fs::read_dir(self.evicted_dir()) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    pub fn sweep_transient(&self) {
        self.drain_evicted();
        let cutoff = SystemTime::now() - SWEEP_MAX_AGE;
        for dir in [self.tmp_dir(), self.staging_root()] {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in rd.flatten() {
                let Ok(md) = entry.metadata() else { continue };
                if md.modified().map(|m| m > cutoff).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                if md.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}

fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ImposterKey;

    fn set_mtime(path: &Path, ago: Duration) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now() - ago))
            .unwrap();
    }

    fn put(store: &Store, key: &ImposterKey, len: usize, ago: Duration) -> PathBuf {
        let dir = store.level_dir(key.tile.level);
        std::fs::create_dir_all(&dir).unwrap();
        let path = store.zip_path(key);
        std::fs::write(&path, vec![0u8; len]).unwrap();
        set_mtime(&path, ago);
        path
    }

    #[test]
    fn evicts_oldest_first_until_under_budget() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), 250);
        store.init().unwrap();
        let oldest = put(
            &store,
            &ImposterKey::new(0, 0, 0, 1).unwrap(),
            100,
            Duration::from_secs(4 * 3600),
        );
        let middle = put(
            &store,
            &ImposterKey::new(0, 1, 0, 2).unwrap(),
            100,
            Duration::from_secs(3 * 3600),
        );
        let newest = put(
            &store,
            &ImposterKey::new(0, 2, 0, 3).unwrap(),
            100,
            Duration::from_secs(2 * 3600),
        );
        let total = store.evict_pass().unwrap();
        assert!(total <= 250);
        assert!(!oldest.exists());
        assert!(middle.exists());
        assert!(newest.exists());
        assert_eq!(std::fs::read_dir(store.evicted_dir()).unwrap().count(), 0);
    }

    #[test]
    fn young_entries_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), 50);
        store.init().unwrap();
        let young = put(
            &store,
            &ImposterKey::new(0, 0, 0, 1).unwrap(),
            100,
            Duration::ZERO,
        );
        let old = put(
            &store,
            &ImposterKey::new(0, 1, 0, 2).unwrap(),
            100,
            Duration::from_secs(2 * 3600),
        );
        store.evict_pass().unwrap();
        assert!(young.exists());
        assert!(!old.exists());
    }

    #[tokio::test]
    async fn touch_on_serve_reorders_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), 150);
        store.init().unwrap();
        let key_a = ImposterKey::new(0, 0, 0, 1).unwrap();
        let path_a = put(&store, &key_a, 100, Duration::from_secs(4 * 3600));
        let path_b = put(
            &store,
            &ImposterKey::new(0, 1, 0, 2).unwrap(),
            100,
            Duration::from_secs(2 * 3600),
        );
        assert!(store.read_hit(&key_a).await.is_some());
        store.evict_pass().unwrap();
        assert!(path_a.exists());
        assert!(!path_b.exists());
    }

    #[test]
    fn usage_counts_all_levels() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), u64::MAX);
        store.init().unwrap();
        put(
            &store,
            &ImposterKey::new(0, 0, 0, 1).unwrap(),
            10,
            Duration::ZERO,
        );
        put(
            &store,
            &ImposterKey::new(3, -8, 16, 2).unwrap(),
            20,
            Duration::ZERO,
        );
        let usage = store.usage();
        assert_eq!(usage.entries, 2);
        assert_eq!(usage.bytes, 30);
    }

    #[tokio::test]
    async fn read_hit_misses_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), u64::MAX);
        store.init().unwrap();
        let key = ImposterKey::new(0, 0, 0, 1).unwrap();
        assert!(store.read_hit(&key).await.is_none());
    }

    #[test]
    fn sweep_removes_stale_transients() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf(), u64::MAX);
        store.init().unwrap();
        let stale = store.tmp_dir().join("stale");
        std::fs::write(&stale, b"x").unwrap();
        set_mtime(&stale, Duration::from_secs(2 * 86400));
        let fresh = store.tmp_dir().join("fresh");
        std::fs::write(&fresh, b"x").unwrap();
        let stale_dir = store.staging_root().join("old-job");
        std::fs::create_dir_all(&stale_dir).unwrap();
        std::fs::write(stale_dir.join("f"), b"x").unwrap();
        let f = std::fs::File::open(&stale_dir).unwrap();
        f.set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::now() - Duration::from_secs(2 * 86400)),
        )
        .unwrap();
        store.sweep_transient();
        assert!(!stale.exists());
        assert!(fresh.exists());
        assert!(!stale_dir.exists());
    }
}
