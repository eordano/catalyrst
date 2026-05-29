//! Generic TTL'd in-memory response cache with single-flight coalescing.
//!
//! Public API: [`ResponseCache::new`] + [`ResponseCache::get_or_fetch`].
//!
//! On HIT, returns a clone of the cached value.
//! On MISS, exactly one caller runs the supplied fetch closure; concurrent
//! callers for the same key await its result via [`tokio::sync::Notify`].
//! Errors are NOT cached — the next caller re-tries the closure. If the
//! leader's future is cancelled mid-await, an RAII guard wakes waiters and
//! clears the in-flight slot so a fresh leader can be elected.

use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::Notify;

/// A TTL'd in-memory result cache with single-flight coalescing.
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
    /// In-flight notify; if `Some`, a fetch is running for this key.
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

/// RAII guard for the in-flight leader. If the leader's future is dropped
/// (cancellation) before it publishes a value, the guard clears the in-flight
/// notify and wakes waiters so one of them can become the new leader.
struct LeaderGuard<'a, K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    cache: &'a ResponseCache<K, V>,
    key: K,
    notify: Arc<Notify>,
    /// Set to `true` by the leader on success; suppresses cleanup-on-drop
    /// since the success path has already published + notified.
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
        // Clear the in-flight slot so the next caller becomes the new leader.
        if let Some(mut slot) = self.cache.map.get_mut(&self.key) {
            slot.notify = None;
        }
        // Wake any waiters; they will re-check and re-elect a leader.
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

    /// Fetch-with-coalescing. The closure runs at most once per (key, TTL window)
    /// even under high concurrency. Returns the cached or freshly-fetched value.
    ///
    /// On the closure's `Err` path, the failure is NOT cached — the next caller
    /// re-tries (fail-open for transient errors).
    pub async fn get_or_fetch<F, Fut, E>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<V, E>> + Send,
    {
        loop {
            // Step 1: under shard lock, decide whether to return cached, await, or lead.
            let action = {
                let mut entry = self.map.entry(key.clone()).or_insert_with(Slot::empty);
                if let Some((at, v)) = entry.cached.as_ref() {
                    if at.elapsed() < self.ttl {
                        return Ok(v.clone());
                    }
                }
                if let Some(notify) = entry.notify.as_ref() {
                    // Someone else is fetching — wait on their notify.
                    Action::Wait(notify.clone())
                } else {
                    // Become leader.
                    let notify = Arc::new(Notify::new());
                    entry.notify = Some(notify.clone());
                    Action::Lead(notify)
                }
            };

            match action {
                Action::Wait(notify) => {
                    notify.notified().await;
                    // After being woken, loop and re-check the cache. The leader
                    // may have published a value (HIT next iter) or been
                    // cancelled (in which case we'll become the new leader).
                    continue;
                }
                Action::Lead(notify) => {
                    let mut guard = LeaderGuard {
                        cache: self,
                        key: key.clone(),
                        notify: notify.clone(),
                        finished: false,
                    };

                    // Evict if the cache has grown unbounded. Cheap and infrequent.
                    if self.map.len() > self.max_entries {
                        self.map.clear();
                        // Re-insert our in-flight slot so waiters/late arrivals
                        // can find the notify; the leader is still us.
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
                            // Publish under the shard lock, clear in-flight, notify.
                            if let Some(mut slot) = self.map.get_mut(&key) {
                                slot.cached = Some((Instant::now(), value.clone()));
                                slot.notify = None;
                            } else {
                                // Entry got evicted between fetch start and end;
                                // re-insert the fresh value.
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
                            // Error path: don't cache. Clear in-flight so the next
                            // caller re-tries. Wake waiters so they retry too.
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

    /// Force-clear (for tests). Not used in handlers.
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
        // Second call must HIT (closure never runs) — original value returned.
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
                        // Hold the lead long enough that all 20 callers pile up.
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

        // Spawn the leader task; it sleeps forever inside the fetch closure.
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

        // Give the leader time to register itself as in-flight.
        sleep(Duration::from_millis(50)).await;

        // Spawn a waiter that should pile up on the leader's Notify.
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

        // Let the waiter actually park on the notify.
        sleep(Duration::from_millis(50)).await;

        // Cancel the leader. Its RAII guard must wake waiters.
        leader.abort();
        let _ = leader.await;

        // Waiter must be elected as the new leader, run its closure, and finish.
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
