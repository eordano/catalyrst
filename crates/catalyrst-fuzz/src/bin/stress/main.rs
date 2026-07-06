mod active;
mod caches;
mod locks;
mod pipeline;
mod storage;

use std::sync::atomic::{AtomicUsize, Ordering};

use active::test_active_entities_cache_stress;
use caches::{test_failed_deployments_cache_stress, test_validation_cache_stress};
use locks::{test_pointer_lock_manager_stress, test_sequential_task_executor_stress};
use pipeline::test_parallel_pipeline_stress;
use storage::test_content_storage_concurrent_writes;

static FAILURES: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    println!("  [PASS] {}", name);
}

fn fail(name: &str, detail: &str) {
    eprintln!("  [FAIL] {} -- {}", name, detail);
    FAILURES.fetch_add(1, Ordering::SeqCst);
}

#[tokio::main]
async fn main() {
    println!("=== catalyrst concurrency stress test suite ===");

    test_validation_cache_stress().await;
    test_failed_deployments_cache_stress().await;
    test_pointer_lock_manager_stress().await;
    test_parallel_pipeline_stress().await;
    test_active_entities_cache_stress().await;
    test_content_storage_concurrent_writes().await;
    test_sequential_task_executor_stress().await;

    println!("\n=== summary ===");
    let failures = FAILURES.load(Ordering::SeqCst);
    if failures == 0 {
        println!("All tests passed.");
        std::process::exit(0);
    } else {
        eprintln!("{} test(s) FAILED.", failures);
        std::process::exit(1);
    }
}
