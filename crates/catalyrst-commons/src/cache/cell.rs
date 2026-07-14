use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

struct Slot<T> {
    cached: Option<Cached<T>>,
    attempted_at: Option<Instant>,
}

impl<T> Slot<T> {
    fn empty() -> Self {
        Self {
            cached: None,
            attempted_at: None,
        }
    }
}

/// Single-slot async cache: one value, refetched once its TTL lapses.
///
/// A failed refresh serves the previous value rather than propagating the error, so a
/// flaky upstream degrades into staleness instead of an outage. The refresh runs under
/// the slot mutex, so concurrent callers coalesce onto one fetch.
pub struct TtlCell<T> {
    name: &'static str,
    slot: Mutex<Slot<T>>,
}

impl<T: Clone> TtlCell<T> {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            slot: Mutex::new(Slot::empty()),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub async fn get_or_refresh<F, Fut, E>(&self, ttl: Duration, fetch: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: Display,
    {
        let mut slot = self.slot.lock().await;

        let stale = slot
            .cached
            .as_ref()
            .map(|c| (c.value.clone(), c.fetched_at));
        if let Some((value, fetched_at)) = &stale {
            if fetched_at.elapsed() < ttl {
                return Ok(value.clone());
            }
        }

        slot.attempted_at = Some(Instant::now());
        match fetch().await {
            Ok(value) => {
                slot.cached = Some(Cached {
                    value: value.clone(),
                    fetched_at: Instant::now(),
                });
                Ok(value)
            }
            Err(err) => match stale {
                Some((value, _)) => {
                    tracing::warn!(cache = self.name, %err, "refresh failed; serving stale value");
                    Ok(value)
                }
                None => Err(err),
            },
        }
    }

    /// Like [`TtlCell::get_or_refresh`], except a failed fetch also starts the TTL clock,
    /// so an upstream that is down or refusing is retried once per `ttl` instead of on
    /// every call. Returns the freshest value available, or `None` while no fetch has
    /// ever succeeded.
    pub async fn get_or_refresh_backoff<F, Fut, E>(&self, ttl: Duration, fetch: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: Display,
    {
        let mut slot = self.slot.lock().await;

        if let Some(cached) = slot.cached.as_ref() {
            if cached.fetched_at.elapsed() < ttl {
                return Some(cached.value.clone());
            }
        }
        if let Some(attempted_at) = slot.attempted_at {
            if attempted_at.elapsed() < ttl {
                return slot.cached.as_ref().map(|c| c.value.clone());
            }
        }

        slot.attempted_at = Some(Instant::now());
        match fetch().await {
            Ok(value) => {
                slot.cached = Some(Cached {
                    value: value.clone(),
                    fetched_at: Instant::now(),
                });
                Some(value)
            }
            Err(err) => {
                tracing::warn!(cache = self.name, %err, "refresh failed; backing off for the ttl");
                slot.cached.as_ref().map(|c| c.value.clone())
            }
        }
    }

    pub async fn get(&self, ttl: Duration) -> Option<T> {
        let slot = self.slot.lock().await;
        slot.cached
            .as_ref()
            .filter(|c| c.fetched_at.elapsed() < ttl)
            .map(|c| c.value.clone())
    }

    /// The last value fetched, however old. For degraded reads that prefer stale data
    /// over no data.
    pub async fn last(&self) -> Option<T> {
        self.slot
            .lock()
            .await
            .cached
            .as_ref()
            .map(|c| c.value.clone())
    }

    pub async fn set(&self, value: T) {
        let mut slot = self.slot.lock().await;
        slot.cached = Some(Cached {
            value,
            fetched_at: Instant::now(),
        });
    }

    pub async fn invalidate(&self) {
        *self.slot.lock().await = Slot::empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[tokio::test(start_paused = true)]
    async fn fresh_value_is_served_without_refetching() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let calls = counter();

        let c = calls.clone();
        let first = cell
            .get_or_refresh(Duration::from_secs(60), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(1)
            })
            .await
            .unwrap();
        let c = calls.clone();
        let second = cell
            .get_or_refresh(Duration::from_secs(60), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(2)
            })
            .await
            .unwrap();

        assert_eq!((first, second), (1, 1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_refetches() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(30);

        cell.get_or_refresh(ttl, || async { Ok::<_, String>(1) })
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(31)).await;
        let v = cell
            .get_or_refresh(ttl, || async { Ok::<_, String>(2) })
            .await
            .unwrap();

        assert_eq!(v, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_refresh_serves_stale() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(30);

        cell.get_or_refresh(ttl, || async { Ok::<_, String>(7) })
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(31)).await;
        let v = cell
            .get_or_refresh(ttl, || async { Err::<i32, String>("upstream down".into()) })
            .await
            .unwrap();

        assert_eq!(v, 7, "a failed refresh must degrade to the stale value");
    }

    #[tokio::test(start_paused = true)]
    async fn first_fetch_error_propagates() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let r = cell
            .get_or_refresh(Duration::from_secs(30), || async {
                Err::<i32, String>("boom".into())
            })
            .await;
        assert_eq!(r.unwrap_err(), "boom");
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_callers_coalesce_onto_one_fetch() {
        let cell: Arc<TtlCell<i32>> = Arc::new(TtlCell::new("test"));
        let calls = counter();

        let mut handles = Vec::new();
        for _ in 0..16 {
            let cell = cell.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cell.get_or_refresh(Duration::from_secs(60), || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok::<_, String>(42)
                })
                .await
                .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 42);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn get_and_last_track_freshness_separately() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        assert_eq!(cell.get(Duration::from_secs(30)).await, None);
        assert_eq!(cell.last().await, None);

        cell.set(5).await;
        assert_eq!(cell.get(Duration::from_secs(30)).await, Some(5));

        tokio::time::advance(Duration::from_secs(31)).await;
        assert_eq!(cell.get(Duration::from_secs(30)).await, None);
        assert_eq!(cell.last().await, Some(5));

        cell.invalidate().await;
        assert_eq!(cell.last().await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_holds_a_failing_upstream_to_one_attempt_per_ttl() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(30);
        let calls = counter();

        for _ in 0..5 {
            let c = calls.clone();
            let v = cell
                .get_or_refresh_backoff(ttl, || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, String>("upstream down".into())
                })
                .await;
            assert_eq!(v, None);
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "a failed fetch must start the ttl clock like a successful one"
        );

        tokio::time::advance(Duration::from_secs(31)).await;
        let c = calls.clone();
        let v = cell
            .get_or_refresh_backoff(ttl, || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(7)
            })
            .await;
        assert_eq!(v, Some(7));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_serves_stale_while_it_waits_out_the_ttl() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(30);
        let calls = counter();

