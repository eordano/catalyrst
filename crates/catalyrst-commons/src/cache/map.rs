use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::Notify;
use tokio::time::Instant;

struct Entry<V> {
    generation: u64,
    at: Instant,
    value: V,
}

struct Slot<V> {
    cached: Option<Entry<V>>,
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

enum Action {
    Wait(Arc<Notify>),
    Lead(Arc<Notify>),
}

const WAIT_RECHECK: Duration = Duration::from_millis(250);

/// Concurrent misses on the same key elect one leader and park the rest on a
/// [`Notify`]; the leader always wakes them, including when its task is cancelled
/// mid-fetch (see `LeaderGuard`), so a dropped caller can never strand the others.
/// Waiters park with a short re-check bound as well, so even a wakeup that lands before
/// a waiter registers costs one extra loop rather than the whole request.
///
/// [`TtlMap::bump_generation`] invalidates every entry at once without walking the map:
/// entries record the generation in force when their fetch *started*, so a bump also
/// discards results already in flight.
pub struct TtlMap<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    map: DashMap<K, Slot<V>>,
    ttl: Duration,
    max_entries: Option<usize>,
    generation: AtomicU64,
    name: &'static str,
}

struct LeaderGuard<'a, K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    cache: &'a TtlMap<K, V>,
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

