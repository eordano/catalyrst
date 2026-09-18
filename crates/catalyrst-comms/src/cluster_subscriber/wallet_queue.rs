use futures::FutureExt as _;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub const MAX_TOTAL_WORK: usize = 1_024;
pub const MAX_WALLET_WORK: usize = 16;
pub const MAX_RUNNING_WORK: usize = 32;
/// Contains the maximum 60 s token lifetime, LiveKit's 60 s verifier leeway, a two-second
/// boundary margin, two removal phases and the source checks surrounding replacement minting.
pub const WORK_DEADLINE: Duration = Duration::from_secs(150);

pub(super) type QueuedWork = futures::future::BoxFuture<'static, ()>;
pub(super) type Completion = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub(super) struct WorkOwner {
    cancelled: CancellationToken,
    outstanding: AtomicUsize,
    idle: Notify,
}

impl WorkOwner {
    pub(super) async fn settle(&self) {
        loop {
            let idle = self.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if self.outstanding.load(Ordering::SeqCst) == 0 {
                return;
            }
            idle.await;
        }
    }
}

struct Ownership(Arc<WorkOwner>);

impl Drop for Ownership {
    fn drop(&mut self) {
        self.0.outstanding.fetch_sub(1, Ordering::SeqCst);
        self.0.idle.notify_waiters();
    }
}

struct Completed(Option<Completion>);

impl Drop for Completed {
    fn drop(&mut self) {
        if let Some(done) = self.0.take() {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(done)).is_err() {
                tracing::error!("cluster completion panicked");
            }
        }
    }
}

struct Work {
    future: Option<QueuedWork>,
    _completed: Completed,
    owner: Ownership,
    expires: Instant,
}

#[derive(Default)]
struct Lane {
    pending: VecDeque<Work>,
    running: bool,
}

#[derive(Default)]
struct State {
    lanes: HashMap<String, Lane>,
    ready: VecDeque<String>,
    running: usize,
    total: usize,
}

/// Bounded FIFO per wallet, round-robin between wallets. At most one item per
/// wallet executes at once, and only a bounded number of tasks exist globally.
/// Admission is synchronous, before scheduling, so accepted arrival order survives.
/// No replica affinity or distributed publication ordering is implied.
#[derive(Default)]
pub struct WalletQueue {
    state: Mutex<State>,
    unscoped_owner: Arc<WorkOwner>,
}

impl WalletQueue {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Rejects new work at either cap. Rejection drops its captured state and runs
    /// its completion outside the queue lock; accepted work gets the same cleanup
    /// on success, panic or its queue-inclusive deadline. Completion follows lane
    /// retirement when this was the wallet's last item.
    pub fn enqueue(self: &Arc<Self>, wallet: &str, work: QueuedWork, done: Completion) -> bool {
        self.enqueue_scoped(self.unscoped_owner.clone(), wallet, work, done)
    }

    pub(super) fn enqueue_scoped(
        self: &Arc<Self>,
        owner: Arc<WorkOwner>,
        wallet: &str,
        work: QueuedWork,
        done: Completion,
    ) -> bool {
        owner.outstanding.fetch_add(1, Ordering::SeqCst);
        let mut work = Some(Work {
            future: Some(work),
            _completed: Completed(Some(done)),
            owner: Ownership(owner.clone()),
            expires: Instant::now() + WORK_DEADLINE,
        });
        let scheduled = {
            let mut state = self.state();
            if owner.cancelled.is_cancelled() {
                Err("retired")
            } else if state.total >= MAX_TOTAL_WORK {
                Err("global_full")
            } else if state.lanes.get(wallet).is_some_and(|lane| {
                lane.pending.len() + usize::from(lane.running) >= MAX_WALLET_WORK
            }) {
                Err("wallet_full")
            } else {
                if !state.lanes.contains_key(wallet) {
                    state.ready.push_back(wallet.to_owned());
                }
                state
                    .lanes
                    .entry(wallet.to_owned())
                    .or_default()
                    .pending
                    .push_back(work.take().unwrap());
                state.total += 1;
                Ok(Self::take_ready(&mut state))
            }
        };
        match scheduled {
            Ok(ready) => {
                self.start_ready(ready);
                true
            }
            Err(reason) => {
                crate::metrics::cluster_work_rejected(reason);
                drop(work);
                false
            }
        }
    }