        cell.get_or_refresh_backoff(ttl, || async { Ok::<_, String>(1) })
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(31)).await;

        for _ in 0..3 {
            let c = calls.clone();
            let v = cell
                .get_or_refresh_backoff(ttl, || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, String>("upstream down".into())
                })
                .await;
            assert_eq!(
                v,
                Some(1),
                "a failed refresh must degrade to the stale value"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn invalidate_clears_the_backoff_too() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(30);

        let v = cell
            .get_or_refresh_backoff(ttl, || async { Err::<i32, String>("boom".into()) })
            .await;
        assert_eq!(v, None);
        cell.invalidate().await;
        let v = cell
            .get_or_refresh_backoff(ttl, || async { Ok::<_, String>(4) })
            .await;
        assert_eq!(v, Some(4));
    }

    #[tokio::test(start_paused = true)]
    async fn invalidate_forces_a_refetch() {
        let cell: TtlCell<i32> = TtlCell::new("test");
        let ttl = Duration::from_secs(600);
        cell.get_or_refresh(ttl, || async { Ok::<_, String>(1) })
            .await
            .unwrap();
        cell.invalidate().await;
        let v = cell
            .get_or_refresh(ttl, || async { Ok::<_, String>(2) })
            .await
            .unwrap();
        assert_eq!(v, 2);
    }
}
