pub mod active_entities;
pub mod challenge_supervisor;
pub mod deployment_service;
pub mod deployments;
pub mod failed_deployments_reporter;
pub mod garbage_collection;
pub mod parallel_pipeline;
pub mod pointer_manager;
pub mod sequential_task_executor;

pub use active_entities::{ActiveEntities, NotActiveEntity};
pub use challenge_supervisor::{ChallengeSupervisor, IChallengeSupervisor};
pub use deployment_service::{DeploymentService, DeploymentServiceConfig};
pub use deployments::{DeploymentsComponent, DeploymentsQuery};
pub use failed_deployments_reporter::{FailedDeploymentsReporter, IFailedDeploymentsReporter};
pub use garbage_collection::{GarbageCollectionManager, GarbageCollectionConfig};
pub use parallel_pipeline::{
    EntityFetcher, FetchError, FetchedEntity, HttpEntityFetcher,
    ParallelDeploymentPipeline, PipelineConfig, PipelineResult, SyncTask,
};
pub use pointer_manager::{DeltaPointerResult, PointerManager};
pub use sequential_task_executor::{SequentialTaskExecutor, ISequentialTaskExecutor};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use catalyrst_types::{AuthChain, AuthLink, AuthLinkType, EntityType, EntityVersion};

pub type DeploymentId = i64;

pub type ContentHash = String;

