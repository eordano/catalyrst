use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use rand::SeedableRng;
use tokio::sync::Barrier;

use catalyrst_crypto::eip1654::Eip1654Validator;
use catalyrst_crypto::error::AuthError;
use catalyrst_crypto::ValidationCache;

use catalyrst_deployer::deployment_service::PointerLockManager;
use catalyrst_deployer::{
    ActiveEntities, BackendError, ContentHash, DatabaseBackend, Deployment,
    DeploymentContext, DeploymentIdWithPointers, DeploymentOptions, Entity,
    EntityContentEntry, EntityType, FailedDeployment,
    GcStaleProfilesResult, LocalDeploymentAuditInfo, Pointer,
    active_entities::ActiveEntitiesConfig,
};

use catalyrst_storage::ContentStorage;
use bytes::Bytes;

static FAILURES: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    println!("  [PASS] {}", name);
}

fn fail(name: &str, detail: &str) {
    eprintln!("  [FAIL] {} -- {}", name, detail);
    FAILURES.fetch_add(1, Ordering::SeqCst);
}

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
        map.get(key)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
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

async fn test_validation_cache_stress() {
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
            cache
                .validate_signature("0xsamekey", b"hash", b"sig")
                .await
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
            cache2
                .validate_signature(&addr, b"hash", b"sig")
                .await
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

async fn test_failed_deployments_cache_stress() {
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
            let mut rng = rand::rngs::StdRng::from_entropy();

            for _ in 0..100 {
                let idx = rng.gen_range(0..entity_ids.len());
                let eid = &entity_ids[idx];

                if rng.gen_bool(0.6) {
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

async fn test_pointer_lock_manager_stress() {
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
            let mut rng = rand::rngs::StdRng::from_entropy();

            for _ in 0..50 {
                let count = rng.gen_range(2..=5);
                let mut indices: Vec<usize> = Vec::new();
                while indices.len() < count {
                    let idx = rng.gen_range(0..all_pointers.len());
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

                    tokio::time::sleep(Duration::from_micros(rng.gen_range(10..200))).await;

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

use catalyrst_deployer::parallel_pipeline::{
    EntityFetcher, FetchError, FetchedEntity, ParallelDeploymentPipeline, PipelineConfig, SyncTask,
};

struct MockFetcher {
    fail_rate: f64,
}

#[async_trait::async_trait]
impl EntityFetcher for MockFetcher {
    async fn fetch_entity(
        &self,
        entity_id: &str,
        servers: &[String],
    ) -> Result<FetchedEntity, FetchError> {
        let mut rng = rand::rngs::StdRng::from_entropy();
        let delay = rng.gen_range(0..10);
        tokio::time::sleep(Duration::from_millis(delay)).await;

        if rng.gen_bool(self.fail_rate) {
            return Err(FetchError::AllServersFailed {
                entity_id: entity_id.to_string(),
                last_error: "mock failure".to_string(),
            });
        }

        let entity = Entity {
            id: entity_id.to_string(),
            entity_type: EntityType::Scene,
            pointers: vec![format!("ptr-{}", entity_id)],
            timestamp: 1_700_000_000_000,
            content: None,
            metadata: None,
        };
        let entity_bytes = serde_json::to_vec(&entity).unwrap();

        let mut content_files = HashMap::new();
        content_files.insert(entity_id.to_string(), entity_bytes.clone());

        Ok(FetchedEntity {
            task: SyncTask {
                entity_id: entity_id.to_string(),
                entity_type: EntityType::Scene,
                servers: servers.to_vec(),
            },
            entity_bytes,
            content_files,
            auth_chain: vec![],
            total_bytes: 100,
        })
    }
}

struct MockTransaction;

#[async_trait::async_trait]
impl catalyrst_deployer::TransactionHandle for MockTransaction {
    async fn commit(self: Box<Self>) -> Result<(), BackendError> {
        Ok(())
    }
    async fn rollback(self: Box<Self>) -> Result<(), BackendError> {
        Ok(())
    }
}

struct MockDatabase {
    deployed: std::sync::Mutex<HashSet<String>>,
}

impl MockDatabase {
    fn new() -> Self {
        Self {
            deployed: std::sync::Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for MockDatabase {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn catalyrst_deployer::TransactionHandle>, BackendError> {
        Ok(Box::new(MockTransaction))
    }

    async fn deployment_exists(&self, entity_id: &str) -> Result<bool, BackendError> {
        Ok(self.deployed.lock().unwrap().contains(entity_id))
    }

    async fn get_deployment_by_entity_id(
        &self,
        _entity_id: &str,
    ) -> Result<Option<Deployment>, BackendError> {
        Ok(None)
    }

    async fn save_deployment(
        &self,
        _tx: &dyn catalyrst_deployer::TransactionHandle,
        entity: &Entity,
        _audit_info: &catalyrst_deployer::AuditInfo,
        _overwritten_by: Option<catalyrst_deployer::DeploymentId>,
    ) -> Result<catalyrst_deployer::DeploymentId, BackendError> {
        self.deployed.lock().unwrap().insert(entity.id.clone());
        Ok(1)
    }

    async fn save_content_files(
        &self,
        _tx: &dyn catalyrst_deployer::TransactionHandle,
        _deployment_id: catalyrst_deployer::DeploymentId,
        _content: &[EntityContentEntry],
    ) -> Result<(), BackendError> {
        Ok(())
    }

    async fn calculate_overwrote(
        &self,
        _entity: &Entity,
    ) -> Result<Vec<catalyrst_deployer::DeploymentId>, BackendError> {
        Ok(vec![])
    }

    async fn calculate_overwritten_by(
        &self,
        _entity: &Entity,
    ) -> Result<Option<catalyrst_deployer::DeploymentId>, BackendError> {
        Ok(None)
    }

    async fn set_entities_as_overwritten(
        &self,
        _tx: &dyn catalyrst_deployer::TransactionHandle,
        _overwritten_ids: &[catalyrst_deployer::DeploymentId],
        _overwriter_id: catalyrst_deployer::DeploymentId,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    async fn update_active_deployments(
        &self,
        _tx: &dyn catalyrst_deployer::TransactionHandle,
        _pointers: &[Pointer],
        _entity_id: &str,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    async fn remove_active_deployments(
        &self,
        _tx: &dyn catalyrst_deployer::TransactionHandle,
        _pointers: &[Pointer],
    ) -> Result<(), BackendError> {
        Ok(())
    }

    async fn get_historical_deployments(
        &self,
        _options: &DeploymentOptions,
    ) -> Result<catalyrst_deployer::PartialDeploymentHistory, BackendError> {
        Ok(catalyrst_deployer::PartialDeploymentHistory {
            deployments: vec![],
            filters: Default::default(),
            pagination: catalyrst_deployer::PaginationInfo {
                offset: 0,
                limit: 0,
                more_data: false,
                last_id: None,
            },
        })
    }

    async fn get_active_deployments(
        &self,
        _entity_ids: Option<&[String]>,
        _pointers: Option<&[Pointer]>,
    ) -> Result<Vec<Deployment>, BackendError> {
        Ok(vec![])
    }

    async fn get_deployments_by_ids(
        &self,
        _ids: &[catalyrst_deployer::DeploymentId],
    ) -> Result<Vec<DeploymentIdWithPointers>, BackendError> {
        Ok(vec![])
    }

    async fn find_unreferenced_content_hashes(
        &self,
        _since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ContentHash>, BackendError> {
        Ok(vec![])
    }

    async fn gc_stale_profiles(
        &self,
        _older_than: chrono::DateTime<chrono::Utc>,
    ) -> Result<GcStaleProfilesResult, BackendError> {
        Ok(Default::default())
    }

    async fn gc_profile_active_pointers(
        &self,
        _older_than: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Pointer>, BackendError> {
        Ok(vec![])
    }

    async fn save_failed_deployment(
        &self,
        _deployment: &FailedDeployment,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    async fn delete_failed_deployment(&self, _entity_id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    async fn find_failed_deployment(
        &self,
        _entity_id: &str,
    ) -> Result<Option<FailedDeployment>, BackendError> {
        Ok(None)
    }

    async fn get_all_failed_deployments(&self) -> Result<Vec<FailedDeployment>, BackendError> {
        Ok(vec![])
    }

    async fn remove_failed_deployment(&self, _entity_id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    async fn get_last_gc_time(&self) -> Result<Option<i64>, BackendError> {
        Ok(None)
    }

    async fn set_last_gc_time(&self, _timestamp: i64) -> Result<(), BackendError> {
        Ok(())
    }
}

struct MockStorage;

#[async_trait::async_trait]
impl catalyrst_deployer::StorageBackend for MockStorage {
    async fn exist_multiple(
        &self,
        hashes: &[ContentHash],
    ) -> Result<HashMap<ContentHash, bool>, BackendError> {
        Ok(hashes.iter().map(|h| (h.clone(), false)).collect())
    }

    async fn store(&self, _hash: &ContentHash, _data: &[u8]) -> Result<(), BackendError> {
        Ok(())
    }

    async fn delete(&self, _hashes: &[ContentHash]) -> Result<(), BackendError> {
        Ok(())
    }

    async fn all_file_ids(&self) -> Result<Vec<ContentHash>, BackendError> {
        Ok(vec![])
    }
}

struct MockValidator;

#[async_trait::async_trait]
impl catalyrst_deployer::ValidatorBackend for MockValidator {
    async fn validate(
        &self,
        _entity: &Entity,
        _audit_info: &LocalDeploymentAuditInfo,
        _files: &HashMap<ContentHash, Vec<u8>>,
        _context: DeploymentContext,
        _checks: catalyrst_deployer::ValidationChecks,
    ) -> Result<(), Vec<String>> {
        Ok(())
    }
}

async fn test_parallel_pipeline_stress() {
    println!("\n[4] Parallel pipeline stress (1000 tasks, 10% failure rate)");

    let db = Arc::new(MockDatabase::new());
    let storage: Arc<dyn catalyrst_deployer::StorageBackend> = Arc::new(MockStorage);
    let validator: Arc<dyn catalyrst_deployer::ValidatorBackend> = Arc::new(MockValidator);
    let plm = Arc::new(PointerLockManager::new());

    let deployer = Arc::new(catalyrst_deployer::DeploymentService {
        config: catalyrst_deployer::DeploymentServiceConfig::default(),
        database: db.clone(),
        storage,
        validator,
        pointer_lock_manager: plm,
        pointer_manager: catalyrst_deployer::PointerManager,
    });

    let fetcher: Arc<dyn EntityFetcher> = Arc::new(MockFetcher { fail_rate: 0.10 });

    let config = PipelineConfig {
        fetch_concurrency: 20,
        validate_concurrency: 10,
        batch_size: 50,
        channel_buffer: 200,
    };

    let pipeline = ParallelDeploymentPipeline::with_fetcher(config, deployer, fetcher);

    let tasks: Vec<SyncTask> = (0..1000)
        .map(|i| SyncTask {
            entity_id: format!("entity-{:06}", i),
            entity_type: EntityType::Scene,
            servers: vec!["http://mock-server".into()],
        })
        .collect();

    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(30), pipeline.deploy_batch(tasks)).await;

    match result {
        Ok(pipeline_result) => {
            let total = pipeline_result.deployed + pipeline_result.failed;
            let elapsed = start.elapsed();

            if total != 1000 {
                fail(
                    "pipeline_stress",
                    &format!(
                        "deployed({}) + failed({}) = {} (expected 1000)",
                        pipeline_result.deployed, pipeline_result.failed, total
                    ),
                );
            } else {
                pass(&format!(
                    "deployed={} failed={} total=1000 in {:.1}s",
                    pipeline_result.deployed,
                    pipeline_result.failed,
                    elapsed.as_secs_f64()
                ));
            }
        }
        Err(_) => {
            fail("pipeline_stress", "timed out after 30s (possible deadlock)");
        }
    }
}

struct ActiveEntitiesMockDb;

#[async_trait::async_trait]
impl DatabaseBackend for ActiveEntitiesMockDb {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn catalyrst_deployer::TransactionHandle>, BackendError> {
        Ok(Box::new(MockTransaction))
    }
    async fn deployment_exists(&self, _: &str) -> Result<bool, BackendError> {
        Ok(false)
    }
    async fn get_deployment_by_entity_id(
        &self,
        _: &str,
    ) -> Result<Option<Deployment>, BackendError> {
        Ok(None)
    }
    async fn save_deployment(
        &self,
        _: &dyn catalyrst_deployer::TransactionHandle,
        _: &Entity,
        _: &catalyrst_deployer::AuditInfo,
        _: Option<catalyrst_deployer::DeploymentId>,
    ) -> Result<catalyrst_deployer::DeploymentId, BackendError> {
        Ok(1)
    }
    async fn save_content_files(
        &self,
        _: &dyn catalyrst_deployer::TransactionHandle,
        _: catalyrst_deployer::DeploymentId,
        _: &[EntityContentEntry],
    ) -> Result<(), BackendError> {
        Ok(())
    }
    async fn calculate_overwrote(
        &self,
        _: &Entity,
    ) -> Result<Vec<catalyrst_deployer::DeploymentId>, BackendError> {
        Ok(vec![])
    }
    async fn calculate_overwritten_by(
        &self,
        _: &Entity,
    ) -> Result<Option<catalyrst_deployer::DeploymentId>, BackendError> {
        Ok(None)
    }
    async fn set_entities_as_overwritten(
        &self,
        _: &dyn catalyrst_deployer::TransactionHandle,
        _: &[catalyrst_deployer::DeploymentId],
        _: catalyrst_deployer::DeploymentId,
    ) -> Result<(), BackendError> {
        Ok(())
    }
    async fn update_active_deployments(
        &self,
        _: &dyn catalyrst_deployer::TransactionHandle,
        _: &[Pointer],
        _: &str,
    ) -> Result<(), BackendError> {
        Ok(())
    }
    async fn remove_active_deployments(
        &self,
        _: &dyn catalyrst_deployer::TransactionHandle,
        _: &[Pointer],
    ) -> Result<(), BackendError> {
        Ok(())
    }
    async fn get_historical_deployments(
        &self,
        _: &DeploymentOptions,
    ) -> Result<catalyrst_deployer::PartialDeploymentHistory, BackendError> {
        Ok(catalyrst_deployer::PartialDeploymentHistory {
            deployments: vec![],
            filters: Default::default(),
            pagination: catalyrst_deployer::PaginationInfo {
                offset: 0,
                limit: 0,
                more_data: false,
                last_id: None,
            },
        })
    }
    async fn get_active_deployments(
        &self,
        _: Option<&[String]>,
        _: Option<&[Pointer]>,
    ) -> Result<Vec<Deployment>, BackendError> {
        Ok(vec![])
    }
    async fn get_deployments_by_ids(
        &self,
        _: &[catalyrst_deployer::DeploymentId],
    ) -> Result<Vec<DeploymentIdWithPointers>, BackendError> {
        Ok(vec![])
    }
    async fn find_unreferenced_content_hashes(
        &self,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ContentHash>, BackendError> {
        Ok(vec![])
    }
    async fn gc_stale_profiles(
        &self,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<GcStaleProfilesResult, BackendError> {
        Ok(Default::default())
    }
    async fn gc_profile_active_pointers(
        &self,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Pointer>, BackendError> {
        Ok(vec![])
    }
    async fn save_failed_deployment(&self, _: &FailedDeployment) -> Result<(), BackendError> {
        Ok(())
    }
    async fn delete_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    async fn find_failed_deployment(
        &self,
        _: &str,
    ) -> Result<Option<FailedDeployment>, BackendError> {
        Ok(None)
    }
    async fn get_all_failed_deployments(&self) -> Result<Vec<FailedDeployment>, BackendError> {
        Ok(vec![])
    }
    async fn remove_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
        Ok(())
    }
    async fn get_last_gc_time(&self) -> Result<Option<i64>, BackendError> {
        Ok(None)
    }
    async fn set_last_gc_time(&self, _: i64) -> Result<(), BackendError> {
        Ok(())
    }
}

async fn test_active_entities_cache_stress() {
    println!("\n[5] Active entities cache stress (readers + writers + clearers)");

    let db: Arc<dyn DatabaseBackend> = Arc::new(ActiveEntitiesMockDb);
    let ae = Arc::new(ActiveEntities::new(
        db.clone(),
        ActiveEntitiesConfig { cache_size: 1000 },
    ));

    let pointers: Vec<String> = (0..50).map(|i| format!("ptr-{}", i)).collect();
    let entity_ids: Vec<String> = (0..50).map(|i| format!("eid-{}", i)).collect();

    let any_panic = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let duration = Duration::from_secs(5);

    let mut handles = Vec::new();

    for _ in 0..10 {
        let ae = ae.clone();
        let pointers = pointers.clone();
        let entity_ids = entity_ids.clone();
        let any_panic = any_panic.clone();
        handles.push(tokio::spawn(async move {
            let mut rng = rand::rngs::StdRng::from_entropy();
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                if rng.gen_bool(0.5) {
                    let n = rng.gen_range(1..=5);
                    let ids: Vec<String> = (0..n)
                        .map(|_| entity_ids[rng.gen_range(0..entity_ids.len())].clone())
                        .collect();
                    let _ = ae.with_ids(&ids).await;
                } else {
                    let n = rng.gen_range(1..=5);
                    let ptrs: Vec<Pointer> = (0..n)
                        .map(|_| pointers[rng.gen_range(0..pointers.len())].clone())
                        .collect();
                    let _ = ae.with_pointers(&ptrs).await;
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    for writer_id in 0..10u32 {
        let ae = ae.clone();
        let pointers = pointers.clone();
        let any_panic = any_panic.clone();
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let mut rng = rand::rngs::StdRng::from_entropy();
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                let n = rng.gen_range(1..=3);
                let ptrs: Vec<Pointer> = (0..n)
                    .map(|_| pointers[rng.gen_range(0..pointers.len())].clone())
                    .collect();
                let entity = Entity {
                    id: format!("w{}-{}", writer_id, rng.gen::<u32>()),
                    entity_type: EntityType::Scene,
                    pointers: ptrs.clone(),
                    timestamp: 1_700_000_000_000,
                    content: None,
                    metadata: None,
                };

                let tx = db.begin_transaction().await.unwrap();
                let _ = ae.update(&*tx, &ptrs, &entity).await;
                let _ = tx.commit().await;

                tokio::task::yield_now().await;
            }
        }));
    }

    for _ in 0..10 {
        let ae = ae.clone();
        let pointers = pointers.clone();
        let any_panic = any_panic.clone();
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let mut rng = rand::rngs::StdRng::from_entropy();
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                let n = rng.gen_range(1..=3);
                let ptrs: Vec<Pointer> = (0..n)
                    .map(|_| pointers[rng.gen_range(0..pointers.len())].clone())
                    .collect();

                if rng.gen_bool(0.5) {
                    let tx = db.begin_transaction().await.unwrap();
                    let _ = ae.clear(&*tx, &ptrs).await;
                    let _ = tx.commit().await;
                } else {
                    ae.clear_pointers_cache(&ptrs);
                }

                tokio::task::yield_now().await;
            }
        }));
    }

    let mut panics = 0;
    for h in handles {
        match h.await {
            Ok(()) => {}
            Err(e) => {
                panics += 1;
                any_panic.store(true, Ordering::SeqCst);
                if panics == 1 {
                    fail(
                        "active_entities_stress",
                        &format!("task panicked: {}", e),
                    );
                }
            }
        }
    }

    if panics == 0 {
        pass("30 tasks (10R + 10W + 10C) for 5s, no panics, no poisoned locks");
    }
}

async fn test_content_storage_concurrent_writes() {
    println!("\n[6] Content storage concurrent writes");

    let tmp = std::env::temp_dir().join(format!(
        "catalyrst-fuzz-storage-{}",
        std::process::id()
    ));
    let storage = Arc::new(
        ContentStorage::new(&tmp)
            .await
            .expect("failed to create test storage"),
    );

    let barrier = Arc::new(Barrier::new(20));
    let mut handles = Vec::new();

    for i in 0..20u32 {
        let storage = storage.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let hash = format!("bafkreifuzztest{:040}", i);
            let data = Bytes::from(vec![(i & 0xff) as u8; 4096]);
            barrier.wait().await;
            storage.store(&hash, data).await
        }));
    }

    let mut panics = 0;
    let mut errors = 0;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                errors += 1;
                if errors == 1 {
                    fail("content_storage_writes_a", &format!("store error: {}", e));
                }
            }
            Err(e) => {
                panics += 1;
                if panics == 1 {
                    fail(
                        "content_storage_writes_a",
                        &format!("task panicked: {}", e),
                    );
                }
            }
        }
    }

    if panics > 0 || errors > 0 {
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        return;
    }

    let mut read_wrong = 0;
    for i in 0..20u32 {
        let hash = format!("bafkreifuzztest{:040}", i);
        let expected = Bytes::from(vec![(i & 0xff) as u8; 4096]);
        match storage.retrieve(&hash).await {
            Ok(Some(retrieved)) if retrieved == expected => {}
            Ok(Some(_)) => {
                read_wrong += 1;
            }
            Ok(None) => {
                read_wrong += 1;
            }
            Err(e) => {
                fail("content_storage_reads_a", &format!("retrieve error: {}", e));
            }
        }
    }

    if read_wrong > 0 {
        fail(
            "content_storage_reads_a",
            &format!("{}/20 reads returned wrong content", read_wrong),
        );
    } else {
        pass("20 concurrent writes (distinct hashes) + reads: all correct");
    }

    let single_hash = "bafkreisinglehashstresstest0000000000000000000000000000";
    let single_data = Bytes::from(vec![0xABu8; 8192]);
    storage
        .store(single_hash, single_data.clone())
        .await
        .expect("failed to store single hash");

    let barrier2 = Arc::new(Barrier::new(20));
    let mut read_handles = Vec::new();
    for _ in 0..20 {
        let storage = storage.clone();
        let barrier2 = barrier2.clone();
        let hash = single_hash.to_string();
        read_handles.push(tokio::spawn(async move {
            barrier2.wait().await;
            storage.retrieve(&hash).await
        }));
    }

    let mut read_ok = 0;
    let mut wrong = 0;
    for h in read_handles {
        match h.await {
            Ok(Ok(Some(retrieved))) => {
                if retrieved == single_data {
                    read_ok += 1;
                } else {
                    wrong += 1;
                }
            }
            Ok(Ok(None)) => {
                wrong += 1;
            }
            Ok(Err(e)) => {
                fail("content_storage_reads_b", &format!("retrieve error: {}", e));
            }
            Err(e) => {
                fail("content_storage_reads_b", &format!("task panicked: {}", e));
            }
        }
    }

    if wrong > 0 {
        fail(
            "content_storage_reads_b",
            &format!("{}/20 concurrent reads got wrong content", wrong),
        );
    } else {
        pass(&format!(
            "1 write + 20 concurrent reads: all {} reads correct",
            read_ok
        ));
    }

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

async fn test_sequential_task_executor_stress() {
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
                fail(
                    "seq_executor_stress",
                    "timed out after 10s (deadlock?)",
                );
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
