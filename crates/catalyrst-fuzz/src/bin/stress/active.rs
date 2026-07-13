use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::RngExt;
use rand::SeedableRng;

use catalyrst_deployer::{
    active_entities::ActiveEntitiesConfig, ActiveEntities, BackendError, ContentHash,
    DatabaseBackend, Deployment, DeploymentIdWithPointers, DeploymentOptions, Entity,
    EntityContentEntry, EntityType, FailedDeployment, GcStaleProfilesResult, Pointer,
};

use crate::pipeline::MockTransaction;
use crate::{fail, pass};

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

pub(crate) async fn test_active_entities_cache_stress() {
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
            let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                if rng.random_bool(0.5) {
                    let n = rng.random_range(1..=5);
                    let ids: Vec<String> = (0..n)
                        .map(|_| entity_ids[rng.random_range(0..entity_ids.len())].clone())
                        .collect();
                    let _ = ae.with_ids(&ids).await;
                } else {
                    let n = rng.random_range(1..=5);
                    let ptrs: Vec<Pointer> = (0..n)
                        .map(|_| pointers[rng.random_range(0..pointers.len())].clone())
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
            let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                let n = rng.random_range(1..=3);
                let ptrs: Vec<Pointer> = (0..n)
                    .map(|_| pointers[rng.random_range(0..pointers.len())].clone())
                    .collect();
                let entity = Entity {
                    id: format!("w{}-{}", writer_id, rng.random::<u32>()),
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
            let mut rng = rand::rngs::StdRng::from_rng(&mut rand::rng());
            while start.elapsed() < duration && !any_panic.load(Ordering::Relaxed) {
                let n = rng.random_range(1..=3);
                let ptrs: Vec<Pointer> = (0..n)
                    .map(|_| pointers[rng.random_range(0..pointers.len())].clone())
                    .collect();

                if rng.random_bool(0.5) {
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
                    fail("active_entities_stress", &format!("task panicked: {}", e));
                }
            }
        }
    }

    if panics == 0 {
        pass("30 tasks (10R + 10W + 10C) for 5s, no panics, no poisoned locks");
    }
}
