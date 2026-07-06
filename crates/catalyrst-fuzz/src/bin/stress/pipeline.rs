use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::RngExt;
use rand::SeedableRng;

use catalyrst_deployer::deployment_service::PointerLockManager;
use catalyrst_deployer::{
    BackendError, ContentHash, DatabaseBackend, Deployment, DeploymentContext,
    DeploymentIdWithPointers, DeploymentOptions, Entity, EntityContentEntry, EntityType,
    FailedDeployment, GcStaleProfilesResult, LocalDeploymentAuditInfo, Pointer,
};

use crate::{fail, pass};

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
        let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
        let delay = rng.random_range(0..10);
        tokio::time::sleep(Duration::from_millis(delay)).await;

        if rng.random_bool(self.fail_rate) {
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

pub(crate) struct MockTransaction;

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

pub(crate) async fn test_parallel_pipeline_stress() {
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