impl<K, V> TtlMap<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(name: &'static str, ttl: Duration) -> Self {
        Self {
            map: DashMap::new(),
            ttl,
            max_entries: None,
            generation: AtomicU64::new(0),
            name,
        }
    }

    pub fn bounded(name: &'static str, ttl: Duration, max_entries: usize) -> Self {
        Self {
            max_entries: Some(max_entries),
            ..Self::new(name, ttl)
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Invalidates every entry, present and in flight, in O(1).
    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&self) {
        self.map.clear();
    }

    /// Drops only the entries a read would refuse anyway -- stale, or superseded by a
    /// generation bump -- so a bounded cache at its cap can shed dead weight before
    /// resorting to [`TtlMap::clear`]. Slots with a fetch in flight are kept, so no
    /// waiter loses its wakeup.
    pub fn retain_fresh(&self) {
        let generation = self.generation();
        let ttl = self.ttl;
        self.map.retain(|_, slot| {
            if slot.notify.is_some() {
                return true;
            }
            match slot.cached.as_ref() {
                Some(entry) => entry.generation == generation && entry.at.elapsed() < ttl,
                None => false,
            }
        });
    }

    /// Clears one key's value while leaving any in-flight leader's wakeup intact.
    pub fn invalidate(&self, key: &K) {
        if let Some(mut slot) = self.map.get_mut(key) {
            slot.cached = None;
        }
    }

    pub fn get_fresh(&self, key: &K) -> Option<V> {
        let generation = self.generation();
        let slot = self.map.get(key)?;
        let entry = slot.cached.as_ref()?;
        (entry.generation == generation && entry.at.elapsed() < self.ttl)
            .then(|| entry.value.clone())
    }

    pub fn insert(&self, key: K, value: V) {
        let generation = self.generation();
        let mut slot = self.map.entry(key).or_insert_with(Slot::empty);
        slot.cached = Some(Entry {
            generation,
            at: Instant::now(),
            value,
        });
    }

    pub async fn get_or_fetch<F, Fut, E>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        loop {
            let generation = self.generation();
            let action = {
                let mut entry = self.map.entry(key.clone()).or_insert_with(Slot::empty);
                if let Some(cached) = entry.cached.as_ref() {
                    if cached.generation == generation && cached.at.elapsed() < self.ttl {
                        return Ok(cached.value.clone());
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
                    let _ = tokio::time::timeout(WAIT_RECHECK, notify.notified()).await;
                    continue;
                }
                Action::Lead(notify) => {
                    let mut guard = LeaderGuard {
                        cache: self,
                        key: key.clone(),
                        notify: notify.clone(),
                        finished: false,
                    };

                    if let Some(max) = self.max_entries {
                        if self.map.len() > max {
                            self.map.clear();
                            self.map.insert(
                                key.clone(),
                                Slot {
                                    cached: None,
                                    notify: Some(notify.clone()),
                                },
                            );
                        }
                    }

                    match fetch().await {
                        Ok(value) => {
                            let entry = Entry {
                                generation,
                                at: Instant::now(),
                                value: value.clone(),
                            };
                            match self.map.get_mut(&key) {
                                Some(mut slot) => {
                                    slot.cached = Some(entry);
                                    slot.notify = None;
                                }
                                None => {
                                    self.map.insert(
                                        key.clone(),
                                        Slot {
                                            cached: Some(entry),
                                            notify: None,
                                        },
                                    );
                                }
                            }
                            guard.finished = true;
                            notify.notify_waiters();
                            return Ok(value);
                        }
                        Err(err) => {
                            if let Some(mut slot) = self.map.get_mut(&key) {
                                slot.notify = None;
                            }
                            guard.finished = true;
                            notify.notify_waiters();
                            return Err(err);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::time::timeout;

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[tokio::test(start_paused = true)]
    async fn cache_hit_returns_clone() {
        let cache: TtlMap<String, i32> = TtlMap::bounded("test", Duration::from_secs(60), 100);
        let calls = counter();

        let c = calls.clone();
        let v1 = cache
            .get_or_fetch("k".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(42)
            })
            .await
            .unwrap();
        let c = calls.clone();
        let v2 = cache
            .get_or_fetch("k".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(100)
            })
            .await
            .unwrap();

        assert_eq!((v1, v2), (42, 42));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn expired_entry_refetched() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(30));
        cache
            .get_or_fetch("k", || async { Ok::<_, ()>(1) })
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(31)).await;
        let v = cache
            .get_or_fetch("k", || async { Ok::<_, ()>(2) })
            .await
            .unwrap();
        assert_eq!(v, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_misses_coalesce_into_one_call() {
        let cache: Arc<TtlMap<&'static str, i32>> =
            Arc::new(TtlMap::bounded("test", Duration::from_secs(60), 100));
        let calls = counter();

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cache = cache.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("k", || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok::<_, ()>(7)
                    })
                    .await
                    .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 7);
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "fetch closure must run exactly once for coalesced callers"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_leader_does_not_deadlock_waiters() {
        let cache: Arc<TtlMap<&'static str, i32>> =
            Arc::new(TtlMap::new("test", Duration::from_secs(60)));

        let leader_cache = cache.clone();
        let leader = tokio::spawn(async move {
            leader_cache
                .get_or_fetch("k", || async {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    Ok::<_, ()>(42)
                })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let waiter_cache = cache.clone();
        let calls = counter();
        let c = calls.clone();
        let waiter = tokio::spawn(async move {
            waiter_cache
                .get_or_fetch("k", || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(99)
                })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        leader.abort();
        let _ = leader.await;

        let v = timeout(Duration::from_secs(5), waiter)
            .await
            .expect("waiter must not deadlock")
            .unwrap();
        assert_eq!(v, 99);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn errors_are_not_cached() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(60));
        let r1 = cache
            .get_or_fetch("k", || async { Err::<i32, &'static str>("boom") })
            .await;
        assert!(r1.is_err());
        let r2 = cache
            .get_or_fetch("k", || async { Ok::<_, &'static str>(7) })
            .await;
        assert_eq!(r2.unwrap(), 7);
    }

    #[tokio::test(start_paused = true)]
    async fn clear_drops_entries() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(60));
        cache
            .get_or_fetch("k", || async { Ok::<_, ()>(1) })
            .await
            .unwrap();
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        let v = cache
            .get_or_fetch("k", || async { Ok::<_, ()>(2) })
            .await
            .unwrap();
        assert_eq!(v, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn bump_generation_invalidates_everything() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(600));
        cache.insert("a", 1);
        cache.insert("b", 2);
        assert_eq!(cache.get_fresh(&"a"), Some(1));
        assert_eq!(cache.get_fresh(&"b"), Some(2));

        cache.bump_generation();
        assert_eq!(cache.get_fresh(&"a"), None, "a bump must invalidate");
        assert_eq!(cache.get_fresh(&"b"), None);

        let v = cache
            .get_or_fetch("a", || async { Ok::<_, ()>(9) })
            .await
            .unwrap();
        assert_eq!(v, 9);
    }

    #[tokio::test(start_paused = true)]
    async fn bump_during_a_fetch_discards_the_in_flight_result() {
        let cache: Arc<TtlMap<&'static str, i32>> =
            Arc::new(TtlMap::new("test", Duration::from_secs(600)));

        let fetching = cache.clone();
        let handle = tokio::spawn(async move {
            fetching
                .get_or_fetch("k", || async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<_, ()>(1)
                })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        cache.bump_generation();
        assert_eq!(handle.await.unwrap(), 1);

        assert_eq!(
            cache.get_fresh(&"k"),
            None,
            "a result whose fetch straddled the bump must not be served"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn invalidate_evicts_a_single_key() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(600));
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.invalidate(&"a");
        assert_eq!(cache.get_fresh(&"a"), None);
        assert_eq!(cache.get_fresh(&"b"), Some(2));
    }

    #[tokio::test(start_paused = true)]
    async fn max_entries_bounds_the_map() {
        let cache: TtlMap<u32, u32> = TtlMap::bounded("test", Duration::from_secs(600), 4);
        for k in 0..32u32 {
            cache
                .get_or_fetch(k, || async move { Ok::<_, ()>(k) })
                .await
                .unwrap();
        }
        assert!(cache.len() <= 5, "bounded map grew to {}", cache.len());
    }

    #[tokio::test(start_paused = true)]
    async fn a_waiter_that_missed_its_wakeup_recovers_instead_of_hanging() {
        let cache: Arc<TtlMap<&'static str, i32>> =
            Arc::new(TtlMap::new("test", Duration::from_secs(600)));

        cache.map.insert(
            "k",
            Slot {
                cached: None,
                notify: Some(Arc::new(Notify::new())),
            },
        );

        let waiter_cache = cache.clone();
        let waiter = tokio::spawn(async move {
            waiter_cache
                .get_or_fetch("k", || async { Ok::<_, ()>(5) })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(10)).await;

        if let Some(mut slot) = cache.map.get_mut(&"k") {
            slot.notify = None;
        }

        let v = timeout(Duration::from_secs(5), waiter)
            .await
            .expect("a waiter whose leader never signalled must still finish")
            .unwrap();
        assert_eq!(v, 5);
    }

    #[tokio::test(start_paused = true)]
    async fn retain_fresh_drops_only_what_a_read_would_refuse() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(30));
        cache.insert("old", 1);
        tokio::time::advance(Duration::from_secs(31)).await;
        cache.insert("new", 2);

        cache.retain_fresh();
        assert_eq!(cache.get_fresh(&"old"), None);
        assert_eq!(cache.get_fresh(&"new"), Some(2));
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retain_fresh_drops_everything_after_a_generation_bump() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(600));
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.bump_generation();
        cache.retain_fresh();
        assert!(cache.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn retain_fresh_keeps_a_slot_with_a_fetch_in_flight() {
        let cache: Arc<TtlMap<&'static str, i32>> =
            Arc::new(TtlMap::new("test", Duration::from_secs(600)));

        let leader_cache = cache.clone();
        let leader = tokio::spawn(async move {
            leader_cache
                .get_or_fetch("k", || async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<_, ()>(3)
                })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(10)).await;

        cache.retain_fresh();
        assert_eq!(cache.len(), 1, "an in-flight leader's slot must survive");
        assert_eq!(leader.await.unwrap(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn stale_entry_is_not_returned_by_get_fresh() {
        let cache: TtlMap<&'static str, i32> = TtlMap::new("test", Duration::from_secs(30));
        cache.insert("k", 1);
        tokio::time::advance(Duration::from_secs(31)).await;
        assert_eq!(cache.get_fresh(&"k"), None);
    }
}
