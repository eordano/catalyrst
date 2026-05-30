use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::{
    AuditInfo, AuthChain, BackendError, ContentHash, DatabaseBackend, DeploymentContext,
    DeploymentId, DeploymentResult, Entity, EntityType, EntityVersion, LocalDeploymentAuditInfo,
    Pointer, StorageBackend, ValidationChecks, ValidatorBackend,
};
use crate::pointer_manager::{DeltaPointerResult, PointerManager};

#[derive(Debug, Clone)]
pub struct DeploymentServiceConfig {
    pub legacy_content_migration_timestamp_ms: i64,

    pub request_ttl_backwards_ms: i64,
}

impl Default for DeploymentServiceConfig {
    fn default() -> Self {
        Self {
            legacy_content_migration_timestamp_ms: 1_582_167_600_000,
            request_ttl_backwards_ms: 30 * 24 * 60 * 60 * 1000,
        }
    }
}

pub struct PointerLockManager {
    locked: parking_lot::Mutex<HashSet<(EntityType, String)>>,
}

impl PointerLockManager {
    pub fn new() -> Self {
        Self {
            locked: parking_lot::Mutex::new(HashSet::new()),
        }
    }

    pub fn try_acquire(&self, entity_type: EntityType, pointers: &[Pointer]) -> Vec<Pointer> {
        let mut guard = self.locked.lock();
        let mut overlapping = Vec::new();

        for p in pointers {
            let key = (entity_type, p.to_lowercase());
            if guard.contains(&key) {
                overlapping.push(p.clone());
            }
        }

        if overlapping.is_empty() {
            for p in pointers {
                guard.insert((entity_type, p.to_lowercase()));
            }
        }

        overlapping
    }

    pub fn release(&self, entity_type: EntityType, pointers: &[Pointer]) {
        let mut guard = self.locked.lock();
        for p in pointers {
            guard.remove(&(entity_type, p.to_lowercase()));
        }
    }
}

impl Default for PointerLockManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DeploymentService {
    pub config: DeploymentServiceConfig,
    pub database: Arc<dyn DatabaseBackend>,
    pub storage: Arc<dyn StorageBackend>,
    pub validator: Arc<dyn ValidatorBackend>,
    pub pointer_lock_manager: Arc<PointerLockManager>,
    pub pointer_manager: PointerManager,
}

pub enum DeploymentFiles {
    Hashed(HashMap<ContentHash, Vec<u8>>),
    Raw(Vec<Vec<u8>>),
}

impl DeploymentService {
    pub async fn deploy_entity(
        &self,
        files: DeploymentFiles,
        entity_id: &str,
        audit_info: &LocalDeploymentAuditInfo,
        context: DeploymentContext,
    ) -> DeploymentResult {
        match self.database.get_deployment_by_entity_id(entity_id).await {
            Ok(Some(existing)) => {
                debug!(
                    entity_id,
                    local_timestamp = existing.audit_info.local_timestamp,
                    "entity was already deployed"
                );
                return DeploymentResult::Success(existing.audit_info.local_timestamp);
            }
            Ok(None) => {}
            Err(e) => {
                error!(entity_id, error = %e, "failed to check existing deployment");
                return DeploymentResult::Invalid(vec![
                    "Internal error checking existing deployment".into(),
                ]);
            }
        }

        let hashes: HashMap<ContentHash, Vec<u8>> = match files {
            DeploymentFiles::Hashed(map) => map,
            DeploymentFiles::Raw(buffers) => {
                let _ = buffers;
                return DeploymentResult::Invalid(vec![
                    "Raw (unhashed) file upload not yet implemented in catalyrst-deployer; supply pre-hashed files.".into()
                ]);
            }
        };

        let entity_bytes = match hashes.get(entity_id) {
            Some(bytes) => bytes.clone(),
            None => {
                return DeploymentResult::Invalid(vec!["Failed to find the entity file.".into()]);
            }
        };

        let mut entity: Entity = match serde_json::from_slice(&entity_bytes) {
            Ok(e) => e,
            Err(e) => {
                warn!(entity_id, error = %e, "failed to parse entity JSON");
                return DeploymentResult::Invalid(vec![
                    "There was a problem parsing the entity".into(),
                ]);
            }
        };

        // Entity-id reconciliation (mirrors the reference content-server +
        // `DeploymentBuilder`). A standard catalyst-client deploy uploads an
        // id-less entity file; its id is `hashV1(idless_file)`, supplied
        // out-of-band as the multipart `entityId` field — which is also the
        // hash key (`entity_id`) under which the file bytes are stored here. So:
        //   * if the file carries no `id`, adopt the deploy's `entity_id`;
        //   * if it carries one, it MUST equal `entity_id` (a mismatched
        //     embedded id is a forged/inconsistent deploy and is rejected).
        // Either way `entity.id` ends up bound to the content hash, keeping the
        // downstream validator's `entity.id == entity_id` invariant intact.
        if entity.id.is_empty() {
            entity.id = entity_id.to_string();
        } else if entity.id != entity_id {
            warn!(
                entity_id,
                embedded_id = %entity.id,
                "entity file's embedded id does not match the deploy's entity id"
            );
            return DeploymentResult::Invalid(vec![format!(
                "Entity id mismatch: the entity file declares id '{}' but was deployed as '{}'.",
                entity.id, entity_id
            )]);
        }

        if entity.pointers.is_empty() {
            return DeploymentResult::Invalid(vec![
                "The entity does not have any pointer.".into(),
            ]);
        }

        let overlapping = self
            .pointer_lock_manager
            .try_acquire(entity.entity_type, &entity.pointers);
        if !overlapping.is_empty() {
            return DeploymentResult::Invalid(vec![format!(
                "The following pointers are currently being deployed: '{}'. Please try again in a few seconds.",
                overlapping.join(",")
            )]);
        }

        let result = self
            .deploy_entity_inner(&entity, entity_id, audit_info, &hashes, context)
            .await;

        self.pointer_lock_manager
            .release(entity.entity_type, &entity.pointers);

        result
    }

