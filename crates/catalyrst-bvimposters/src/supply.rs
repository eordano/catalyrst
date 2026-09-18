use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::body::Bytes;
use tokio::io::AsyncWriteExt;

use crate::key::ImposterKey;
use crate::store::Store;

const MISS_TTL: Duration = Duration::from_secs(60);
const MISS_MAX_ENTRIES: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Store,
    Cdn,
}

pub enum Served {
    Hit(Bytes, Source),
    Miss,
}

struct ByteLru {
    budget: usize,
    used: usize,
    seq: u64,
    map: HashMap<ImposterKey, (Bytes, u64)>,
    order: BTreeMap<u64, ImposterKey>,
}

impl ByteLru {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            used: 0,
            seq: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn get(&mut self, key: &ImposterKey) -> Option<Bytes> {
        let (bytes, seq) = self.map.get_mut(key)?;
        self.order.remove(seq);
        self.seq += 1;
        *seq = self.seq;
        self.order.insert(self.seq, *key);
        Some(bytes.clone())
    }

    fn insert(&mut self, key: ImposterKey, bytes: Bytes) {
        if bytes.len() > self.budget {
            return;
        }
        self.remove(&key);
        self.seq += 1;
        self.used += bytes.len();
        self.map.insert(key, (bytes, self.seq));
        self.order.insert(self.seq, key);
        while self.used > self.budget {
            let Some((_, victim)) = self.order.pop_first() else {
                break;
            };
            if let Some((old, _)) = self.map.remove(&victim) {
                self.used -= old.len();
            }
        }
    }

    fn remove(&mut self, key: &ImposterKey) {
        if let Some((old, seq)) = self.map.remove(key) {
            self.order.remove(&seq);
            self.used -= old.len();
        }
    }
}

pub struct Supply {
    store: Arc<Store>,
    locks: Mutex<HashMap<ImposterKey, Arc<tokio::sync::Mutex<()>>>>,
    zips: Mutex<ByteLru>,
    specs: Mutex<ByteLru>,
    misses: Mutex<HashMap<ImposterKey, Instant>>,
}

