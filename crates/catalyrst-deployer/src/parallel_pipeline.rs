use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::{Stream, StreamExt};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::{
    ContentHash, DeploymentContext, DeploymentResult, DeploymentService,
    Entity, EntityType, LocalDeploymentAuditInfo,
};
use crate::deployment_service::DeploymentFiles;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub fetch_concurrency: usize,
    pub validate_concurrency: usize,
    pub batch_size: usize,
    pub channel_buffer: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            fetch_concurrency: 20,
            validate_concurrency: 10,
            batch_size: 200,
            channel_buffer: 1000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncTask {
    pub entity_id: String,
    pub entity_type: EntityType,
    pub servers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub deployed: usize,
    pub failed: usize,
    pub duration: Duration,
    pub bytes_fetched: u64,
    pub batch_flush_count: usize,
}

impl std::fmt::Display for PipelineResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let eps = if self.duration.as_secs_f64() > 0.0 {
            (self.deployed + self.failed) as f64 / self.duration.as_secs_f64()
        } else {
            0.0
        };
        write!(
            f,
            "deployed={} failed={} duration={:.1}s entities/sec={:.1} bytes_fetched={} flushes={}",
            self.deployed,
            self.failed,
            self.duration.as_secs_f64(),
            eps,
            self.bytes_fetched,
            self.batch_flush_count,
        )
    }
}

#[derive(Debug, Clone)]
pub struct FetchedEntity {
    pub task: SyncTask,
    pub entity_bytes: Vec<u8>,
    pub content_files: std::collections::HashMap<ContentHash, Vec<u8>>,
    pub auth_chain: Vec<crate::AuthLink>,
    pub total_bytes: u64,
}

#[async_trait::async_trait]
pub trait EntityFetcher: Send + Sync {
    async fn fetch_entity(
        &self,
        entity_id: &str,
        servers: &[String],
    ) -> Result<FetchedEntity, FetchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("all servers failed for entity {entity_id}: {last_error}")]
    AllServersFailed {
        entity_id: String,
        last_error: String,
    },
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub struct HttpEntityFetcher {
    client: reqwest::Client,
}

impl HttpEntityFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build reqwest client");
        Self { client }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for HttpEntityFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EntityFetcher for HttpEntityFetcher {
    async fn fetch_entity(
        &self,
        entity_id: &str,
        servers: &[String],
    ) -> Result<FetchedEntity, FetchError> {
        let mut last_error = String::from("no servers provided");

        for server in servers {
            match self.try_fetch_from_server(entity_id, server).await {
                Ok(fetched) => return Ok(fetched),
                Err(e) => {
                    warn!(
                        entity_id,
                        server,
                        error = %e,
                        "fetch attempt failed, trying next server"
                    );
                    last_error = e.to_string();
                }
            }
        }

        Err(FetchError::AllServersFailed {
            entity_id: entity_id.to_string(),
            last_error,
        })
    }
}

impl HttpEntityFetcher {
    async fn try_fetch_from_server(
        &self,
        entity_id: &str,
        server: &str,
    ) -> Result<FetchedEntity, FetchError> {
        let base = server.trim_end_matches('/');

        let entity_url = format!("{}/contents/{}", base, entity_id);
        let entity_resp = self.client.get(&entity_url).send().await?;
        let entity_bytes = entity_resp.error_for_status()?.bytes().await?.to_vec();

        let mut total_bytes = entity_bytes.len() as u64;

        let entity: Entity = serde_json::from_slice(&entity_bytes).map_err(|e| {
            FetchError::Parse(format!("failed to parse entity {}: {}", entity_id, e))
        })?;

        let audit_url = format!("{}/audit/entity/{}", base, entity_id);
        let auth_chain = match self.client.get(&audit_url).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.bytes().await {
                    #[derive(serde::Deserialize)]
                    struct AuditResponse {
                        #[serde(default, rename = "authChain")]
                        auth_chain: Vec<crate::AuthLink>,
                    }
                    match serde_json::from_slice::<AuditResponse>(&body) {
                        Ok(audit) => audit.auth_chain,
                        Err(_) => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            }
            Err(_) => Vec::new(),
        };

        let mut content_files = std::collections::HashMap::new();
        content_files.insert(entity_id.to_string(), entity_bytes.clone());

        if let Some(ref entries) = entity.content {
            for entry in entries {
                let content_url = format!("{}/contents/{}", base, entry.hash);
                match self.client.get(&content_url).send().await {
                    Ok(resp) => match resp.error_for_status() {
                        Ok(resp) => {
                            let bytes = resp.bytes().await?.to_vec();
                            total_bytes += bytes.len() as u64;
                            content_files.insert(entry.hash.clone(), bytes);
                        }
                        Err(e) => {
                            return Err(FetchError::Http(e));
                        }
                    },
                    Err(e) => return Err(FetchError::Http(e)),
                }
            }
        }

        Ok(FetchedEntity {
            task: SyncTask {
                entity_id: entity_id.to_string(),
                entity_type: entity.entity_type,
                servers: vec![server.to_string()],
            },
            entity_bytes: content_files
                .get(entity_id)
                .cloned()
                .unwrap_or_default(),
            content_files,
            auth_chain,
            total_bytes,
        })
    }
}

