use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use super::backends::{LiveDeploymentRepository, LiveFailedDeploymentsStore, LiveSyncDeployer};
use super::bloom_filter::BloomFilter;
use super::{
    DeploymentContext, FailedDeployment, FailureReason, SyncDeployment, SyncError, TimeRange,
};

/// Per-stream accounting of what was handed to the deployer against what the deployer
/// confirmed. Draining (`on_idle`) only proves the queue emptied, not that every entity in it
/// was deployed, so callers create one of these per snapshot or per pointer-changes run and
/// refuse to advance their sync cursor while `acknowledged < scheduled`.
///
/// "Acknowledged" means the entity is durably accounted for: either deployed, or recorded in
/// `failed_deployments` where the retry loop owns it. An entity that fails BOTH is never
/// acknowledged -- that is the silent-loss case the sync frontier must not advance past.
#[derive(Debug, Default)]
pub struct DeploymentReport {
    scheduled: std::sync::atomic::AtomicU64,
    acknowledged: std::sync::atomic::AtomicU64,
    lost: std::sync::atomic::AtomicU64,
}

impl DeploymentReport {
    pub fn scheduled(&self) -> u64 {
        self.scheduled.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn acknowledged(&self) -> u64 {
        self.acknowledged.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Entities of THIS report dropped with no durable record anywhere -- neither deployed nor
    /// recorded in failed_deployments. Attributed per report (the batch flush carries each
    /// entity's report) so a loss in one stream does not force every concurrent pass to hold
    /// back, the way comparing the deployer's global counter did: during the straggler retry
    /// that global check ran concurrently with live steady-state streams, and any loss anywhere
    /// marked every snapshot in the straggler pass as failed forever.
    pub fn lost(&self) -> u64 {
        self.lost.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Counted on the same code paths that bump the deployers' global `lost` counters, which
    /// remain as process-wide metrics only.
    pub fn record_lost(&self) {
        self.lost.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Counted when an entity is handed to the deployer, before any await: a synchronous
    /// completion acknowledging first would let acknowledged briefly exceed scheduled and
    /// make an incomplete run look complete.
    pub fn record_scheduled(&self) {
        self.scheduled
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Counted once the entity is durably accounted for: deployed, or recorded in
    /// failed_deployments where the retry loop owns it.
    pub fn record_acknowledged(&self) {
        self.acknowledged
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// True when everything scheduled through this report came back acknowledged. Only
    /// meaningful after the deployer has drained (`on_idle`), like upstream's post-drain
    /// processed-marker re-check.
    pub fn is_complete(&self) -> bool {
        self.acknowledged() >= self.scheduled()
    }
}

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
    lost: Arc<std::sync::atomic::AtomicU64>,
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
            lost: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Cumulative count of entities dropped with no durable record anywhere: neither deployed
    /// nor recorded in failed_deployments (both writes failed), including entities lost by a
    /// failed batch flush inside the underlying deployer. Callers snapshot this before a phase
    /// and compare after draining -- any growth means the sync frontier must not advance,
    /// because nothing will re-deliver those entities.
    pub fn lost_count(&self) -> u64 {
        self.lost.load(std::sync::atomic::Ordering::SeqCst) + self.deployer.lost_count()
    }

    pub async fn schedule_entity_deployment(
        &self,
        entity: SyncDeployment,
        content_servers: &[String],
        report: Option<&Arc<DeploymentReport>>,
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
            // Enroll in the notify queue BEFORE the re-check: a `notified()`
            // future only registers on first poll, so without `enable()` a
            // completion's notify_waiters() landing between the re-check and
            // the await is simply lost -- and when that completion was the
            // last one in flight, this await sleeps forever. That lost
            // wakeup is the silent post-catch-up ingest wedge (unit active,
            // zero logs, frontier frozen, restart cures).
            let notified = self.idle_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
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
        let lost = self.lost.clone();
        let servers: Vec<String> = self.servers.read().clone();
        let report = report.cloned();

        in_flight.fetch_add(1, std::sync::atomic::Ordering::Release);
        if let Some(report) = &report {
            report.record_scheduled();
        }

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
                report.as_ref(),
            )
            .await;

            let acknowledged = match result {
                Ok(()) => {
                    deployed_bloom.write().add(&entity.entity_id);
                    info!(
                        entity_id = %entity.entity_id,
                        entity_type = %entity.entity_type,
                        "Synced deployment successful"
                    );
                    true
                }
                Err(e) => {
                    warn!(
                        entity_id = %entity.entity_id,
                        entity_type = %entity.entity_type,
                        error = %e,
                        "Entity deployment failed"
                    );
                    // A failure only counts as handled once it is durably recorded, so the
                    // retry loop owns it. If even that fails, the entity is acknowledged to
                    // nobody -- leave it unacknowledged and count it as lost, which holds the
                    // sync frontier back so the entity is re-delivered after a restart.
                    match failed_store
                        .report_failure(FailedDeployment {
                            entity_type: entity.entity_type.clone(),
                            entity_id: entity.entity_id.clone(),
                            reason: FailureReason::DeploymentError,
                            auth_chain: entity.auth_chain.clone(),
                            error_description: e.to_string(),
                            failure_timestamp: chrono::Utc::now().timestamp_millis(),
                            snapshot_hash: None,
                        })
                        .await
                    {
                        Ok(()) => true,
                        Err(record_err) => {
                            lost.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            if let Some(report) = &report {
                                report.record_lost();
                            }
                            error!(
                                entity_id = %entity.entity_id,
                                error = %record_err,
                                "Failed deployment could not be recorded in failed_deployments; \
                                 holding the sync frontier back"
                            );
                            false
                        }
                    }
                }
            };

            if acknowledged {
                if let Some(report) = &report {
                    report.record_acknowledged();
                }
            }

            in_flight.fetch_sub(1, std::sync::atomic::Ordering::Release);
            idle_notify.notify_waiters();
        });

        Ok(())
    }

    pub async fn on_idle(&self) -> Result<(), SyncError> {
        loop {
            // Same lost-wakeup hazard as the schedule gate above: enable()
            // enrolls before the in_flight check, so the final completion's
            // notify_waiters() cannot slip through the gap. This is the path
            // the long-poll loop parks in every poll boundary -- the exact
            // spot the ingest wedged.
            let notified = self.idle_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
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
