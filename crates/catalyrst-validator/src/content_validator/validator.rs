use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use tracing::{debug, warn};

use super::helpers::{
    count_urn_segments, is_ipfs_v2_hash, profile_has_emotes, should_validate_face_thumbnail,
    validate_profile_emote_urns, validate_profile_wearable_urns,
};
use super::spring_bones::validate_spring_bones_metadata;
use crate::checker::{
    validate_item_access, validate_outfits_access, validate_profile_access, validate_scene_access,
    validate_store_access, BlockchainChecker,
};
use crate::error::ValidationResponse;
use crate::image_metadata::{check_face256_thumbnail_image, check_wearable_thumbnail_image};
use crate::types::*;

#[async_trait]
pub trait ExternalCalls: Send + Sync {
    async fn is_content_stored_already(&self, hashes: &[String]) -> HashMap<String, bool>;

    async fn fetch_content_file_size(&self, hash: &str) -> Option<usize>;

    async fn validate_signature(
        &self,
        entity_id: &str,
        audit_info: &DeploymentAuditInfo,
        timestamp: Timestamp,
    ) -> Result<(), String>;

    fn owner_address(&self, audit_info: &DeploymentAuditInfo) -> String;

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool;

    async fn calculate_files_hashes(
        &self,
        files: &HashMap<String, Vec<u8>>,
    ) -> HashMap<String, CalculatedHash>;
}

pub struct CalculatedHash {
    pub calculated_hash: String,
    pub buffer: Vec<u8>,
}

pub struct ContentValidator<E: ExternalCalls, B: BlockchainChecker> {
    pub external_calls: E,
    pub blockchain_checker: B,
    pub ignore_blockchain_access: bool,
}

impl<E: ExternalCalls, B: BlockchainChecker> ContentValidator<E, B> {
    pub fn new(external_calls: E, blockchain_checker: B, ignore_blockchain_access: bool) -> Self {
        Self {
            external_calls,
            blockchain_checker,
            ignore_blockchain_access,
        }
    }

    pub async fn validate(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let result = self.validate_entity_structure(deployment);
        if !result.is_ok() {
            debug!("Validation failed at entity structure");
            return result;
        }

        let result = self.validate_ipfs_hashing(deployment);
        if !result.is_ok() {
            debug!("Validation failed at IPFS hashing");
            return result;
        }

        let result = self.validate_adr45(deployment);
        if !result.is_ok() {
            debug!("Validation failed at ADR-45 check");
            return result;
        }

        let result = self.validate_signature(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at signature");
            return result;
        }

        let result = self.validate_size(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at size");
            return result;
        }

        let result = self.validate_items(deployment);
        if !result.is_ok() {
            debug!("Validation failed at item validation");
            return result;
        }

        let result = self.validate_profile_content(deployment);
        if !result.is_ok() {
            debug!("Validation failed at profile content");
            return result;
        }

        let result = self.validate_item_thumbnail(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at item thumbnail");
            return result;
        }

        let result = self.validate_face_thumbnail(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at face thumbnail");
            return result;
        }

        let result = self.validate_scene_content(deployment);
        if !result.is_ok() {
            debug!("Validation failed at scene content");
            return result;
        }

        let result = self.validate_content(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at content cross-check");
            return result;
        }

        let result = self.validate_outfits_content(deployment);
        if !result.is_ok() {
            debug!("Validation failed at outfits content");
            return result;
        }

        let result = crate::third_party::validate_third_party_merkle_proof_content(deployment);
        if !result.is_ok() {
            debug!("Validation failed at third-party merkle proof");
            return result;
        }

        let result = self.validate_access(deployment).await;
        if !result.is_ok() {
            debug!("Validation failed at access check");
            return result;
        }

        ValidationResponse::Ok
    }

