use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{error, info, warn};

use super::backends::{LiveDeploymentRepository, LiveProcessedSnapshotStore};
use super::batch_deployer::BatchDeployer;
use super::pointer_changes::{self, PointerChangesOptions};
use super::snapshots;
use super::{SyncError, SyncState, TimeRange, Timestamp};

#[derive(Debug, Clone)]
pub struct SyncOrchestratorConfig {
    pub from_timestamp: Timestamp,
    pub request_max_retries: u32,
    pub request_retry_wait_ms: u64,
    pub delete_snapshots_after_use: bool,
    pub pointer_changes_wait_time_ms: u64,
    pub bootstrap_reconnect_time_ms: u64,
    pub bootstrap_reconnect_exponent: f64,
    pub bootstrap_max_reconnect_ms: u64,
    pub syncing_reconnect_time_ms: u64,
    pub syncing_reconnect_exponent: f64,
    pub syncing_max_reconnect_ms: u64,
    pub re_snapshot_interval_ms: u64,
    pub phased_sync: bool,
}

impl Default for SyncOrchestratorConfig {
    fn default() -> Self {
        Self {
            from_timestamp: 0,
            request_max_retries: 10,
            request_retry_wait_ms: 1000,
            delete_snapshots_after_use: true,
            pointer_changes_wait_time_ms: 30_000,
            bootstrap_reconnect_time_ms: 5_000,
            bootstrap_reconnect_exponent: 1.5,
            bootstrap_max_reconnect_ms: 3_600_000,
            syncing_reconnect_time_ms: 5_000,
            syncing_reconnect_exponent: 1.1,
            syncing_max_reconnect_ms: 86_400_000,
            re_snapshot_interval_ms: 86_400_000 * 14,
            phased_sync: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerPhase {
    BootstrappingSnapshots,
    BootstrappingPointerChanges,
    Syncing,
}

struct ServerState {
    phase: ServerPhase,
    last_snapshot_timestamp: Timestamp,
    sync_task: Option<tokio::task::JoinHandle<()>>,
}

pub struct SyncOrchestrator {
    config: SyncOrchestratorConfig,
    http_client: reqwest::Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
    deployer: Arc<BatchDeployer>,
    processed_store: Arc<LiveProcessedSnapshotStore>,
    snapshot_store: Arc<catalyrst_storage::SnapshotStorage>,
    deployment_repo: Arc<LiveDeploymentRepository>,

    servers: Arc<Mutex<HashMap<String, ServerState>>>,
    state: Arc<RwLock<SyncState>>,
    bootstrap_done: Arc<Notify>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
    stop_notify: Arc<Notify>,
    bootstrap_handle: Arc<Mutex<Option<tokio::task::AbortHandle>>>,

    paused: Arc<std::sync::atomic::AtomicBool>,

    control_notify: Arc<Notify>,
}

const POINTER_CHANGES_SHIFT_MS: Timestamp = 20 * 60_000;

#[derive(Clone)]
pub struct SyncControlHandle {
    paused: Arc<std::sync::atomic::AtomicBool>,
    control_notify: Arc<Notify>,
}

impl SyncControlHandle {
    pub fn pause(&self) {
        self.paused.store(true, std::sync::atomic::Ordering::SeqCst);

        self.control_notify.notify_waiters();
    }

    pub fn resume(&self) {
        self.paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.control_notify.notify_waiters();
    }

    pub fn force(&self) {
        self.paused
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.control_notify.notify_waiters();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl SyncOrchestrator {
    pub fn new(
        config: SyncOrchestratorConfig,
        http_client: reqwest::Client,
        storage: Arc<catalyrst_storage::ContentStorage>,
        deployer: Arc<BatchDeployer>,
        processed_store: Arc<LiveProcessedSnapshotStore>,
        snapshot_store: Arc<catalyrst_storage::SnapshotStorage>,
        deployment_repo: Arc<LiveDeploymentRepository>,
    ) -> Self {
        SyncOrchestrator {
            config,
            http_client,
            storage,
            deployer,
            processed_store,
            snapshot_store,
            deployment_repo,
            servers: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(RwLock::new(SyncState::Bootstrapping)),
            bootstrap_done: Arc::new(Notify::new()),
            stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            bootstrap_handle: Arc::new(Mutex::new(None)),
            paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            control_notify: Arc::new(Notify::new()),
        }
    }

    pub fn control_handle(&self) -> SyncControlHandle {
        SyncControlHandle {
            paused: self.paused.clone(),
            control_notify: self.control_notify.clone(),
        }
    }

    pub async fn sync_with_servers(
        &self,
        peer_servers: HashSet<String>,
    ) -> Result<SyncHandle, SyncError> {
        if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SyncError::Stopped);
        }

        let mut servers = self.servers.lock().await;

        for url in &peer_servers {
            if !servers.contains_key(url) {
                info!(server = %url, "Adding new server to sync");
                servers.insert(
                    url.clone(),
                    ServerState {
                        phase: ServerPhase::BootstrappingSnapshots,
                        last_snapshot_timestamp: self.config.from_timestamp,
                        sync_task: None,
                    },
                );
            }
        }

        servers.retain(|url, state| {
            if peer_servers.contains(url) {
                true
            } else {
                info!(server = %url, "Removing server from sync");
                if let Some(handle) = state.sync_task.take() {
                    handle.abort();
                }
                false
            }
        });

        drop(servers);

        {
            let mut prev = self.bootstrap_handle.lock().await;
            if let Some(handle) = prev.take() {
                info!("Aborting previous bootstrap task before starting new one");
                handle.abort();
            }
        }

        let bootstrap_done = self.bootstrap_done.clone();
        let orchestrator = self.clone_refs();

        let handle = tokio::spawn(async move {
            if let Err(e) = orchestrator.run_bootstrap().await {
                error!(error = %e, "Bootstrap failed");
            }
        });

        {
            let mut prev = self.bootstrap_handle.lock().await;
            *prev = Some(handle.abort_handle());
        }

        Ok(SyncHandle {
            bootstrap_done,
            _task: handle,
        })
    }

    pub async fn stop(&self) {
        info!("Stopping sync orchestrator");
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.stop_notify.notify_waiters();

        {
            let mut bh = self.bootstrap_handle.lock().await;
            if let Some(handle) = bh.take() {
                handle.abort();
                info!("Aborted bootstrap task");
            }
        }

        let mut servers = self.servers.lock().await;
        for (url, state) in servers.iter_mut() {
            if let Some(handle) = state.sync_task.take() {
                handle.abort();
                info!(server = %url, "Aborted sync task");
            }
        }
    }

    pub async fn state(&self) -> SyncState {
        self.state.read().await.clone()
    }

    pub fn state_handle(&self) -> Arc<RwLock<SyncState>> {
        self.state.clone()
    }

    fn clone_refs(&self) -> SyncOrchestratorRefs {
        SyncOrchestratorRefs {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            storage: self.storage.clone(),
            deployer: self.deployer.clone(),
            processed_store: self.processed_store.clone(),
            snapshot_store: self.snapshot_store.clone(),
            deployment_repo: self.deployment_repo.clone(),
            servers: self.servers.clone(),
            state: self.state.clone(),
            bootstrap_done: self.bootstrap_done.clone(),
            stopped: self.stopped.clone(),
            stop_notify: self.stop_notify.clone(),
            paused: self.paused.clone(),
            control_notify: self.control_notify.clone(),
        }
    }
}

struct SyncOrchestratorRefs {
    config: SyncOrchestratorConfig,
    http_client: reqwest::Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
    deployer: Arc<BatchDeployer>,
    processed_store: Arc<LiveProcessedSnapshotStore>,
    snapshot_store: Arc<catalyrst_storage::SnapshotStorage>,
    deployment_repo: Arc<LiveDeploymentRepository>,
    servers: Arc<Mutex<HashMap<String, ServerState>>>,
    state: Arc<RwLock<SyncState>>,
    bootstrap_done: Arc<Notify>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
    stop_notify: Arc<Notify>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    control_notify: Arc<Notify>,
}

impl SyncOrchestratorRefs {
    async fn wait_while_paused(&self) -> bool {
        loop {
            if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
                return true;
            }
            if !self.paused.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }

            let notified = self.control_notify.notified();
            if !self.paused.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            tokio::select! {
                _ = self.stop_notify.notified() => return true,
                _ = notified => {}
            }
        }
    }

    async fn run_bootstrap(&self) -> Result<(), SyncError> {
        if self.config.phased_sync {
            self.run_phased_bootstrap().await
        } else {
            self.run_full_bootstrap().await
        }
    }

    async fn run_full_bootstrap(&self) -> Result<(), SyncError> {
        let frontier = self.deployment_repo.get_sync_frontier().await?;
        if frontier > 0 {
            info!(frontier, "Resuming sync from persisted frontier");
            let mut servers = self.servers.lock().await;
            for state in servers.values_mut() {
                state.last_snapshot_timestamp = state.last_snapshot_timestamp.max(frontier);
            }
        }

        info!("Phase 1: Bootstrap from snapshots");
        self.bootstrap_from_snapshots(None, true).await?;

        info!("Phase 2: Bootstrap from pointer-changes");
        self.bootstrap_from_pointer_changes(None).await?;
        self.save_frontier().await;

        info!("Resolving deleter_deployment for overwritten entities");
        self.resolve_deleters().await;

        info!("Bootstrap complete, entering steady-state sync");
        *self.state.write().await = SyncState::Syncing;
        self.bootstrap_done.notify_waiters();

        self.start_steady_state_sync().await?;
        Ok(())
    }

    async fn run_phased_bootstrap(&self) -> Result<(), SyncError> {
        let frontier = self.deployment_repo.get_sync_frontier().await?;
        let resuming = frontier > 0;
        if resuming {
            info!(frontier, "Resuming sync from persisted frontier");
            let mut servers = self.servers.lock().await;
            for state in servers.values_mut() {
                state.last_snapshot_timestamp = state.last_snapshot_timestamp.max(frontier);
            }
        }

        let non_profile_filter: HashSet<String> = super::NON_PROFILE_TYPES
            .iter()
            .map(|s| s.to_string())
            .collect();
        let profile_filter: HashSet<String> = ["profile".to_string()].into_iter().collect();

        info!(types = ?non_profile_filter, "Phase 1: Bootstrap non-profile entities from snapshots");
        self.bootstrap_from_snapshots(Some(&non_profile_filter), false)
            .await?;

        info!("Phase 2: Non-profile pointer-changes catch-up");
        self.bootstrap_from_pointer_changes(Some(&non_profile_filter))
            .await?;
        self.save_frontier().await;

        info!("Phase 3: Partially synced — non-profile types ready, starting to serve queries");
        {
            *self.state.write().await = SyncState::PartiallySynced {
                ready_types: non_profile_filter.clone(),
            };
        }
        self.bootstrap_done.notify_waiters();

        {
            let mut servers = self.servers.lock().await;
            for state in servers.values_mut() {
                state.phase = ServerPhase::BootstrappingSnapshots;
            }
        }

        info!("Phase 4: Bootstrap profiles from snapshots");
        self.bootstrap_from_snapshots(Some(&profile_filter), true)
            .await?;

        info!("Phase 5: Profile pointer-changes catch-up");
        self.bootstrap_from_pointer_changes(Some(&profile_filter))
            .await?;
        self.save_frontier().await;

        info!("Phase 6: Resolving deleter_deployment for overwritten entities");
        self.resolve_deleters().await;

        info!("Bootstrap complete, entering steady-state sync");
        *self.state.write().await = SyncState::Syncing;

        self.start_steady_state_sync().await?;
        Ok(())
    }

    async fn resolve_deleters(&self) {
        if let Err(e) = self.deployment_repo.resolve_deleter_deployments().await {
            warn!(error = %e, "Failed to resolve deleter_deployment");
        }
    }

    async fn save_frontier(&self) {
        let ts = {
            let servers = self.servers.lock().await;
            servers
                .values()
                .map(|s| s.last_snapshot_timestamp)
                .max()
                .unwrap_or(0)
        };
        if ts > 0 {
            if let Err(e) = self.deployment_repo.set_sync_frontier(ts).await {
                warn!(error = %e, "Failed to persist sync frontier");
            } else {
                info!(frontier = ts, "Sync frontier persisted");
            }
        }
        let _ = self
            .deployment_repo
            .set_sync_heartbeat(chrono::Utc::now().timestamp_millis())
            .await;
    }

    async fn bootstrap_from_snapshots(
        &self,
        entity_type_filter: Option<&HashSet<String>>,
        mark_processed: bool,
    ) -> Result<(), SyncError> {
        let bootstrapping: Vec<String> = {
            let servers = self.servers.lock().await;
            servers
                .iter()
                .filter(|(_, s)| s.phase == ServerPhase::BootstrappingSnapshots)
                .map(|(url, _)| url.clone())
                .collect()
        };

        if bootstrapping.is_empty() {
            return Ok(());
        }

        if self.wait_while_paused().await {
            return Err(SyncError::Stopped);
        }

        info!(
            servers = ?bootstrapping,
            has_type_filter = entity_type_filter.is_some(),
            "Bootstrapping from snapshots"
        );

        let mut snapshots_by_hash: HashMap<String, (super::SnapshotMetadata, HashSet<String>)> =
            HashMap::new();
        let mut last_ts_by_server: HashMap<String, Timestamp> = HashMap::new();

        for server in &bootstrapping {
            match snapshots::fetch_snapshots(
                &self.http_client,
                server,
                self.config.request_max_retries,
            )
            .await
            {
                Ok(snaps) => {
                    if let Some(max_ts) = snaps.iter().map(|s| s.time_range.end_timestamp).max() {
                        last_ts_by_server.insert(server.clone(), max_ts);
                    }
                    for snap in snaps {
                        let entry = snapshots_by_hash
                            .entry(snap.hash.clone())
                            .or_insert_with(|| (snap.clone(), HashSet::new()));
                        entry.1.insert(server.clone());
                    }
                }
                Err(e) => {
                    warn!(server = %server, error = %e, "Failed to fetch snapshots");
                }
            }
        }

        let mut time_ranges_to_deploy: Vec<TimeRange> = Vec::new();
        let mut snapshots_to_process: Vec<(String, HashSet<String>)> = Vec::new();

        for (hash, (metadata, servers)) in &snapshots_by_hash {
            let replaced_groups: Vec<Vec<String>> = metadata
                .replaced_snapshot_hashes
                .as_ref()
                .map(|v| vec![v.clone()])
                .unwrap_or_default();

            match snapshots::should_deploy_snapshot(
                self.processed_store.as_ref(),
                self.snapshot_store.as_ref(),
                self.config.from_timestamp,
                hash,
                metadata.time_range.end_timestamp,
                &replaced_groups,
            )
            .await
            {
                Ok(true) => {
                    time_ranges_to_deploy.push(metadata.time_range);
                    snapshots_to_process.push((hash.clone(), servers.clone()));
                }
                Ok(false) => {}
                Err(e) => {
                    warn!(hash, error = %e, "Error checking snapshot");
                }
            }
        }

        if !snapshots_to_process.is_empty() {
            let mut need_download = Vec::new();
            for item in &snapshots_to_process {
                if !self.storage.exist(&item.0).await.unwrap_or(true) {
                    need_download.push(item.clone());
                }
            }
            if !need_download.is_empty() {
                info!(
                    count = need_download.len(),
                    "Pre-downloading snapshot files in parallel"
                );
                snapshots::download_snapshot_files(
                    &self.http_client,
                    self.storage.clone(),
                    &need_download,
                    self.config.request_max_retries,
                    self.config.request_retry_wait_ms,
                )
                .await;
            }
        }

        let snapshot_concurrency: usize = std::env::var("SYNC_SNAPSHOT_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(4);

        futures::stream::iter(snapshots_to_process.iter())
            .for_each_concurrent(snapshot_concurrency, |(hash, servers)| async move {
                if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if let Err(e) = snapshots::deploy_entities_from_snapshot(
                    &self.http_client,
                    self.storage.as_ref(),
                    self.deployer.as_ref(),
                    hash,
                    servers,
                    self.config.from_timestamp,
                    self.config.request_max_retries,
                    self.config.request_retry_wait_ms,
                    entity_type_filter,
                    || self.stopped.load(std::sync::atomic::Ordering::SeqCst),
                )
                .await
                {
                    warn!(snapshot_hash = %hash, error = %e, "Snapshot deployment failed");
                }
            })
            .await;

        if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SyncError::Stopped);
        }

        self.deployer.on_idle().await?;

        if mark_processed {
            for (hash, _) in &snapshots_to_process {
                let _ = self.processed_store.mark_processed(hash).await;
            }
        }

        {
            let mut servers = self.servers.lock().await;
            for (url, ts) in &last_ts_by_server {
                if let Some(state) = servers.get_mut(url) {
                    state.last_snapshot_timestamp = state.last_snapshot_timestamp.max(*ts);
                    state.phase = ServerPhase::BootstrappingPointerChanges;
                }
            }
        }

        Ok(())
    }

