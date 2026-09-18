use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};
use uuid::Uuid;

pub struct PullCache {
    dir: PathBuf,
    budget: u64,
    index: Mutex<LruIndex>,
}

#[derive(Default)]
struct LruIndex {
    total: u64,
    next_seq: u64,
    entries: HashMap<String, (u64, u64)>,
    order: BTreeMap<u64, String>,
}

impl LruIndex {
    fn touch(&mut self, hash: &str) {
        let Some((_, seq)) = self.entries.get_mut(hash) else {
            return;
        };
        self.order.remove(seq);
        *seq = self.next_seq;
        self.order.insert(self.next_seq, hash.to_string());
        self.next_seq += 1;
    }

    fn insert(&mut self, hash: String, size: u64) {
        if let Some((old_size, old_seq)) = self.entries.remove(&hash) {
            self.order.remove(&old_seq);
            self.total -= old_size;
        }
        self.order.insert(self.next_seq, hash.clone());
        self.entries.insert(hash, (size, self.next_seq));
        self.next_seq += 1;
        self.total += size;
    }

    fn remove(&mut self, hash: &str) -> Option<u64> {
        let (size, seq) = self.entries.remove(hash)?;
        self.order.remove(&seq);
        self.total -= size;
        Some(size)
    }

    fn over_budget(&mut self, budget: u64) -> Vec<String> {
        let mut victims = Vec::new();
        while self.total > budget {
            let Some((_, hash)) = self.order.pop_first() else {
                break;
            };
            if let Some((size, _)) = self.entries.remove(&hash) {
                self.total -= size;
            }
            victims.push(hash);
        }
        victims
    }
}

pub async fn write_atomic(dir: &Path, hash: &str, bytes: &[u8]) -> Result<()> {
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("create {}", dir.display()))?;
    let tmp = dir.join(format!(".{hash}.{}.tmp", Uuid::new_v4()));
    tokio::fs::write(&tmp, bytes)
        .await
        .with_context(|| format!("write {}", tmp.display()))?;
    tokio::fs::rename(&tmp, dir.join(hash))
        .await
        .with_context(|| format!("rename {}", tmp.display()))
}

impl PullCache {
    pub fn open(dir: impl Into<PathBuf>, budget: u64) -> Result<Self> {
        if budget == 0 {
            return Err(anyhow!("pull cache budget must be positive"));
        }
        let dir = dir.into();
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let mut found: Vec<(SystemTime, String, u64)> = Vec::new();
        for entry in std::fs::read_dir(&dir).with_context(|| format!("scan {}", dir.display()))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata()?;
            if !meta.is_file() {
                continue;
            }
            if !catalyrst_hashing::is_canonical_cid(&name) {
                let _ = std::fs::remove_file(entry.path());
                continue;
            }
            found.push((
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                name,
                meta.len(),
            ));
        }
        found.sort();
        let mut index = LruIndex::default();
        for (_, name, len) in found {
            index.insert(name, len);
        }
        for hash in index.over_budget(budget) {
            let _ = std::fs::remove_file(dir.join(&hash));
        }
        Ok(Self {
            dir,
            budget,
            index: Mutex::new(index),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    pub fn total_bytes(&self) -> u64 {
        self.index.lock().map(|i| i.total).unwrap_or(0)
    }

    pub fn contains(&self, hash: &str) -> bool {
        self.index
            .lock()
            .map(|i| i.entries.contains_key(hash))
            .unwrap_or(false)
    }

    pub async fn read(&self, hash: &str) -> std::io::Result<Option<Vec<u8>>> {
        match tokio::fs::read(self.dir.join(hash)).await {
            Ok(bytes) => {
                if let Ok(mut index) = self.index.lock() {
                    index.touch(hash);
                }
                Ok(Some(bytes))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Ok(mut index) = self.index.lock() {
                    index.remove(hash);
                }
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn store(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > self.budget {
            return Err(anyhow!(
                "{hash} is {} bytes, over the {} byte pull cache budget",
                bytes.len(),
                self.budget
            ));
        }
        write_atomic(&self.dir, hash, bytes).await?;
        let victims = match self.index.lock() {
            Ok(mut index) => {
                index.insert(hash.to_string(), bytes.len() as u64);
                index.over_budget(self.budget)
            }
            Err(_) => Vec::new(),
        };
        for victim in victims {
            if let Err(e) = tokio::fs::remove_file(self.dir.join(&victim)).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(error = %e, hash = %victim, "pull cache eviction failed");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("catalyrst-pull-cache-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cid(bytes: &[u8]) -> String {
        catalyrst_hashing::hash_bytes_v1(bytes)
    }

    #[tokio::test]
    async fn evicts_least_recently_used_entries_past_the_budget() {
        let cache = PullCache::open(scratch(), 10).unwrap();
        let (a, b, c) = (b"aaaa".as_slice(), b"bbbb".as_slice(), b"cccc".as_slice());
        let (ha, hb, hc) = (cid(a), cid(b), cid(c));
        cache.store(&ha, a).await.unwrap();
        cache.store(&hb, b).await.unwrap();
        assert_eq!(cache.total_bytes(), 8);

        assert_eq!(cache.read(&ha).await.unwrap().as_deref(), Some(a));
        cache.store(&hc, c).await.unwrap();

        assert!(cache.contains(&ha));
        assert!(!cache.contains(&hb));
        assert!(cache.contains(&hc));
        assert!(!cache.dir().join(&hb).exists());
        assert_eq!(cache.read(&hb).await.unwrap(), None);
        assert_eq!(cache.total_bytes(), 8);
    }

    #[tokio::test]
    async fn refuses_entries_larger_than_the_budget() {
        let cache = PullCache::open(scratch(), 3).unwrap();
        let bytes = b"too big";
        let err = cache.store(&cid(bytes), bytes).await.unwrap_err();
        assert!(err.to_string().contains("budget"));
        assert_eq!(cache.total_bytes(), 0);
    }

    #[tokio::test]
    async fn reopen_indexes_existing_files_and_trims_to_a_smaller_budget() {
        let dir = scratch();
        let older = b"older-bytes";
        let newer = b"newer-bytes";
        let (h_old, h_new) = (cid(older), cid(newer));
        std::fs::write(dir.join(&h_old), older).unwrap();
        std::fs::write(dir.join("not-a-cid.tmp"), b"junk").unwrap();
        let old_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        std::fs::File::open(dir.join(&h_old))
            .unwrap()
            .set_modified(old_time)
            .unwrap();
        std::fs::write(dir.join(&h_new), newer).unwrap();

        let cache = PullCache::open(&dir, 100).unwrap();
        assert!(cache.contains(&h_old));
        assert!(cache.contains(&h_new));
        assert!(!dir.join("not-a-cid.tmp").exists());
        drop(cache);

        let cache = PullCache::open(&dir, 12).unwrap();
        assert!(!cache.contains(&h_old));
        assert!(cache.contains(&h_new));
        assert!(!dir.join(&h_old).exists());
    }

    #[test]
    fn zero_budget_is_rejected() {
        assert!(PullCache::open(scratch(), 0).is_err());
    }
}
