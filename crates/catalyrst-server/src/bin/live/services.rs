use super::*;

pub(crate) struct ReadOnlyDeployer;

#[async_trait]
impl Deployer for ReadOnlyDeployer {
    async fn deploy_entity(
        &self,
        _files: Vec<Bytes>,
        _entity_id: &str,
        _auth_chain: Value,
        _context: &str,
    ) -> Result<i64, Vec<String>> {
        Err(vec![
            "Live server is read-only; deployments are not supported".to_string(),
        ])
    }
}

pub(crate) struct MemoryDenylist {
    ids: std::sync::RwLock<std::collections::HashSet<String>>,
}

impl MemoryDenylist {
    pub(crate) fn new() -> Self {
        Self {
            ids: std::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }
}

impl Denylist for MemoryDenylist {
    fn is_denylisted(&self, id: &str) -> bool {
        self.ids.read().map(|s| s.contains(id)).unwrap_or(false)
    }

    fn add(&self, id: &str) -> Result<bool, String> {
        self.ids
            .write()
            .map(|mut s| s.insert(id.to_string()))
            .map_err(|_| "denylist lock poisoned".to_string())
    }

    fn remove(&self, id: &str) -> Result<bool, String> {
        self.ids
            .write()
            .map(|mut s| s.remove(id))
            .map_err(|_| "denylist lock poisoned".to_string())
    }

    fn list(&self) -> Vec<String> {
        self.ids
            .read()
            .map(|s| {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }
}

pub(crate) struct LiveAcceptingUsers(pub(crate) std::sync::atomic::AtomicBool);
impl AcceptingUsers for LiveAcceptingUsers {
    fn is_accepting(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn set_accepting(&self, accepting: bool) -> Result<(), String> {
        self.0
            .store(accepting, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

pub(crate) struct UuidChallengeSupervisor;
impl ChallengeSupervisor for UuidChallengeSupervisor {
    fn get_challenge_text(&self) -> String {
        format!("dcl-crypto-{}", uuid::Uuid::new_v4())
    }
}

pub(crate) struct LiveSynchronizationState {
    sync_state: Option<Arc<tokio::sync::RwLock<catalyrst_server::sync::SyncState>>>,

    paused: std::sync::atomic::AtomicBool,

    control: Option<catalyrst_server::sync::sync_orchestrator::SyncControlHandle>,

    gauges: Option<catalyrst_server::sync::SyncGauges>,
}

impl LiveSynchronizationState {
    pub(crate) fn new() -> Self {
        Self {
            sync_state: None,
            paused: std::sync::atomic::AtomicBool::new(false),
            control: None,
            gauges: None,
        }
    }

    pub(crate) fn with_sync_state(
        sync_state: Arc<tokio::sync::RwLock<catalyrst_server::sync::SyncState>>,
        control: Option<catalyrst_server::sync::sync_orchestrator::SyncControlHandle>,
        gauges: catalyrst_server::sync::SyncGauges,
    ) -> Self {
        Self {
            sync_state: Some(sync_state),
            paused: std::sync::atomic::AtomicBool::new(false),
            control,
            gauges: Some(gauges),
        }
    }

    fn read_state(&self) -> Option<catalyrst_server::sync::SyncState> {
        let handle = self.sync_state.as_ref()?;
        Some(handle.try_read().ok()?.clone())
    }
}

impl SynchronizationState for LiveSynchronizationState {
    fn get_state(&self) -> String {
        match self.read_state() {
            None => "Syncing".to_string(),
            Some(catalyrst_server::sync::SyncState::Bootstrapping) => "Bootstrapping".to_string(),
            Some(catalyrst_server::sync::SyncState::PartiallySynced { .. }) => {
                "Syncing".to_string()
            }
            Some(catalyrst_server::sync::SyncState::Syncing) => "Syncing".to_string(),
        }
    }

    fn is_type_ready(&self, entity_type: &str) -> bool {
        match self.read_state() {
            None => true,
            Some(catalyrst_server::sync::SyncState::Syncing) => true,
            Some(catalyrst_server::sync::SyncState::PartiallySynced { ready_types }) => {
                ready_types.contains(entity_type)
            }
            Some(catalyrst_server::sync::SyncState::Bootstrapping) => false,
        }
    }

    fn ready_types(&self) -> Option<Vec<String>> {
        match self.read_state() {
            None => None,
            Some(catalyrst_server::sync::SyncState::Syncing) => None,
            Some(catalyrst_server::sync::SyncState::PartiallySynced { ready_types }) => {
                let mut types: Vec<String> = ready_types.iter().cloned().collect();
                types.sort();
                Some(types)
            }
            Some(catalyrst_server::sync::SyncState::Bootstrapping) => Some(vec![]),
        }
    }

    fn sync_frontier_ms(&self) -> Option<i64> {
        let v = self
            .gauges
            .as_ref()?
            .frontier_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        (v > 0).then_some(v)
    }

    fn sync_heartbeat_ms(&self) -> Option<i64> {
        let v = self
            .gauges
            .as_ref()?
            .heartbeat_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        (v > 0).then_some(v)
    }

    fn control(&self) -> SyncControl {
        let paused = match &self.control {
            Some(c) => c.is_paused(),
            None => self.paused.load(std::sync::atomic::Ordering::Relaxed),
        };
        if paused {
            SyncControl::Paused
        } else {
            SyncControl::Run
        }
    }

    fn pause(&self) -> Result<(), String> {
        self.paused
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(c) = &self.control {
            c.pause();
        }
        Ok(())
    }

    fn resume(&self) -> Result<(), String> {
        self.paused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(c) = &self.control {
            c.resume();
        }
        Ok(())
    }

    fn force(&self) -> Result<(), String> {
        self.paused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(c) = &self.control {
            c.force();
        }
        Ok(())
    }
}

pub(crate) struct LiveSnapshotGenerator {
    snapshots: Arc<RwLock<Option<Value>>>,
}

impl LiveSnapshotGenerator {
    pub(crate) async fn load(pool: &PgPool) -> Self {
        #[derive(sqlx::FromRow)]
        struct SnapRow {
            hash: Option<String>,
            init_ts_ms: f64,
            end_ts_ms: f64,
            number_of_entities: i32,
            replaced_hashes: Vec<String>,
            gen_ts_ms: f64,
        }

        let rows = sqlx::query_as::<_, SnapRow>(
            r#"
            SELECT hash,
                   (EXTRACT(EPOCH FROM init_timestamp) * 1000)::float8 AS init_ts_ms,
                   (EXTRACT(EPOCH FROM end_timestamp) * 1000)::float8 AS end_ts_ms,
                   number_of_entities,
                   replaced_hashes,
                   (EXTRACT(EPOCH FROM generation_time) * 1000)::float8 AS gen_ts_ms
            FROM snapshots
            ORDER BY end_timestamp DESC
            "#,
        )
        .fetch_all(pool)
        .await;

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "Failed to load snapshots");
                return Self {
                    snapshots: Arc::new(RwLock::new(None)),
                };
            }
        };

        if rows.is_empty() {
            tracing::warn!("No snapshots found in database");
            return Self {
                snapshots: Arc::new(RwLock::new(None)),
            };
        }

        let arr: Vec<Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "hash": r.hash,
                    "timeRange": {
                        "initTimestamp": r.init_ts_ms as i64,
                        "endTimestamp": r.end_ts_ms as i64,
                    },
                    "replacedSnapshotHashes": r.replaced_hashes,
                    "numberOfEntities": r.number_of_entities,
                    "generationTimestamp": r.gen_ts_ms as i64,
                })
            })
            .collect();