impl Supply {
    pub fn new(store: Arc<Store>, mem_budget: usize) -> Self {
        Self {
            store,
            locks: Mutex::new(HashMap::new()),
            zips: Mutex::new(ByteLru::new(mem_budget)),
            specs: Mutex::new(ByteLru::new((mem_budget / 8).max(1 << 20))),
            misses: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get<F, Fut>(&self, key: &ImposterKey, fetch: F) -> Result<Served>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<Vec<u8>>>>,
    {
        if let Some(bytes) = self.cached(key) {
            return Ok(Served::Hit(bytes, Source::Store));
        }
        if self.is_known_miss(key) {
            return Ok(Served::Miss);
        }
        let lock = self.lock_for(key);
        let result = {
            let _guard = lock.lock().await;
            if let Some(bytes) = self.cached(key) {
                Ok(Served::Hit(bytes, Source::Store))
            } else if let Some(bytes) = self.read_store(key).await {
                Ok(Served::Hit(bytes, Source::Store))
            } else {
                match fetch().await {
                    Ok(Some(bytes)) => {
                        let bytes = Bytes::from(bytes);
                        match self.land(key, &bytes).await {
                            Ok(()) => {
                                self.remember(key, bytes.clone());
                                Ok(Served::Hit(bytes, Source::Cdn))
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Ok(None) => {
                        self.remember_miss(key);
                        Ok(Served::Miss)
                    }
                    Err(e) => Err(e),
                }
            }
        };
        self.release(key, lock);
        result
    }

    // Store-only read that bypasses the memory caches (quarantined keys).
    pub async fn get_stored(&self, key: &ImposterKey) -> Served {
        match self.store.read_hit(key).await {
            Some(bytes) => Served::Hit(Bytes::from(bytes), Source::Store),
            None => Served::Miss,
        }
    }

    pub async fn spec(&self, key: &ImposterKey) -> Result<Option<Bytes>> {
        if let Some(spec) = self.specs.lock().unwrap().get(key) {
            return Ok(Some(spec));
        }
        let zip = match self.cached(key) {
            Some(zip) => zip,
            None => match self.read_store(key).await {
                Some(zip) => zip,
                None => return Ok(None),
            },
        };
        let spec = Bytes::from(crate::zips::extract_spec(&zip, key)?);
        self.specs.lock().unwrap().insert(*key, spec.clone());
        Ok(Some(spec))
    }

    fn cached(&self, key: &ImposterKey) -> Option<Bytes> {
        let bytes = self.zips.lock().unwrap().get(key)?;
        self.store.note_access(key);
        Some(bytes)
    }

    async fn read_store(&self, key: &ImposterKey) -> Option<Bytes> {
        let bytes = Bytes::from(self.store.read_hit(key).await?);
        self.remember(key, bytes.clone());
        Some(bytes)
    }

    fn remember(&self, key: &ImposterKey, bytes: Bytes) {
        self.misses.lock().unwrap().remove(key);
        self.zips.lock().unwrap().insert(*key, bytes);
    }

    fn remember_miss(&self, key: &ImposterKey) {
        let mut misses = self.misses.lock().unwrap();
        let now = Instant::now();
        if misses.len() >= MISS_MAX_ENTRIES {
            misses.retain(|_, until| *until > now);
            if misses.len() >= MISS_MAX_ENTRIES {
                misses.clear();
            }
        }
        misses.insert(*key, now + MISS_TTL);
    }

    fn is_known_miss(&self, key: &ImposterKey) -> bool {
        let mut misses = self.misses.lock().unwrap();
        match misses.get(key) {
            Some(until) if *until > Instant::now() => true,
            Some(_) => {
                misses.remove(key);
                false
            }
            None => false,
        }
    }

    async fn land(&self, key: &ImposterKey, bytes: &[u8]) -> Result<()> {
        crate::zips::verify_zip(bytes, key)?;
        let tmp = self.store.tmp_dir().join(uuid::Uuid::new_v4().to_string());
        let mut f = tokio::fs::File::create(&tmp)
            .await
            .with_context(|| format!("creating {}", tmp.display()))?;
        f.write_all(bytes)
            .await
            .with_context(|| format!("writing {}", tmp.display()))?;
        f.sync_all()
            .await
            .with_context(|| format!("syncing {}", tmp.display()))?;
        drop(f);
        self.store.land_tmp(&tmp, key, bytes.len() as u64).await?;
        self.store.spawn_evict_if_over_budget();
        Ok(())
    }

    fn lock_for(&self, key: &ImposterKey) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().unwrap().entry(*key).or_default().clone()
    }

    fn release(&self, key: &ImposterKey, lock: Arc<tokio::sync::Mutex<()>>) {
        let mut map = self.locks.lock().unwrap();
        drop(lock);
        if map
            .get(key)
            .map(|entry| Arc::strong_count(entry) == 1)
            .unwrap_or(false)
        {
            map.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const MEM: usize = 64 << 20;

    #[tokio::test]
    async fn coalesces_concurrent_fetches() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Arc::new(Supply::new(store.clone(), MEM));
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let bytes = crate::zips::test_zip_bytes(0, 100, 3504527830);
        let count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let supply = supply.clone();
            let count = count.clone();
            let bytes = bytes.clone();
            handles.push(tokio::spawn(async move {
                let fetch = move || async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok(Some(bytes))
                };
                match supply.get(&key, fetch).await.unwrap() {
                    Served::Hit(body, _) => body,
                    Served::Miss => panic!("miss"),
                }
            }));
        }
        for handle in handles {
            assert_eq!(handle.await.unwrap(), bytes);
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(supply.locks.lock().unwrap().is_empty());
        assert!(store.zip_path(&key).exists());
        assert_eq!(store.usage_snapshot().entries, 1);
        assert_eq!(store.usage_snapshot().bytes, bytes.len() as u64);
    }

    #[tokio::test]
    async fn miss_does_not_land_anything() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 123).unwrap();
        let served = supply.get(&key, || async { Ok(None) }).await.unwrap();
        assert!(matches!(served, Served::Miss));
        assert_eq!(store.usage().entries, 0);
    }

    #[tokio::test]
    async fn miss_is_negatively_cached_until_landed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        assert!(matches!(
            supply.get(&key, || async { Ok(None) }).await.unwrap(),
            Served::Miss
        ));
        let fetched = Arc::new(AtomicUsize::new(0));
        let counter = fetched.clone();
        let bytes = crate::zips::test_zip_bytes(0, 100, 3504527830);
        let again = supply
            .get(&key, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Some(bytes))
            })
            .await
            .unwrap();
        assert!(matches!(again, Served::Miss));
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn bad_upstream_body_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 123).unwrap();
        let result = supply
            .get(&key, || async { Ok(Some(b"not a zip".to_vec())) })
            .await;
        assert!(result.is_err());
        assert_eq!(store.usage().entries, 0);
        assert!(!supply.is_known_miss(&key));
    }

    #[tokio::test]
    async fn second_get_serves_from_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let bytes = crate::zips::test_zip_bytes(0, 100, 3504527830);
        let first = supply
            .get(&key, || async { Ok(Some(bytes.clone())) })
            .await
            .unwrap();
        assert!(matches!(first, Served::Hit(_, Source::Cdn)));
        let fetched = Arc::new(AtomicUsize::new(0));
        let counter = fetched.clone();
        let second = supply
            .get(&key, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            })
            .await
            .unwrap();
        match second {
            Served::Hit(body, Source::Store) => assert_eq!(body, bytes),
            _ => panic!("expected store hit"),
        }
        assert_eq!(fetched.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn memory_hit_skips_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let bytes = crate::zips::test_zip_bytes(0, 100, 3504527830);
        std::fs::create_dir_all(store.level_dir(0)).unwrap();
        std::fs::write(store.zip_path(&key), &bytes).unwrap();
        let first = supply.get(&key, || async { Ok(None) }).await.unwrap();
        assert!(matches!(first, Served::Hit(_, Source::Store)));
        std::fs::remove_file(store.zip_path(&key)).unwrap();
        match supply.get(&key, || async { Ok(None) }).await.unwrap() {
            Served::Hit(body, Source::Store) => assert_eq!(body, bytes),
            _ => panic!("expected memory hit"),
        }
        assert!(matches!(supply.get_stored(&key).await, Served::Miss));
    }

    #[tokio::test]
    async fn spec_is_memoized_from_the_stored_zip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone(), MEM);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        assert!(supply.spec(&key).await.unwrap().is_none());
        std::fs::create_dir_all(store.level_dir(0)).unwrap();
        std::fs::write(
            store.zip_path(&key),
            crate::zips::test_zip_bytes(0, 100, 3504527830),
        )
        .unwrap();
        let spec = supply.spec(&key).await.unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&spec).unwrap();
        assert_eq!(value["crc"].as_u64(), Some(3504527830));
        std::fs::remove_file(store.zip_path(&key)).unwrap();
        assert_eq!(supply.spec(&key).await.unwrap().unwrap(), spec);
    }

    #[test]
    fn byte_lru_evicts_least_recently_used_within_budget() {
        let mut lru = ByteLru::new(10);
        let a = ImposterKey::new(0, 0, 0, 1).unwrap();
        let b = ImposterKey::new(0, 1, 0, 2).unwrap();
        let c = ImposterKey::new(0, 2, 0, 3).unwrap();
        lru.insert(a, Bytes::from_static(&[0; 4]));
        lru.insert(b, Bytes::from_static(&[0; 4]));
        assert!(lru.get(&a).is_some());
        lru.insert(c, Bytes::from_static(&[0; 4]));
        assert!(lru.get(&b).is_none());
        assert!(lru.get(&a).is_some());
        assert!(lru.get(&c).is_some());
        assert_eq!(lru.used, 8);
        lru.insert(a, Bytes::from_static(&[0; 11]));
        assert!(lru.get(&a).is_some());
        assert_eq!(lru.used, 8);
    }
}
