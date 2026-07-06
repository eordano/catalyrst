use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::Notify;

pub struct ResponseCache<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    map: DashMap<K, Slot<V>>,
    ttl: Duration,
    max_entries: usize,
    #[allow(dead_code)]
    name: &'static str,
}

struct Slot<V> {
    cached: Option<(Instant, V)>,

    notify: Option<Arc<Notify>>,
}

impl<V> Slot<V> {
    fn empty() -> Self {
        Self {
            cached: None,
            notify: None,
        }
    }
}

struct LeaderGuard<'a, K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    cache: &'a ResponseCache<K, V>,
    key: K,
    notify: Arc<Notify>,

    finished: bool,
}

impl<K, V> Drop for LeaderGuard<'_, K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        if self.finished {
            return;
        }

        if let Some(mut slot) = self.cache.map.get_mut(&self.key) {
            slot.notify = None;
        }

        self.notify.notify_waiters();
    }
}

impl<K, V> ResponseCache<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(name: &'static str, ttl: Duration, max_entries: usize) -> Self {
        Self {
            map: DashMap::new(),
            ttl,
            max_entries,
            name,
        }
    }

    pub async fn get_or_fetch<F, Fut, E>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>> + Send,
    {
        loop {
            let action = {
                let mut entry = self.map.entry(key.clone()).or_insert_with(Slot::empty);
                if let Some((at, v)) = entry.cached.as_ref() {
                    if at.elapsed() < self.ttl {
                        return Ok(v.clone());
                    }
                }
                if let Some(notify) = entry.notify.as_ref() {
                    Action::Wait(notify.clone())
                } else {
                    let notify = Arc::new(Notify::new());
                    entry.notify = Some(notify.clone());
                    Action::Lead(notify)
                }
            };

            match action {
                Action::Wait(notify) => {
                    notify.notified().await;

                    continue;
                }
                Action::Lead(notify) => {
                    let mut guard = LeaderGuard {
                        cache: self,
                        key: key.clone(),
                        notify: notify.clone(),
                        finished: false,
                    };

                    if self.map.len() > self.max_entries {
                        self.map.clear();

                        self.map.insert(
                            key.clone(),
                            Slot {
                                cached: None,
                                notify: Some(notify.clone()),
                            },
                        );
                    }

                    let result = fetch().await;
                    match result {
                        Ok(value) => {
                            if let Some(mut slot) = self.map.get_mut(&key) {
                                slot.cached = Some((Instant::now(), value.clone()));
                                slot.notify = None;
                            } else {
                                self.map.insert(
                                    key.clone(),
                                    Slot {
                                        cached: Some((Instant::now(), value.clone())),
                                        notify: None,
                                    },
                                );
                            }
                            guard.finished = true;
                            notify.notify_waiters();
                            return Ok(value);
                        }
                        Err(e) => {
                            if let Some(mut slot) = self.map.get_mut(&key) {
                                slot.notify = None;
                            }
                            guard.finished = true;
                            notify.notify_waiters();
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub fn clear(&self) {
        self.map.clear();
    }
}

enum Action {
    Wait(Arc<Notify>),
    Lead(Arc<Notify>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn cache_hit_returns_clone() {
        let cache: ResponseCache<String, i32> =
            ResponseCache::new("test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let v1 = cache
            .get_or_fetch("k".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(42)
            })
            .await
            .unwrap();
        assert_eq!(v1, 42);
        let c = counter.clone();
        let v2 = cache
            .get_or_fetch("k".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(100)
            })
            .await
            .unwrap();

        assert_eq!(v2, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expired_entry_refetched() {
        let cache: ResponseCache<&'static str, i32> =
            ResponseCache::new("test", Duration::from_millis(20), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let _ = cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(1)
            })
            .await;
        sleep(Duration::from_millis(40)).await;
        let c = counter.clone();
        let v = cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(2)
            })
            .await
            .unwrap();
        assert_eq!(v, 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_misses_coalesce_into_one_call() {
        let cache: Arc<ResponseCache<&'static str, i32>> =
            Arc::new(ResponseCache::new("test", Duration::from_secs(60), 100));
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("k", || async move {
                        counter.fetch_add(1, Ordering::SeqCst);

                        sleep(Duration::from_millis(50)).await;
                        Ok::<_, ()>(7)
                    })
                    .await
                    .unwrap()
            }));
        }
        for h in handles {
            let v = h.await.unwrap();
            assert_eq!(v, 7);
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "fetch closure must run exactly once for coalesced callers"
        );
    }

    #[tokio::test]
    async fn cancelled_leader_does_not_deadlock_waiters() {
        let cache: Arc<ResponseCache<&'static str, i32>> =
            Arc::new(ResponseCache::new("test", Duration::from_secs(60), 100));

        let cache_clone = cache.clone();
        let leader = tokio::spawn(async move {
            cache_clone
                .get_or_fetch("k", || async {
                    sleep(Duration::from_secs(3600)).await;
                    Ok::<_, ()>(42)
                })
                .await
                .unwrap()
        });

        sleep(Duration::from_millis(50)).await;

        let cache_clone = cache.clone();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let waiter = tokio::spawn(async move {
            cache_clone
                .get_or_fetch("k", || async move {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(99)
                })
                .await
                .unwrap()
        });

        sleep(Duration::from_millis(50)).await;

        leader.abort();
        let _ = leader.await;

        let v = timeout(Duration::from_secs(5), waiter)
            .await
            .expect("waiter must not deadlock")
            .unwrap();
        assert_eq!(v, 99);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let cache: ResponseCache<&'static str, i32> =
            ResponseCache::new("test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let r1 = cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, &'static str>("boom")
            })
            .await;
        assert!(r1.is_err());
        let c = counter.clone();
        let r2 = cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(7)
            })
            .await;
        assert_eq!(r2.unwrap(), 7);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn clear_drops_entries() {
        let cache: ResponseCache<&'static str, i32> =
            ResponseCache::new("test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(1)
            })
            .await
            .unwrap();
        cache.clear();
        let c = counter.clone();
        cache
            .get_or_fetch("k", || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(2)
            })
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