    fn take_ready(state: &mut State) -> Vec<(String, Work)> {
        let mut ready = Vec::new();
        while state.running < MAX_RUNNING_WORK {
            let Some(wallet) = state.ready.pop_front() else {
                break;
            };
            let lane = state
                .lanes
                .get_mut(&wallet)
                .expect("ready wallet has a lane");
            debug_assert!(!lane.running);
            let work = lane
                .pending
                .pop_front()
                .expect("ready wallet has pending work");
            lane.running = true;
            state.running += 1;
            ready.push((wallet, work));
        }
        crate::metrics::cluster_work_depths(state.running, state.total - state.running);
        ready
    }

    fn start_ready(self: &Arc<Self>, ready: Vec<(String, Work)>) {
        for (wallet, work) in ready {
            let queue = self.clone();
            tokio::spawn(async move { queue.run(wallet, work).await });
        }
    }

    async fn run(self: Arc<Self>, wallet: String, mut work: Work) {
        if work.owner.0.cancelled.is_cancelled() {
            crate::metrics::cluster_work_cancelled();
        } else if Instant::now() >= work.expires {
            crate::metrics::cluster_work_expired();
        } else {
            let future = work.future.take().unwrap();
            tokio::select! {
                biased;
                _ = work.owner.0.cancelled.cancelled() => crate::metrics::cluster_work_cancelled(),
                _ = tokio::time::sleep_until(work.expires) => crate::metrics::cluster_work_expired(),
                result = std::panic::AssertUnwindSafe(future).catch_unwind() => {
                    if result.is_err() {
                        tracing::error!(%wallet, "cluster work panicked; the lane continues");
                    }
                }
            }
        }
        let ready = {
            let mut state = self.state();
            let lane = state
                .lanes
                .get_mut(&wallet)
                .expect("running wallet has a lane");
            lane.running = false;
            if lane.pending.is_empty() {
                state.lanes.remove(&wallet);
            } else {
                state.ready.push_back(wallet);
            }
            state.running -= 1;
            state.total -= 1;
            Self::take_ready(&mut state)
        };
        drop(work);
        self.start_ready(ready);
    }

    pub(super) fn cancel(self: &Arc<Self>, owner: &Arc<WorkOwner>) {
        let removed = {
            let mut state = self.state();
            owner.cancelled.cancel();
            let mut removed = Vec::new();
            state.lanes.retain(|_, lane| {
                let mut kept = VecDeque::new();
                for work in lane.pending.drain(..) {
                    if Arc::ptr_eq(&work.owner.0, owner) {
                        removed.push(work);
                    } else {
                        kept.push_back(work);
                    }
                }
                lane.pending = kept;
                lane.running || !lane.pending.is_empty()
            });
            let State { ready, lanes, .. } = &mut *state;
            ready.retain(|wallet| lanes.contains_key(wallet));
            state.total -= removed.len();
            crate::metrics::cluster_work_depths(state.running, state.total - state.running);
            removed
        };
        for work in removed {
            crate::metrics::cluster_work_cancelled();
            drop(work);
        }
    }

    pub fn tracked_wallets(&self) -> usize {
        self.state().lanes.len()
    }

    pub fn outstanding_work(&self) -> usize {
        self.state().total
    }

    pub fn running_work(&self) -> usize {
        self.state().running
    }

    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.state.lock().unwrap();
            panic!("poison registry");
        }));
    }
}

#[cfg(test)]
mod tests;
