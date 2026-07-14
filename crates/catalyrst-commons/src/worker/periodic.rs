use std::fmt::Display;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use futures::FutureExt;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

/// How a pass that overruns its period is rescheduled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pacing {
    /// Keep the original phase and drop the ticks that were missed. The right choice for
    /// refresh work, where only the latest pass matters.
    #[default]
    Skip,
    /// Keep every missed tick and fire them back to back. For work where each pass has
    /// to happen even if it happens late.
    Delay,
    /// Wait the full period *after* each pass finishes, so a slow upstream is never hit
    /// back to back regardless of how long a pass takes.
    SleepAfterWork,
}

#[derive(Clone, Debug, Default)]
pub struct PeriodicCfg {
    pub pacing: Pacing,
    /// Upper bound on a random delay before the first pass, to keep the workers of a
    /// freshly restarted fleet from hitting the same upstream in lockstep.
    pub jitter: Option<Duration>,
    /// Hold the first pass back by one full period. For loops whose work is pointless
    /// until something else has happened -- a startup refresh the loop would duplicate,
    /// peers that have not connected yet.
    pub skip_first: bool,
}

impl PeriodicCfg {
    pub fn new(pacing: Pacing) -> Self {
        Self {
            pacing,
            jitter: None,
            skip_first: false,
        }
    }

    pub fn with_jitter(mut self, jitter: Duration) -> Self {
        self.jitter = Some(jitter);
        self
    }

    pub fn after_first_period(mut self) -> Self {
        self.skip_first = true;
        self
    }
}

/// Spawns `task` on a `period` loop that logs every outcome under `name` and exits when
/// `shutdown` is cancelled.
///
/// The first pass runs immediately (after any jitter) unless the config asks for
/// [`PeriodicCfg::after_first_period`]. A panicking pass is caught and logged like an
/// error rather than killing the loop, so one bad input cannot silently stop a service's
/// background work for the rest of its uptime.
pub fn spawn_periodic<F, Fut, E>(
    name: &'static str,
    period: Duration,
    cfg: PeriodicCfg,
    shutdown: CancellationToken,
    mut task: F,
) -> JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    tokio::spawn(async move {
        if let Some(bound) = cfg.jitter {
            let delay = first_fire_jitter(name, bound);
            if !delay.is_zero() {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        tracing::info!(worker = name, "cancelled before the first pass");
                        return;
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }

        tracing::info!(
            worker = name,
            period_ms = period.as_millis() as u64,
            pacing = ?cfg.pacing,
            "periodic worker started"
        );

        match cfg.pacing {
            Pacing::SleepAfterWork => {
                if cfg.skip_first {
                    tokio::select! {
                        _ = shutdown.cancelled() => {
                            tracing::info!(worker = name, "cancelled before the first pass");
                            return;
                        }
                        _ = tokio::time::sleep(period) => {}
                    }
                }
                loop {
                    if shutdown.is_cancelled() {
                        break;
                    }
                    run_pass(name, &mut task).await;
                    tokio::select! {
                        _ = shutdown.cancelled() => break,
                        _ = tokio::time::sleep(period) => {}
                    }
                }
            }
            pacing => {
                let mut ticker = tokio::time::interval(period);
                ticker.set_missed_tick_behavior(match pacing {
                    Pacing::Delay => MissedTickBehavior::Delay,
                    _ => MissedTickBehavior::Skip,
                });
                if cfg.skip_first {
                    ticker.tick().await;
                }
                loop {
                    tokio::select! {
                        _ = shutdown.cancelled() => break,
                        _ = ticker.tick() => {}
                    }
                    if shutdown.is_cancelled() {
                        break;
                    }
                    run_pass(name, &mut task).await;
                }
            }
        }

        tracing::info!(worker = name, "periodic worker stopped");
    })
}

async fn run_pass<F, Fut, E>(name: &'static str, task: &mut F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), E>>,
    E: Display,
{
    let started = Instant::now();
    let outcome = AssertUnwindSafe(async { task().await })
        .catch_unwind()
        .await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(Ok(())) => tracing::debug!(worker = name, elapsed_ms, "pass ok"),
        Ok(Err(err)) => tracing::warn!(worker = name, elapsed_ms, %err, "pass failed"),
        Err(payload) => tracing::error!(
            worker = name,
            elapsed_ms,
            panic = panic_message(payload.as_ref()),
            "pass panicked"
        ),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.as_str()
    } else {
        "non-string panic payload"
    }
}

