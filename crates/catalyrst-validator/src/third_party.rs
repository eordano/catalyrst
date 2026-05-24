use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{ValidatorError, ValidationResponse};
use crate::types::*;

#[async_trait]
pub trait ThirdPartyContractRegistry: Send + Sync {
    fn is_erc721(&self, contract_address: &str) -> bool;

    fn is_erc1155(&self, contract_address: &str) -> bool;

    fn is_unknown(&self, contract_address: &str) -> bool;

    async fn ensure_contracts_known(
        &self,
        contract_addresses: &[String],
    ) -> Result<(), ValidatorError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerkleProof {
    pub proof: Vec<String>,
    pub index: u64,
    pub entity_hash: String,
    pub hashing_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThirdPartyProps {
    pub merkle_proof: MerkleProof,
    #[serde(default)]
    pub content: std::collections::HashMap<String, String>,
    pub id: String,
}

pub fn validate_third_party_merkle_proof_content(
    deployment: &DeploymentToValidate,
) -> ValidationResponse {
    let entity = &deployment.entity;
    let metadata = match &entity.metadata {
        Some(m) => m,
        None => return ValidationResponse::Ok,
    };

    if metadata.get("merkleProof").is_none() {
        return ValidationResponse::Ok;
    }

    let tp_props: ThirdPartyProps = match serde_json::from_value(metadata.clone()) {
        Ok(p) => p,
        Err(e) => {
            return ValidationResponse::fail(format!(
                "Failed to parse third-party metadata: {e}"
            ));
        }
    };

    if entity.pointers.is_empty()
        || tp_props.id.to_lowercase() != entity.pointers[0].to_lowercase()
    {
        return ValidationResponse::fail(format!(
            "The id '{}' does not match the pointer '{}'",
            tp_props.id,
            entity.pointers.first().map(|s| s.as_str()).unwrap_or("")
        ));
    }

    let all_content_in_files = tp_props.content.iter().all(|(file, hash)| {
        entity
            .content
            .iter()
            .any(|c| c.file == *file && c.hash == *hash)
    });

    let all_files_in_content = entity.content.iter().all(|c| {
        tp_props
            .content
            .get(&c.file)
            .map(|h| h == &c.hash)
            .unwrap_or(false)
    });

    if !all_content_in_files || !all_files_in_content {
        return ValidationResponse::fail(
            "The content declared in the metadata does not match the files uploaded with the entity"
                .to_string(),
        );
    }

    if tp_props.merkle_proof.entity_hash.is_empty() {
        return ValidationResponse::fail(
            "The entity hash in the merkle proof is empty".to_string(),
        );
    }

    if tp_props.merkle_proof.proof.is_empty() {
        return ValidationResponse::fail(
            "The merkle proof is empty".to_string(),
        );
    }

    if tp_props.merkle_proof.hashing_keys.is_empty() {
        return ValidationResponse::fail(
            "The hashing keys in the merkle proof are empty".to_string(),
        );
    }

    ValidationResponse::Ok
}

pub fn verify_third_party_merkle_proof(proof: &MerkleProof, root: &[u8; 32]) -> bool {
    let decoded: Option<Vec<[u8; 32]>> = proof
        .proof
        .iter()
        .map(|p| crate::merkle::decode_hash32(p))
        .collect();
    let Some(decoded) = decoded else {
        return false;
    };
    crate::merkle::verify_proof(proof.index, &proof.entity_hash, &decoded, root)
}

pub fn get_third_party_id(urn: &str) -> Option<String> {
    let parts: Vec<&str> = urn.split(':').collect();
    if parts.len() >= 5 && parts[3].starts_with("collections-thirdparty") {
        Some(parts[..5].join(":"))
    } else {
        None
    }
}

pub fn hex_to_bytes(value: &str) -> Option<Vec<u8>> {
    let hex_str = value.strip_prefix("0x").unwrap_or(value);
    hex_decode(hex_str)
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_party_id_extraction() {
        assert_eq!(
            get_third_party_id(
                "urn:decentraland:matic:collections-thirdparty:tp-name:collection:item"
            ),
            Some("urn:decentraland:matic:collections-thirdparty:tp-name".to_string())
        );
        assert_eq!(
            get_third_party_id("urn:decentraland:matic:collections-v2:0xabc:0"),
            None
        );
    }

    #[test]
    fn hex_conversion() {
        assert_eq!(
            hex_to_bytes("0xdeadbeef"),
            Some(vec![0xde, 0xad, 0xbe, 0xef])
        );
        assert_eq!(
            hex_to_bytes("deadbeef"),
            Some(vec![0xde, 0xad, 0xbe, 0xef])
        );
        assert_eq!(hex_to_bytes("0x"), Some(vec![]));
    }

    #[test]
    fn validate_non_third_party_passes() {
        let deployment = DeploymentToValidate {
            entity: Entity {
                id: "bafkrei".to_string(),
                entity_type: EntityType::Wearable,
                pointers: vec!["urn:decentraland:matic:collections-v2:0xabc:0".to_string()],
                timestamp: 1700000000000,
                content: vec![],
                version: "v3".to_string(),
                metadata: Some(serde_json::json!({
                    "name": "Regular Wearable",
                    "data": { "representations": [], "tags": [], "category": "hat" }
                })),
            },
            files: std::collections::HashMap::new(),
            audit_info: DeploymentAuditInfo {
                auth_chain: vec![],
            },
        };

        assert!(validate_third_party_merkle_proof_content(&deployment).is_ok());
    }
}
