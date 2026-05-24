pub mod batch_deployer;
pub mod bloom_filter;
pub mod deploy_remote_entity;
pub mod peer_cluster;
pub mod pointer_changes;
pub mod retry_failed;
pub mod snapshots;
pub mod sync_orchestrator;
pub mod time_range;

use std::collections::HashSet;
use serde::{Deserialize, Serialize};

pub type Timestamp = i64;

pub type EntityId = String;

pub type Pointer = String;

pub type ContentFileHash = String;

pub use catalyrst_types::{AuthChain, AuthLink, AuthLinkType};

pub type EntityType = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDeployment {
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub pointers: Vec<Pointer>,
    pub auth_chain: AuthChain,
    pub entity_timestamp: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_timestamp: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub hash: String,
    pub time_range: TimeRange,
    pub number_of_entities: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_snapshot_hashes: Option<Vec<String>>,
    pub generation_timestamp: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub init_timestamp: Timestamp,
    pub end_timestamp: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    DeploymentError,
    NoEntity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedDeployment {
    pub entity_type: EntityType,
    pub entity_id: EntityId,
    pub reason: FailureReason,
    pub auth_chain: AuthChain,
    pub error_description: String,
    pub failure_timestamp: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalystServerInfo {
    pub address: String,
    pub owner: String,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentContext {
    Local,
    Synced,
    SyncedFix,
}

pub const NON_PROFILE_TYPES: &[&str] = &["scene", "wearable", "emote", "store", "outfits"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    Bootstrapping,
    PartiallySynced { ready_types: HashSet<String> },
    Syncing,
}

#[async_trait::async_trait]
pub trait ContentStorage: Send + Sync + 'static {
    async fn exists(&self, hash: &str) -> Result<bool, SyncError>;
    async fn store(&self, hash: &str, data: bytes::Bytes) -> Result<(), SyncError>;
    async fn retrieve(&self, hash: &str) -> Result<Option<bytes::Bytes>, SyncError>;
    async fn delete(&self, hashes: &[String]) -> Result<(), SyncError>;
}

#[async_trait::async_trait]
pub trait Deployer: Send + Sync + 'static {
    async fn deploy_entity(
        &self,
        entity_data: &[u8],
        entity_id: &str,
        auth_chain: &AuthChain,
        context: DeploymentContext,
    ) -> Result<(), SyncError>;

    async fn flush(&self) -> Result<(), SyncError> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait ProcessedSnapshotStore: Send + Sync + 'static {
    async fn filter_processed(&self, hashes: &[String]) -> Result<HashSet<String>, SyncError>;
    async fn mark_processed(&self, hash: &str) -> Result<(), SyncError>;
}

#[async_trait::async_trait]
pub trait SnapshotStorageCheck: Send + Sync + 'static {
    async fn has(&self, snapshot_hash: &str) -> Result<bool, SyncError>;
}

#[async_trait::async_trait]
pub trait FailedDeploymentsStore: Send + Sync + 'static {
    async fn report_failure(&self, failure: FailedDeployment) -> Result<(), SyncError>;
    async fn find_failed(&self, entity_id: &str) -> Result<Option<FailedDeployment>, SyncError>;
    async fn get_all_failed(&self) -> Result<Vec<FailedDeployment>, SyncError>;
    async fn remove(&self, entity_id: &str) -> Result<(), SyncError>;
}

#[async_trait::async_trait]
pub trait DeploymentRepository: Send + Sync + 'static {
    async fn is_entity_deployed(
        &self,
        entity_id: &str,
        entity_timestamp: Timestamp,
    ) -> Result<bool, SyncError>;

    async fn get_sync_frontier(&self) -> Result<Timestamp, SyncError> {
        Ok(0)
    }

    async fn set_sync_frontier(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        let _ = timestamp;
        Ok(())
    }

    async fn resolve_deleter_deployments(&self) -> Result<(), SyncError> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait DaoClient: Send + Sync + 'static {
    async fn get_all_content_servers(&self) -> Result<Vec<CatalystServerInfo>, SyncError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Deployment rejected: {0}")]
    DeploymentRejected(String),

    #[error("Entity not found in storage: {entity_id}")]
    EntityNotFound { entity_id: String },

    #[error("No servers available")]
    NoServers,

    #[error("Sync stopped")]
    Stopped,

    #[error("{0}")]
    Other(String),
}

pub use batch_deployer::BatchDeployer;
pub use bloom_filter::BloomFilter;
pub use peer_cluster::PeerCluster;
pub use retry_failed::RetryFailedDeployments;
pub use sync_orchestrator::SyncOrchestrator;
