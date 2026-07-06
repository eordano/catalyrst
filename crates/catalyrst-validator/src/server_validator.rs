use async_trait::async_trait;

use crate::error::ServerValidationResult;
use crate::types::{DeploymentContext, Entity};

const REQUEST_TTL_FORWARDS_MS: i64 = 15 * 60 * 1000;

pub const IGNORING_FIX_ERROR: &str =
    "Ignoring fix for failed deployment since there are newer entities. ";

#[async_trait]
pub trait ServiceCalls: Send + Sync {
    async fn are_there_newer_entities(&self, entity: &Entity) -> bool;

    async fn is_entity_deployed_already(&self, entity: &Entity) -> bool;

    async fn is_not_failed_deployment(&self, entity: &Entity) -> bool;

    async fn is_entity_rate_limited(&self, entity: &Entity) -> bool;

    async fn is_request_ttl_backwards(&self, entity: &Entity) -> bool;
}

#[async_trait]
pub trait FailedDeploymentRemover: Send + Sync {
    async fn remove_failed_deployment(&self, entity_id: &str);
}

pub async fn validate_server_side(
    entity: &Entity,
    context: DeploymentContext,
    service_calls: &dyn ServiceCalls,
    failed_deployment_remover: &dyn FailedDeploymentRemover,
) -> ServerValidationResult {
    match context {
        DeploymentContext::Synced | DeploymentContext::SyncedLegacyEntity => {
            return ServerValidationResult::Ok;
        }
        _ => {}
    }

    match context {
        DeploymentContext::Local => {
            if let Some(err) = local_checks(entity, service_calls).await {
                return ServerValidationResult::fail(err);
            }
        }
        DeploymentContext::FixAttempt => {
            if service_calls.are_there_newer_entities(entity).await {
                failed_deployment_remover
                    .remove_failed_deployment(&entity.id)
                    .await;
                return ServerValidationResult::fail(format!(
                    "{IGNORING_FIX_ERROR} (pointers={})",
                    entity.pointers.join(",")
                ));
            }

            if let Some(err) = fix_attempt_checks(entity, service_calls).await {
                return ServerValidationResult::fail(err);
            }
        }
        _ => {}
    }

    ServerValidationResult::Ok
}

async fn local_checks(entity: &Entity, service_calls: &dyn ServiceCalls) -> Option<String> {
    if service_calls.are_there_newer_entities(entity).await {
        return Some(format!(
            "There is a newer entity pointed by one or more of the pointers you provided \
             (entityId={} pointers={}).",
            entity.id,
            entity.pointers.join(",")
        ));
    }

    if service_calls.is_entity_deployed_already(entity).await {
        return Some("This entity was already deployed. You can't redeploy it".to_string());
    }

    if service_calls.is_entity_rate_limited(entity).await {
        return Some(format!(
            "Entity rate limited (entityId={} pointers={}).",
            entity.id,
            entity.pointers.join(",")
        ));
    }

    if service_calls.is_request_ttl_backwards(entity).await {
        return Some(format!(
            "The request is not recent enough, please submit it again with a new timestamp \
             (entityId={} pointers={}).",
            entity.id,
            entity.pointers.join(",")
        ));
    }

    if is_request_ttl_forwards(entity) {
        return Some(format!(
            "The request is too far in the future, please submit it again with a new timestamp \
             (entityId={} pointers={}).",
            entity.id,
            entity.pointers.join(",")
        ));
    }

    None
}

fn is_request_ttl_forwards(entity: &Entity) -> bool {
    let now_ms = chrono::Utc::now().timestamp_millis();
    now_ms - entity.timestamp < -REQUEST_TTL_FORWARDS_MS
}

async fn fix_attempt_checks(entity: &Entity, service_calls: &dyn ServiceCalls) -> Option<String> {
    if service_calls.is_not_failed_deployment(entity).await {
        return Some("You are trying to fix an entity that is not marked as failed".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockServiceCalls {
        newer: bool,
        deployed: bool,
        not_failed: bool,
        rate_limited: bool,
        ttl_backwards: bool,
    }

    #[async_trait]
    impl ServiceCalls for MockServiceCalls {
        async fn are_there_newer_entities(&self, _: &Entity) -> bool {
            self.newer
        }
        async fn is_entity_deployed_already(&self, _: &Entity) -> bool {
            self.deployed
        }
        async fn is_not_failed_deployment(&self, _: &Entity) -> bool {
            self.not_failed
        }
        async fn is_entity_rate_limited(&self, _: &Entity) -> bool {
            self.rate_limited
        }
        async fn is_request_ttl_backwards(&self, _: &Entity) -> bool {
            self.ttl_backwards
        }
    }

    struct NoopRemover;

    #[async_trait]
    impl FailedDeploymentRemover for NoopRemover {
        async fn remove_failed_deployment(&self, _: &str) {}
    }

    fn test_entity() -> Entity {
        Entity {
            id: "bafkrei".to_string(),
            entity_type: crate::types::EntityType::Scene,
            pointers: vec!["0,0".to_string()],
            timestamp: chrono::Utc::now().timestamp_millis(),
            content: vec![],
            version: "v3".to_string(),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn synced_context_skips_all_checks() {
        let service = MockServiceCalls {
            newer: true,
            deployed: true,
            not_failed: true,
            rate_limited: true,
            ttl_backwards: true,
        };
        let result = validate_server_side(
            &test_entity(),
            DeploymentContext::Synced,
            &service,
            &NoopRemover,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn local_rejects_newer_entities() {
        let service = MockServiceCalls {
            newer: true,
            deployed: false,
            not_failed: false,
            rate_limited: false,
            ttl_backwards: false,
        };
        let result = validate_server_side(
            &test_entity(),
            DeploymentContext::Local,
            &service,
            &NoopRemover,
        )
        .await;
        assert!(!result.is_ok());
    }

    #[tokio::test]
    async fn local_passes_happy_path() {
        let service = MockServiceCalls {
            newer: false,
            deployed: false,
            not_failed: false,
            rate_limited: false,
            ttl_backwards: false,
        };
        let result = validate_server_side(
            &test_entity(),
            DeploymentContext::Local,
            &service,
            &NoopRemover,
        )
        .await;
        assert!(result.is_ok());
    }
}