pub type Pointer = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentContext {
    Local,
    Synced,
    SyncedLegacyEntity,
    FixAttempt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    // A standard catalyst-client deploy uploads an id-less file; the id is
    // `hashV1(idless_file)`, supplied out-of-band and bound to the content hash
    // in `deploy_entity`. Optional on the wire, always present on re-serialise.
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type")]
    pub entity_type: EntityType,
    pub pointers: Vec<Pointer>,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<EntityContentEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContentEntry {
    pub file: String,
    pub hash: ContentHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentContent {
    pub key: String,
    pub hash: ContentHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditInfo {
    pub version: EntityVersion,
    pub auth_chain: AuthChain,
    pub local_timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwritten_by: Option<String>,
    #[serde(default)]
    pub is_denylisted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denylisted_content: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalDeploymentAuditInfo {
    pub auth_chain: AuthChain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub entity_version: EntityVersion,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub entity_timestamp: i64,
    pub deployed_by: String,
    pub pointers: Vec<Pointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<DeploymentContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub audit_info: AuditInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedDeployment {
    pub entity_id: String,
    pub entity_type: EntityType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_chain: Option<AuthChain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
    #[serde(default)]
    pub from_snapshot: bool,
}

#[derive(Debug, Clone)]
pub enum DeploymentResult {
    Success(i64),
    Invalid(Vec<String>),
}

impl DeploymentResult {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeploymentFilters {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub deployed_by: Option<Vec<String>>,
    pub entity_types: Option<Vec<EntityType>>,
    pub entity_ids: Option<Vec<String>>,
    pub pointers: Option<Vec<Pointer>>,
    pub only_currently_pointed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortingField {
    LocalTimestamp,
    EntityTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortingOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSorting {
    pub field: Option<SortingField>,
    pub order: Option<SortingOrder>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentOptions {
    pub filters: Option<DeploymentFilters>,
    pub sort_by: Option<DeploymentSorting>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub last_id: Option<String>,
    pub include_denylisted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialDeploymentHistory {
    pub deployments: Vec<Deployment>,
    pub filters: DeploymentFilters,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub offset: i64,
    pub limit: i64,
    pub more_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("database error: {0}")]
    Database(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[async_trait::async_trait]
pub trait TransactionHandle: Send + Sync {
    async fn commit(self: Box<Self>) -> Result<(), BackendError>;
    async fn rollback(self: Box<Self>) -> Result<(), BackendError>;
}

#[async_trait::async_trait]
pub trait DatabaseBackend: Send + Sync {
    async fn begin_transaction(&self) -> Result<Box<dyn TransactionHandle>, BackendError>;

    async fn deployment_exists(&self, entity_id: &str) -> Result<bool, BackendError>;

    async fn get_deployment_by_entity_id(
        &self,
        entity_id: &str,
    ) -> Result<Option<Deployment>, BackendError>;

    async fn save_deployment(
        &self,
        tx: &dyn TransactionHandle,
        entity: &Entity,
        audit_info: &AuditInfo,
        overwritten_by: Option<DeploymentId>,
    ) -> Result<DeploymentId, BackendError>;

    async fn save_content_files(
        &self,
        tx: &dyn TransactionHandle,
        deployment_id: DeploymentId,
        content: &[EntityContentEntry],
    ) -> Result<(), BackendError>;

    async fn calculate_overwrote(
        &self,
        entity: &Entity,
    ) -> Result<Vec<DeploymentId>, BackendError>;

    async fn calculate_overwritten_by(
        &self,
        entity: &Entity,
    ) -> Result<Option<DeploymentId>, BackendError>;

    async fn set_entities_as_overwritten(
        &self,
        tx: &dyn TransactionHandle,
        overwritten_ids: &[DeploymentId],
        overwriter_id: DeploymentId,
    ) -> Result<(), BackendError>;

    async fn update_active_deployments(
        &self,
        tx: &dyn TransactionHandle,
        pointers: &[Pointer],
        entity_id: &str,
    ) -> Result<(), BackendError>;

    async fn remove_active_deployments(
        &self,
        tx: &dyn TransactionHandle,
        pointers: &[Pointer],
    ) -> Result<(), BackendError>;

    async fn get_historical_deployments(
        &self,
        options: &DeploymentOptions,
    ) -> Result<PartialDeploymentHistory, BackendError>;

    async fn get_active_deployments(
        &self,
        entity_ids: Option<&[String]>,
        pointers: Option<&[Pointer]>,
    ) -> Result<Vec<Deployment>, BackendError>;

    async fn get_deployments_by_ids(
        &self,
        ids: &[DeploymentId],
    ) -> Result<Vec<DeploymentIdWithPointers>, BackendError>;

    async fn find_unreferenced_content_hashes(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<ContentHash>, BackendError>;

    async fn gc_stale_profiles(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<GcStaleProfilesResult, BackendError>;

    async fn gc_profile_active_pointers(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<Vec<Pointer>, BackendError>;

    async fn save_failed_deployment(
        &self,
        deployment: &FailedDeployment,
    ) -> Result<(), BackendError>;

    async fn delete_failed_deployment(&self, entity_id: &str) -> Result<(), BackendError>;

    async fn find_failed_deployment(
        &self,
        entity_id: &str,
    ) -> Result<Option<FailedDeployment>, BackendError>;

    async fn get_all_failed_deployments(&self) -> Result<Vec<FailedDeployment>, BackendError>;

    async fn remove_failed_deployment(&self, entity_id: &str) -> Result<(), BackendError>;

    async fn get_last_gc_time(&self) -> Result<Option<i64>, BackendError>;

    async fn set_last_gc_time(&self, timestamp: i64) -> Result<(), BackendError>;
}

#[derive(Debug, Clone)]
pub struct DeploymentIdWithPointers {
    pub id: DeploymentId,
    pub pointers: Vec<Pointer>,
}

#[derive(Debug, Clone, Default)]
pub struct GcStaleProfilesResult {
    pub deleted_hashes: Vec<ContentHash>,
    pub deleted_deployment_ids: Vec<DeploymentId>,
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    async fn exist_multiple(&self, hashes: &[ContentHash]) -> Result<HashMap<ContentHash, bool>, BackendError>;

    async fn store(&self, hash: &ContentHash, data: &[u8]) -> Result<(), BackendError>;

    async fn delete(&self, hashes: &[ContentHash]) -> Result<(), BackendError>;

    async fn all_file_ids(&self) -> Result<Vec<ContentHash>, BackendError>;
}

#[async_trait::async_trait]
pub trait ValidatorBackend: Send + Sync {
    async fn validate(
        &self,
        entity: &Entity,
        audit_info: &LocalDeploymentAuditInfo,
        files: &HashMap<ContentHash, Vec<u8>>,
        context: DeploymentContext,
        checks: ValidationChecks,
    ) -> Result<(), Vec<String>>;
}

#[derive(Debug, Clone)]
pub struct ValidationChecks {
    pub has_newer_entities: bool,
    pub is_already_deployed: bool,
    pub is_failed_deployment: bool,
    pub is_rate_limited: bool,
    pub is_request_ttl_exceeded: bool,
    pub is_content_unchanged: bool,
}

#[async_trait::async_trait]
pub trait AuthenticatorBackend: Send + Sync {
    fn owner_address(auth_chain: &AuthChain) -> String
    where
        Self: Sized;

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool;
}
