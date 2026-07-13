use std::collections::{HashMap, HashSet};

use crate::{
    BackendError, DatabaseBackend, DeploymentId, DeploymentIdWithPointers, Entity, Pointer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaPointerResult {
    Set,
    Cleared,
}

#[derive(Debug, Clone)]
pub struct PointerDelta {
    pub before: Option<DeploymentId>,
    pub after: DeltaPointerResult,
}

pub type PointerManagerResult = HashMap<Pointer, PointerDelta>;

pub struct PointerManager;

impl PointerManager {
    pub async fn reference_entity_from_pointers(
        &self,
        db: &dyn DatabaseBackend,
        entity: &Entity,
        overwritten_deployment_ids: &HashSet<DeploymentId>,
        is_entity_overwritten: bool,
    ) -> Result<PointerManagerResult, BackendError> {
        let mut result = PointerManagerResult::new();

        if is_entity_overwritten {
            return Ok(result);
        }

        let overwritten_ids: Vec<DeploymentId> =
            overwritten_deployment_ids.iter().copied().collect();
        let overwritten_deployments: Vec<DeploymentIdWithPointers> = if overwritten_ids.is_empty() {
            Vec::new()
        } else {
            db.get_deployments_by_ids(&overwritten_ids).await?
        };

        for pointer in &entity.pointers {
            let before = overwritten_deployments
                .iter()
                .find(|dep| dep.pointers.contains(pointer))
                .map(|dep| dep.id);

            result.insert(
                pointer.clone(),
                PointerDelta {
                    before,
                    after: DeltaPointerResult::Set,
                },
            );
        }

        for dep in &overwritten_deployments {
            for pointer in &dep.pointers {
                if !result.contains_key(pointer) {
                    result.insert(
                        pointer.clone(),
                        PointerDelta {
                            before: Some(dep.id),
                            after: DeltaPointerResult::Cleared,
                        },
                    );
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityType;

    fn make_entity(pointers: &[&str]) -> Entity {
        Entity {
            id: "entity-1".into(),
            entity_type: EntityType::Scene,
            pointers: pointers.iter().map(|p| p.to_string()).collect(),
            timestamp: 1_000_000,
            content: None,
            metadata: None,
        }
    }

    #[test]
    fn overwritten_entity_returns_empty() {
        let pm = PointerManager;
        let entity = make_entity(&["0,0", "0,1"]);

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let result = rt.block_on(async {
            struct PanicDb;

            #[async_trait::async_trait]
            impl DatabaseBackend for PanicDb {
                async fn begin_transaction(
                    &self,
                ) -> Result<Box<dyn crate::TransactionHandle>, BackendError> {
                    unimplemented!()
                }
                async fn deployment_exists(&self, _: &str) -> Result<bool, BackendError> {
                    unimplemented!()
                }
                async fn get_deployment_by_entity_id(
                    &self,
                    _: &str,
                ) -> Result<Option<crate::Deployment>, BackendError> {
                    unimplemented!()
                }
                async fn save_deployment(
                    &self,
                    _: &dyn crate::TransactionHandle,
                    _: &Entity,
                    _: &crate::AuditInfo,
                    _: Option<DeploymentId>,
                ) -> Result<DeploymentId, BackendError> {
                    unimplemented!()
                }
                async fn save_content_files(
                    &self,
                    _: &dyn crate::TransactionHandle,
                    _: DeploymentId,
                    _: &[crate::EntityContentEntry],
                ) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn calculate_overwrote(
                    &self,
                    _: &Entity,
                ) -> Result<Vec<DeploymentId>, BackendError> {
                    unimplemented!()
                }
                async fn calculate_overwritten_by(
                    &self,
                    _: &Entity,
                ) -> Result<Option<DeploymentId>, BackendError> {
                    unimplemented!()
                }
                async fn set_entities_as_overwritten(
                    &self,
                    _: &dyn crate::TransactionHandle,
                    _: &[DeploymentId],
                    _: DeploymentId,
                ) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn update_active_deployments(
                    &self,
                    _: &dyn crate::TransactionHandle,
                    _: &[Pointer],
                    _: &str,
                ) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn remove_active_deployments(
                    &self,
                    _: &dyn crate::TransactionHandle,
                    _: &[Pointer],
                ) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn get_historical_deployments(
                    &self,
                    _: &crate::DeploymentOptions,
                ) -> Result<crate::PartialDeploymentHistory, BackendError> {
                    unimplemented!()
                }
                async fn get_active_deployments(
                    &self,
                    _: Option<&[String]>,
                    _: Option<&[Pointer]>,
                ) -> Result<Vec<crate::Deployment>, BackendError> {
                    unimplemented!()
                }
                async fn get_deployments_by_ids(
                    &self,
                    _: &[DeploymentId],
                ) -> Result<Vec<DeploymentIdWithPointers>, BackendError> {
                    unimplemented!()
                }
                async fn find_unreferenced_content_hashes(
                    &self,
                    _: chrono::DateTime<chrono::Utc>,
                ) -> Result<Vec<crate::ContentHash>, BackendError> {
                    unimplemented!()
                }
                async fn gc_stale_profiles(
                    &self,
                    _: chrono::DateTime<chrono::Utc>,
                ) -> Result<crate::GcStaleProfilesResult, BackendError> {
                    unimplemented!()
                }
                async fn gc_profile_active_pointers(
                    &self,
                    _: chrono::DateTime<chrono::Utc>,
                ) -> Result<Vec<Pointer>, BackendError> {
                    unimplemented!()
                }
                async fn save_failed_deployment(
                    &self,
                    _: &crate::FailedDeployment,
                ) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn delete_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn find_failed_deployment(
                    &self,
                    _: &str,
                ) -> Result<Option<crate::FailedDeployment>, BackendError> {
                    unimplemented!()
                }
                async fn get_all_failed_deployments(
                    &self,
                ) -> Result<Vec<crate::FailedDeployment>, BackendError> {
                    unimplemented!()
                }
                async fn remove_failed_deployment(&self, _: &str) -> Result<(), BackendError> {
                    unimplemented!()
                }
                async fn get_last_gc_time(&self) -> Result<Option<i64>, BackendError> {
                    unimplemented!()
                }
                async fn set_last_gc_time(&self, _: i64) -> Result<(), BackendError> {
                    unimplemented!()
                }
            }

            let db = PanicDb;
            let overwrote = HashSet::new();
            pm.reference_entity_from_pointers(&db, &entity, &overwrote, true)
                .await
                .unwrap()
        });

        assert!(
            result.is_empty(),
            "overwritten entity should produce no pointer changes"
        );
    }
}