    async fn deploy_entity_inner(
        &self,
        entity: &Entity,
        entity_id: &str,
        audit_info: &LocalDeploymentAuditInfo,
        hashes: &HashMap<ContentHash, Vec<u8>>,
        context: DeploymentContext,
    ) -> DeploymentResult {
        let context = self.classify_context(entity, &audit_info.auth_chain, context);

        let is_content_unchanged =
            self.check_content_unchanged(entity, context).await;

        info!(
            entity_id,
            pointers = ?entity.pointers,
            "deploying entity"
        );

        let checks = match self.build_validation_checks(entity, is_content_unchanged, context).await {
            Ok(c) => c,
            Err(e) => {
                error!(entity_id, error = %e, "failed building validation checks");
                return DeploymentResult::Invalid(vec![
                    "Internal error building validation checks".into(),
                ]);
            }
        };

        if let Err(errors) = self
            .validator
            .validate(entity, audit_info, hashes, context, checks)
            .await
        {
            warn!(entity_id, ?errors, "validation failed");
            return DeploymentResult::Invalid(errors);
        }

        if let Err(e) = self.store_content(hashes).await {
            error!(entity_id, error = %e, "failed to store content");
            return DeploymentResult::Invalid(vec![
                "Failed to store entity content".into(),
            ]);
        }

        match self.persist_deployment(entity, audit_info, hashes).await {
            Ok(local_timestamp) => {
                info!(entity_id, local_timestamp, "entity deployed");

                let _ = self.database.remove_failed_deployment(entity_id).await;

                DeploymentResult::Success(local_timestamp)
            }
            Err(e) => {
                error!(entity_id, error = %e, "failed to persist deployment");
                DeploymentResult::Invalid(vec![
                    "There was an error deploying the entity".into(),
                ])
            }
        }
    }

    fn classify_context(
        &self,
        entity: &Entity,
        _auth_chain: &AuthChain,
        context: DeploymentContext,
    ) -> DeploymentContext {
        if matches!(
            context,
            DeploymentContext::Synced | DeploymentContext::FixAttempt
        ) && entity.timestamp < self.config.legacy_content_migration_timestamp_ms
        {
            return DeploymentContext::SyncedLegacyEntity;
        }
        context
    }

    async fn check_content_unchanged(
        &self,
        entity: &Entity,
        context: DeploymentContext,
    ) -> bool {
        if context != DeploymentContext::Local || entity.entity_type != EntityType::Profile {
            return false;
        }

        match self
            .database
            .get_active_deployments(None, Some(&entity.pointers))
            .await
        {
            Ok(active) if !active.is_empty() => {
                active[0].metadata == entity.metadata
            }
            Ok(_) => false,
            Err(e) => {
                warn!(error = %e, "failed to check content unchanged, assuming changed");
                false
            }
        }
    }

    async fn build_validation_checks(
        &self,
        entity: &Entity,
        is_content_unchanged: bool,
        _context: DeploymentContext,
    ) -> Result<ValidationChecks, BackendError> {
        let is_already_deployed = self
            .database
            .deployment_exists(&entity.id)
            .await?;

        let is_failed_deployment = self
            .database
            .find_failed_deployment(&entity.id)
            .await?
            .is_some();

        let has_newer_entities = {
            let active = self
                .database
                .get_active_deployments(None, Some(&entity.pointers))
                .await?;
            active.iter().any(|d| d.entity_timestamp > entity.timestamp)
        };

        let now = chrono::Utc::now().timestamp_millis();
        let is_request_ttl_exceeded =
            (now - entity.timestamp) > self.config.request_ttl_backwards_ms;

        let is_rate_limited = false;

        Ok(ValidationChecks {
            has_newer_entities,
            is_already_deployed,
            is_failed_deployment,
            is_rate_limited,
            is_request_ttl_exceeded,
            is_content_unchanged,
        })
    }