    fn validate_entity_structure(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;

        if entity.pointers.is_empty() {
            return ValidationResponse::fail(
                "The entity needs to be pointed by one or more pointers.".to_string(),
            );
        }

        let unique: HashSet<&String> = entity.pointers.iter().collect();
        if unique.len() != entity.pointers.len() {
            return ValidationResponse::fail(
                "There are repeated pointers in your request.".to_string(),
            );
        }

        ValidationResponse::Ok
    }

    fn validate_ipfs_hashing(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;

        if entity.timestamp < adr_timestamps::ADR_45 {
            return ValidationResponse::Ok;
        }

        let mut all_hashes = vec![&entity.id];
        for cm in &entity.content {
            all_hashes.push(&cm.hash);
        }

        let errors: Vec<String> = all_hashes
            .into_iter()
            .filter(|hash| !is_ipfs_v2_hash(hash))
            .map(|hash| format!("This hash '{hash}' is not valid. It should be IPFS v2 format."))
            .collect();

        ValidationResponse::from_errors(errors)
    }

    fn validate_adr45(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;

        if entity.version != "v3" && entity.timestamp > adr_timestamps::ADR_45 {
            return ValidationResponse::fail(
                "Only entities v3 are allowed after the ADR-45. \
                 Check http://adr.decentraland.org/adr/ADR-45 for more information"
                    .to_string(),
            );
        }

        ValidationResponse::Ok
    }

    async fn validate_signature(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;
        match self
            .external_calls
            .validate_signature(&entity.id, &deployment.audit_info, entity.timestamp)
            .await
        {
            Ok(()) => ValidationResponse::Ok,
            Err(msg) => ValidationResponse::fail(format!("The signature is invalid. {msg}")),
        }
    }

    async fn validate_size(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;

        if entity.timestamp <= adr_timestamps::LEGACY_CONTENT_MIGRATION {
            return ValidationResponse::Ok;
        }

        let max_size_mb = match max_size_mb(entity.entity_type) {
            Some(size) => size,
            None => {
                return ValidationResponse::fail(format!(
                    "Type {} is not supported yet",
                    entity.entity_type
                ));
            }
        };

        let max_size_mb = if entity.entity_type == EntityType::Wearable {
            if let Some(category) = entity
                .metadata
                .as_ref()
                .and_then(|m| m.get("data"))
                .and_then(|d| d.get("category"))
                .and_then(|c| c.as_str())
            {
                if category == "skin" {
                    SKIN_MAX_SIZE_MB
                } else {
                    max_size_mb
                }
            } else {
                max_size_mb
            }
        } else {
            max_size_mb
        };

        let max_size_bytes = max_size_mb * 1024 * 1024;

        let total_size = if entity.timestamp > adr_timestamps::ADR_45 {
            match self.calculate_deployment_size(deployment).await {
                Ok(size) => size,
                Err(msg) => return ValidationResponse::fail(msg),
            }
        } else {
            deployment.files.values().map(|f| f.len() as u64).sum()
        };

        let size_per_pointer = total_size / entity.pointers.len().max(1) as u64;
        if size_per_pointer > max_size_bytes {
            return ValidationResponse::fail(format!(
                "The deployment is too big. The maximum allowed size per pointer is \
                 {max_size_mb} MB for {}. You can upload up to {} bytes but you tried \
                 to upload {total_size}.",
                entity.entity_type,
                entity.pointers.len() as u64 * max_size_bytes
            ));
        }

        ValidationResponse::Ok
    }

    async fn calculate_deployment_size(
        &self,
        deployment: &DeploymentToValidate,
    ) -> Result<u64, String> {
        let mut total: u64 = 0;
        let unique_hashes: HashSet<&String> =
            deployment.entity.content.iter().map(|c| &c.hash).collect();

        for hash in unique_hashes {
            if let Some(uploaded) = deployment.files.get(hash) {
                total += uploaded.len() as u64;
            } else {
                match self.external_calls.fetch_content_file_size(hash).await {
                    Some(size) => total += size as u64,
                    None => {
                        return Err(format!("Couldn't fetch content file with hash: {hash}"));
                    }
                }
            }
        }

        Ok(total)
    }

