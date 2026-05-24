use thiserror::Error;

use crate::entity::{EntityId, EntityType, Pointer};

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("entity validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("missing content files: {hashes:?}")]
    MissingContent { hashes: Vec<String> },

    #[error("authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("entity {entity_id} is older than the current entity for pointers {pointers:?}")]
    EntityIsOlder {
        entity_id: EntityId,
        pointers: Vec<Pointer>,
    },

    #[error("unknown entity type: {entity_type}")]
    UnknownEntityType { entity_type: String },

    #[error("rate limited for entity type {entity_type}")]
    RateLimited { entity_type: EntityType },

    #[error("server is in read-only mode")]
    ReadOnly,

    #[error("entity not found: {entity_id}")]
    EntityNotFound { entity_id: EntityId },

    #[error("entity {entity_id} is denylisted")]
    Denylisted { entity_id: EntityId },

    #[error("storage error: {0}")]
    Storage(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type ContentResult<T> = Result<T, ContentError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedDeploymentReason {
    BlockchainAccessCheck,
    ContentDownloadFailed,
    ValidationFailed,
    Other(String),
}

impl std::fmt::Display for FailedDeploymentReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailedDeploymentReason::BlockchainAccessCheck => {
                write!(f, "blockchain_access_check")
            }
            FailedDeploymentReason::ContentDownloadFailed => {
                write!(f, "content_download_failed")
            }
            FailedDeploymentReason::ValidationFailed => write!(f, "validation_failed"),
            FailedDeploymentReason::Other(s) => write!(f, "{}", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_error_display() {
        let err = ContentError::ValidationFailed {
            reason: "bad metadata".into(),
        };
        assert_eq!(err.to_string(), "entity validation failed: bad metadata");
    }

    #[test]
    fn failed_deployment_reason_display() {
        assert_eq!(
            FailedDeploymentReason::BlockchainAccessCheck.to_string(),
            "blockchain_access_check"
        );
    }
}
