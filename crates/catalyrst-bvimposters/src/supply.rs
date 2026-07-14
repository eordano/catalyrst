use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;

use crate::key::ImposterKey;
use crate::store::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Store,
    Cdn,
}

pub enum Served {
    Hit(Vec<u8>, Source),
    Miss,
}

pub struct Supply {
    store: Arc<Store>,
    locks: Mutex<HashMap<ImposterKey, Arc<tokio::sync::Mutex<()>>>>,
}

impl Supply {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            locks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get<F, Fut>(&self, key: &ImposterKey, fetch: F) -> Result<Served>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<Vec<u8>>>>,
    {
        if let Some(bytes) = self.store.read_hit(key).await {
            return Ok(Served::Hit(bytes, Source::Store));
        }
        let lock = self.lock_for(key);
        let result = {
            let _guard = lock.lock().await;
            if let Some(bytes) = self.store.read_hit(key).await {
                Ok(Served::Hit(bytes, Source::Store))
            } else {
                match fetch().await {
                    Ok(Some(bytes)) => match self.land(key, &bytes).await {
                        Ok(()) => Ok(Served::Hit(bytes, Source::Cdn)),
                        Err(e) => Err(e),
                    },
                    Ok(None) => Ok(Served::Miss),
                    Err(e) => Err(e),
                }
            }
        };
        self.release(key, lock);
        result
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
        self.store.land_tmp(&tmp, key).await?;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let _ = store.evict_pass();
        });
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

    #[tokio::test]
    async fn coalesces_concurrent_fetches() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Arc::new(Supply::new(store.clone()));
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
    }

    #[tokio::test]
    async fn miss_does_not_land_anything() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone());
        let key = ImposterKey::new(0, 0, 100, 123).unwrap();
        let served = supply.get(&key, || async { Ok(None) }).await.unwrap();
        assert!(matches!(served, Served::Miss));
        assert_eq!(store.usage().entries, 0);
    }

    #[tokio::test]
    async fn bad_upstream_body_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone());
        let key = ImposterKey::new(0, 0, 100, 123).unwrap();
        let result = supply
            .get(&key, || async { Ok(Some(b"not a zip".to_vec())) })
            .await;
        assert!(result.is_err());
        assert_eq!(store.usage().entries, 0);
    }

    #[tokio::test]
    async fn second_get_serves_from_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::new(dir.path().to_path_buf(), u64::MAX));
        store.init().unwrap();
        let supply = Supply::new(store.clone());
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
}