        tracing::info!(count = arr.len(), "Snapshots loaded into memory");
        Self {
            snapshots: Arc::new(RwLock::new(Some(Value::Array(arr)))),
        }
    }

    pub(crate) fn snapshots_handle(&self) -> Arc<RwLock<Option<Value>>> {
        self.snapshots.clone()
    }
}

pub(crate) fn snapshots_metadata_to_json(
    snapshots: &[catalyrst_db::snapshots_repository::SnapshotMetadata],
) -> Value {
    let arr: Vec<Value> = snapshots
        .iter()
        .map(|s| {
            json!({
                "hash": s.hash,
                "timeRange": {
                    "initTimestamp": s.time_range.init_timestamp as i64,
                    "endTimestamp": s.time_range.end_timestamp as i64,
                },
                "replacedSnapshotHashes": s.replaced_snapshot_hashes,
                "numberOfEntities": s.number_of_entities,
                "generationTimestamp": s.generation_timestamp as i64,
            })
        })
        .collect();
    Value::Array(arr)
}

impl SnapshotGenerator for LiveSnapshotGenerator {
    fn get_current_snapshots(&self) -> Option<Value> {
        self.snapshots.try_read().ok()?.clone()
    }
}

pub(crate) struct LiveContentCluster;
#[async_trait]
impl ContentCluster for LiveContentCluster {
    fn get_status(&self) -> Value {
        json!({})
    }
}
