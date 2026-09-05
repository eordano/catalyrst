use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{error, info, warn};

use super::backends::{LiveDeploymentRepository, LiveProcessedSnapshotStore};
use super::batch_deployer::{BatchDeployer, DeploymentReport};
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
    // Global entity-type allowlist for everything this node syncs (None = all types). Every
    // bootstrap slice is intersected with it and the steady-state streams honour it too, so
    // an excluded type never lands. Snapshots are marked processed once the ALLOWED types
    // are proven deployed -- widening the allowlist later will not backfill the types a
    // marked snapshot skipped; that needs a fresh sync DB.
    pub entity_types: Option<HashSet<String>>,
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
            entity_types: None,
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

    // Snapshots whose deployment was incomplete in some earlier pass (deploy error, unreadable
    // lines, or unacknowledged entities). A hash in here must never be marked processed by a
    // filtered pass -- only a clean unfiltered pass proves the gap closed -- and its servers stay
    // in snapshot bootstrap until then.
    failed_snapshot_hashes: Arc<std::sync::Mutex<HashSet<String>>>,
    // Snapshots cleanly and fully deployed by a non-marking pass (phased sync deploys
    // non-profile types first without marking). A filtered marking pass may only retire a
    // snapshot that is in here: marking one it never saw deployed for the other types would
    // silently skip those entities forever.
    unmarked_clean_snapshots: Arc<std::sync::Mutex<HashSet<String>>>,
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
            failed_snapshot_hashes: Arc::new(std::sync::Mutex::new(HashSet::new())),
            unmarked_clean_snapshots: Arc::new(std::sync::Mutex::new(HashSet::new())),
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
            failed_snapshot_hashes: self.failed_snapshot_hashes.clone(),
            unmarked_clean_snapshots: self.unmarked_clean_snapshots.clone(),
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
    failed_snapshot_hashes: Arc<std::sync::Mutex<HashSet<String>>>,
    unmarked_clean_snapshots: Arc<std::sync::Mutex<HashSet<String>>>,
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

    /// Sleeps `ms`, returning true if the orchestrator stopped meanwhile.
    async fn sleep_or_stop(&self, ms: u64) -> bool {
        if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return true;
        }
        tokio::select! {
            _ = self.stop_notify.notified() => true,
            _ = tokio::time::sleep(std::time::Duration::from_millis(ms)) => {
                self.stopped.load(std::sync::atomic::Ordering::SeqCst)
            }
        }
    }

    async fn run_bootstrap(&self) -> Result<(), SyncError> {
        // A failed bootstrap is retried with exponential backoff rather than abandoned: every
        // step is idempotent (processed snapshots are skipped, deployed entities are deduped,
        // per-server phases persist across attempts), so a retry resumes where the failed
        // attempt left off. Without this, holding failed servers back would just trade silent
        // data loss for a permanent silent stall.
        let mut backoff_ms = self.config.bootstrap_reconnect_time_ms.max(1) as f64;
        loop {
            // Phasing exists to serve the non-profile types before the profile bulk; an
            // allowlist that drops either side leaves nothing to phase, so it degenerates to
            // one full (allowlist-filtered) pass.
            let phased = self.config.phased_sync
                && self.config.entity_types.as_ref().is_none_or(|allow| {
                    allow.contains("profile")
                        && super::NON_PROFILE_TYPES.iter().any(|t| allow.contains(*t))
                });
            let result = if phased {
                self.run_phased_bootstrap().await
            } else {
                self.run_full_bootstrap().await
            };
            match result {
                Ok(()) => break,
                Err(SyncError::Stopped) => return Err(SyncError::Stopped),
                Err(e) => {
                    warn!(error = %e, backoff_ms, "Bootstrap failed; retrying");
                    if self.sleep_or_stop(backoff_ms as u64).await {
                        return Err(SyncError::Stopped);
                    }
                    backoff_ms = (backoff_ms * self.config.bootstrap_reconnect_exponent)
                        .min(self.config.bootstrap_max_reconnect_ms as f64);
                }
            }
        }

        // Servers whose snapshots or pointer-changes could not be fully deployed were held
        // back (their timestamps unadvanced, their phase unpromoted) so the sync frontier
        // cannot pass their undeployed entities. Healthy servers are already steady-syncing;
        // keep re-bootstrapping the held-back ones until they complete too.
        self.retry_bootstrap_stragglers().await
    }

    /// Re-runs the bootstrap phases for servers that did not make it to the syncing state,
    /// with exponential backoff, until none remain (or the orchestrator stops). Runs with
    /// only the node's global allowlist (no slice filter) on purpose: a straggler may have
    /// been held back by either phased slice, and only a pass covering every synced type
    /// proves its snapshots complete so they can be marked processed.
    async fn retry_bootstrap_stragglers(&self) -> Result<(), SyncError> {
        let mut backoff_ms = self.config.bootstrap_reconnect_time_ms.max(1) as f64;
        loop {
            let pending: Vec<String> = {
                let servers = self.servers.lock().await;
                servers
                    .iter()
                    .filter(|(_, s)| s.phase != ServerPhase::Syncing)
                    .map(|(url, _)| url.clone())
                    .collect()
            };
            if pending.is_empty() {
                // Every server reached the syncing state, so no snapshot can still be pending a
                // proof: drop the cross-pass bookkeeping instead of letting it grow for the
                // process lifetime. (Retired snapshots are already pruned per-verdict; this
                // clears hashes for snapshots that stopped being advertised, or whose servers
                // were removed, mid-bootstrap.) A later sync_with_servers repopulates both sets
                // from scratch as its passes run.
                self.failed_snapshot_hashes.lock().unwrap().clear();
                self.unmarked_clean_snapshots.lock().unwrap().clear();
                return Ok(());
            }
            info!(
                servers = ?pending,
                backoff_ms,
                "Servers held back from sync; retrying their bootstrap"
            );
            if self.sleep_or_stop(backoff_ms as u64).await {
                return Err(SyncError::Stopped);
            }

            match self
                .bootstrap_from_snapshots(self.config.entity_types.as_ref(), true)
                .await
            {
                Ok(()) => {}
                Err(SyncError::Stopped) => return Err(SyncError::Stopped),
                Err(e) => warn!(error = %e, "Snapshot bootstrap retry failed"),
            }
            match self
                .bootstrap_from_pointer_changes(self.config.entity_types.as_ref())
                .await
            {
                Ok(()) => {}
                Err(SyncError::Stopped) => return Err(SyncError::Stopped),
                Err(e) => warn!(error = %e, "Pointer-changes bootstrap retry failed"),
            }

            let still_pending = {
                let servers = self.servers.lock().await;
                servers
                    .values()
                    .filter(|s| s.phase != ServerPhase::Syncing)
                    .count()
            };
            if still_pending < pending.len() {
                self.save_frontier().await;
                self.resolve_deleters().await;
                self.start_steady_state_sync().await?;
                backoff_ms = self.config.bootstrap_reconnect_time_ms.max(1) as f64;
            } else {
                backoff_ms = (backoff_ms * self.config.bootstrap_reconnect_exponent)
                    .min(self.config.bootstrap_max_reconnect_ms as f64);
            }
        }
    }

    /// Fast-forwards each server's in-memory resume point from ITS OWN persisted cursor
    /// (`resume_point` is the rule). A server whose cursor lags the global frontier resumes
    /// from the lag -- the frontier is max-over-servers, so riding it would skip the lagging
    /// server's not-yet-deployed entities until the re-snapshot pass.
    async fn resume_servers_from_cursors(&self) -> Result<(), SyncError> {
        let frontier = self.deployment_repo.get_sync_frontier().await?;
        let urls: Vec<String> = {
            let servers = self.servers.lock().await;
            servers.keys().cloned().collect()
        };
        // Cursor lookups happen outside the servers lock; a server removed by a concurrent
        // sync_with_servers meanwhile simply has no entry to fast-forward any more.
        let mut resume_by_url: HashMap<String, (Timestamp, bool)> = HashMap::new();
        for url in urls {
            let cursor = self.deployment_repo.get_server_sync_cursor(&url).await?;
            resume_by_url.insert(url, (resume_point(cursor, frontier), cursor.is_some()));
        }
        let mut servers = self.servers.lock().await;
        for (url, state) in servers.iter_mut() {
            let Some(&(resume, own_cursor)) = resume_by_url.get(url) else {
                continue;
            };
            if resume > 0 {
                info!(
                    server = %url,
                    resume,
                    own_cursor,
                    "Resuming server from persisted cursor"
                );
                state.last_snapshot_timestamp = state.last_snapshot_timestamp.max(resume);
            }
        }
        Ok(())
    }

    async fn run_full_bootstrap(&self) -> Result<(), SyncError> {
        self.resume_servers_from_cursors().await?;

        info!("Phase 1: Bootstrap from snapshots");
        self.bootstrap_from_snapshots(self.config.entity_types.as_ref(), true)
            .await?;

        info!("Phase 2: Bootstrap from pointer-changes");
        self.bootstrap_from_pointer_changes(self.config.entity_types.as_ref())
            .await?;
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
        self.resume_servers_from_cursors().await?;

        // run_bootstrap only phases when the allowlist keeps both slices non-empty, so the
        // intersection below never empties a phase.
        let non_profile_filter: HashSet<String> = super::NON_PROFILE_TYPES
            .iter()
            .filter(|t| {
                self.config
                    .entity_types
                    .as_ref()
                    .is_none_or(|allow| allow.contains(**t))
            })
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

        info!(
            "Phase 3: Partially synced \u{2014} non-profile types ready, starting to serve queries"
        );
        {
            *self.state.write().await = SyncState::PartiallySynced {
                ready_types: non_profile_filter.clone(),
            };
        }
        self.bootstrap_done.notify_waiters();

        {
            let mut servers = self.servers.lock().await;
            for state in servers.values_mut() {
                // Servers with a live steady-state stream are already fully bootstrapped from
                // an earlier attempt; re-running the phases for them would just re-stream
                // content they already have.
                if state.sync_task.is_none() {
                    state.phase = ServerPhase::BootstrappingSnapshots;
                }
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
        // Resume state is per-server (upstream's lastEntityTimestampFromSnapshotsByServer
        // shape, persisted): every durable point -- a confirmed pointer-changes poll boundary,
        // a completed snapshot bootstrap -- advances that server's own GREATEST-monotonic
        // cursor, and bootstrap resumes each server from its own cursor. The invariant: no
        // server's persisted cursor ever passes an entity of THAT server's history that is
        // not durably accounted for, regardless of any other server's progress. The global
        // 'sync_frontier' scalar keeps its max-over-servers writers unchanged -- it feeds the
        // freshness gauge and external consumers, and is the resume fallback only for a
        // server with no cursor of its own. This method records liveness only.
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

        let mut snapshots_by_hash: HashMap<
            String,
            (Vec<super::SnapshotMetadata>, HashSet<String>),
        > = HashMap::new();
        let mut last_ts_by_server: HashMap<String, Timestamp> = HashMap::new();
        // Servers whose snapshot list could not be fully read: each discarded entry stood for a
        // whole time range, so advancing past max(endTimestamp) of what parsed would skip
        // whatever lived in the ranges we threw away (upstream 53e9c07 adds such servers to
        // serversWithFailedSnapshots). What DID parse is still deployed -- that work is real --
        // but the server stays in snapshot bootstrap so the list is fetched again.
        let mut held_from_fetch: HashSet<String> = HashSet::new();

        for server in &bootstrapping {
            match snapshots::fetch_snapshots(
                &self.http_client,
                server,
                self.config.request_max_retries,
            )
            .await
            {
                Ok(fetched) => apply_snapshot_fetch(
                    server,
                    fetched,
                    self.config.from_timestamp,
                    &mut last_ts_by_server,
                    &mut snapshots_by_hash,
                    &mut held_from_fetch,
                ),
                Err(e) => {
                    warn!(server = %server, error = %e, "Failed to fetch snapshots");
                }
            }
        }

        // One chunked processed-snapshots lookup for every candidate hash and replaced hash in
        // the pass, then the pure decision run to a fixed point (decide_snapshot_pass): a
        // replacement chain collapses fully here instead of one link per bootstrap, and sorting
        // the candidates makes the outcome independent of HashMap iteration order. Each
        // candidate carries max(endTimestamp) across ALL advertisers and one replaced-hash
        // group per advertiser, so peers with different replacement histories reach one verdict.
        let mut candidates: Vec<(String, Timestamp, Vec<Vec<String>>)> = snapshots_by_hash
            .iter()
            .map(|(hash, (metas, _))| {
                let greatest_end = metas
                    .iter()
                    .map(|m| m.time_range.end_timestamp)
                    .max()
                    .unwrap_or(self.config.from_timestamp);
                let groups: Vec<Vec<String>> = metas
                    .iter()
                    .map(|m| m.replaced_snapshot_hashes.clone().unwrap_or_default())
                    .collect();
                (hash.clone(), greatest_end, groups)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0));

        let all_hashes: Vec<String> = candidates
            .iter()
            .flat_map(|(hash, _, groups)| {
                std::iter::once(hash.clone()).chain(groups.iter().flatten().cloned())
            })
            .collect();
        let mut processed = self
            .processed_store
            .filter_processed_in_chunks(all_hashes)
            .await?;

        let (deployable, newly_marked) = snapshots::decide_snapshot_pass(
            &candidates,
            &mut processed,
            self.config.from_timestamp,
        );

        for hash in &newly_marked {
            // The decision already treated this hash as processed (that is what let the rest of
            // the chain collapse), so failing to persist the mark must fail the whole pass --
            // a later bootstrap would otherwise re-decide differently.
            self.processed_store.mark_processed(hash).await?;
        }

        let mut time_ranges_to_deploy: Vec<TimeRange> = Vec::new();
        let mut snapshots_to_process: Vec<(String, HashSet<String>)> = Vec::new();

        for hash in deployable {
            // The one async check left out of the pure decision (its result cannot change
            // within a pass): a snapshot already present in the node's own snapshot store is
            // never re-deployed.
            if self.snapshot_store.exist(&hash).await? {
                continue;
            }
            let (metas, servers) = &snapshots_by_hash[&hash];
            time_ranges_to_deploy.extend(metas.iter().map(|m| m.time_range));
            snapshots_to_process.push((hash, servers.clone()));
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

        // One report per snapshot, so an incomplete deployment discovered after the drain can
        // be attributed back to the servers advertising exactly that snapshot -- the counting
        // equivalent of upstream re-reading the processed-snapshot marker once the deployer
        // drained.
        let pass: Vec<(String, HashSet<String>, Arc<DeploymentReport>)> = snapshots_to_process
            .into_iter()
            .map(|(hash, servers)| (hash, servers, Arc::new(DeploymentReport::default())))
            .collect();
        let failed_hashes = std::sync::Mutex::new(HashSet::<String>::new());

        if !time_ranges_to_deploy.is_empty() {
            self.deployer
                .prepare_for_deployments_in(&time_ranges_to_deploy)
                .await?;
        }

        futures::stream::iter(pass.iter())
            .for_each_concurrent(snapshot_concurrency, |(hash, servers, report)| {
                let failed_hashes = &failed_hashes;
                async move {
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
                        report,
                        || self.stopped.load(std::sync::atomic::Ordering::SeqCst),
                    )
                    .await
                    {
                        warn!(snapshot_hash = %hash, error = %e, "Snapshot deployment failed");
                        failed_hashes.lock().unwrap().insert(hash.clone());
                    }
                }
            })
            .await;

        if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SyncError::Stopped);
        }

        // Scheduling only proves handoff. Everything decided below -- marking a snapshot
        // processed, advancing a server's timestamp, promoting its phase -- is only meaningful
        // once the deployer has fully drained.
        self.deployer.on_idle().await?;

        if self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SyncError::Stopped);
        }

        let mut failed_hashes = failed_hashes.into_inner().unwrap();
        // The drain proves the queue emptied; the report proves everything that went into it
        // came back durably accounted for (deployed, or recorded in failed_deployments where
        // the retry loop owns it). A snapshot short of that is treated exactly like one whose
        // deployment failed outright.
        for (hash, _, report) in &pass {
            if !report.is_complete() {
                warn!(
                    snapshot_hash = %hash,
                    scheduled = report.scheduled(),
                    acknowledged = report.acknowledged(),
                    "Snapshot deployments not fully acknowledged after drain; treating the snapshot as failed"
                );
                failed_hashes.insert(hash.clone());
            }
            // Losses are attributed per report (the batch flush carries each entity's report),
            // so only the snapshots that actually lost entities are held back. The straggler
            // retry runs this pass concurrently with live steady-state streams, and the old
            // global-counter comparison let any loss anywhere mark every snapshot here failed --
            // while losses recurred the straggler could never complete.
            if report.lost() > 0 {
                warn!(
                    snapshot_hash = %hash,
                    lost = report.lost(),
                    "Deployer lost entities of this snapshot without a durable record; \
                     treating the snapshot as failed"
                );
                failed_hashes.insert(hash.clone());
            }
        }

        // Decide, under the bookkeeping locks, which snapshots this pass may retire and which
        // servers it must hold back; the awaits happen after the locks are released. The
        // phased-sync rules live in snapshot_pass_verdict, documented on
        // failed_snapshot_hashes / unmarked_clean_snapshots.
        let (held_servers, to_mark): (HashSet<String>, Vec<String>) = {
            let mut failed_global = self.failed_snapshot_hashes.lock().unwrap();
            let mut clean_unmarked = self.unmarked_clean_snapshots.lock().unwrap();
            // Servers with a partially-readable snapshot list are held exactly like servers
            // with a failed snapshot: either way the pass cannot prove their history complete.
            let mut held = held_from_fetch;
            let mut to_mark = Vec::new();
            for (hash, servers, _) in &pass {
                let verdict = snapshot_pass_verdict(
                    failed_hashes.contains(hash),
                    mark_processed,
                    pass_covers_all_synced_types(
                        entity_type_filter,
                        self.config.entity_types.as_ref(),
                    ),
                    failed_global.contains(hash),
                    clean_unmarked.contains(hash),
                );
                match verdict {
                    SnapshotPassVerdict::Failed => {
                        failed_global.insert(hash.clone());
                        clean_unmarked.remove(hash);
                        held.extend(servers.iter().cloned());
                    }
                    SnapshotPassVerdict::CleanUnmarked => {
                        clean_unmarked.insert(hash.clone());
                    }
                    SnapshotPassVerdict::Retire => {
                        failed_global.remove(hash);
                        clean_unmarked.remove(hash);
                        to_mark.push(hash.clone());
                    }
                    SnapshotPassVerdict::Unproven => {
                        held.extend(servers.iter().cloned());
                    }
                }
            }
            (held, to_mark)
        };

        for hash in &to_mark {
            if let Err(e) = self.processed_store.mark_processed(hash).await {
                // The entities are deployed either way; an unmarked snapshot only costs a
                // redundant re-stream on some later pass.
                warn!(snapshot_hash = %hash, error = %e, "Failed to mark snapshot as processed");
            }
        }

        let advanced: Vec<(String, Timestamp)> = {
            let mut servers = self.servers.lock().await;
            let mut advanced = Vec::new();
            for (url, ts) in &last_ts_by_server {
                if held_servers.contains(url) {
                    info!(
                        server = %url,
                        "Keeping the server in snapshot bootstrap: not all of its snapshots \
                         were fully deployed"
                    );
                    continue;
                }
                // A server removed by a concurrent sync_with_servers has no entry any more,
                // so it cannot be resurrected here.
                if let Some(state) = servers.get_mut(url) {
                    state.last_snapshot_timestamp = state.last_snapshot_timestamp.max(*ts);
                    state.phase = ServerPhase::BootstrappingPointerChanges;
                    advanced.push((url.clone(), state.last_snapshot_timestamp));
                }
            }
            advanced
        };

        // Snapshot-bootstrap completion is a durable per-server point exactly like a
        // confirmed pointer-changes poll boundary: everything of this server's history up to
        // the adopted timestamp is deployed or durably accounted for (a held server never
        // reaches here). Persist it as the server's own cursor so a restart resumes from it.
        // Safe under phased sync's non-marking slice too: the cursor only positions
        // pointer-changes, and the not-yet-marked snapshots re-deploy the other types on a
        // restarted bootstrap regardless.
        for (url, ts) in advanced {
            let _ = self
                .deployment_repo
                .advance_server_sync_cursor(&url, ts)
                .await;
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
                let progress = std::sync::atomic::AtomicI64::new(from_timestamp);
                // Per-run accounting: the stream drains the deployer and verifies this report
                // at every poll boundary before committing progress, so an Ok return -- and
                // every value `progress` ever holds -- reflects confirmed deployments only
                // (deployed, or durably recorded in failed_deployments).
                let report = Arc::new(DeploymentReport::default());
                let outcome = pointer_changes::deploy_entities_from_pointer_changes(
                    &client,
                    &server,
                    &options,
                    deployer.as_ref(),
                    &all_servers,
                    filter_ref,
                    Some(heartbeat_repo.clone()),
                    &report,
                    &progress,
                    || stopped.load(std::sync::atomic::Ordering::SeqCst),
                )
                .await;
                let reached = progress.load(std::sync::atomic::Ordering::Relaxed);
                match outcome {
                    Ok(greatest_ts) => {
                        let mut servers = servers_map.lock().await;
                        if let Some(state) = servers.get_mut(&server) {
                            state.last_snapshot_timestamp =
                                state.last_snapshot_timestamp.max(greatest_ts);
                            // A stream cut short by shutdown also returns Ok (with only its
                            // confirmed progress); promoting it would let save_frontier trust
                            // a backlog it never finished, so promote only while live.
                            if !stopped.load(std::sync::atomic::Ordering::SeqCst) {
                                state.phase = ServerPhase::Syncing;
                            }
                        }
                    }
                    Err(e) => {
                        // Confirmed progress is kept -- those entities are durably accounted
                        // for -- but the server stays in pointer-changes bootstrap so the
                        // retry resumes the rest of its backlog.
                        warn!(server = %server, error = %e, "Pointer-changes bootstrap failed");
                        let mut servers = servers_map.lock().await;
                        if let Some(state) = servers.get_mut(&server) {
                            state.last_snapshot_timestamp =
                                state.last_snapshot_timestamp.max(reached);
                        }
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
                // Skip servers that already have a live stream: the straggler retry calls
                // this again as held-back servers complete, and spawning a second stream for
                // an already-syncing server would leave the first running unaborted -- two
                // pollers double-deploying the same deltas.
                .filter(|(_, s)| s.phase == ServerPhase::Syncing && s.sync_task.is_none())
                .map(|(url, s)| (url.clone(), s.last_snapshot_timestamp))
                .collect()
        };

        let entity_types: Option<Arc<HashSet<String>>> =
            self.config.entity_types.clone().map(Arc::new);
        for (server, from_timestamp) in syncing {
            let server_key = server.clone();
            let client = self.http_client.clone();
            let deployer = self.deployer.clone();
            let entity_types = entity_types.clone();
            let servers_map = self.servers.clone();
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
                    let progress = std::sync::atomic::AtomicI64::new(from_timestamp);
                    // Fresh per attempt: the stream verifies (deployer drain plus full
                    // acknowledgment) cumulatively against this report at every poll
                    // boundary before committing progress or persisting the frontier, and
                    // fails the attempt otherwise -- so the cursor and the persisted
                    // frontier never move past an entity that is not durably accounted for.
                    let report = Arc::new(DeploymentReport::default());
                    let outcome = pointer_changes::deploy_entities_from_pointer_changes(
                        &client,
                        &server,
                        &options,
                        deployer.as_ref(),
                        &all_servers,
                        entity_types.as_deref(),
                        Some(deploy_repo.clone()),
                        &report,
                        &progress,
                        || stopped.load(std::sync::atomic::Ordering::SeqCst),
                    )
                    .await;
                    // A long-poll stream ends only by shutdown or failure, so progress made
                    // before an error must advance the cursor too; resetting to the last Ok
                    // re-pulls and re-deploys the whole backlog on every reconnect.
                    let reached = progress.load(std::sync::atomic::Ordering::Relaxed);
                    if reached > from_timestamp {
                        from_timestamp = reached;
                        let _ = deploy_repo.advance_sync_frontier(from_timestamp).await;
                        let _ = deploy_repo
                            .advance_server_sync_cursor(&server, from_timestamp)
                            .await;
                    }
                    match outcome {
                        Ok(greatest_ts) => {
                            if greatest_ts > from_timestamp {
                                from_timestamp = greatest_ts;
                                let _ = deploy_repo.advance_sync_frontier(from_timestamp).await;
                                let _ = deploy_repo
                                    .advance_server_sync_cursor(&server, from_timestamp)
                                    .await;
                            }
                            backoff_ms = reconnect_time as f64;
                        }
                        Err(e) => {
                            error!(server = %server, error = %e, "Sync stream failed");
                            backoff_ms *= reconnect_exponent;
                            backoff_ms = backoff_ms.min(max_reconnect as f64);
                        }
                    }
                    // Mirror the confirmed cursor into the shared server state, so
                    // save_frontier's min-over-syncing floor reads live progress rather than
                    // the timestamp this server finished bootstrap with -- the stale value that
                    // used to be re-offered (harmlessly now, but noisily) hours later when a
                    // straggler completed.
                    {
                        let mut servers = servers_map.lock().await;
                        if let Some(state) = servers.get_mut(&server) {
                            state.last_snapshot_timestamp =
                                state.last_snapshot_timestamp.max(from_timestamp);
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

/// Folds one server's /snapshots fetch result into a bootstrap pass's working state.
///
/// The server's resume point is max(endTimestamp) over the entries that VALIDATED -- recorded
/// unconditionally, because whether it may be adopted is decided later: a server with discarded
/// entries goes into `held_servers` (each discarded entry stood for a time range nothing else
/// covers, so the surviving subset is not that server's complete history), and held servers
/// never have their timestamp or phase advanced. A server with no snapshots at all falls back
/// to the genesis timestamp so it still advances to pointer-changes bootstrap instead of being
/// re-fetched forever; a FAILED fetch leaves no entry anywhere, which holds the server back.
///
/// Every advertiser's metadata for a hash is kept (not just the first server's), so the
/// deployment decision can weigh each advertiser's replaced-hash group and the greatest
/// end-timestamp across all of them.
fn apply_snapshot_fetch(
    server: &str,
    fetched: snapshots::SnapshotsFromServer,
    genesis_timestamp: Timestamp,
    last_ts_by_server: &mut HashMap<String, Timestamp>,
    snapshots_by_hash: &mut HashMap<String, (Vec<super::SnapshotMetadata>, HashSet<String>)>,
    held_servers: &mut HashSet<String>,
) {
    if fetched.discarded > 0 {
        warn!(
            server,
            discarded = fetched.discarded,
            "Snapshot list was not fully readable; keeping the server in snapshot bootstrap"
        );
        held_servers.insert(server.to_string());
    }
    let last_ts = fetched
        .snapshots
        .iter()
        .map(|s| s.time_range.end_timestamp)
        .max()
        .unwrap_or(genesis_timestamp);
    last_ts_by_server.insert(server.to_string(), last_ts);
    for snap in fetched.snapshots {
        let entry = snapshots_by_hash
            .entry(snap.hash.clone())
            .or_insert_with(|| (Vec::new(), HashSet::new()));
        entry.0.push(snap);
        entry.1.insert(server.to_string());
    }
}

/// Where a server's bootstrap resumes from: its OWN persisted cursor when one exists -- even
/// when that lags the global frontier, which is the invariant this function carries -- the
/// global frontier for a server with no cursor row (never synced before, or persisted by a
/// build predating per-server cursors), and zero when neither exists, leaving the caller's
/// in-memory floor (config.from_timestamp) in charge.
pub fn resume_point(own_cursor: Option<Timestamp>, global_frontier: Timestamp) -> Timestamp {
    own_cursor.unwrap_or(global_frontier)
}

/// What one snapshot bootstrap pass concluded about a single snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotPassVerdict {
    /// The pass could not prove the snapshot fully deployed (deploy error, unacknowledged
    /// entities, or lost-entity contamination): record it as failed and hold its advertising
    /// servers in snapshot bootstrap.
    Failed,
    /// Cleanly deployed for this pass's types, deliberately left unmarked (phased sync's
    /// non-marking slice): remember it so a later marking slice can prove the whole snapshot.
    CleanUnmarked,
    /// Provably complete: mark it processed and clear its bookkeeping.
    Retire,
    /// Clean for this slice, but the rest of the snapshot is unproven -- an earlier pass failed
    /// on it, or it never went through the non-marking slice. Marking it would silently skip
    /// the unproven types' entities forever, and advancing its servers would move their
    /// timestamps past those entities with nothing left to re-deliver them. Hold the servers
    /// back so the unfiltered straggler retry deploys the whole snapshot and retires it.
    Unproven,
}

/// Whether a pass filtered to `pass_filter` deploys every entity type this node syncs
/// (`allowlist`; None = all types) -- the condition under which a clean marking pass proves a
/// snapshot complete. Under an allowlist "complete" deliberately means the allowed types
/// only: the excluded types are never wanted, so waiting for them would leave every snapshot
/// unretirable forever.
fn pass_covers_all_synced_types(
    pass_filter: Option<&HashSet<String>>,
    allowlist: Option<&HashSet<String>>,
) -> bool {
    match (pass_filter, allowlist) {
        (None, _) => true,
        (Some(f), Some(allow)) => allow.iter().all(|t| f.contains(t)),
        (Some(_), None) => false,
    }
}

/// Pure decision table for one snapshot at the end of a bootstrap pass. Inputs are: whether
/// this pass failed on the snapshot, whether this pass marks snapshots processed, whether it
/// covered every entity type this node syncs (pass_covers_all_synced_types), and the
/// snapshot's cross-pass bookkeeping (failed in an earlier pass / cleanly deployed by an
/// earlier non-marking pass).
fn snapshot_pass_verdict(
    failed_this_pass: bool,
    mark_processed: bool,
    covers_all_synced_types: bool,
    failed_earlier: bool,
    clean_unmarked_earlier: bool,
) -> SnapshotPassVerdict {
    if failed_this_pass {
        SnapshotPassVerdict::Failed
    } else if !mark_processed {
        SnapshotPassVerdict::CleanUnmarked
    } else if covers_all_synced_types {
        // A clean pass over every synced type proves the whole snapshot deployed (as far as
        // this node is concerned), whatever any earlier pass saw.
        SnapshotPassVerdict::Retire
    } else if !failed_earlier && clean_unmarked_earlier {
        // Filtered marking pass: this slice is clean now, and the complementary slice was
        // cleanly deployed by an earlier non-marking pass.
        SnapshotPassVerdict::Retire
    } else {
        SnapshotPassVerdict::Unproven
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::super::batch_deployer::DeploymentReport;
    use super::super::snapshots::SnapshotsFromServer;
    use super::super::{SnapshotMetadata, TimeRange, Timestamp};
    use super::{
        apply_snapshot_fetch, pass_covers_all_synced_types, resume_point, snapshot_pass_verdict,
        SnapshotPassVerdict,
    };

    fn snapshot(hash: &str, end: Timestamp) -> SnapshotMetadata {
        SnapshotMetadata {
            hash: hash.to_string(),
            time_range: TimeRange {
                init_timestamp: 0,
                end_timestamp: end,
            },
            number_of_entities: 1,
            replaced_snapshot_hashes: None,
            generation_timestamp: end,
        }
    }

    // Upstream 53e9c07: discardedEntries > 0 puts the server in serversWithFailedSnapshots --
    // its surviving entries still deploy, but the server itself must stay in snapshot bootstrap
    // with an unadopted timestamp, because each discarded entry stood for a time range that the
    // surviving max(endTimestamp) would otherwise skip forever.
    #[test]
    fn discarded_snapshot_entries_hold_the_server_back() {
        let mut last_ts = HashMap::new();
        let mut by_hash = HashMap::new();
        let mut held = HashSet::new();
        apply_snapshot_fetch(
            "https://peer.test",
            SnapshotsFromServer {
                snapshots: vec![snapshot("QmClean", 1_700_000_000_000)],
                discarded: 2,
            },
            0,
            &mut last_ts,
            &mut by_hash,
            &mut held,
        );
        assert!(
            held.contains("https://peer.test"),
            "a partially-readable snapshot list must hold the server in bootstrap"
        );
        // The surviving subset is still deployed -- that work is real.
        assert!(by_hash.contains_key("QmClean"));

        // A fully-readable list does not hold the server.
        let mut held_clean = HashSet::new();
        apply_snapshot_fetch(
            "https://ok.test",
            SnapshotsFromServer {
                snapshots: vec![snapshot("QmClean", 1_700_000_000_000)],
                discarded: 0,
            },
            0,
            &mut last_ts,
            &mut by_hash,
            &mut held_clean,
        );
        assert!(held_clean.is_empty());
        assert_eq!(last_ts["https://ok.test"], 1_700_000_000_000);
        // Both advertisers' metadata for the shared hash is kept.
        assert_eq!(by_hash["QmClean"].0.len(), 2);
        assert_eq!(by_hash["QmClean"].1.len(), 2);
    }

    #[test]
    fn empty_snapshot_list_falls_back_to_genesis() {
        let mut last_ts = HashMap::new();
        let mut by_hash = HashMap::new();
        let mut held = HashSet::new();
        apply_snapshot_fetch(
            "https://new.test",
            SnapshotsFromServer {
                snapshots: vec![],
                discarded: 0,
            },
            42,
            &mut last_ts,
            &mut by_hash,
            &mut held,
        );
        assert_eq!(last_ts["https://new.test"], 42);
        assert!(held.is_empty());
    }

    // The upstream fix this table ports (snapshots-fetcher 53e9c07): a snapshot that failed --
    // or could not be proven complete -- must never advance its servers' timestamps or be
    // marked processed, in any kind of pass.
    #[test]
    fn failed_pass_always_holds_regardless_of_history() {
        for mark in [false, true] {
            for unfiltered in [false, true] {
                for failed_earlier in [false, true] {
                    for clean_earlier in [false, true] {
                        assert_eq!(
                            snapshot_pass_verdict(
                                true,
                                mark,
                                unfiltered,
                                failed_earlier,
                                clean_earlier
                            ),
                            SnapshotPassVerdict::Failed
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn non_marking_slice_remembers_clean_snapshots() {
        // Phased sync's first slice never marks; a clean snapshot is remembered so the
        // marking slice can retire it later.
        assert_eq!(
            snapshot_pass_verdict(false, false, false, false, false),
            SnapshotPassVerdict::CleanUnmarked
        );
        // Even one that failed some earlier pass: the record of THAT stays in
        // failed_snapshot_hashes, which the marking decision consults separately.
        assert_eq!(
            snapshot_pass_verdict(false, false, false, true, false),
            SnapshotPassVerdict::CleanUnmarked
        );
    }

    #[test]
    fn clean_unfiltered_pass_retires_even_previous_failures() {
        // An unfiltered pass deploys every entity type, so a clean one is complete proof --
        // it clears an earlier failure rather than deferring to it.
        assert_eq!(
            snapshot_pass_verdict(false, true, true, true, true),
            SnapshotPassVerdict::Retire
        );
        assert_eq!(
            snapshot_pass_verdict(false, true, true, false, false),
            SnapshotPassVerdict::Retire
        );
    }

    #[test]
    fn filtered_marking_pass_needs_the_complementary_slice_proven() {
        // Clean profile slice + clean earlier non-profile slice = whole snapshot proven.
        assert_eq!(
            snapshot_pass_verdict(false, true, false, false, true),
            SnapshotPassVerdict::Retire
        );
        // A snapshot that appeared after the non-marking slice ran: marking it would skip
        // the other types' entities forever, and advancing its servers would skip them too.
        assert_eq!(
            snapshot_pass_verdict(false, true, false, false, false),
            SnapshotPassVerdict::Unproven
        );
        // A snapshot that failed an earlier pass is never retired by a filtered pass, even
        // when both slices have since looked clean individually.
        assert_eq!(
            snapshot_pass_verdict(false, true, false, true, true),
            SnapshotPassVerdict::Unproven
        );
    }

    // Under a global allowlist "complete" means the allowed types only: a pass filtered to
    // exactly (or beyond) the allowlist proves everything this node will ever sync, while
    // any narrower slice does not.
    #[test]
    fn allowlist_defines_what_a_complete_pass_is() {
        let set =
            |types: &[&str]| -> HashSet<String> { types.iter().map(|s| s.to_string()).collect() };
        // No pass filter deploys everything, allowlist or not.
        assert!(pass_covers_all_synced_types(None, None));
        assert!(pass_covers_all_synced_types(None, Some(&set(&["scene"]))));
        // Without an allowlist, any filtered pass is partial by definition.
        assert!(!pass_covers_all_synced_types(Some(&set(&["scene"])), None));
        // Scene-only node: a scene-filtered pass is complete proof...
        assert!(pass_covers_all_synced_types(
            Some(&set(&["scene"])),
            Some(&set(&["scene"]))
        ));
        // ...and so is a broader one (the phased non-profile slice on a scene-only node).
        assert!(pass_covers_all_synced_types(
            Some(&set(&["scene", "wearable"])),
            Some(&set(&["scene"]))
        ));
        // But a slice missing an allowed type proves nothing about that type.
        assert!(!pass_covers_all_synced_types(
            Some(&set(&["profile"])),
            Some(&set(&["scene", "profile"]))
        ));
    }

    // The per-server resume rule. The lag case is the whole point of persisted per-server
    // cursors: the global frontier is max-over-servers, so a server that stalled in bootstrap
    // must resume from its own confirmed cursor, not ride another server's progress past its
    // undeployed entities.
    #[test]
    fn a_server_resumes_from_its_own_cursor_even_when_it_lags_the_global_frontier() {
        assert_eq!(resume_point(Some(1_000), 9_000), 1_000);
        // A cursor ahead of the frontier wins too -- GREATEST-monotonic writes can leave the
        // scalar behind a server's own confirmed progress.
        assert_eq!(resume_point(Some(9_500), 9_000), 9_500);
    }

    #[test]
    fn a_server_with_no_cursor_row_falls_back_to_the_global_frontier() {
        // Never synced before, or persisted by a build predating per-server cursors.
        assert_eq!(resume_point(None, 9_000), 9_000);
    }

    #[test]
    fn with_neither_cursor_nor_frontier_the_config_floor_stays_in_charge() {
        // Zero means "no fast-forward": the caller's in-memory config.from_timestamp holds.
        assert_eq!(resume_point(None, 0), 0);
    }

    // Post-drain completeness: the report is the counting equivalent of upstream re-reading
    // the processed-snapshot marker after deployer.onIdle().
    #[test]
    fn deployment_report_incomplete_until_all_acknowledged() {
        let report = DeploymentReport::default();
        assert!(report.is_complete(), "an empty run is trivially complete");
        report.record_scheduled();
        report.record_scheduled();
        assert!(!report.is_complete());
        report.record_acknowledged();
        assert!(!report.is_complete());
        report.record_acknowledged();
        assert!(report.is_complete());
    }
}
