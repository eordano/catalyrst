use std::sync::Arc;
use tokio::sync::{Notify, Semaphore};
use tracing::{debug, error, info, warn};

use crate::{
    ContentStorage, DeploymentContext, Deployer, FailedDeploymentsStore,
    SyncError,
};
use crate::deploy_remote_entity::deploy_entity_streaming;

#[derive(Debug, Clone)]
pub struct RetryFailedConfig {
    pub retry_delay_ms: u64,
}

impl Default for RetryFailedConfig {
    fn default() -> Self {
        Self {
            retry_delay_ms: 900_000,
        }
    }
}

pub struct RetryFailedDeployments {
    config: RetryFailedConfig,
    http_client: reqwest::Client,
    storage: Arc<dyn ContentStorage>,
    deployer: Arc<dyn Deployer>,
    failed_store: Arc<dyn FailedDeploymentsStore>,
    peer_servers: Arc<tokio::sync::RwLock<Vec<String>>>,
    stop_notify: Arc<Notify>,
}

impl RetryFailedDeployments {
    pub fn new(
        config: RetryFailedConfig,
        http_client: reqwest::Client,
        storage: Arc<dyn ContentStorage>,
        deployer: Arc<dyn Deployer>,
        failed_store: Arc<dyn FailedDeploymentsStore>,
        peer_servers: Arc<tokio::sync::RwLock<Vec<String>>>,
    ) -> Self {
        RetryFailedDeployments {
            config,
            http_client,
            storage,
            deployer,
            failed_store,
            peer_servers,
            stop_notify: Arc::new(Notify::new()),
        }
    }

    pub async fn execute_retry_cycle(&self) -> Result<RetryStats, SyncError> {
        let failed = self.failed_store.get_all_failed().await?;

        if failed.is_empty() {
            debug!("No failed deployments to retry");
            return Ok(RetryStats::default());
        }

        info!(count = failed.len(), "Retrying failed deployments");

        let servers = self.peer_servers.read().await.clone();
        if servers.is_empty() {
            warn!("No peer servers available for retry");
            return Err(SyncError::NoServers);
        }

        let content_semaphore = Arc::new(Semaphore::new(50));
        let mut stats = RetryStats::default();

        for failure in &failed {
            match deploy_entity_streaming(
                &self.http_client,
                self.storage.clone(),
                self.deployer.as_ref(),
                &failure.entity_id,
                &failure.auth_chain,
                &servers,
                DeploymentContext::SyncedFix,
                content_semaphore.clone(),
            )
            .await
            {
                Ok(()) => {
                    info!(entity_id = %failure.entity_id, "Successfully retried failed deployment");
                    if let Err(e) = self.failed_store.remove(&failure.entity_id).await {
                        error!(entity_id = %failure.entity_id, error = %e, "Failed to remove successful retry from store");
                    }
                    stats.succeeded += 1;
                }
                Err(e) => {
                    debug!(entity_id = %failure.entity_id, error = %e, "Retry still failing");
                    stats.still_failing += 1;
                }
            }
        }

        info!(succeeded = stats.succeeded, still_failing = stats.still_failing, "Retry cycle complete");
        Ok(stats)
    }

    pub async fn run(&self) {
        info!(delay_ms = self.config.retry_delay_ms, "Starting retry-failed-deployments loop");

        loop {
            tokio::select! {
                _ = self.stop_notify.notified() => {
                    info!("Retry-failed-deployments loop stopped");
                    return;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(self.config.retry_delay_ms)) => {
                    match self.execute_retry_cycle().await {
                        Ok(_) => {}
                        Err(e) => {
                            error!(error = %e, "Error during retry cycle");
                        }
                    }
                }
            }
        }
    }

    pub fn stop(&self) {
        self.stop_notify.notify_one();
    }
}

#[derive(Debug, Default, Clone)]
pub struct RetryStats {
    pub succeeded: u64,
    pub still_failing: u64,
}
