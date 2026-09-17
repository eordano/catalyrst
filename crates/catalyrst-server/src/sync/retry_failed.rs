use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{Notify, Semaphore};
use tracing::{debug, error, info, warn};

use super::backends::{LiveFailedDeploymentsStore, LiveSyncDeployer};
use super::deploy_remote_entity::deploy_entity_streaming;
use super::{DeploymentContext, FailedDeployment, SyncError};

const BASE_RETRY_INTERVAL_MS: u64 = 900_000;
const MAX_RETRY_INTERVAL_MS: u64 = 86_400_000;
const DEFAULT_MAX_RETRIES: u32 = 10;

#[derive(Debug, Clone)]
pub struct RetryFailedConfig {
    /// Cadence of the polling loop. Distinct from the per-entity backoff below, which decides
    /// whether an entity the loop walks past is due at all.
    pub retry_delay_ms: u64,
    /// Attempts an entity gets before the worker gives up on it and drops the row.
    pub max_retries: u32,
}

impl Default for RetryFailedConfig {
    fn default() -> Self {
        Self {
            retry_delay_ms: 900_000,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl RetryFailedConfig {
    pub fn from_env() -> Self {
        Self {
            max_retries: parse_non_negative(
                "MAX_FAILED_DEPLOYMENT_RETRIES",
                std::env::var("MAX_FAILED_DEPLOYMENT_RETRIES")
                    .ok()
                    .as_deref(),
                DEFAULT_MAX_RETRIES,
            ),
            ..Default::default()
        }
    }
}

fn parse_non_negative(name: &str, value: Option<&str>, default: u32) -> u32 {
    let Some(raw) = value else { return default };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default;
    }
    match trimmed.parse::<u32>() {
        Ok(n) => n,
        Err(_) => panic!("invalid {name}: expected a non-negative integer but got {raw:?}"),
    }
}

/// Wait an entity earns after spending `retry_count` attempts: the base interval doubled per
/// attempt, clamped at a day so a long-lived failure still gets a daily look.
fn backoff_ms(retry_count: u32) -> u64 {
    BASE_RETRY_INTERVAL_MS
        .saturating_mul(1u64.checked_shl(retry_count).unwrap_or(u64::MAX))
        .min(MAX_RETRY_INTERVAL_MS)
}

/// Attempt state an entity earns for the attempt about to be dispatched: one more attempt spent
/// against the cap, and the wait that attempt buys. The wait is returned relative because the
/// deadline belongs to the instant the failure is recorded, not to the instant the attempt was
/// dispatched -- the work in between can take a while, and stamping early would hand back a
/// deadline that is already in the past by the time the re-report lands.
fn next_attempt(failure: &FailedDeployment) -> (u32, u64) {
    (failure.retry_count + 1, backoff_ms(failure.retry_count))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryDisposition {
    Due,
    Waiting { remaining_ms: i64 },
    Exhausted,
    NoAuthChain,
}

fn classify(failure: &FailedDeployment, max_retries: u32, now_ms: i64) -> RetryDisposition {
    if failure.auth_chain.is_empty() {
        return RetryDisposition::NoAuthChain;
    }
    if failure.retry_count >= max_retries {
        return RetryDisposition::Exhausted;
    }
    let remaining_ms = failure.next_retry_at - now_ms;
    if remaining_ms > 0 {
        return RetryDisposition::Waiting { remaining_ms };
    }
    RetryDisposition::Due
}

pub struct RetryFailedDeployments {
    config: RetryFailedConfig,
    http_client: reqwest::Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
    deployer: Arc<LiveSyncDeployer>,
    failed_store: Arc<LiveFailedDeploymentsStore>,
    peer_servers: Arc<tokio::sync::RwLock<Vec<String>>>,
    stop_notify: Arc<Notify>,
}

impl RetryFailedDeployments {
    pub fn new(
        config: RetryFailedConfig,
        http_client: reqwest::Client,
        storage: Arc<catalyrst_storage::ContentStorage>,
        deployer: Arc<LiveSyncDeployer>,
        failed_store: Arc<LiveFailedDeploymentsStore>,
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

    /// Drops the entries that burned through the cap, answering how many actually went. Runs
    /// before the peer-server guard: a node that loses its peer list must still shed entries it
    /// already gave up on rather than hold them at the cap forever.
    async fn evict_exhausted(&self, exhausted: &[String]) -> u64 {
        if exhausted.is_empty() {
            return 0;
        }
        match self
            .failed_store
            .remove_exhausted(exhausted, self.config.max_retries)
            .await
        {
            Ok(removed) => removed.len() as u64,
            Err(e) => {
                error!(error = %e, count = exhausted.len(), "Failed to evict exhausted failed deployments");
                0
            }
        }
    }

    pub async fn execute_retry_cycle(&self) -> Result<RetryStats, SyncError> {
        let failed = self.failed_store.get_all_failed().await?;

        if failed.is_empty() {
            debug!("No failed deployments to retry");
            return Ok(RetryStats::default());
        }

        info!(count = failed.len(), "Retrying failed deployments");

        use std::sync::atomic::{AtomicU64, Ordering};

        let retry_concurrency: usize = std::env::var("SYNC_RETRY_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(10);

        let content_semaphore = Arc::new(Semaphore::new(50));
        let succeeded = AtomicU64::new(0);
        let still_failing = AtomicU64::new(0);

        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut due: Vec<&FailedDeployment> = Vec::new();
        let mut exhausted: Vec<String> = Vec::new();
        let mut waiting: u64 = 0;

        for failure in &failed {
            match classify(failure, self.config.max_retries, now_ms) {
                RetryDisposition::Exhausted => {
                    warn!(
                        entity_id = %failure.entity_id,
                        entity_type = %failure.entity_type,
                        retry_count = failure.retry_count,
                        "Permanently giving up on failed deployment"
                    );
                    exhausted.push(failure.entity_id.clone());
                }
                RetryDisposition::Waiting { remaining_ms } => {
                    debug!(
                        entity_id = %failure.entity_id,
                        entity_type = %failure.entity_type,
                        retry_count = failure.retry_count,
                        next_retry_in_seconds = remaining_ms / 1000,
                        "Skipping failed deployment until backoff expires"
                    );
                    waiting += 1;
                }
                RetryDisposition::NoAuthChain => {
                    info!(
                        entity_id = %failure.entity_id,
                        entity_type = %failure.entity_type,
                        "Can't retry failed deployment. Because it lacks of authChain"
                    );
                }
                RetryDisposition::Due => due.push(failure),
            }
        }

        let evicted = self.evict_exhausted(&exhausted).await;

        let servers = self.peer_servers.read().await.clone();
        if servers.is_empty() {
            warn!("No peer servers available for retry");
            return Ok(RetryStats {
                waiting,
                evicted,
                ..Default::default()
            });
        }

        let servers_ref: &[String] = &servers;
        let succeeded_ref = &succeeded;
        let still_failing_ref = &still_failing;

        futures::stream::iter(due)
            .for_each_concurrent(retry_concurrency, |failure| {
                let content_semaphore = content_semaphore.clone();
                async move {
                    let attempt = next_attempt(failure);
                    match deploy_entity_streaming(
                        &self.http_client,
                        self.storage.clone(),
                        self.deployer.as_ref(),
                        &failure.entity_id,
                        &failure.auth_chain,
                        servers_ref,
                        DeploymentContext::SyncedFix,
                        content_semaphore,
                        None,
                        Some(attempt),
                    )
                    .await
                    {
                        Ok(()) => {
                            info!(entity_id = %failure.entity_id, "Successfully retried failed deployment");
                            if let Err(e) = self
                                .failed_store
                                .remove(&failure.entity_id, failure.retry_count)
                                .await
                            {
                                error!(entity_id = %failure.entity_id, error = %e, "Failed to remove successful retry from store");
                            }
                            succeeded_ref.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            debug!(entity_id = %failure.entity_id, error = %e, "Retry still failing");
                            let deferred = FailedDeployment {
                                error_description: e.to_string(),
                                retry_count: attempt.0,
                                next_retry_at: chrono::Utc::now().timestamp_millis()
                                    + attempt.1 as i64,
                                ..failure.clone()
                            };
                            if let Err(report_err) = self.failed_store.report_failure(deferred).await {
                                error!(entity_id = %failure.entity_id, error = %report_err, "Failed to record retry backoff");
                            }
                            still_failing_ref.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })
            .await;

        let stats = RetryStats {
            succeeded: succeeded.load(Ordering::Relaxed),
            still_failing: still_failing.load(Ordering::Relaxed),
            waiting,
            evicted,
        };

        info!(
            succeeded = stats.succeeded,
            still_failing = stats.still_failing,
            waiting = stats.waiting,
            evicted = stats.evicted,
            "Retry cycle complete"
        );
        Ok(stats)
    }

    pub async fn run(&self) {
        info!(
            delay_ms = self.config.retry_delay_ms,
            max_retries = self.config.max_retries,
            "Starting retry-failed-deployments loop"
        );

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
    pub waiting: u64,
    pub evicted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::{AuthLink, AuthLinkType, FailureReason};

    fn failure(retry_count: u32, next_retry_at: i64) -> FailedDeployment {
        FailedDeployment {
            entity_type: "scene".to_string(),
            entity_id: "bafytest".to_string(),
            reason: FailureReason::DeploymentError,
            auth_chain: vec![AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: "0x0000000000000000000000000000000000000001".to_string(),
                signature: None,
            }],
            error_description: "boom".to_string(),
            failure_timestamp: 0,
            snapshot_hash: None,
            retry_count,
            next_retry_at,
        }
    }

    fn without_auth_chain(mut failure: FailedDeployment) -> FailedDeployment {
        failure.auth_chain = Vec::new();
        failure
    }

    #[test]
    fn backoff_doubles_per_spent_attempt() {
        assert_eq!(backoff_ms(0), BASE_RETRY_INTERVAL_MS);
        assert_eq!(backoff_ms(1), BASE_RETRY_INTERVAL_MS * 2);
        assert_eq!(backoff_ms(4), BASE_RETRY_INTERVAL_MS * 16);
    }

    #[test]
    fn backoff_clamps_at_a_day_without_overflowing() {
        assert_eq!(backoff_ms(7), MAX_RETRY_INTERVAL_MS);
        assert_eq!(backoff_ms(64), MAX_RETRY_INTERVAL_MS);
        assert_eq!(backoff_ms(u32::MAX), MAX_RETRY_INTERVAL_MS);
    }

    #[test]
    fn an_attempt_spends_one_retry_and_buys_the_backoff_of_the_attempt_before_it() {
        assert_eq!(next_attempt(&failure(0, 0)), (1, BASE_RETRY_INTERVAL_MS));
        assert_eq!(next_attempt(&failure(3, 0)), (4, backoff_ms(3)));
        assert_eq!(
            next_attempt(&failure(9, 1_700_000_000_000)),
            (10, MAX_RETRY_INTERVAL_MS)
        );
    }

    #[test]
    fn an_entity_at_the_cap_is_exhausted_not_retried() {
        assert_eq!(
            classify(&failure(10, 0), 10, 1_000),
            RetryDisposition::Exhausted
        );
        assert_eq!(
            classify(&failure(11, 0), 10, 1_000),
            RetryDisposition::Exhausted
        );
        assert_eq!(classify(&failure(9, 0), 10, 1_000), RetryDisposition::Due);
    }

    #[test]
    fn a_cap_of_zero_gives_up_on_the_first_look() {
        assert_eq!(
            classify(&failure(0, 0), 0, 1_000),
            RetryDisposition::Exhausted
        );
    }

    #[test]
    fn an_entity_still_in_backoff_waits() {
        assert_eq!(
            classify(&failure(2, 5_000), 10, 1_000),
            RetryDisposition::Waiting {
                remaining_ms: 4_000
            }
        );
        assert_eq!(
            classify(&failure(2, 1_000), 10, 1_000),
            RetryDisposition::Due
        );
        assert_eq!(classify(&failure(2, 0), 10, 1_000), RetryDisposition::Due);
    }

    #[test]
    fn the_cap_is_checked_before_the_deadline() {
        assert_eq!(
            classify(&failure(10, i64::MAX), 10, 1_000),
            RetryDisposition::Exhausted
        );
    }

    #[test]
    fn an_entity_without_an_auth_chain_is_never_retried_or_evicted() {
        assert_eq!(
            classify(&without_auth_chain(failure(0, 0)), 10, 1_000),
            RetryDisposition::NoAuthChain
        );
        assert_eq!(
            classify(&without_auth_chain(failure(10, 0)), 10, 1_000),
            RetryDisposition::NoAuthChain
        );
        assert_eq!(
            classify(&without_auth_chain(failure(99, i64::MAX)), 0, 1_000),
            RetryDisposition::NoAuthChain
        );
    }

    #[test]
    fn max_retries_defaults_and_accepts_zero() {
        assert_eq!(parse_non_negative("X", None, 10), 10);
        assert_eq!(parse_non_negative("X", Some("  "), 10), 10);
        assert_eq!(parse_non_negative("X", Some(" 0 "), 10), 0);
        assert_eq!(parse_non_negative("X", Some("3"), 10), 3);
    }

    #[test]
    #[should_panic(expected = "invalid X")]
    fn max_retries_rejects_garbage() {
        parse_non_negative("X", Some("ten"), 10);
    }
}