    fn validate_items(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;

        match entity.entity_type {
            EntityType::Wearable => {
                let result = self.validate_wearable_representations(deployment);
                if !result.is_ok() {
                    return result;
                }
                validate_spring_bones_metadata(&deployment.entity)
            }
            EntityType::Emote => self.validate_emote_representations(deployment),
            _ => ValidationResponse::Ok,
        }
    }

    fn validate_wearable_representations(
        &self,
        deployment: &DeploymentToValidate,
    ) -> ValidationResponse {
        let entity = &deployment.entity;
        let metadata = match &entity.metadata {
            Some(m) => m,
            None => return ValidationResponse::fail("No wearable metadata found".to_string()),
        };

        let representations = metadata
            .get("data")
            .and_then(|d| d.get("representations"))
            .and_then(|r| r.as_array());

        if let Some(reps) = representations {
            if !reps.is_empty() {
                let entity_files: HashSet<&str> =
                    entity.content.iter().map(|c| c.file.as_str()).collect();

                for rep in reps {
                    if let Some(contents) = rep.get("contents").and_then(|c| c.as_array()) {
                        for content_ref in contents {
                            if let Some(file_name) = content_ref.as_str() {
                                if !entity_files.contains(file_name) {
                                    return ValidationResponse::fail(format!(
                                        "Representation content: '{file_name}' is not one \
                                         of the content files"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        ValidationResponse::Ok
    }

    fn validate_emote_representations(
        &self,
        deployment: &DeploymentToValidate,
    ) -> ValidationResponse {
        let entity = &deployment.entity;

        if entity.timestamp < adr_timestamps::ADR_74 {
            return ValidationResponse::fail(format!(
                "The emote timestamp {} is before ADR 74. \
                 Emotes did not exist before ADR 74.",
                entity.timestamp
            ));
        }

        let metadata = match &entity.metadata {
            Some(m) => m,
            None => return ValidationResponse::fail("No emote metadata found".to_string()),
        };

        let representations = metadata
            .get("emoteDataADR74")
            .and_then(|d| d.get("representations"))
            .and_then(|r| r.as_array());

        if let Some(reps) = representations {
            if !reps.is_empty() {
                let entity_files: HashSet<&str> =
                    entity.content.iter().map(|c| c.file.as_str()).collect();

                for rep in reps {
                    if let Some(contents) = rep.get("contents").and_then(|c| c.as_array()) {
                        for content_ref in contents {
                            if let Some(file_name) = content_ref.as_str() {
                                if !entity_files.contains(file_name) {
                                    return ValidationResponse::fail(format!(
                                        "Representation content: '{file_name}' is not one \
                                         of the content files"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        ValidationResponse::Ok
    }

    async fn validate_item_thumbnail(
        &self,
        deployment: &DeploymentToValidate,
    ) -> ValidationResponse {
        let entity = &deployment.entity;

        if !matches!(entity.entity_type, EntityType::Wearable | EntityType::Emote) {
            return ValidationResponse::Ok;
        }
        if entity.timestamp < adr_timestamps::ADR_45 {
            return ValidationResponse::Ok;
        }

        let thumbnail = entity
            .metadata
            .as_ref()
            .and_then(|m| m.get("thumbnail"))
            .and_then(|t| t.as_str());

        let hash = entity
            .content
            .iter()
            .find(|c| Some(c.file.as_str()) == thumbnail)
            .map(|c| c.hash.clone());

        let hash = match hash {
            Some(h) => h,
            None => {
                return ValidationResponse::fail(format!(
                    "Couldn't find hash for thumbnail file with name: {}",
                    thumbnail.unwrap_or("undefined")
                ));
            }
        };

        match deployment.files.get(&hash) {
            Some(buffer) => ValidationResponse::from_errors(check_wearable_thumbnail_image(buffer)),
            None => {
                let stored = self
                    .external_calls
                    .is_content_stored_already(std::slice::from_ref(&hash))
                    .await;
                if stored.get(&hash).copied().unwrap_or(false) {
                    ValidationResponse::Ok
                } else {
                    ValidationResponse::fail(format!(
                        "Couldn't find thumbnail file with hash: {hash}"
                    ))
                }
            }
        }
    }

    async fn validate_face_thumbnail(
        &self,
        deployment: &DeploymentToValidate,
    ) -> ValidationResponse {
        let entity = &deployment.entity;
        if entity.entity_type != EntityType::Profile {
            return ValidationResponse::Ok;
        }
        if !should_validate_face_thumbnail(entity, &deployment.files) {
            return ValidationResponse::Ok;
        }

        let avatars = entity
            .metadata
            .as_ref()
            .and_then(|m| m.get("avatars"))
            .and_then(|a| a.as_array());
        let avatars = match avatars {
            Some(a) => a,
            None => return ValidationResponse::Ok,
        };

        let mut errors: Vec<String> = Vec::new();
        for avatar in avatars {
            let hash = avatar
                .get("avatar")
                .and_then(|a| a.get("snapshots"))
                .and_then(|s| s.get("face256"))
                .and_then(|h| h.as_str());
            let hash = match hash {
                Some(h) => h.to_string(),
                None => {
                    return ValidationResponse::fail(
                        "Couldn't find hash for face256 thumbnail file with name: 'face256'"
                            .to_string(),
                    );
                }
            };

            let stored = self
                .external_calls
                .is_content_stored_already(std::slice::from_ref(&hash))
                .await;
            if stored.get(&hash).copied().unwrap_or(false) {
                continue;
            }

            match deployment.files.get(&hash) {
                Some(buffer) => errors.extend(check_face256_thumbnail_image(buffer)),
                None => {
                    return ValidationResponse::fail(format!(
                        "Couldn't find thumbnail file with hash: {hash}"
                    ));
                }
            }
        }

        ValidationResponse::from_errors(errors)
    }

    fn validate_profile_content(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;
        if entity.entity_type != EntityType::Profile {
            return ValidationResponse::Ok;
        }

        if entity.timestamp >= adr_timestamps::ADR_290_REJECTED {
            if let Some(metadata) = &entity.metadata {
                if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
                    for avatar in avatars {
                        if avatar
                            .get("avatar")
                            .and_then(|a| a.get("snapshots"))
                            .is_some()
                        {
                            return ValidationResponse::fail(
                                "Avatars must not have snapshots.".to_string(),
                            );
                        }
                    }
                }
            }

            if !entity.content.is_empty() {
                return ValidationResponse::fail(format!(
                    "Entity has content files when it should not: {}",
                    entity
                        .content
                        .iter()
                        .map(|c| c.file.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if deployment.files.len() > 1 {
                return ValidationResponse::fail(format!(
                    "Entity has uploaded files when it should not: {}",
                    deployment
                        .files
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        if entity.timestamp >= adr_timestamps::ADR_75 {
            if let Some(metadata) = &entity.metadata {
                if let Err(msg) = validate_profile_wearable_urns(metadata, entity.timestamp) {
                    return ValidationResponse::fail(msg);
                }
            }
        }

        if entity.timestamp >= adr_timestamps::ADR_74 {
            if let Some(metadata) = &entity.metadata {
                if let Err(msg) = validate_profile_emote_urns(metadata, entity.timestamp) {
                    return ValidationResponse::fail(msg);
                }

                if !profile_has_emotes(metadata) {
                    return ValidationResponse::fail(
                        "Profile must have emotes after ADR 74.".to_string(),
                    );
                }
            }
        }

        if entity.timestamp >= adr_timestamps::ADR_232 {
            if let Some(metadata) = &entity.metadata {
                if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
                    for avatar in avatars {
                        if let Some(wearables) = avatar
                            .get("avatar")
                            .and_then(|a| a.get("wearables"))
                            .and_then(|w| w.as_array())
                        {
                            let unique: HashSet<&str> =
                                wearables.iter().filter_map(|v| v.as_str()).collect();
                            if unique.len() != wearables.len() {
                                return ValidationResponse::fail(
                                    "Wearables should not be repeated.".to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(metadata) = &entity.metadata {
            if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
                let mut used_slots = HashSet::new();
                for avatar in avatars {
                    if let Some(emotes) = avatar
                        .get("avatar")
                        .and_then(|a| a.get("emotes"))
                        .and_then(|e| e.as_array())
                    {
                        for emote in emotes {
                            if let Some(slot) = emote.get("slot").and_then(|s| s.as_i64()) {
                                if !used_slots.insert(slot) {
                                    return ValidationResponse::fail(format!(
                                        "Emote slot {slot} should not be repeated."
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        ValidationResponse::Ok
    }

    fn validate_scene_content(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;
        if entity.entity_type != EntityType::Scene {
            return ValidationResponse::Ok;
        }

        if entity.timestamp >= adr_timestamps::ADR_173 {
            if let Some(metadata) = &entity.metadata {
                if metadata.get("worldConfiguration").is_some() {
                    return ValidationResponse::fail(
                        "The scene.json contains a worldConfiguration section, which is \
                         not allowed for Genesis City scenes (see ADR-173: \
                         http://adr.decentraland.org/adr/ADR-173). Please remove it and \
                         try again."
                            .to_string(),
                    );
                }
            }
        }

        if entity.timestamp >= adr_timestamps::ADR_236 {
            if let Some(metadata) = &entity.metadata {
                if let Some(thumbnail) = metadata
                    .get("display")
                    .and_then(|d| d.get("navmapThumbnail"))
                    .and_then(|t| t.as_str())
                {
                    let is_present = entity.content.iter().any(|c| c.file == thumbnail);
                    if !is_present {
                        return ValidationResponse::fail(format!(
                            "Scene thumbnail '{thumbnail}' must be a file included in \
                             the deployment."
                        ));
                    }
                }
            }
        }

        ValidationResponse::Ok
    }

    async fn validate_content(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;
        let files = &deployment.files;
        let mut errors = Vec::new();

        if !entity.content.is_empty() {
            let hashes: Vec<String> = entity.content.iter().map(|c| c.hash.clone()).collect();
            let stored = self.external_calls.is_content_stored_already(&hashes).await;

            for cm in &entity.content {
                let is_uploaded = files.contains_key(&cm.hash);
                let is_stored = stored.get(&cm.hash).copied().unwrap_or(false);
                if !is_uploaded && !is_stored {
                    errors.push(format!(
                        "This hash is referenced in the entity but was not uploaded \
                         or previously available: {}",
                        cm.hash
                    ));
                }
            }
        }

        let entity_hashes: HashSet<&String> = entity.content.iter().map(|c| &c.hash).collect();
        for hash in files.keys() {
            if !entity_hashes.contains(hash) && *hash != entity.id {
                errors.push(format!(
                    "This hash was uploaded but is not referenced in the entity: {hash}"
                ));
            }
        }

        ValidationResponse::from_errors(errors)
    }

    fn validate_outfits_content(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        let entity = &deployment.entity;
        if entity.entity_type != EntityType::Outfits {
            return ValidationResponse::Ok;
        }

        let metadata = match &entity.metadata {
            Some(m) => m,
            None => return ValidationResponse::Ok,
        };

        if let Some(outfits) = metadata.get("outfits").and_then(|v| v.as_array()) {
            let mut used_slots = HashSet::new();
            for outfit in outfits {
                if let Some(slot) = outfit.get("slot").and_then(|s| s.as_i64()) {
                    if !(0..=9).contains(&slot) {
                        return ValidationResponse::fail(
                            "Outfits slots are invalid, they must be between 0 and 9 \
                             inclusive"
                                .to_string(),
                        );
                    }
                    if !used_slots.insert(slot) {
                        return ValidationResponse::fail("Outfits slots are repeated".to_string());
                    }
                }
            }

            let has_extra = outfits.iter().any(|o| {
                o.get("slot")
                    .and_then(|s| s.as_i64())
                    .map(|s| s > 4)
                    .unwrap_or(false)
            });
            if has_extra {
                let names = metadata
                    .get("namesForExtraSlots")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if names == 0 {
                    return ValidationResponse::fail(
                        "A name must be provided if extra slots are used, but none \
                         were provided."
                            .to_string(),
                    );
                }
            }
        }

        if entity.timestamp >= adr_timestamps::ADR_244 {
            let mut non_item_urns: Vec<String> = Vec::new();
            if let Some(outfits) = metadata.get("outfits").and_then(|v| v.as_array()) {
                for outfit in outfits {
                    if let Some(wearables) = outfit
                        .get("outfit")
                        .and_then(|o| o.get("wearables"))
                        .and_then(|w| w.as_array())
                    {
                        for w in wearables {
                            if let Some(urn) = w.as_str() {
                                let lower = urn.to_lowercase();
                                if (lower.contains("collections-v1:")
                                    || lower.contains("collections-v2:"))
                                    && count_urn_segments(&lower) <= 6
                                {
                                    non_item_urns.push(urn.to_string());
                                }
                            }
                        }
                    }
                }
            }
            if !non_item_urns.is_empty() {
                return ValidationResponse::fail(format!(
                    "Wearable pointers {} should be items, not assets. \
                     The URNs must include the tokenId.",
                    non_item_urns.join(", ")
                ));
            }
        }

        ValidationResponse::Ok
    }

    async fn validate_access(&self, deployment: &DeploymentToValidate) -> ValidationResponse {
        if self.ignore_blockchain_access {
            let deployer = self.external_calls.owner_address(&deployment.audit_info);
            let local = crate::checker::validate_local_pointer_ownership(deployment, &deployer);
            if !matches!(local, ValidationResponse::Ok) {
                return local;
            }
            warn!(
                entity_id = %deployment.entity.id,
                entity_type = %deployment.entity.entity_type,
                pointers = ?deployment.entity.pointers,
                "blockchain access checks bypassed for deployment \
                 — should only be enabled on sync-only nodes"
            );
            return ValidationResponse::Ok;
        }

        let entity = &deployment.entity;
        let deployer = self.external_calls.owner_address(&deployment.audit_info);

        if entity.timestamp <= adr_timestamps::LEGACY_CONTENT_MIGRATION
            && self
                .external_calls
                .is_address_owned_by_decentraland(&deployer)
        {
            return ValidationResponse::Ok;
        }

        if entity.entity_type == EntityType::Scene
            && entity.pointers.iter().any(|p| p.starts_with("default"))
        {
            return ValidationResponse::fail(
                "Scene pointers should only contain two integers separated by a comma, \
                 for example (10,10) or (120,-45)."
                    .to_string(),
            );
        }

        match entity.entity_type {
            EntityType::Scene => {
                validate_scene_access(deployment, &self.blockchain_checker, &deployer).await
            }
            EntityType::Profile => {
                validate_profile_access(deployment, &self.blockchain_checker, &deployer).await
            }
            EntityType::Store => validate_store_access(deployment, &deployer),
            EntityType::Wearable | EntityType::Emote => {
                validate_item_access(deployment, &self.blockchain_checker, &deployer).await
            }
            EntityType::Outfits => {
                validate_outfits_access(deployment, &self.blockchain_checker, &deployer).await
            }
        }
    }
}
