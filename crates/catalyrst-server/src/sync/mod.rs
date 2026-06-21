pub mod backends;
pub mod batch_deployer;
pub mod bloom_filter;
pub mod deploy_remote_entity;
pub mod pointer_changes;
pub mod retry_failed;
pub mod snapshots;
pub mod sync_orchestrator;
pub mod time_range;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub use catalyrst_types::snapshot::{decompress_snapshot, parse_snapshot_entities, SyncDeployment};
pub use catalyrst_types::{AuthChain, AuthLink, AuthLinkType};
pub use catalyrst_types::{ContentFileHash, EntityId, Pointer, Timestamp};

pub type EntityType = String;

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

impl From<catalyrst_storage::StorageError> for SyncError {
    fn from(e: catalyrst_storage::StorageError) -> Self {
        SyncError::Storage(e.to_string())
    }
}

pub use backends::{
    LiveDeploymentRepository, LiveFailedDeploymentsStore, LiveProcessedSnapshotStore,
    LiveSyncDeployer, SyncGauges,
};
pub use batch_deployer::BatchDeployer;
pub use bloom_filter::BloomFilter;
pub use retry_failed::RetryFailedDeployments;
pub use sync_orchestrator::SyncOrchestrator;
