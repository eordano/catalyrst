use std::collections::HashMap;
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
    async fn active_entities_by_pointers(&self, pointers: &[String]) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError>;

    async fn active_entity_ids_by_content_hash(&self, hash: &str) -> Result<Vec<String>, DatabaseError>;

    async fn get_deployments(&self, options: &DeploymentQueryOptions) -> Result<DeploymentQueryResult, DatabaseError>;

    async fn get_pointer_changes(
        &self,
        options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError>;

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError>;

    async fn get_audit_info(&self, entity_type: &str, entity_id: &str) -> Result<Option<Value>, DatabaseError>;

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError>;
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
}

pub trait Denylist: Send + Sync {
    fn is_denylisted(&self, id: &str) -> bool;
}

pub trait ChallengeSupervisor: Send + Sync {
    fn get_challenge_text(&self) -> String;
}

pub trait SynchronizationState: Send + Sync {
    fn get_state(&self) -> String;

    fn is_type_ready(&self, _entity_type: &str) -> bool {
        true
    }

    fn ready_types(&self) -> Option<Vec<String>> {
        None
    }
}

pub trait SnapshotGenerator: Send + Sync {
    fn get_current_snapshots(&self) -> Option<Value>;
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

    pub deployments_cache: DashMap<String, CacheEntry>,

    pub content_version: String,
    pub lambdas_version: String,
    pub commit_hash: String,
    pub eth_network: String,
    pub content_server_address: String,
    pub read_only: bool,

    /// `max-age` (seconds) for the opt-in `Cache-Control` header on the active-entity listing
    /// endpoints (`/entities/active`, `/entities/:type`). `0` disables the header. Tunable via
    /// `ENTITIES_CACHE_CONTROL_MAX_AGE` (default 10).
    pub entities_cache_control_max_age: u64,

    pub content_public_url: String,
    pub lambdas_public_url: String,
    pub realm_name: Option<String>,

    pub squid_pool: Option<sqlx::PgPool>,
    pub profile_cdn_base_url: String,
    /// Base URL the squid-stored LAND/estate `image` map-thumbnail URLs are
    /// rewritten to (replacing the prod `https://api.decentraland.org` prefix),
    /// so lambdas land listings reference the LOCAL catalyrst-map (:5143).
    pub land_image_base_url: String,
}