    async fn store_content(
        &self,
        hashes: &HashMap<ContentHash, Vec<u8>>,
    ) -> Result<(), BackendError> {
        let keys: Vec<ContentHash> = hashes.keys().cloned().collect();
        let existing = self.storage.exist_multiple(&keys).await?;

        for (hash, data) in hashes {
            if !existing.get(hash).copied().unwrap_or(false) {
                self.storage.store(hash, data).await?;
            }
        }
        Ok(())
    }

    async fn persist_deployment(
        &self,
        entity: &Entity,
        audit_info: &LocalDeploymentAuditInfo,
        _hashes: &HashMap<ContentHash, Vec<u8>>,
    ) -> Result<i64, BackendError> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        let full_audit = AuditInfo {
            version: EntityVersion::V3,
            auth_chain: audit_info.auth_chain.clone(),
            local_timestamp: now_ms,
            overwritten_by: None,
            is_denylisted: false,
            denylisted_content: Vec::new(),
        };

        let overwrote_ids = self.database.calculate_overwrote(entity).await?;
        let overwritten_by = self.database.calculate_overwritten_by(entity).await?;

        let tx = self.database.begin_transaction().await?;

        let deployment_id = self
            .database
            .save_deployment(&*tx, entity, &full_audit, overwritten_by)
            .await?;

        if let Some(ref content) = entity.content {
            self.database
                .save_content_files(&*tx, deployment_id, content)
                .await?;
        }

        let overwrote_set: HashSet<DeploymentId> = overwrote_ids.iter().copied().collect();
        let is_overwritten = overwritten_by.is_some();

        let pointer_results = self
            .pointer_manager
            .reference_entity_from_pointers(
                &*self.database,
                entity,
                &overwrote_set,
                is_overwritten,
            )
            .await?;

        let mut cleared_pointers = Vec::new();
        let mut set_pointers = Vec::new();

        for (pointer, delta) in &pointer_results {
            match delta.after {
                DeltaPointerResult::Cleared => cleared_pointers.push(pointer.clone()),
                DeltaPointerResult::Set => set_pointers.push(pointer.clone()),
            }
        }

        if !cleared_pointers.is_empty() {
            self.database
                .remove_active_deployments(&*tx, &cleared_pointers)
                .await?;
        }

        if !set_pointers.is_empty() {
            self.database
                .update_active_deployments(&*tx, &set_pointers, &entity.id)
                .await?;
        }

        if !overwrote_ids.is_empty() {
            self.database
                .set_entities_as_overwritten(&*tx, &overwrote_ids, deployment_id)
                .await?;
        }

        tx.commit().await?;

        Ok(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_deserializes_without_id_field() {
        // Standard catalyst-client deploys upload an id-less entity file; the
        // id is hashV1(file), supplied out-of-band. The Entity struct must
        // parse such a file (id defaults to empty, then the deployer binds it
        // to the content hash). Previously this failed (no serde default).
        let idless = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1700000000000,
            "content": [{"file": "scene.json", "hash": "bafkreiaaaa"}]
        }"#;
        let e: Entity = serde_json::from_slice(idless.as_bytes()).expect("id-less parse");
        assert_eq!(e.id, "");
        assert_eq!(e.pointers, vec!["0,0".to_string()]);

        // A file that DOES carry an id still parses (id preserved for the
        // deployer's match-or-reject check).
        let with_id = r#"{
            "id": "bafkreitest",
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1
        }"#;
        let e: Entity = serde_json::from_slice(with_id.as_bytes()).expect("with-id parse");
        assert_eq!(e.id, "bafkreitest");
    }

    #[test]
    fn pointer_lock_manager_basics() {
        let plm = PointerLockManager::new();

        let overlap = plm.try_acquire(EntityType::Scene, &["0,0".into(), "0,1".into()]);
        assert!(overlap.is_empty());

        let overlap = plm.try_acquire(EntityType::Scene, &["0,1".into(), "1,1".into()]);
        assert_eq!(overlap, vec!["0,1".to_string()]);

        plm.release(EntityType::Scene, &["0,0".into(), "0,1".into()]);
        let overlap = plm.try_acquire(EntityType::Scene, &["0,1".into(), "1,1".into()]);
        assert!(overlap.is_empty());
    }

    #[test]
    fn pointer_lock_manager_different_types_dont_conflict() {
        let plm = PointerLockManager::new();
        let overlap = plm.try_acquire(EntityType::Scene, &["0,0".into()]);
        assert!(overlap.is_empty());

        let overlap = plm.try_acquire(EntityType::Profile, &["0,0".into()]);
        assert!(overlap.is_empty());
    }
}