    async fn bootstrap_from_pointer_changes(
        &self,
        entity_type_filter: Option<&HashSet<String>>,
    ) -> Result<(), SyncError> {
        let bootstrapping: Vec<(String, Timestamp)> = {
            let servers = self.servers.lock().await;
            servers
                .iter()
                .filter(|(_, s)| s.phase == ServerPhase::BootstrappingPointerChanges)
                .map(|(url, s)| {
                    let from = (s.last_snapshot_timestamp - POINTER_CHANGES_SHIFT_MS).max(0);
                    (url.clone(), from)
                })
                .collect()
        };

        if bootstrapping.is_empty() {
            return Ok(());
        }

        if self.wait_while_paused().await {
            return Err(SyncError::Stopped);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let min_from = bootstrapping.iter().map(|(_, ts)| *ts).min().unwrap_or(0);
        self.deployer
            .prepare_for_deployments_in(&[TimeRange {
                init_timestamp: min_from,
                end_timestamp: now,
            }])
            .await?;

        let filter_owned: Option<Arc<HashSet<String>>> =
            entity_type_filter.map(|f| Arc::new(f.clone()));

        let mut handles = Vec::new();
        for (server, from_timestamp) in bootstrapping {
            let client = self.http_client.clone();
            let deployer = self.deployer.clone();
            let servers_map = self.servers.clone();
            let stopped = self.stopped.clone();
            let filter_clone = filter_owned.clone();
            let heartbeat_repo = self.deployment_repo.clone();
            let all_servers: Vec<String> = {
                let s = self.servers.lock().await;
                s.keys().cloned().collect()
            };

            handles.push(tokio::spawn(async move {
                let options = PointerChangesOptions {
                    from_timestamp,
                    wait_time_ms: 0,
                };
                let filter_ref = filter_clone.as_deref();
                match pointer_changes::deploy_entities_from_pointer_changes(
                    &client,
                    &server,
                    &options,
                    deployer.as_ref(),
                    &all_servers,
                    filter_ref,
                    Some(heartbeat_repo.clone()),
                    || stopped.load(std::sync::atomic::Ordering::SeqCst),
                )
                .await
                {
                    Ok(greatest_ts) => {
                        let mut servers = servers_map.lock().await;
                        if let Some(state) = servers.get_mut(&server) {
                            state.last_snapshot_timestamp =
                                state.last_snapshot_timestamp.max(greatest_ts);
                            state.phase = ServerPhase::Syncing;
                        }
                    }
                    Err(e) => {
                        warn!(server = %server, error = %e, "Pointer-changes bootstrap failed");
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        self.deployer.on_idle().await?;
        Ok(())
    }

    async fn start_steady_state_sync(&self) -> Result<(), SyncError> {
        let syncing: Vec<(String, Timestamp)> = {
            let servers = self.servers.lock().await;
            servers
                .iter()
                .filter(|(_, s)| s.phase == ServerPhase::Syncing)
                .map(|(url, s)| (url.clone(), s.last_snapshot_timestamp))
                .collect()
        };

        for (server, from_timestamp) in syncing {
            let server_key = server.clone();
            let client = self.http_client.clone();
            let deployer = self.deployer.clone();
            let stopped = self.stopped.clone();
            let stop_notify = self.stop_notify.clone();
            let paused = self.paused.clone();
            let control_notify = self.control_notify.clone();
            let wait_time_ms = self.config.pointer_changes_wait_time_ms;
            let reconnect_time = self.config.syncing_reconnect_time_ms;
            let reconnect_exponent = self.config.syncing_reconnect_exponent;
            let max_reconnect = self.config.syncing_max_reconnect_ms;
            let all_servers: Vec<String> = {
                let s = self.servers.lock().await;
                s.keys().cloned().collect()
            };

            let deploy_repo = self.deployment_repo.clone();

            let handle = tokio::spawn(async move {
                let mut backoff_ms = reconnect_time as f64;
                let mut from_timestamp = from_timestamp;
                loop {
                    if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }

                    if paused.load(std::sync::atomic::Ordering::SeqCst) {
                        loop {
                            if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                                return;
                            }
                            if !paused.load(std::sync::atomic::Ordering::SeqCst) {
                                break;
                            }
                            let notified = control_notify.notified();
                            if !paused.load(std::sync::atomic::Ordering::SeqCst) {
                                break;
                            }
                            tokio::select! {
                                _ = stop_notify.notified() => return,
                                _ = notified => {}
                            }
                        }
                    }
                    let options = PointerChangesOptions {
                        from_timestamp,
                        wait_time_ms,
                    };
                    match pointer_changes::deploy_entities_from_pointer_changes(
                        &client,
                        &server,
                        &options,
                        deployer.as_ref(),
                        &all_servers,
                        None,
                        Some(deploy_repo.clone()),
                        || stopped.load(std::sync::atomic::Ordering::SeqCst),
                    )
                    .await
                    {
                        Ok(greatest_ts) => {
                            if greatest_ts > from_timestamp {
                                from_timestamp = greatest_ts;
                                let _ = deploy_repo.set_sync_frontier(from_timestamp).await;
                            }
                            backoff_ms = reconnect_time as f64;
                        }
                        Err(e) => {
                            error!(server = %server, error = %e, "Sync stream failed");
                            backoff_ms *= reconnect_exponent;
                            backoff_ms = backoff_ms.min(max_reconnect as f64);
                        }
                    }
                    tokio::select! {
                        _ = stop_notify.notified() => return,

                        _ = control_notify.notified() => {}
                        _ = tokio::time::sleep(std::time::Duration::from_millis(backoff_ms as u64)) => {}
                    }
                }
            });

            let mut servers = self.servers.lock().await;
            if let Some(state) = servers.get_mut(&server_key) {
                state.sync_task = Some(handle);
            }
        }

        Ok(())
    }
}

pub struct SyncHandle {
    bootstrap_done: Arc<Notify>,
    _task: tokio::task::JoinHandle<()>,
}

impl SyncHandle {
    pub async fn wait_for_bootstrap(&self) {
        self.bootstrap_done.notified().await;
    }
}
