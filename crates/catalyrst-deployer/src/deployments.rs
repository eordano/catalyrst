use std::sync::Arc;

use tracing::{debug, error, info};

use crate::{
    BackendError, DatabaseBackend, Deployment, DeploymentId, DeploymentOptions, Entity,
    EntityContentEntry, FailedDeployment, PaginationInfo, PartialDeploymentHistory,
};

pub const MAX_HISTORY_LIMIT: i64 = 500;

pub fn curated_offset(options: &DeploymentOptions) -> i64 {
    options.offset.filter(|&o| o >= 0).unwrap_or(0)
}

pub fn curated_limit(options: &DeploymentOptions) -> i64 {
    match options.limit {
        Some(l) if l > 0 && l <= MAX_HISTORY_LIMIT => l,
        _ => MAX_HISTORY_LIMIT,
    }
}

pub async fn get_deployments(
    db: &dyn DatabaseBackend,
    options: &DeploymentOptions,
    denylist_filter: Option<&dyn Fn(&str) -> bool>,
) -> Result<PartialDeploymentHistory, BackendError> {
    let offset = curated_offset(options);
    let limit = curated_limit(options);

    let mut query_options = options.clone();
    query_options.offset = Some(offset);
    query_options.limit = Some(limit + 1);

    let result = db.get_historical_deployments(&query_options).await?;

    let more_data = result.deployments.len() as i64 > limit;
    let mut deployments: Vec<Deployment> = result
        .deployments
        .into_iter()
        .take(limit as usize)
        .collect();

    if let Some(is_denylisted) = denylist_filter {
        if !options.include_denylisted.unwrap_or(false) {
            deployments.retain(|d| !is_denylisted(&d.entity_id));
        }
    }

    Ok(PartialDeploymentHistory {
        deployments,
        filters: options.filters.clone().unwrap_or_default(),
        pagination: PaginationInfo {
            offset,
            limit,
            more_data,
            last_id: options.last_id.clone(),
        },
    })
}

pub async fn get_deployments_for_active_entities(
    db: &dyn DatabaseBackend,
    entity_ids: Option<&[String]>,
    pointers: Option<&[String]>,
) -> Result<Vec<Deployment>, BackendError> {
    let both = entity_ids.map(|e| !e.is_empty()).unwrap_or(false)
        && pointers.map(|p| !p.is_empty()).unwrap_or(false);
    let neither = entity_ids.map(|e| e.is_empty()).unwrap_or(true)
        && pointers.map(|p| p.is_empty()).unwrap_or(true);

    if both || neither {
        return Err(BackendError::Validation(
            "exactly one of entity_ids or pointers must be provided".into(),
        ));
    }

    db.get_active_deployments(entity_ids, pointers).await
}

pub fn map_deployments_to_entities(deployments: &[Deployment]) -> Vec<Entity> {
    deployments
        .iter()
        .map(|d| Entity {
            id: d.entity_id.clone(),
            entity_type: d.entity_type,
            pointers: d.pointers.clone(),
            timestamp: d.entity_timestamp,
            content: d.content.as_ref().map(|c| {
                c.iter()
                    .map(|dc| EntityContentEntry {
                        file: dc.key.clone(),
                        hash: dc.hash.clone(),
                    })
                    .collect()
            }),
            metadata: d.metadata.clone(),
        })
        .collect()
}

pub async fn is_entity_deployed(
    db: &dyn DatabaseBackend,
    entity_id: &str,
    bloom_check: impl FnOnce(&str) -> bool,
) -> Result<bool, BackendError> {
    if bloom_check(entity_id) {
        db.deployment_exists(entity_id).await
    } else {
        Ok(false)
    }
}

pub async fn retry_failed_deployments<F, Fut>(
    db: &dyn DatabaseBackend,
    redeploy_from_remote: F,
) -> Result<(), BackendError>
where
    F: Fn(FailedDeployment) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let failures = db.get_all_failed_deployments().await?;

    for failure in failures {
        let entity_id = failure.entity_id.clone();
        let entity_type = failure.entity_type;

        if failure.auth_chain.is_none() {
            info!(
                entity_id,
                ?entity_type,
                "skipping failed deployment retry — no auth chain"
            );
            continue;
        }

        debug!(entity_id, ?entity_type, "retrying failed deployment");

        match redeploy_from_remote(failure.clone()).await {
            Ok(()) => {
                debug!(entity_id, ?entity_type, "retry succeeded");
            }
            Err(err_msg) => {
                if err_msg.contains("IGNORING_FIX_ERROR") {
                    debug!(
                        entity_id,
                        ?entity_type,
                        "retired superseded failed deployment"
                    );
                    continue;
                }

                let updated = FailedDeployment {
                    error_description: Some(err_msg.clone()),
                    ..failure
                };
                if let Err(e) = db.save_failed_deployment(&updated).await {
                    error!(
                        entity_id,
                        ?entity_type,
                        error = %e,
                        "failed to re-report failed deployment"
                    );
                }

                error!(
                    entity_id,
                    ?entity_type,
                    error_description = err_msg,
                    "retry failed again"
                );
            }
        }
    }

    Ok(())
}

#[async_trait::async_trait]
pub trait DeploymentsQuery: Send + Sync {
    async fn get_deployments_for_active_third_party_items_by_entity_ids(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<Deployment>, BackendError>;

    async fn update_materialized_views(&self) -> Result<(), BackendError>;
}

pub struct DeploymentsComponent {
    pub database: Arc<dyn DatabaseBackend>,
}

#[async_trait::async_trait]
impl DeploymentsQuery for DeploymentsComponent {
    async fn get_deployments_for_active_third_party_items_by_entity_ids(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<Deployment>, BackendError> {
        self.database
            .get_active_deployments(Some(entity_ids), None)
            .await
    }

    async fn update_materialized_views(&self) -> Result<(), BackendError> {
        info!("update_materialized_views: no-op in this crate; implement in the database backend");
        Ok(())
    }
}

pub async fn calculate_overwrites(
    db: &dyn DatabaseBackend,
    entity: &Entity,
) -> Result<OverwriteInfo, BackendError> {
    let overwrote = db.calculate_overwrote(entity).await?;
    let overwritten_by = db.calculate_overwritten_by(entity).await?;

    Ok(OverwriteInfo {
        overwrote,
        overwritten_by,
    })
}

#[derive(Debug, Clone)]
pub struct OverwriteInfo {
    pub overwrote: Vec<DeploymentId>,
    pub overwritten_by: Option<DeploymentId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_limits() {
        let opts = DeploymentOptions::default();
        assert_eq!(curated_offset(&opts), 0);
        assert_eq!(curated_limit(&opts), MAX_HISTORY_LIMIT);

        let opts = DeploymentOptions {
            offset: Some(-5),
            limit: Some(1000),
            ..Default::default()
        };
        assert_eq!(curated_offset(&opts), 0);
        assert_eq!(curated_limit(&opts), MAX_HISTORY_LIMIT);

        let opts = DeploymentOptions {
            offset: Some(10),
            limit: Some(50),
            ..Default::default()
        };
        assert_eq!(curated_offset(&opts), 10);
        assert_eq!(curated_limit(&opts), 50);
    }
}
