use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use dashmap::DashMap;
use serde_json::Value;

#[async_trait]
pub trait ContentStorage: Send + Sync {
    async fn retrieve(&self, hash: &str) -> Option<Bytes>;

    async fn retrieve_stream(&self, hash: &str) -> Option<(Body, u64)>;

    async fn retrieve_range(&self, hash: &str, start: u64, end: u64) -> Option<Bytes>;

    async fn file_info(&self, hash: &str) -> Option<FileInfo>;

    async fn exist_multiple(&self, hashes: &[String]) -> HashMap<String, bool>;
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: Option<u64>,
    pub content_size: Option<u64>,
    pub encoding: Option<String>,
}

#[async_trait]
pub trait Database: Send + Sync {
    async fn active_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError>;

    async fn active_entity_ids_by_content_hash(
        &self,
        hash: &str,
    ) -> Result<Vec<String>, DatabaseError>;

    async fn get_deployments(
        &self,
        options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError>;

    async fn get_pointer_changes(
        &self,
        options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError>;

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError>;

    async fn get_audit_info(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError>;

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError>;

    async fn clear_failed_deployment(&self, _entity_id: &str) -> Result<u64, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "clear_failed_deployment not supported by this backend".to_string(),
        ))
    }

    async fn clear_all_failed_deployments(&self) -> Result<u64, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "clear_all_failed_deployments not supported by this backend".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct PrefixQueryResult {
    pub total: i64,
    pub entities: Vec<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentQueryOptions {
    pub entity_types: Vec<String>,
    pub entity_ids: Vec<String>,
    pub pointers: Vec<String>,
    pub deployed_by: Vec<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub only_currently_pointed: Option<bool>,
    pub fields: Vec<String>,
    pub sorting_field: Option<String>,
    pub sorting_order: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub last_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploymentQueryResult {
    pub deployments: Vec<Value>,
    pub filters: Value,
    pub pagination: PaginationResult,
}

#[derive(Debug, Clone, Default)]
pub struct PointerChangesQueryOptions {
    pub entity_types: Vec<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub include_auth_chain: bool,
    pub sorting_field: Option<String>,
    pub sorting_order: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub last_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PointerChangesQueryResult {
    pub deltas: Vec<Value>,
    pub filters: Value,
    pub pagination: PaginationResult,
}

#[derive(Debug, Clone)]
pub struct PaginationResult {
    pub offset: i64,
    pub limit: i64,
    pub more_data: bool,
    pub next: Option<String>,
    pub last_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("query failed: {0}")]
    QueryFailed(String),
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

#[async_trait]
pub trait Deployer: Send + Sync {
    async fn deploy_entity(
        &self,
        files: Vec<Bytes>,
        entity_id: &str,
        auth_chain: Value,
        context: &str,
    ) -> Result<i64, Vec<String>>;

    async fn retry_failed_deployment(&self, _entity_id: &str) -> Result<String, Vec<String>> {
        Err(vec![
            "retry_failed_deployment not supported by this deployer".to_string(),
        ])
    }
}

pub trait Denylist: Send + Sync {
    fn is_denylisted(&self, id: &str) -> bool;

    fn add(&self, _id: &str) -> Result<bool, String> {
        Err("denylist add not supported by this backend".to_string())
    }

    fn remove(&self, _id: &str) -> Result<bool, String> {
        Err("denylist remove not supported by this backend".to_string())
    }

    fn list(&self) -> Vec<String> {
        Vec::new()
    }
}

pub trait ChallengeSupervisor: Send + Sync {
    fn get_challenge_text(&self) -> String;

    fn refresh(&self) -> String {
        self.get_challenge_text()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncControl {
    Run,
    Paused,
}

pub trait SynchronizationState: Send + Sync {
    fn get_state(&self) -> String;

    fn is_type_ready(&self, _entity_type: &str) -> bool {
        true
    }

    fn ready_types(&self) -> Option<Vec<String>> {
        None
    }

    fn sync_frontier_ms(&self) -> Option<i64> {
        None
    }

    fn sync_heartbeat_ms(&self) -> Option<i64> {
        None
    }

    fn control(&self) -> SyncControl {
        SyncControl::Run
    }

    fn pause(&self) -> Result<(), String> {
        Err("sync pause not supported by this backend".to_string())
    }

    fn resume(&self) -> Result<(), String> {
        Err("sync resume not supported by this backend".to_string())
    }

    fn force(&self) -> Result<(), String> {
        Err("sync force not supported by this backend".to_string())
    }
}

pub trait SnapshotGenerator: Send + Sync {
    fn get_current_snapshots(&self) -> Option<Value>;

    fn trigger_regeneration(&self) -> Result<String, String> {
        Err("snapshot regeneration not supported by this backend".to_string())
    }
}

pub trait AcceptingUsers: Send + Sync {
    fn is_accepting(&self) -> bool;

    fn set_accepting(&self, _accepting: bool) -> Result<(), String> {
        Err("accepting-users toggle not supported by this backend".to_string())
    }
}

#[async_trait]
pub trait ContentCluster: Send + Sync {
    fn get_status(&self) -> Value;
}

#[derive(Clone)]
pub struct CacheEntry {
    pub bytes: Bytes,
    pub inserted_at: Instant,
}

impl CacheEntry {
    pub fn is_expired(&self, ttl: std::time::Duration) -> bool {
        self.inserted_at.elapsed() > ttl
    }
}

pub const DEPLOYMENTS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

pub const DEPLOYMENTS_CACHE_MAX_ENTRIES: usize = 1000;

pub struct AppState {
    pub storage: Arc<dyn ContentStorage>,
    pub database: Arc<dyn Database>,
    pub deployer: Arc<dyn Deployer>,
    pub denylist: Arc<dyn Denylist>,

    pub challenge_supervisor: Arc<dyn ChallengeSupervisor>,
    pub synchronization_state: Arc<dyn SynchronizationState>,
    pub snapshot_generator: Arc<dyn SnapshotGenerator>,
    pub content_cluster: Arc<dyn ContentCluster>,
    pub accepting_users: Arc<dyn AcceptingUsers>,

    pub deployments_cache: DashMap<String, CacheEntry>,

    pub content_version: String,
    pub lambdas_version: String,
    pub commit_hash: String,
    pub eth_network: String,
    pub content_server_address: String,

    pub read_only: AtomicBool,

    pub audit_pool: Option<sqlx::PgPool>,

    pub entities_cache_control_max_age: u64,

    pub content_public_url: String,
    pub lambdas_public_url: String,
    pub realm_name: Option<String>,

    pub squid_pool: Option<sqlx::PgPool>,
    pub profile_cdn_base_url: String,

    pub land_image_base_url: String,
}

impl AppState {
    pub fn is_read_only(&self) -> bool {
        self.read_only.load(Ordering::Relaxed)
    }

    pub fn set_read_only(&self, read_only: bool) -> bool {
        self.read_only.store(read_only, Ordering::Relaxed);
        read_only
    }
}
