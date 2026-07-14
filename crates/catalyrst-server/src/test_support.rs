use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use catalyrst_storage::StorageError;
use serde_json::Value;

use crate::state::*;

pub(crate) struct FaultyStorage;

fn injected_fault() -> StorageError {
    StorageError::Io(std::io::Error::other("injected storage fault"))
}

#[async_trait]
impl ContentStorage for FaultyStorage {
    async fn retrieve(&self, _hash: &str) -> Result<Option<Bytes>, StorageError> {
        Err(injected_fault())
    }

    async fn retrieve_stream(&self, _hash: &str) -> Result<Option<(Body, u64)>, StorageError> {
        Err(injected_fault())
    }

    async fn retrieve_range(
        &self,
        _hash: &str,
        _start: u64,
        _end: u64,
    ) -> Result<Option<Bytes>, StorageError> {
        Err(injected_fault())
    }

    async fn file_info(&self, _hash: &str) -> Result<Option<FileInfo>, StorageError> {
        Err(injected_fault())
    }

    async fn exist_multiple(
        &self,
        _hashes: &[String],
    ) -> Result<HashMap<String, bool>, StorageError> {
        Err(injected_fault())
    }
}

pub(crate) struct EmptyStorage;

#[async_trait]
impl ContentStorage for EmptyStorage {
    async fn retrieve(&self, _hash: &str) -> Result<Option<Bytes>, StorageError> {
        Ok(None)
    }

    async fn retrieve_stream(&self, _hash: &str) -> Result<Option<(Body, u64)>, StorageError> {
        Ok(None)
    }

    async fn retrieve_range(
        &self,
        _hash: &str,
        _start: u64,
        _end: u64,
    ) -> Result<Option<Bytes>, StorageError> {
        Ok(None)
    }

    async fn file_info(&self, _hash: &str) -> Result<Option<FileInfo>, StorageError> {
        Ok(None)
    }

    async fn exist_multiple(
        &self,
        hashes: &[String],
    ) -> Result<HashMap<String, bool>, StorageError> {
        Ok(hashes.iter().map(|h| (h.clone(), false)).collect())
    }
}

struct StubDatabase;

#[async_trait]
impl Database for StubDatabase {
    async fn active_entities_by_pointers(
        &self,
        _pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn active_entities_by_ids(&self, _ids: &[String]) -> Result<Vec<Value>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn active_entities_by_prefix(
        &self,
        _prefix: &str,
        _offset: i64,
        _limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError> {
        Ok(PrefixQueryResult {
            total: 0,
            entities: Vec::new(),
        })
    }

    async fn active_entity_ids_by_content_hash(
        &self,
        _hash: &str,
    ) -> Result<Vec<String>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn get_deployments(
        &self,
        _options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "get_deployments not supported by the test stub".to_string(),
        ))
    }

    async fn get_pointer_changes(
        &self,
        _options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "get_pointer_changes not supported by the test stub".to_string(),
        ))
    }

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn get_audit_info(
        &self,
        _entity_type: &str,
        _entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError> {
        Ok(None)
    }

    async fn find_entity_by_pointer(&self, _pointer: &str) -> Result<Option<Value>, DatabaseError> {
        Ok(None)
    }
}

struct StubDeployer;

#[async_trait]
impl Deployer for StubDeployer {
    async fn deploy_entity(
        &self,
        _files: Vec<Bytes>,
        _entity_id: &str,
        _auth_chain: Value,
        _context: &str,
    ) -> Result<i64, DeployFailure> {
        Err(vec!["deploys not supported by the test stub".to_string()].into())
    }
}

struct NoDenylist;

impl Denylist for NoDenylist {
    fn is_denylisted(&self, _id: &str) -> bool {
        false
    }
}

struct StubChallenge;

impl ChallengeSupervisor for StubChallenge {
    fn get_challenge_text(&self) -> String {
        "test-challenge".to_string()
    }
}

struct StubSyncState;

impl SynchronizationState for StubSyncState {
    fn get_state(&self) -> String {
        "synced".to_string()
    }
}

struct StubSnapshots;

impl SnapshotGenerator for StubSnapshots {
    fn get_current_snapshots(&self) -> Option<Vec<crate::sync::SnapshotMetadata>> {
        None
    }
}

struct StubCluster;

#[async_trait]
impl ContentCluster for StubCluster {
    fn get_status(&self) -> Value {
        Value::Null
    }
}

struct StubAccepting;

impl AcceptingUsers for StubAccepting {
    fn is_accepting(&self) -> bool {
        true
    }
}

pub(crate) fn app_state_with_storage(storage: Arc<dyn ContentStorage>) -> Arc<AppState> {
    Arc::new(AppState {
        storage,
        database: Arc::new(StubDatabase),
        deployer: Arc::new(StubDeployer),
        denylist: Arc::new(NoDenylist),
        challenge_supervisor: Arc::new(StubChallenge),
        synchronization_state: Arc::new(StubSyncState),
        snapshot_generator: Arc::new(StubSnapshots),
        content_cluster: Arc::new(StubCluster),
        accepting_users: Arc::new(StubAccepting),
        deployments_cache: dashmap::DashMap::new(),
        content_version: "test".to_string(),
        lambdas_version: "test".to_string(),
        commit_hash: "test".to_string(),
        eth_network: "sepolia".to_string(),
        content_server_address: "http://localhost/content".to_string(),
        read_only: AtomicBool::new(false),
        audit_pool: None,
        content_pool: None,
        entities_cache_control_max_age: 10,
        content_public_url: "http://localhost/content".to_string(),
        lambdas_public_url: "http://localhost/lambdas".to_string(),
        realm_name: None,
        squid_pool: None,
        profile_cdn_base_url: String::new(),
        land_image_base_url: String::new(),
    })
}