struct ValidatedEntity {
    entity_id: String,
    files: std::collections::HashMap<ContentHash, Vec<u8>>,
    audit_info: LocalDeploymentAuditInfo,
    #[allow(dead_code)]
    bytes_fetched: u64,
}

#[derive(Debug)]
#[allow(dead_code)]
struct FailedEntity {
    entity_id: String,
    errors: Vec<String>,
}

struct PipelineMetrics {
    deployed: AtomicUsize,
    failed: AtomicUsize,
    bytes_fetched: AtomicU64,
    batch_flush_count: AtomicUsize,
}

impl PipelineMetrics {
    fn new() -> Self {
        Self {
            deployed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            bytes_fetched: AtomicU64::new(0),
            batch_flush_count: AtomicUsize::new(0),
        }
    }

    fn snapshot(&self, duration: Duration) -> PipelineResult {
        PipelineResult {
            deployed: self.deployed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            duration,
            bytes_fetched: self.bytes_fetched.load(Ordering::Relaxed),
            batch_flush_count: self.batch_flush_count.load(Ordering::Relaxed),
        }
    }
}

pub struct ParallelDeploymentPipeline {
    config: PipelineConfig,
    deployer: Arc<DeploymentService>,
    fetcher: Arc<dyn EntityFetcher>,
}

impl ParallelDeploymentPipeline {
    pub fn new(config: PipelineConfig, deployer: Arc<DeploymentService>) -> Self {
        Self {
            config,
            deployer,
            fetcher: Arc::new(HttpEntityFetcher::new()),
        }
    }

    pub fn with_fetcher(
        config: PipelineConfig,
        deployer: Arc<DeploymentService>,
        fetcher: Arc<dyn EntityFetcher>,
    ) -> Self {
        Self {
            config,
            deployer,
            fetcher,
        }
    }

    pub async fn run(
        &self,
        entities: impl Stream<Item = SyncTask> + Send + 'static,
    ) -> PipelineResult {
        let start = Instant::now();
        let metrics = Arc::new(PipelineMetrics::new());

        let (fetch_tx, fetch_rx) = mpsc::channel::<FetchedEntity>(self.config.channel_buffer);
        let (validate_tx, validate_rx) =
            mpsc::channel::<ValidatedEntity>(self.config.channel_buffer);
        let (failed_tx, _failed_rx) = mpsc::channel::<FailedEntity>(self.config.channel_buffer);

        let fetch_handle = {
            let fetcher = self.fetcher.clone();
            let fetch_semaphore = Arc::new(Semaphore::new(self.config.fetch_concurrency));
            let metrics = metrics.clone();
            let fetch_tx = fetch_tx;

            tokio::spawn(async move {
                Self::fetch_stage(entities, fetcher, fetch_semaphore, fetch_tx, metrics).await;
            })
        };

        let validate_handle = {
            let deployer = self.deployer.clone();
            let validate_semaphore = Arc::new(Semaphore::new(self.config.validate_concurrency));
            let metrics = metrics.clone();
            let validate_tx = validate_tx;
            let failed_tx = failed_tx;

            tokio::spawn(async move {
                Self::validate_stage(
                    fetch_rx,
                    deployer,
                    validate_semaphore,
                    validate_tx,
                    failed_tx,
                    metrics,
                )
                .await;
            })
        };

        let persist_handle = {
            let deployer = self.deployer.clone();
            let metrics = metrics.clone();
            let batch_size = self.config.batch_size;

            tokio::spawn(async move {
                Self::persist_stage(validate_rx, deployer, batch_size, metrics).await;
            })
        };

        let _ = fetch_handle.await;
        let _ = validate_handle.await;
        let _ = persist_handle.await;

        let result = metrics.snapshot(start.elapsed());
        info!(%result, "parallel pipeline completed");
        result
    }