fn first_fire_jitter(name: &str, bound: Duration) -> Duration {
    let bound_nanos = bound.as_nanos().min(u64::MAX as u128) as u64;
    if bound_nanos == 0 {
        return Duration::ZERO;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let entropy = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    h ^= entropy.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h = h.wrapping_mul(0x0000_0100_0000_01b3);
    h ^= h >> 33;
    Duration::from_nanos(h % bound_nanos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
    use tokio::time::timeout;

    type Stamps = UnboundedReceiver<Instant>;

    fn stamping_worker(
        pacing: Pacing,
        period: Duration,
        work: Duration,
    ) -> (JoinHandle<()>, CancellationToken, Stamps) {
        let (tx, rx) = unbounded_channel();
        let token = CancellationToken::new();
        let handle = spawn_periodic(
            "test",
            period,
            PeriodicCfg::new(pacing),
            token.clone(),
            move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(Instant::now());
                    tokio::time::sleep(work).await;
                    Ok::<(), String>(())
                }
            },
        );
        (handle, token, rx)
    }

    async fn deltas(rx: &mut Stamps, n: usize) -> Vec<Duration> {
        let mut stamps = Vec::new();
        for _ in 0..n {
            stamps.push(rx.recv().await.expect("worker must keep firing"));
        }
        stamps.windows(2).map(|w| w[1] - w[0]).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn skip_pacing_keeps_the_period_regardless_of_work_time() {
        let (handle, token, mut rx) = stamping_worker(
            Pacing::Skip,
            Duration::from_secs(1),
            Duration::from_millis(300),
        );
        let gaps = deltas(&mut rx, 4).await;
        assert!(
            gaps.iter().all(|g| *g == Duration::from_secs(1)),
            "{gaps:?}"
        );
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_after_work_adds_the_work_time_to_the_period() {
        let (handle, token, mut rx) = stamping_worker(
            Pacing::SleepAfterWork,
            Duration::from_secs(1),
            Duration::from_millis(300),
        );
        let gaps = deltas(&mut rx, 4).await;
        assert!(
            gaps.iter().all(|g| *g == Duration::from_millis(1300)),
            "{gaps:?}"
        );
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn delay_pacing_absorbs_an_overrun_without_bursting() {
        let (handle, token, mut rx) = stamping_worker(
            Pacing::Delay,
            Duration::from_secs(1),
            Duration::from_millis(1500),
        );
        let gaps = deltas(&mut rx, 4).await;
        assert!(
            gaps.iter().all(|g| *g == Duration::from_millis(1500)),
            "{gaps:?}"
        );
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn first_pass_runs_immediately_without_jitter() {
        let start = Instant::now();
        let (handle, token, mut rx) =
            stamping_worker(Pacing::Skip, Duration::from_secs(60), Duration::ZERO);
        let first = rx.recv().await.unwrap();
        assert_eq!(first - start, Duration::ZERO);
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    async fn first_pass_at(pacing: Pacing, period: Duration) -> Duration {
        let (tx, mut rx) = unbounded_channel();
        let token = CancellationToken::new();
        let start = Instant::now();
        let handle = spawn_periodic(
            "deferred",
            period,
            PeriodicCfg::new(pacing).after_first_period(),
            token.clone(),
            move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(Instant::now());
                    Ok::<(), String>(())
                }
            },
        );
        let first = rx.recv().await.unwrap() - start;
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
        first
    }

    #[tokio::test(start_paused = true)]
    async fn after_first_period_holds_the_first_pass_back_by_one_period() {
        let period = Duration::from_secs(60);
        for pacing in [Pacing::Skip, Pacing::Delay, Pacing::SleepAfterWork] {
            assert_eq!(first_pass_at(pacing, period).await, period, "{pacing:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn jitter_delays_the_first_pass_within_its_bound() {
        let bound = Duration::from_secs(30);
        let (tx, mut rx) = unbounded_channel();
        let token = CancellationToken::new();
        let start = Instant::now();
        let handle = spawn_periodic(
            "jittered",
            Duration::from_secs(60),
            PeriodicCfg::new(Pacing::Skip).with_jitter(bound),
            token.clone(),
            move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(Instant::now());
                    Ok::<(), String>(())
                }
            },
        );
        let first = rx.recv().await.unwrap();
        assert!(first - start < bound, "jitter must stay under its bound");
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_pass_does_not_kill_the_loop() {
        let (tx, mut rx) = unbounded_channel();
        let passes = Arc::new(AtomicUsize::new(0));
        let token = CancellationToken::new();
        let handle = spawn_periodic(
            "panicky",
            Duration::from_secs(1),
            PeriodicCfg::default(),
            token.clone(),
            move || {
                let tx = tx.clone();
                let passes = passes.clone();
                async move {
                    let n = passes.fetch_add(1, Ordering::SeqCst);
                    let _ = tx.send(n);
                    if n == 0 {
                        panic!("first pass explodes");
                    }
                    Ok::<(), String>(())
                }
            },
        );

        assert_eq!(rx.recv().await, Some(0));
        assert_eq!(rx.recv().await, Some(1), "loop must survive the panic");
        assert_eq!(rx.recv().await, Some(2));
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_failing_pass_does_not_stop_the_loop() {
        let (tx, mut rx) = unbounded_channel();
        let token = CancellationToken::new();
        let handle = spawn_periodic(
            "failing",
            Duration::from_secs(1),
            PeriodicCfg::default(),
            token.clone(),
            move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(());
                    Err::<(), String>("upstream is down".into())
                }
            },
        );
        for _ in 0..3 {
            assert_eq!(rx.recv().await, Some(()));
        }
        token.cancel();
        let _ = timeout(Duration::from_secs(5), handle).await;
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_stops_the_loop() {
        let (handle, token, mut rx) =
            stamping_worker(Pacing::Skip, Duration::from_secs(1), Duration::ZERO);
        let _ = rx.recv().await;
        token.cancel();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("worker must exit on cancellation")
            .expect("worker must not panic out");
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_before_the_first_pass_skips_the_work() {
        let (tx, mut rx) = unbounded_channel();
        let token = CancellationToken::new();
        token.cancel();
        let handle = spawn_periodic(
            "already-cancelled",
            Duration::from_secs(1),
            PeriodicCfg::new(Pacing::Skip).with_jitter(Duration::from_secs(30)),
            token.clone(),
            move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(());
                    Ok::<(), String>(())
                }
            },
        );
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("worker must exit immediately")
            .unwrap();
        assert_eq!(rx.try_recv().ok(), None, "no pass may have run");
    }

    #[test]
    fn jitter_is_bounded_and_zero_bound_is_immediate() {
        assert_eq!(first_fire_jitter("x", Duration::ZERO), Duration::ZERO);
        for name in ["a", "sync", "refresh-catalog"] {
            let bound = Duration::from_secs(10);
            assert!(first_fire_jitter(name, bound) < bound);
        }
    }
}
