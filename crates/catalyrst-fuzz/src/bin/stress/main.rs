mod caches;
mod storage;

use std::sync::atomic::{AtomicUsize, Ordering};

use caches::test_validation_cache_stress;
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
    test_content_storage_concurrent_writes().await;

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