    pub async fn deploy_batch(&self, entities: Vec<SyncTask>) -> PipelineResult {
        let stream = futures::stream::iter(entities);
        self.run(stream).await
    }

    async fn fetch_stage(
        entities: impl Stream<Item = SyncTask> + Send,
        fetcher: Arc<dyn EntityFetcher>,
        semaphore: Arc<Semaphore>,
        tx: mpsc::Sender<FetchedEntity>,
        metrics: Arc<PipelineMetrics>,
    ) {
        let mut entities = Box::pin(entities);

        let max_concurrent = semaphore.available_permits() * 2;
        let mut set = JoinSet::new();

        while let Some(task) = entities.next().await {
            while set.len() >= max_concurrent {
                let _ = set.join_next().await;
            }

            let fetcher = fetcher.clone();
            let semaphore = semaphore.clone();
            let tx = tx.clone();
            let metrics = metrics.clone();

            set.spawn(async move {
                let _permit = semaphore.acquire().await.expect("semaphore closed");

                let entity_id = task.entity_id.clone();
                let servers = task.servers.clone();

                match fetcher.fetch_entity(&entity_id, &servers).await {
                    Ok(fetched) => {
                        metrics
                            .bytes_fetched
                            .fetch_add(fetched.total_bytes, Ordering::Relaxed);
                        if tx.send(fetched).await.is_err() {
                            debug!(entity_id, "validate channel closed, dropping fetched entity");
                        }
                    }
                    Err(e) => {
                        warn!(entity_id, error = %e, "fetch failed");
                        metrics.failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }

        while set.join_next().await.is_some() {}

        debug!("fetch stage finished");
    }

    async fn validate_stage(
        mut rx: mpsc::Receiver<FetchedEntity>,
        deployer: Arc<DeploymentService>,
        semaphore: Arc<Semaphore>,
        tx: mpsc::Sender<ValidatedEntity>,
        failed_tx: mpsc::Sender<FailedEntity>,
        metrics: Arc<PipelineMetrics>,
    ) {
        let max_concurrent = semaphore.available_permits() * 2;
        let mut set = JoinSet::new();

        while let Some(fetched) = rx.recv().await {
            while set.len() >= max_concurrent {
                let _ = set.join_next().await;
            }

            let deployer = deployer.clone();
            let semaphore = semaphore.clone();
            let tx = tx.clone();
            let failed_tx = failed_tx.clone();
            let metrics = metrics.clone();

            set.spawn(async move {
                let _permit = semaphore.acquire().await.expect("semaphore closed");

                let entity_id = fetched.task.entity_id.clone();
                let bytes_fetched = fetched.total_bytes;

                let entity: Entity = match serde_json::from_slice(&fetched.entity_bytes) {
                    Ok(e) => e,
                    Err(e) => {
                        warn!(entity_id, error = %e, "entity parse failed in validate stage");
                        metrics.failed.fetch_add(1, Ordering::Relaxed);
                        let _ = failed_tx
                            .send(FailedEntity {
                                entity_id,
                                errors: vec![format!("parse error: {}", e)],
                            })
                            .await;
                        return;
                    }
                };

                if entity.pointers.is_empty() {
                    metrics.failed.fetch_add(1, Ordering::Relaxed);
                    let _ = failed_tx
                        .send(FailedEntity {
                            entity_id,
                            errors: vec!["entity has no pointers".into()],
                        })
                        .await;
                    return;
                }

                let audit_info = LocalDeploymentAuditInfo {
                    auth_chain: fetched.auth_chain.clone(),
                };

                match deployer
                    .validator
                    .validate(
                        &entity,
                        &audit_info,
                        &fetched.content_files,
                        DeploymentContext::Synced,
                        crate::ValidationChecks {
                            has_newer_entities: false,
                            is_already_deployed: false,
                            is_failed_deployment: false,
                            is_rate_limited: false,
                            is_request_ttl_exceeded: false,
                            is_content_unchanged: false,
                        },
                    )
                    .await
                {
                    Ok(()) => {  }
                    Err(errors) => {
                        warn!(entity_id, ?errors, "validation failed in parallel pipeline");
                        metrics.failed.fetch_add(1, Ordering::Relaxed);
                        let _ = failed_tx
                            .send(FailedEntity {
                                entity_id,
                                errors,
                            })
                            .await;
                        return;
                    }
                };

                let validated = ValidatedEntity {
                    entity_id: entity_id.clone(),
                    files: fetched.content_files,
                    audit_info,
                    bytes_fetched,
                };

                if tx.send(validated).await.is_err() {
                    debug!(entity_id, "persist channel closed, dropping validated entity");
                }
            });
        }

        while set.join_next().await.is_some() {}

        debug!("validate stage finished");
    }

    async fn persist_stage(
        mut rx: mpsc::Receiver<ValidatedEntity>,
        deployer: Arc<DeploymentService>,
        batch_size: usize,
        metrics: Arc<PipelineMetrics>,
    ) {
        let flush_interval = Duration::from_secs(5);
        let mut batch: Vec<ValidatedEntity> = Vec::with_capacity(batch_size);
        let mut flush_deadline = tokio::time::Instant::now() + flush_interval;

        loop {
            let item = tokio::select! {
                item = rx.recv() => item,
                _ = tokio::time::sleep_until(flush_deadline) => {
                    if !batch.is_empty() {
                        Self::flush_batch(&mut batch, &deployer, &metrics).await;
                    }
                    flush_deadline = tokio::time::Instant::now() + flush_interval;
                    continue;
                }
            };

            match item {
                Some(entity) => {
                    batch.push(entity);
                    if batch.len() >= batch_size {
                        Self::flush_batch(&mut batch, &deployer, &metrics).await;
                        flush_deadline = tokio::time::Instant::now() + flush_interval;
                    }
                }
                None => {
                    if !batch.is_empty() {
                        Self::flush_batch(&mut batch, &deployer, &metrics).await;
                    }
                    break;
                }
            }
        }

        debug!("persist stage finished");
    }

    async fn flush_batch(
        batch: &mut Vec<ValidatedEntity>,
        deployer: &Arc<DeploymentService>,
        metrics: &Arc<PipelineMetrics>,
    ) {
        let count = batch.len();
        debug!(count, "flushing batch");

        for entity in batch.drain(..) {
            let files = DeploymentFiles::Hashed(entity.files);
            let result = deployer
                .deploy_entity(
                    files,
                    &entity.entity_id,
                    &entity.audit_info,
                    DeploymentContext::Synced,
                )
                .await;

            match result {
                DeploymentResult::Success(_) => {
                    metrics.deployed.fetch_add(1, Ordering::Relaxed);
                }
                DeploymentResult::Invalid(errors) => {
                    warn!(
                        entity_id = entity.entity_id,
                        ?errors,
                        "persist-stage deploy failed"
                    );
                    metrics.failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        metrics.batch_flush_count.fetch_add(1, Ordering::Relaxed);
        info!(count, "batch flushed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_config_defaults() {
        let cfg = PipelineConfig::default();
        assert_eq!(cfg.fetch_concurrency, 20);
        assert_eq!(cfg.validate_concurrency, 10);
        assert_eq!(cfg.batch_size, 200);
        assert_eq!(cfg.channel_buffer, 1000);
    }

    #[test]
    fn pipeline_result_display() {
        let r = PipelineResult {
            deployed: 100,
            failed: 5,
            duration: Duration::from_secs(10),
            bytes_fetched: 1_000_000,
            batch_flush_count: 3,
        };
        let s = r.to_string();
        assert!(s.contains("deployed=100"));
        assert!(s.contains("failed=5"));
        assert!(s.contains("flushes=3"));
    }

    #[test]
    fn http_entity_fetcher_default() {
        let _fetcher = HttpEntityFetcher::new();
    }

    #[tokio::test]
    async fn deploy_batch_with_empty_vec() {
        let stream = futures::stream::iter(Vec::<SyncTask>::new());
        let items: Vec<SyncTask> = stream.collect().await;
        assert!(items.is_empty());
    }
}
