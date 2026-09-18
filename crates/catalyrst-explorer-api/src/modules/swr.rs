use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::time::Instant;

const WAIT_RECHECK: Duration = Duration::from_millis(250);

struct Slot<T> {
    cached: Option<(T, Instant)>,
    attempted_at: Option<Instant>,
    inflight: Option<Arc<Notify>>,
}

/// Stale-while-revalidate cell: past `ttl` one caller refreshes in a detached task while
/// the rest keep the stale value, callers with nothing cached park on that single fetch,
/// and a failed fetch keeps the stale value and starts the ttl clock. No lock is held
/// across the fetch.
pub(crate) struct SwrCell<T> {
    name: &'static str,
    slot: Arc<Mutex<Slot<T>>>,
}

struct Inflight<T> {
    slot: Arc<Mutex<Slot<T>>>,
    notify: Arc<Notify>,
}

impl<T> Drop for Inflight<T> {
    fn drop(&mut self) {
        let mut slot = self.slot.lock();
        if slot
            .inflight
            .as_ref()
            .is_some_and(|n| Arc::ptr_eq(n, &self.notify))
        {
            slot.inflight = None;
        }
        drop(slot);
        self.notify.notify_waiters();
    }
}

impl<T: Clone + Send + 'static> SwrCell<T> {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            slot: Arc::new(Mutex::new(Slot {
                cached: None,
                attempted_at: None,
                inflight: None,
            })),
        }
    }

    pub(crate) async fn get_or_refresh_backoff<F, Fut, E>(
        &self,
        ttl: Duration,
        fetch: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: Display + Send + 'static,
    {
        let notify = {
            let mut slot = self.slot.lock();
            let stale = match slot.cached.as_ref() {
                Some((value, at)) if at.elapsed() < ttl => return Some(value.clone()),
                Some((value, _)) => Some(value.clone()),
                None => None,
            };
            match slot.inflight.clone() {
                Some(notify) => {
                    if stale.is_some() {
                        return stale;
                    }
                    notify
                }
                None if slot.attempted_at.is_some_and(|at| at.elapsed() < ttl) => return stale,
                None => {
                    let notify = Arc::new(Notify::new());
                    slot.inflight = Some(notify.clone());
                    slot.attempted_at = Some(Instant::now());
                    let guard = Inflight {
                        slot: self.slot.clone(),
                        notify: notify.clone(),
                    };
                    let name = self.name;
                    let fut = fetch();
                    tokio::spawn(async move {
                        let _guard = guard;
                        match fut.await {
                            Ok(value) => {
                                _guard.slot.lock().cached = Some((value, Instant::now()));
                            }
                            Err(err) => {
                                tracing::warn!(cache = name, %err, "refresh failed; serving stale for the ttl");
                            }
                        }
                    });
                    if stale.is_some() {
                        return stale;
                    }
                    notify
                }
            }
        };
        loop {
            let _ = tokio::time::timeout(WAIT_RECHECK, notify.notified()).await;
            let slot = self.slot.lock();
            let still_ours = slot
                .inflight
                .as_ref()
                .is_some_and(|n| Arc::ptr_eq(n, &notify));
            if !still_ours {
                return slot.cached.as_ref().map(|(v, _)| v.clone());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn invalidate(&self) {
        let mut slot = self.slot.lock();
        slot.cached = None;
        slot.attempted_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TTL: Duration = Duration::from_millis(200);

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn concurrent_cold_callers_coalesce_onto_one_fetch() {
        let cell: Arc<SwrCell<i32>> = Arc::new(SwrCell::new("test"));
        let calls = counter();
        let mut handles = Vec::new();
        for _ in 0..16 {
            let cell = cell.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                cell.get_or_refresh_backoff(Duration::from_secs(60), move || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok::<_, String>(42)
                })
                .await
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), Some(42));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stale_is_served_while_one_refresh_runs_in_the_background() {
        let cell: Arc<SwrCell<i32>> = Arc::new(SwrCell::new("test"));
        cell.get_or_refresh_backoff(TTL, || async { Ok::<_, String>(1) })
            .await;
        tokio::time::sleep(TTL + Duration::from_millis(50)).await;

        let calls = counter();
        let started = std::time::Instant::now();
        for _ in 0..8 {
            let c = calls.clone();
            let v = cell
                .get_or_refresh_backoff(TTL, move || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok::<_, String>(2)
                })
                .await;
            assert_eq!(v, Some(1), "stale value must be served without waiting");
        }
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "stale reads must not block"
        );
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let v = cell
            .get_or_refresh_backoff(TTL, || async { Ok::<_, String>(3) })
            .await;
        assert_eq!(v, Some(2));
    }

    #[tokio::test]
    async fn failed_refresh_keeps_stale_and_backs_off_for_the_ttl() {
        let cell: SwrCell<i32> = SwrCell::new("test");
        cell.get_or_refresh_backoff(TTL, || async { Ok::<_, String>(7) })
            .await;
        tokio::time::sleep(TTL + Duration::from_millis(50)).await;

        let calls = counter();
        for _ in 0..5 {
            let c = calls.clone();
            let v = cell
                .get_or_refresh_backoff(TTL, move || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, String>("upstream down".into())
                })
                .await;
            assert_eq!(v, Some(7));
            settle().await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        tokio::time::sleep(TTL).await;
        let c = calls.clone();
        cell.get_or_refresh_backoff(TTL, move || async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok::<_, String>(8)
        })
        .await;
        settle().await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cold_failure_returns_none_and_invalidate_clears_the_backoff() {
        let cell: SwrCell<i32> = SwrCell::new("test");
        let v = cell
            .get_or_refresh_backoff(TTL, || async { Err::<i32, String>("boom".into()) })
            .await;
        assert_eq!(v, None);
        let v = cell
            .get_or_refresh_backoff(TTL, || async { Ok::<_, String>(4) })
            .await;
        assert_eq!(v, None, "a cold failure must back off for the ttl");
        cell.invalidate();
        let v = cell
            .get_or_refresh_backoff(TTL, || async { Ok::<_, String>(4) })
            .await;
        assert_eq!(v, Some(4));
    }
}
