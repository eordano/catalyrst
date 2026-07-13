use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::RngExt;
use rand::SeedableRng;
use tokio::sync::Barrier;

use catalyrst_deployer::deployment_service::PointerLockManager;
use catalyrst_deployer::EntityType;

use crate::{fail, pass};

pub(crate) async fn test_pointer_lock_manager_stress() {
    println!("\n[3] Pointer lock manager stress (acquire/release overlapping sets)");

    let plm = Arc::new(PointerLockManager::new());

    let all_pointers: Vec<String> = (0..30).map(|i| format!("{},{}", i / 5, i % 5)).collect();

    let occupancy: Vec<Arc<AtomicU64>> = (0..30).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let double_acquire = Arc::new(AtomicBool::new(false));

    let barrier = Arc::new(Barrier::new(20));
    let mut handles = Vec::new();

    for _task_id in 0..20u32 {
        let plm = plm.clone();
        let all_pointers = all_pointers.clone();
        let occupancy = occupancy.clone();
        let double_acquire = double_acquire.clone();
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());

            for _ in 0..50 {
                let count = rng.random_range(2..=5);
                let mut indices: Vec<usize> = Vec::new();
                while indices.len() < count {
                    let idx = rng.random_range(0..all_pointers.len());
                    if !indices.contains(&idx) {
                        indices.push(idx);
                    }
                }
                indices.sort();

                let ptrs: Vec<String> = indices.iter().map(|&i| all_pointers[i].clone()).collect();

                let overlap = plm.try_acquire(EntityType::Scene, &ptrs);
                if overlap.is_empty() {
                    for &idx in &indices {
                        let prev = occupancy[idx].fetch_add(1, Ordering::SeqCst);
                        if prev != 0 {
                            double_acquire.store(true, Ordering::SeqCst);
                        }
                    }

                    tokio::time::sleep(Duration::from_micros(rng.random_range(10..200))).await;

                    for &idx in &indices {
                        occupancy[idx].fetch_sub(1, Ordering::SeqCst);
                    }
                    plm.release(EntityType::Scene, &ptrs);
                }

                tokio::task::yield_now().await;
            }
        }));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut panics = 0;
    for h in handles {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, h).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                panics += 1;
                if panics == 1 {
                    fail("pointer_lock_stress", &format!("task panicked: {}", e));
                }
            }
            Err(_) => {
                fail(
                    "pointer_lock_stress",
                    "deadlock detected: timed out after 5s",
                );
                return;
            }
        }
    }

    if panics > 0 {
        return;
    }

    if double_acquire.load(Ordering::SeqCst) {
        fail(
            "pointer_lock_stress",
            "double-acquire detected: two tasks held the same pointer simultaneously",
        );
        return;
    }

    let leftover: usize = occupancy
        .iter()
        .map(|c| c.load(Ordering::SeqCst) as usize)
        .sum();
    if leftover != 0 {
        fail(
            "pointer_lock_stress",
            &format!("leaked locks: {} pointers still occupied", leftover),
        );
        return;
    }

    pass("20 tasks x 50 acquire/release cycles, no double-acquire, no deadlock, no leaks");
}

pub(crate) async fn test_sequential_task_executor_stress() {
    println!("\n[7] Sequential task executor stress (serialization guarantee)");

    use catalyrst_deployer::sequential_task_executor::SequentialTaskExecutor;

    let executor = Arc::new(SequentialTaskExecutor::new());

    let counters: Vec<Arc<AtomicU64>> = (0..5).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let max_concurrent: Vec<Arc<AtomicU64>> = (0..5).map(|_| Arc::new(AtomicU64::new(0))).collect();

    let in_flight: Vec<Arc<AtomicU64>> = (0..5).map(|_| Arc::new(AtomicU64::new(0))).collect();

    let mut handles = Vec::new();

    for queue_idx in 0..5usize {
        for _ in 0..10 {
            let executor = executor.clone();
            let counter = counters[queue_idx].clone();
            let in_flight = in_flight[queue_idx].clone();
            let max_conc = max_concurrent[queue_idx].clone();
            let queue_name = format!("queue-{}", queue_idx);

            handles.push(tokio::spawn(async move {
                let counter = counter.clone();
                let in_flight = in_flight.clone();
                let max_conc = max_conc.clone();
                executor
                    .run_fn(&queue_name, move || {
                        let counter = counter.clone();
                        let in_flight = in_flight.clone();
                        let max_conc = max_conc.clone();
                        async move {
                            let n = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                            loop {
                                let current_max = max_conc.load(Ordering::SeqCst);
                                if n <= current_max {
                                    break;
                                }
                                if max_conc
                                    .compare_exchange(
                                        current_max,
                                        n,
                                        Ordering::SeqCst,
                                        Ordering::SeqCst,
                                    )
                                    .is_ok()
                                {
                                    break;
                                }
                            }

                            tokio::time::sleep(Duration::from_micros(50)).await;

                            counter.fetch_add(1, Ordering::SeqCst);
                            in_flight.fetch_sub(1, Ordering::SeqCst);
                        }
                    })
                    .await;
            }));
        }
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut panics = 0;
    for h in handles {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, h).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                panics += 1;
                if panics == 1 {
                    fail("seq_executor_stress", &format!("task panicked: {}", e));
                }
            }
            Err(_) => {
                fail("seq_executor_stress", "timed out after 10s (deadlock?)");
                return;
            }
        }
    }

    if panics > 0 {
        return;
    }

    let mut all_ok = true;
    for (i, counter) in counters.iter().enumerate() {
        let val = counter.load(Ordering::SeqCst);
        let max = max_concurrent[i].load(Ordering::SeqCst);
        if val != 10 {
            fail(
                "seq_executor_stress",
                &format!("queue-{}: counter = {} (expected 10)", i, val),
            );
            all_ok = false;
        }
        if max > 1 {
            fail(
                "seq_executor_stress",
                &format!(
                    "queue-{}: max concurrent = {} (expected 1, serialization broken)",
                    i, max
                ),
            );
            all_ok = false;
        }
    }

    if all_ok {
        pass("5 queues x 10 tasks: all counters=10, max_concurrent=1 per queue");
    }
}
