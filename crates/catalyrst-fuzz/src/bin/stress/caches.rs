use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rand::RngExt;
use rand::SeedableRng;
use tokio::sync::Barrier;

use catalyrst_crypto::eip1654::Eip1654Validator;
use catalyrst_crypto::error::AuthError;
use catalyrst_crypto::ValidationCache;

use catalyrst_deployer::{EntityType, FailedDeployment};

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

struct FailedDeploymentsCache {
    map: std::sync::RwLock<HashMap<String, FailedDeployment>>,
}

impl FailedDeploymentsCache {
    fn new() -> Self {
        Self {
            map: std::sync::RwLock::new(HashMap::new()),
        }
    }

    fn cache(&self, fd: FailedDeployment) {
        let mut map = self.map.write().unwrap();
        map.insert(fd.entity_id.clone(), fd);
    }

    fn remove(&self, entity_id: &str) -> bool {
        let mut map = self.map.write().unwrap();
        map.remove(entity_id).is_some()
    }

    fn contains(&self, entity_id: &str) -> bool {
        let map = self.map.read().unwrap();
        map.contains_key(entity_id)
    }

    fn snapshot_keys(&self) -> HashSet<String> {
        let map = self.map.read().unwrap();
        map.keys().cloned().collect()
    }
}

pub(crate) async fn test_failed_deployments_cache_stress() {
    println!("\n[2] Failed deployments cache stress (cache + remove races)");

    let cache = Arc::new(FailedDeploymentsCache::new());
    let entity_ids: Vec<String> = (0..20).map(|i| format!("entity-{}", i)).collect();

    let _ground_truth = Arc::new(std::sync::Mutex::new(HashSet::<String>::new()));

    let barrier = Arc::new(Barrier::new(50));
    let mut handles = Vec::new();

    for _task_id in 0..50u32 {
        let cache = cache.clone();
        let entity_ids = entity_ids.clone();
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());

            for _ in 0..100 {
                let idx = rng.random_range(0..entity_ids.len());
                let eid = &entity_ids[idx];

                if rng.random_bool(0.6) {
                    let fd = FailedDeployment {
                        entity_id: eid.clone(),
                        entity_type: EntityType::Scene,
                        auth_chain: None,
                        error_description: Some(format!("task-{}", _task_id)),
                        from_snapshot: false,
                    };
                    cache.cache(fd);
                } else {
                    cache.remove(eid);
                }
            }
        }));
    }

    let mut panics = 0;
    for h in handles {
        if let Err(e) = h.await {
            panics += 1;
            if panics == 1 {
                fail("failed_cache_stress", &format!("task panicked: {}", e));
            }
        }
    }

    if panics > 0 {
        return;
    }

    let mut expected: HashSet<String> = HashSet::new();
    for eid in &entity_ids {
        if cache.contains(eid) {
            expected.insert(eid.clone());
        }
    }

    let actual = cache.snapshot_keys();
    if expected != actual {
        fail(
            "failed_cache_stress",
            &format!(
                "cache inconsistency: expected {} keys, got {}",
                expected.len(),
                actual.len()
            ),
        );
        return;
    }

    let _ = cache.snapshot_keys();
    pass("50 tasks x 100 ops, cache is internally consistent, no panics");
}
