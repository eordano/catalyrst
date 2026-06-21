use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Barrier;

use catalyrst_crypto::eip1654::Eip1654Validator;
use catalyrst_crypto::error::AuthError;
use catalyrst_crypto::ValidationCache;

use crate::{fail, pass};

struct TrackingValidator {
    calls: std::sync::Mutex<HashMap<String, AtomicUsize>>,
    delay: Duration,
}

impl TrackingValidator {
    fn new(delay: Duration) -> Self {
        Self {
            calls: std::sync::Mutex::new(HashMap::new()),
            delay,
        }
    }

    fn total_calls(&self) -> usize {
        let map = self.calls.lock().unwrap();
        map.values().map(|c| c.load(Ordering::SeqCst)).sum()
    }

    fn calls_for_key(&self, key: &str) -> usize {
        let map = self.calls.lock().unwrap();
        map.get(key).map(|c| c.load(Ordering::SeqCst)).unwrap_or(0)
    }
}

#[async_trait::async_trait]
impl Eip1654Validator for TrackingValidator {
    async fn validate_signature(
        &self,
        contract_address: &str,
        _hash: &[u8],
        _signature: &[u8],
    ) -> Result<bool, AuthError> {
        {
            let mut map = self.calls.lock().unwrap();
            map.entry(contract_address.to_lowercase())
                .or_insert_with(|| AtomicUsize::new(0))
                .fetch_add(1, Ordering::SeqCst);
        }
        tokio::time::sleep(self.delay).await;
        Ok(true)
    }
}

pub(crate) async fn test_validation_cache_stress() {
    println!("\n[1] Validation cache stress (coalescing + multi-key)");

    let inner = Arc::new(TrackingValidator::new(Duration::from_millis(50)));
    let cache = Arc::new(ValidationCache::new(inner.clone()));

    let barrier = Arc::new(Barrier::new(100));
    let mut handles = Vec::new();
    for _ in 0..100 {
        let cache = cache.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            cache.validate_signature("0xsamekey", b"hash", b"sig").await
        }));
    }

    let mut results = Vec::new();
    let mut any_panic = false;
    for h in handles {
        match h.await {
            Ok(r) => results.push(r),
            Err(e) => {
                any_panic = true;
                fail("cache_stress_a", &format!("task panicked: {}", e));
            }
        }
    }

    if any_panic {
        return;
    }

    let all_ok = results.iter().all(|r| matches!(r, Ok(true)));
    if !all_ok {
        fail("cache_stress_a", "not all tasks got Ok(true)");
        return;
    }

    let calls = inner.calls_for_key("0xsamekey");
    if calls > 3 {
        fail(
            "cache_stress_a",
            &format!(
                "inner validator called {} times for same key (expected <=3 for coalescing)",
                calls
            ),
        );
        return;
    }

    pass(&format!(
        "100 tasks, same key: inner called {} time(s), all got Ok(true)",
        calls
    ));

    let inner2 = Arc::new(TrackingValidator::new(Duration::from_millis(20)));
    let cache2 = Arc::new(ValidationCache::new(inner2.clone()));

    let barrier2 = Arc::new(Barrier::new(200));
    let mut handles2 = Vec::new();
    for i in 0..200u32 {
        let cache2 = cache2.clone();
        let barrier2 = barrier2.clone();
        let key_idx = i % 50;
        handles2.push(tokio::spawn(async move {
            barrier2.wait().await;
            let addr = format!("0xkey{:04}", key_idx);
            cache2.validate_signature(&addr, b"hash", b"sig").await
        }));
    }

    let mut panics = 0;
    for h in handles2 {
        if let Err(e) = h.await {
            panics += 1;
            if panics == 1 {
                fail("cache_stress_b", &format!("task panicked: {}", e));
            }
        }
    }

    if panics == 0 {
        let total = inner2.total_calls();
        pass(&format!(
            "200 tasks, 50 keys: {} total inner calls (ideal ~50)",
            total
        ));
    }
}
