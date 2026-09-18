use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot, Semaphore};

fn completion(count: &Arc<AtomicUsize>) -> Completion {
    let count = count.clone();
    Box::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    })
}

async fn completed(count: &AtomicUsize, expected: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while count.load(Ordering::SeqCst) != expected {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("every accepted or rejected item must complete exactly once");
}

#[tokio::test(start_paused = true)]
async fn global_admission_and_running_tasks_are_bounded_before_polling() {
    let queue = Arc::new(WalletQueue::default());
    let done = Arc::new(AtomicUsize::new(0));
    let polled = Arc::new(AtomicUsize::new(0));
    for index in 0..MAX_TOTAL_WORK + 100 {
        let polled = polled.clone();
        let accepted = queue.enqueue(
            &format!("wallet-{index}"),
            Box::pin(async move {
                polled.fetch_add(1, Ordering::SeqCst);
                futures::future::pending::<()>().await;
            }),
            completion(&done),
        );
        assert_eq!(accepted, index < MAX_TOTAL_WORK);
    }
    assert_eq!(queue.outstanding_work(), MAX_TOTAL_WORK);
    assert_eq!(queue.tracked_wallets(), MAX_TOTAL_WORK);
    assert_eq!(queue.running_work(), MAX_RUNNING_WORK);
    assert_eq!(done.load(Ordering::SeqCst), 100);
    tokio::task::yield_now().await;
    assert_eq!(polled.load(Ordering::SeqCst), MAX_RUNNING_WORK);
    tokio::time::advance(WORK_DEADLINE).await;
    completed(&done, MAX_TOTAL_WORK + 100).await;
    assert_eq!(polled.load(Ordering::SeqCst), MAX_RUNNING_WORK);
    assert_eq!(queue.outstanding_work(), 0);
    assert_eq!(queue.running_work(), 0);
    assert_eq!(queue.tracked_wallets(), 0);
}

#[tokio::test(start_paused = true)]
async fn one_wallet_is_bounded_and_expired_queue_tails_are_never_polled() {
    let queue = Arc::new(WalletQueue::default());
    let done = Arc::new(AtomicUsize::new(0));
    let polled = Arc::new(AtomicUsize::new(0));
    for index in 0..MAX_WALLET_WORK + 100 {
        let polled = polled.clone();
        assert_eq!(
            queue.enqueue(
                "wallet",
                Box::pin(async move {
                    polled.fetch_add(1, Ordering::SeqCst);
                    futures::future::pending::<()>().await;
                }),
                completion(&done),
            ),
            index < MAX_WALLET_WORK
        );
    }
    tokio::task::yield_now().await;
    assert_eq!(polled.load(Ordering::SeqCst), 1);
    assert_eq!(queue.outstanding_work(), MAX_WALLET_WORK);
    assert_eq!(queue.running_work(), 1);
    tokio::time::advance(WORK_DEADLINE).await;
    completed(&done, MAX_WALLET_WORK + 100).await;
    assert_eq!(polled.load(Ordering::SeqCst), 1);
    assert_eq!(queue.tracked_wallets(), 0);
}

#[tokio::test]
async fn a_cold_wallet_runs_before_the_hot_wallets_next_item() {
    let queue = Arc::new(WalletQueue::default());
    let done = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Semaphore::new(0));
    let (started, mut starts) = mpsc::unbounded_channel();
    let mut gates = Vec::new();
    for index in 0..MAX_RUNNING_WORK {
        let (open, gate) = oneshot::channel();
        gates.push(open);
        let started = started.clone();
        assert!(queue.enqueue(
            &format!("wallet-{index}"),
            Box::pin(async move {
                started.send(()).unwrap();
                gate.await.unwrap();
            }),
            completion(&done),
        ));
    }
    for _ in 0..MAX_RUNNING_WORK {
        starts.recv().await.unwrap();
    }
    let (order, mut observed) = mpsc::unbounded_channel();
    for index in 1..MAX_WALLET_WORK {
        let order = order.clone();
        assert!(queue.enqueue(
            "wallet-0",
            Box::pin(async move { order.send(index).unwrap() }),
            completion(&done),
        ));
    }
    let order = order.clone();
    let held = release.clone();
    assert!(queue.enqueue(
        "cold",
        Box::pin(async move {
            order.send(0).unwrap();
            held.acquire().await.unwrap().forget();
        }),
        completion(&done),
    ));
    gates.remove(0).send(()).unwrap();
    assert_eq!(observed.recv().await, Some(0));
    assert!(observed.try_recv().is_err());
    release.add_permits(1);
    for index in 1..MAX_WALLET_WORK {
        assert_eq!(observed.recv().await, Some(index));
    }
    for gate in gates {
        gate.send(()).unwrap();
    }
    completed(&done, MAX_RUNNING_WORK + MAX_WALLET_WORK).await;
    assert_eq!(queue.tracked_wallets(), 0);
}

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn rejection_releases_captures_before_completion_and_outside_the_lock() {
    let queue = Arc::new(WalletQueue::default());
    let done = Arc::new(AtomicUsize::new(0));
    for _ in 0..MAX_WALLET_WORK {
        assert!(queue.enqueue(
            "wallet",
            Box::pin(futures::future::pending()),
            completion(&done)
        ));
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let captured = Dropped(dropped.clone());
    let check = queue.clone();
    let finished = completion(&done);
    assert!(!queue.enqueue(
        "wallet",
        Box::pin(async move { drop(captured) }),
        Box::new(move || {
            assert!(dropped.load(Ordering::SeqCst));
            assert_eq!(check.outstanding_work(), MAX_WALLET_WORK);
            finished();
        }),
    ));
    assert_eq!(done.load(Ordering::SeqCst), 1);
    tokio::time::advance(WORK_DEADLINE).await;
    completed(&done, MAX_WALLET_WORK + 1).await;
}

#[tokio::test]
async fn work_and_completion_panics_do_not_strand_the_lane() {
    let queue = Arc::new(WalletQueue::default());
    let done = Arc::new(AtomicUsize::new(0));
    assert!(queue.enqueue(
        "wallet",
        Box::pin(async { panic!("work") }),
        Box::new(|| panic!("completion")),
    ));
    assert!(queue.enqueue("wallet", Box::pin(async {}), completion(&done)));
    completed(&done, 1).await;
    assert_eq!(queue.outstanding_work(), 0);
    assert_eq!(queue.tracked_wallets(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_producers_cannot_exceed_either_admission_limit() {
    let queue = Arc::new(WalletQueue::default());
    let release = Arc::new(Semaphore::new(0));
    let accepted = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let mut producers = Vec::new();
    for _ in 0..8 {
        let queue = queue.clone();
        let release = release.clone();
        let accepted = accepted.clone();
        let done = done.clone();
        producers.push(tokio::spawn(async move {
            for index in 0..MAX_TOTAL_WORK {
                let release = release.clone();
                if queue.enqueue(
                    &format!("wallet-{}", index % 128),
                    Box::pin(async move { release.acquire().await.unwrap().forget() }),
                    completion(&done),
                ) {
                    accepted.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }
    for producer in producers {
        producer.await.unwrap();
    }
    assert_eq!(accepted.load(Ordering::SeqCst), MAX_TOTAL_WORK);
    assert_eq!(queue.outstanding_work(), MAX_TOTAL_WORK);
    assert_eq!(queue.running_work(), MAX_RUNNING_WORK);
    for lane in queue.state().lanes.values() {
        assert!(lane.pending.len() + usize::from(lane.running) <= MAX_WALLET_WORK);
    }
    release.add_permits(MAX_TOTAL_WORK);
    completed(&done, MAX_TOTAL_WORK * 8).await;
    assert_eq!(queue.tracked_wallets(), 0);
}

#[tokio::test]
async fn cancelling_queued_work_does_not_wait_for_another_owners_active_work() {
    let queue = Arc::new(WalletQueue::default());
    let running = Arc::new(WorkOwner::default());
    let waiting = Arc::new(WorkOwner::default());
    let done = Arc::new(AtomicUsize::new(0));
    for index in 0..MAX_TOTAL_WORK {
        let owner = if index < MAX_RUNNING_WORK {
            running.clone()
        } else {
            waiting.clone()
        };
        assert!(queue.enqueue_scoped(
            owner,
            &format!("wallet-{index}"),
            Box::pin(futures::future::pending()),
            completion(&done),
        ));
    }
    queue.cancel(&waiting);
    assert_eq!(queue.outstanding_work(), MAX_RUNNING_WORK);
    assert_eq!(
        done.load(Ordering::SeqCst),
        MAX_TOTAL_WORK - MAX_RUNNING_WORK
    );
    tokio::time::timeout(Duration::from_millis(100), waiting.settle())
        .await
        .unwrap();
    assert_eq!(running.outstanding.load(Ordering::SeqCst), MAX_RUNNING_WORK);
    assert!(!queue.enqueue_scoped(waiting, "late", Box::pin(async {}), completion(&done)));
    queue.cancel(&running);
    tokio::time::timeout(Duration::from_millis(100), running.settle())
        .await
        .unwrap();
    assert_eq!(queue.outstanding_work(), 0);
    assert_eq!(queue.tracked_wallets(), 0);
    assert_eq!(done.load(Ordering::SeqCst), MAX_TOTAL_WORK + 1);
}
