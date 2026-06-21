use async_trait::async_trait;

use crate::error::{PermissionResult, ValidationResponse};
use crate::types::*;

#[async_trait]
pub trait L1Checker: Send + Sync {
    async fn check_land(
        &self,
        eth_address: &str,
        parcels: &[(i32, i32)],
        block: u64,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;

    async fn check_names(
        &self,
        eth_address: &str,
        names: &[String],
        block: u64,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;
}

#[async_trait]
pub trait L2Checker: Send + Sync {
    async fn validate_wearables(
        &self,
        eth_address: &str,
        contract_address: &str,
        asset_id: &str,
        hashes: &[String],
        block: u64,
    ) -> Result<bool, crate::error::ValidatorError>;

    async fn validate_third_party(
        &self,
        tp_id: &str,
        root: &[u8],
        block: u64,
    ) -> Result<bool, crate::error::ValidatorError>;
}

#[async_trait]
pub trait ItemChecker: Send + Sync {
    async fn check_items(
        &self,
        eth_address: &str,
        items: &[String],
        block: u64,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;
}

#[async_trait]
pub trait ThirdPartyItemChecker: Send + Sync {
    async fn check_third_party_items(
        &self,
        eth_address: &str,
        item_urns: &[String],
        block: u64,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;
}

#[async_trait]
pub trait NamesOwnership: Send + Sync {
    async fn owns_names_at_timestamp(
        &self,
        eth_address: &str,
        names: &[String],
        timestamp: Timestamp,
    ) -> Result<PermissionResult, crate::error::ValidatorError>;
}

#[async_trait]
pub trait ItemsOwnership: Send + Sync {
    async fn owns_items_at_timestamp(
        &self,
        eth_address: &str,
        urns: &[String],
        timestamp: Timestamp,
    ) -> Result<PermissionResult, crate::error::ValidatorError>;
}

#[async_trait]
pub trait BlockchainChecker: Send + Sync {
    async fn find_blocks_for_timestamp(
        &self,
        timestamp: Timestamp,
        layer: BlockchainLayer,
    ) -> Result<BlockInformation, crate::error::ValidatorError>;

    async fn check_land_access(
        &self,
        eth_address: &str,
        parcels: &[(i32, i32)],
        timestamp: Timestamp,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;

    async fn check_names_ownership(
        &self,
        eth_address: &str,
        names: &[String],
        timestamp: Timestamp,
    ) -> Result<PermissionResult, crate::error::ValidatorError>;

    async fn check_items_ownership(
        &self,
        eth_address: &str,
        urns: &[String],
        timestamp: Timestamp,
    ) -> Result<PermissionResult, crate::error::ValidatorError>;

    async fn check_collection_access(
        &self,
        eth_address: &str,
        contract_address: &str,
        item_id: &str,
        entity: &Entity,
        timestamp: Timestamp,
        layer: BlockchainLayer,
    ) -> Result<bool, crate::error::ValidatorError>;

    async fn check_third_party_access(
        &self,
        asset_urn: &str,
        entity: &Entity,
        deployment: &DeploymentToValidate,
        timestamp: Timestamp,
    ) -> Result<bool, crate::error::ValidatorError>;

    async fn check_third_party_items(
        &self,
        eth_address: &str,
        item_urns: &[String],
        block: u64,
    ) -> Result<Vec<bool>, crate::error::ValidatorError>;

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockchainLayer {
    L1,
    L2,
}

pub async fn validate_scene_access(
    deployment: &DeploymentToValidate,
    checker: &dyn BlockchainChecker,
    deployer_address: &str,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let pointers = &entity.pointers;
    let timestamp = entity.timestamp;

    let mut errors = Vec::new();

    let mut parcels: Vec<(i32, i32)> = Vec::new();
    for pointer in pointers {
        let parts: Vec<&str> = pointer.split(',').collect();
        if parts.len() != 2 {
            return ValidationResponse::fail(format!(
                "Scene pointers should only contain two integers separated by a comma, \
                 for example (10,10) or (120,-45). Invalid pointer: {pointer}"
            ));
        }
        let x = parts[0].trim().parse::<i32>();
        let y = parts[1].trim().parse::<i32>();
        match (x, y) {
            (Ok(x), Ok(y)) => parcels.push((x, y)),
            _ => {
                return ValidationResponse::fail(format!(
                    "Scene pointers should only contain two integers separated by a comma, \
                     for example (10,10) or (120,-45). Invalid pointer: {pointer}"
                ));
            }
        }
    }

    match checker
        .check_land_access(deployer_address, &parcels, timestamp)
        .await
    {
        Ok(results) => {
            for (i, has_access) in results.iter().enumerate() {
                if !*has_access {
                    let (x, y) = parcels[i];
                    errors.push(format!(
                        "The provided Eth Address does not have access to the \
                         following parcel: ({x},{y})"
                    ));
                }
            }
        }
        Err(e) => {
            errors.push(format!("Error checking parcel access: {e}"));
        }
    }

    ValidationResponse::from_errors(errors)
}

pub async fn validate_profile_access(
    deployment: &DeploymentToValidate,
    checker: &dyn BlockchainChecker,
    deployer_address: &str,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let pointers = &entity.pointers;

    if pointers.len() != 1 {
        return ValidationResponse::fail(format!(
            "Only one pointer is allowed when you create a Profile. Received: {pointers:?}"
        ));
    }

    let pointer = pointers[0].to_lowercase();

    if pointer.starts_with("default") {
        if !checker.is_address_owned_by_decentraland(deployer_address) {
            return ValidationResponse::fail(
                "Only Decentraland can add or modify default profiles".to_string(),
            );
        }
        return ValidationResponse::Ok;
    }

    if !is_valid_eth_address(&pointer) {
        return ValidationResponse::fail(
            "The given pointer is not a valid ethereum address.".to_string(),
        );
    }

    if pointer != deployer_address.to_lowercase() {
        return ValidationResponse::fail(format!(
            "You can only alter your own profile. The pointer address and the signer address \
             are different (pointer:{pointer} signer: {})",
            deployer_address.to_lowercase()
        ));
    }

    if entity.timestamp >= adr_timestamps::ADR_75 {
        if let Some(metadata) = &entity.metadata {
            let names = extract_claimed_names(metadata);
            if !names.is_empty() {
                match checker
                    .check_names_ownership(deployer_address, &names, entity.timestamp)
                    .await
                {
                    Ok(result) => {
                        if !result.result {
                            let failing = result
                                .failing
                                .as_deref()
                                .map(|f| f.join(", "))
                                .unwrap_or_default();
                            return ValidationResponse::fail(format!(
                                "The following names ({failing}) are not owned by \
                                 the address {}.",
                                deployer_address.to_lowercase()
                            ));
                        }
                    }
                    Err(e) => {
                        return ValidationResponse::fail(format!(
                            "Error checking name ownership: {e}"
                        ));
                    }
                }
            }
        }
    }

    if let Some(metadata) = &entity.metadata {
        let item_urns = extract_profile_item_urns(metadata, entity.timestamp);
        if !item_urns.is_empty() {
            match checker
                .check_items_ownership(deployer_address, &item_urns, entity.timestamp)
                .await
            {
                Ok(result) => {
                    if !result.result {
                        let failing = result
                            .failing
                            .as_deref()
                            .map(|f| f.join(", "))
                            .unwrap_or_default();
                        return ValidationResponse::fail(format!(
                            "The following items ({failing}) are not owned by \
                             the address {}.",
                            deployer_address.to_lowercase()
                        ));
                    }
                }
                Err(e) => {
                    return ValidationResponse::fail(format!("Error checking item ownership: {e}"));
                }
            }
        }
    }

    ValidationResponse::Ok
}

/// Pure-local pointer ownership: the `pointer == signer` gate that does NOT
/// touch the blockchain. Must run even when `ignore_blockchain_access` is set —
/// that flag is for sync-only nodes and is meant to skip chain queries (LAND,
/// collection, name ownership), NOT to let any wallet overwrite another user's
/// profile/store/outfits. Returns Ok for types whose authorization is inherently
/// chain-based (scene/wearable/emote have nothing local to assert here).
pub fn validate_local_pointer_ownership(
    deployment: &DeploymentToValidate,
    deployer_address: &str,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let deployer = deployer_address.to_lowercase();
    match entity.entity_type {
        EntityType::Profile => {
            // default* profiles are gated separately (DCL allowlist) and only
            // reachable on the full-validation path; leave them to it.
            if let Some(p) = entity.pointers.first() {
                let p = p.to_lowercase();
                if !p.starts_with("default") && is_valid_eth_address(&p) && p != deployer {
                    return ValidationResponse::fail(format!(
                        "You can only alter your own profile. The pointer address and the \
                         signer address are different (pointer:{p} signer: {deployer})"
                    ));
                }
            }
            ValidationResponse::Ok
        }
        EntityType::Store => validate_store_access(deployment, deployer_address),
        EntityType::Outfits => {
            if let Some(p) = entity.pointers.first() {
                let p = p.to_lowercase();
                let addr = p.split(':').next().unwrap_or("");
                if is_valid_eth_address(addr) && addr != deployer {
                    return ValidationResponse::fail(format!(
                        "You can only alter your own outfits. The address of the pointer and \
                         the signer address are different (pointer:{p} signer: {deployer})."
                    ));
                }
            }
            ValidationResponse::Ok
        }
        _ => ValidationResponse::Ok,
    }
}

pub fn validate_store_access(
    deployment: &DeploymentToValidate,
    deployer_address: &str,
) -> ValidationResponse {
    let pointers = &deployment.entity.pointers;

    if pointers.len() != 1 {
        return ValidationResponse::fail(format!(
            "Only one pointer is allowed when you create a Store. Received: {pointers:?}"
        ));
    }

    let pointer = pointers[0].to_lowercase();

    if !pointer.starts_with("urn:decentraland:off-chain:marketplace-stores:") {
        return ValidationResponse::fail(format!(
            "Store pointers should be a urn, for example \
             (urn:decentraland:off-chain:marketplace-stores:{{address}}). \
             Invalid pointer: {pointer}"
        ));
    }

    let address_part = pointer
        .strip_prefix("urn:decentraland:off-chain:marketplace-stores:")
        .unwrap_or("");

    if address_part.to_lowercase() != deployer_address.to_lowercase() {
        return ValidationResponse::fail(format!(
            "You can only alter your own store. The pointer address and the signer address \
             are different (address:{} signer: {}).",
            address_part.to_lowercase(),
            deployer_address.to_lowercase()
        ));
    }

    ValidationResponse::Ok
}

pub async fn validate_outfits_access(
    deployment: &DeploymentToValidate,
    checker: &dyn BlockchainChecker,
    deployer_address: &str,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let pointers = &entity.pointers;

    if pointers.len() != 1 {
        return ValidationResponse::fail(format!(
            "Only one pointer is allowed when you create an Outfits. Received: {pointers:?}"
        ));
    }

    let pointer = pointers[0].to_lowercase();
    let parts: Vec<&str> = pointer.split(':').collect();

    if parts.len() != 2 {
        return ValidationResponse::fail(
            "The pointer is not valid. It should be in the format: <address>:outfits".to_string(),
        );
    }

    if parts[1] != "outfits" {
        return ValidationResponse::fail(
            "The pointer is not valid. It should be in the format: <address>:outfits".to_string(),
        );
    }

    let pointer_address = parts[0];
    if !is_valid_eth_address(pointer_address) {
        return ValidationResponse::fail(
            "The address of the given pointer is not a valid ethereum address.".to_string(),
        );
    }

    if pointer_address != deployer_address.to_lowercase() {
        return ValidationResponse::fail(format!(
            "You can only alter your own outfits. The address of the pointer and the signer \
             address are different (pointer:{pointer} signer: {}).",
            deployer_address.to_lowercase()
        ));
    }

    if let Some(metadata) = &entity.metadata {
        let wearable_urns = extract_outfit_wearable_urns(metadata);
        if !wearable_urns.is_empty() {
            match checker
                .check_items_ownership(deployer_address, &wearable_urns, entity.timestamp)
                .await
            {
                Ok(result) => {
                    if !result.result {
                        let failing = result
                            .failing
                            .as_deref()
                            .map(|f| f.join(", "))
                            .unwrap_or_default();
                        return ValidationResponse::fail(format!(
                            "The following wearables ({failing}) are not owned by \
                             the address {}.",
                            deployer_address.to_lowercase()
                        ));
                    }
                }
                Err(e) => {
                    return ValidationResponse::fail(format!(
                        "Error checking wearable ownership: {e}"
                    ));
                }
            }
        }

        let names = extract_outfit_names_for_extra_slots(metadata);
        if !names.is_empty() {
            match checker
                .check_names_ownership(deployer_address, &names, entity.timestamp)
                .await
            {
                Ok(result) => {
                    if !result.result {
                        let failing = result
                            .failing
                            .as_deref()
                            .map(|f| f.join(", "))
                            .unwrap_or_default();
                        return ValidationResponse::fail(format!(
                            "The following names ({failing}) are not owned by \
                             the address {}.",
                            deployer_address.to_lowercase()
                        ));
                    }
                }
                Err(e) => {
                    return ValidationResponse::fail(format!("Error checking name ownership: {e}"));
                }
            }
        }
    }

    ValidationResponse::Ok
}

pub async fn validate_item_access(
    deployment: &DeploymentToValidate,
    checker: &dyn BlockchainChecker,
    deployer_address: &str,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let pointers = &entity.pointers;

    if pointers.len() != 1 {
        return ValidationResponse::fail(format!(
            "Only one pointer is allowed when you create an item. Received: {pointers:?}"
        ));
    }

    let pointer = &pointers[0];

    let urn_type = classify_item_urn(pointer);
    match urn_type {
        ItemUrnType::Invalid => {
            ValidationResponse::fail(format!(
                "Item pointers should be a urn, for example \
                 (urn:decentraland:{{protocol}}:collections-v2:{{contract(0x[a-fA-F0-9]+)}}:{{id}}). \
                 Invalid pointer: ({pointer})"
            ))
        }
        ItemUrnType::OffChain => {
            if !checker.is_address_owned_by_decentraland(deployer_address) {
                return ValidationResponse::fail(format!(
                    "The provided Eth Address '{deployer_address}' does not have access \
                     to the following item: '{pointer}'"
                ));
            }
            ValidationResponse::Ok
        }
        ItemUrnType::CollectionV1 | ItemUrnType::CollectionV2 => {
            if let Some((contract, item_id, layer)) = parse_collection_urn(pointer) {
                let has_access = match checker
                    .check_collection_access(
                        deployer_address,
                        &contract,
                        &item_id,
                        entity,
                        entity.timestamp,
                        layer,
                    )
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        return ValidationResponse::fail(format!(
                            "Error checking collection access: {e}"
                        ));
                    }
                };

                if !has_access {
                    if layer == BlockchainLayer::L1
                        && checker.is_address_owned_by_decentraland(deployer_address)
                        && pointer.contains("collections-v1")
                    {
                        return ValidationResponse::Ok;
                    }
                    return ValidationResponse::fail(format!(
                        "The provided Eth Address '{deployer_address}' does not have access \
                         to the following item: ({contract}, {item_id})"
                    ));
                }
                return ValidationResponse::Ok;
            }

            ValidationResponse::fail(format!(
                "Could not parse collection URN: {pointer}"
            ))
        }
        ItemUrnType::ThirdParty => {
            let verified = match checker
                .check_third_party_access(pointer, entity, deployment, entity.timestamp)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    return ValidationResponse::fail(format!(
                        "Error checking third-party access: {e}"
                    ));
                }
            };

            if !verified {
                return ValidationResponse::fail(
                    "Couldn't verify merkle proofed entity for third-party wearable".to_string(),
                );
            }
            ValidationResponse::Ok
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemUrnType {
    Invalid,
    OffChain,
    CollectionV1,
    CollectionV2,
    ThirdParty,
}

pub fn classify_item_urn(urn: &str) -> ItemUrnType {
    let lower = urn.to_lowercase();
    if lower.starts_with("urn:decentraland:off-chain:") {
        ItemUrnType::OffChain
    } else if lower.contains(":collections-v1:") {
        ItemUrnType::CollectionV1
    } else if lower.contains(":collections-v2:") {
        ItemUrnType::CollectionV2
    } else if lower.contains(":collections-thirdparty:") {
        ItemUrnType::ThirdParty
    } else {
        ItemUrnType::Invalid
    }
}

fn parse_collection_urn(urn: &str) -> Option<(String, String, BlockchainLayer)> {
    let lower = urn.to_lowercase();
    let parts: Vec<&str> = lower.split(':').collect();

    if parts.len() < 6 {
        return None;
    }

    let network = parts[2];
    let contract = parts[4].to_string();
    let item_id = parts[5..].join(":");

    let layer = if is_l1_network(network) || network == "ethereum" {
        BlockchainLayer::L1
    } else if is_l2_network(network) {
        BlockchainLayer::L2
    } else {
        return None;
    };

    Some((contract, item_id, layer))
}

fn is_old_emote(s: &str) -> bool {
    s.len() <= 20 && s.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_valid_eth_address(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn extract_claimed_names(metadata: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
        for avatar in avatars {
            if avatar.get("hasClaimedName").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(name) = avatar.get("name").and_then(|v| v.as_str()) {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        names.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    names
}

fn extract_profile_item_urns(metadata: &serde_json::Value, timestamp: Timestamp) -> Vec<String> {
    let mut urns = Vec::new();
    if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
        for avatar in avatars {
            if timestamp >= adr_timestamps::ADR_75 {
                if let Some(wearables) = avatar
                    .get("avatar")
                    .and_then(|a| a.get("wearables"))
                    .and_then(|w| w.as_array())
                {
                    for w in wearables {
                        if let Some(urn) = w.as_str() {
                            if !urn.contains("base-avatars") && !is_old_emote(urn) {
                                urns.push(urn.to_string());
                            }
                        }
                    }
                }
            }

            if timestamp >= adr_timestamps::ADR_74 {
                if let Some(emotes) = avatar
                    .get("avatar")
                    .and_then(|a| a.get("emotes"))
                    .and_then(|e| e.as_array())
                {
                    for emote in emotes {
                        if let Some(urn) = emote.get("urn").and_then(|u| u.as_str()) {
                            if !is_old_emote(urn) {
                                urns.push(urn.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    urns
}

fn extract_outfit_wearable_urns(metadata: &serde_json::Value) -> Vec<String> {
    let mut urns = Vec::new();
    if let Some(outfits) = metadata.get("outfits").and_then(|v| v.as_array()) {
        for outfit in outfits {
            if let Some(wearables) = outfit
                .get("outfit")
                .and_then(|o| o.get("wearables"))
                .and_then(|w| w.as_array())
            {
                for w in wearables {
                    if let Some(urn) = w.as_str() {
                        urns.push(urn.to_string());
                    }
                }
            }
        }
    }
    urns
}

fn extract_outfit_names_for_extra_slots(metadata: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(arr) = metadata
        .get("namesForExtraSlots")
        .and_then(|v| v.as_array())
    {
        for name_val in arr {
            if let Some(name) = name_val.as_str() {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    names.push(trimmed.to_string());
                }
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ownership_deployment(etype: EntityType, pointer: &str) -> DeploymentToValidate {
        DeploymentToValidate {
            entity: crate::types::Entity {
                id: "bafkreitest".into(),
                entity_type: etype,
                pointers: vec![pointer.to_string()],
                timestamp: 1_700_000_000_000,
                content: vec![],
                version: "v3".into(),
                metadata: None,
            },
            files: std::collections::HashMap::new(),
            audit_info: crate::types::DeploymentAuditInfo { auth_chain: vec![] },
        }
    }

    // Regression: ignore_blockchain_access must NOT disable the pure-local
    // pointer==signer gate (found 2026-06-11 — any wallet could deploy a
    // profile/store/outfits to another user's pointer).
    #[test]
    fn local_ownership_rejects_foreign_pointer() {
        let me = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let other = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        for d in [
            ownership_deployment(EntityType::Profile, other),
            ownership_deployment(EntityType::Outfits, &format!("{other}:outfits")),
            ownership_deployment(
                EntityType::Store,
                &format!("urn:decentraland:off-chain:marketplace-stores:{other}"),
            ),
        ] {
            assert!(
                !matches!(
                    validate_local_pointer_ownership(&d, me),
                    ValidationResponse::Ok
                ),
                "foreign {:?} pointer must be rejected even with chain checks off",
                d.entity.entity_type
            );
        }
    }

    #[test]
    fn local_ownership_allows_own_pointer() {
        let me = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for d in [
            ownership_deployment(EntityType::Profile, me),
            ownership_deployment(EntityType::Outfits, &format!("{me}:outfits")),
            ownership_deployment(
                EntityType::Store,
                &format!("urn:decentraland:off-chain:marketplace-stores:{me}"),
            ),
            // scenes/wearables have no local pointer to assert — must pass through.
            ownership_deployment(EntityType::Scene, "10,10"),
        ] {
            assert!(
                matches!(
                    validate_local_pointer_ownership(&d, me),
                    ValidationResponse::Ok
                ),
                "own/non-local {:?} pointer must pass the local gate",
                d.entity.entity_type
            );
        }
    }

    #[test]
    fn classify_collection_urns() {
        assert_eq!(
            classify_item_urn("urn:decentraland:off-chain:base-avatars:f_hat"),
            ItemUrnType::OffChain
        );
        assert_eq!(
            classify_item_urn(
                "urn:decentraland:ethereum:collections-v1:community_contest:cw_bell_attendant_hat"
            ),
            ItemUrnType::CollectionV1
        );
        assert_eq!(
            classify_item_urn("urn:decentraland:matic:collections-v2:0xabc123:0"),
            ItemUrnType::CollectionV2
        );
        assert_eq!(
            classify_item_urn(
                "urn:decentraland:matic:collections-thirdparty:tp-name:collection:item"
            ),
            ItemUrnType::ThirdParty
        );
        assert_eq!(classify_item_urn("not-a-urn"), ItemUrnType::Invalid);
    }

    #[test]
    fn old_emote_detection() {
        assert!(is_old_emote("dance"));
        assert!(is_old_emote("wave"));
        assert!(!is_old_emote("urn:decentraland:something"));
        assert!(!is_old_emote("has spaces"));
        assert!(!is_old_emote("aVeryLongEmoteNameThatExceedsTwentyChars"));
    }

    #[test]
    fn eth_address_validation() {
        assert!(is_valid_eth_address(
            "0x1234567890abcdef1234567890abcdef12345678"
        ));
        assert!(!is_valid_eth_address("0x123"));
        assert!(!is_valid_eth_address(
            "1234567890abcdef1234567890abcdef12345678"
        ));
    }

    #[test]
    fn extract_names_from_profile() {
        let metadata = serde_json::json!({
            "avatars": [
                {
                    "name": "TestName",
                    "hasClaimedName": true,
                    "avatar": { "wearables": [] }
                },
                {
                    "name": "Unclaimed",
                    "hasClaimedName": false,
                    "avatar": { "wearables": [] }
                }
            ]
        });
        let names = extract_claimed_names(&metadata);
        assert_eq!(names, vec!["TestName"]);
    }
}
