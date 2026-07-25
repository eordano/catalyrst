use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::backends::{LiveDeploymentRepository, LiveFailedDeploymentsStore, LiveSyncDeployer};
use super::bloom_filter::BloomFilter;
use super::{
    DeploymentContext, FailedDeployment, FailureReason, SyncDeployment, SyncError, TimeRange,
};

#[derive(Debug, Clone)]
pub struct BatchDeployerConfig {
    pub content_download_concurrency: usize,
    pub ignored_types: HashSet<String>,
    pub profile_max_age_ms: i64,
    pub max_queue_depth: usize,
}

impl Default for BatchDeployerConfig {
    fn default() -> Self {
        Self {
            content_download_concurrency: 200,
            ignored_types: HashSet::new(),
            profile_max_age_ms: 31_536_000_000,
            max_queue_depth: 1000,
        }
    }
}

pub struct BatchDeployer {
    config: BatchDeployerConfig,
    http_client: reqwest::Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
    deployer: Arc<LiveSyncDeployer>,
    deployment_repo: Arc<LiveDeploymentRepository>,
    failed_store: Arc<LiveFailedDeploymentsStore>,

    content_semaphore: Arc<Semaphore>,
    in_flight: Arc<std::sync::atomic::AtomicUsize>,
    idle_notify: Arc<tokio::sync::Notify>,
    deployed_bloom: Arc<parking_lot::RwLock<BloomFilter>>,
    servers: Arc<parking_lot::RwLock<Vec<String>>>,
}

impl BatchDeployer {
    pub fn new(
        config: BatchDeployerConfig,
        http_client: reqwest::Client,
        storage: Arc<catalyrst_storage::ContentStorage>,
        deployer: Arc<LiveSyncDeployer>,
        deployment_repo: Arc<LiveDeploymentRepository>,
        failed_store: Arc<LiveFailedDeploymentsStore>,
    ) -> Self {
        Self::with_bloom(
            config,
            http_client,
            storage,
            deployer,
            deployment_repo,
            failed_store,
            BloomFilter::new(),
        )
    }

    pub fn with_bloom(
        config: BatchDeployerConfig,
        http_client: reqwest::Client,
        storage: Arc<catalyrst_storage::ContentStorage>,
        deployer: Arc<LiveSyncDeployer>,
        deployment_repo: Arc<LiveDeploymentRepository>,
        failed_store: Arc<LiveFailedDeploymentsStore>,
        bloom: BloomFilter,
    ) -> Self {
        let content_concurrency = config.content_download_concurrency;
        BatchDeployer {
            config,
            http_client,
            storage,
            deployer,
            deployment_repo,
            failed_store,
            content_semaphore: Arc::new(Semaphore::new(content_concurrency)),
            in_flight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            idle_notify: Arc::new(tokio::sync::Notify::new()),
            deployed_bloom: Arc::new(parking_lot::RwLock::new(bloom)),
            servers: Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }

    pub async fn schedule_entity_deployment(
        &self,
        entity: SyncDeployment,
        content_servers: &[String],
    ) -> Result<(), SyncError> {
        if self.config.ignored_types.contains(&entity.entity_type) {
            return Ok(());
        }

        if entity.entity_type == "profile" {
            let now = chrono::Utc::now().timestamp_millis();
            if entity.entity_timestamp < now - self.config.profile_max_age_ms {
                return Ok(());
            }
        }

        if self.deployed_bloom.read().maybe_contains(&entity.entity_id)
            && self
                .deployment_repo
                .is_entity_deployed(&entity.entity_id, entity.entity_timestamp)
                .await?
        {
            return Ok(());
        }

        {
            let mut s = self.servers.write();
            for server in content_servers {
                if !s.contains(server) {
                    s.push(server.clone());
                }
            }
        }

        while self.in_flight.load(std::sync::atomic::Ordering::Acquire)
            >= self.config.max_queue_depth
        {
            let notified = self.idle_notify.notified();
            if self.in_flight.load(std::sync::atomic::Ordering::Acquire)
                < self.config.max_queue_depth
            {
                break;
            }
            notified.await;
        }

        let http_client = self.http_client.clone();
        let storage = self.storage.clone();
        let deployer = self.deployer.clone();
        let failed_store = self.failed_store.clone();
        let content_semaphore = self.content_semaphore.clone();
        let in_flight = self.in_flight.clone();
        let idle_notify = self.idle_notify.clone();
        let deployed_bloom = self.deployed_bloom.clone();
        let servers: Vec<String> = self.servers.read().clone();

        in_flight.fetch_add(1, std::sync::atomic::Ordering::Release);

        tokio::spawn(async move {
            let result = super::deploy_remote_entity::deploy_entity_streaming(
                &http_client,
                storage,
                deployer.as_ref(),
                &entity.entity_id,
                &entity.auth_chain,
                &servers,
                DeploymentContext::Synced,
                content_semaphore,
            )
            .await;

            match result {
                Ok(()) => {
                    deployed_bloom.write().add(&entity.entity_id);
                    info!(
                        entity_id = %entity.entity_id,
                        entity_type = %entity.entity_type,
                        "Synced deployment successful"
                    );
                }
                Err(e) => {
                    warn!(
                        entity_id = %entity.entity_id,
                        entity_type = %entity.entity_type,
                        error = %e,
                        "Entity deployment failed"
                    );
                    let _ = failed_store
                        .report_failure(FailedDeployment {
                            entity_type: entity.entity_type.clone(),
                            entity_id: entity.entity_id.clone(),
                            reason: FailureReason::DeploymentError,
                            auth_chain: entity.auth_chain.clone(),
                            error_description: e.to_string(),
                            failure_timestamp: chrono::Utc::now().timestamp_millis(),
                            snapshot_hash: None,
                        })
                        .await;
                }
            }

            in_flight.fetch_sub(1, std::sync::atomic::Ordering::Release);
            idle_notify.notify_waiters();
        });

        Ok(())
    }

    pub async fn on_idle(&self) -> Result<(), SyncError> {
        loop {
            let notified = self.idle_notify.notified();
            if self.in_flight.load(std::sync::atomic::Ordering::Acquire) == 0 {
                self.deployer.flush().await?;
                return Ok(());
            }
            notified.await;
        }
    }

    pub async fn prepare_for_deployments_in(
        &self,
        _time_ranges: &[TimeRange],
    ) -> Result<(), SyncError> {
        Ok(())
    }
}
