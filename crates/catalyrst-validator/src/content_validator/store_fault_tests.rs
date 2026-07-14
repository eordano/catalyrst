use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::checker::BlockchainLayer;
use crate::error::{PermissionResult, ValidatorError};

const FAULT: &str = "I/O error: Permission denied (os error 13)";

const ENTITY_HASH: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";
const CONTENT_HASH: &str = "bafkreibpxmt5fzrpsvhfvzqdmnbdgnu4cimvqiuq2jwuqumcxfxx7hzyvi";

#[derive(Default)]
struct StubExternalCalls {
    stored: HashMap<String, bool>,
    sizes: HashMap<String, usize>,
    fault_exist: bool,
    fault_size: bool,
    exist_queries: Mutex<Vec<Vec<String>>>,
}

impl StubExternalCalls {
    fn faulting_exist() -> Self {
        Self {
            fault_exist: true,
            ..Default::default()
        }
    }

    fn with_stored(hash: &str, stored: bool) -> Self {
        Self {
            stored: HashMap::from([(hash.to_string(), stored)]),
            ..Default::default()
        }
    }
}

#[async_trait]
impl ExternalCalls for StubExternalCalls {
    async fn is_content_stored_already(
        &self,
        hashes: &[String],
    ) -> Result<HashMap<String, bool>, String> {
        self.exist_queries.lock().unwrap().push(hashes.to_vec());
        if self.fault_exist {
            return Err(FAULT.to_string());
        }
        Ok(hashes
            .iter()
            .map(|h| (h.clone(), self.stored.get(h).copied().unwrap_or(false)))
            .collect())
    }

    async fn fetch_content_file_size(&self, hash: &str) -> Result<Option<usize>, String> {
        if self.fault_size {
            return Err(FAULT.to_string());
        }
        Ok(self.sizes.get(hash).copied())
    }

    async fn validate_signature(
        &self,
        _entity_id: &str,
        _audit_info: &DeploymentAuditInfo,
        _timestamp: Timestamp,
    ) -> Result<(), String> {
        Ok(())
    }

    fn owner_address(&self, _audit_info: &DeploymentAuditInfo) -> String {
        String::new()
    }

    fn is_address_owned_by_decentraland(&self, _address: &str) -> bool {
        false
    }

    async fn calculate_files_hashes(
        &self,
        _files: &HashMap<String, Vec<u8>>,
    ) -> HashMap<String, CalculatedHash> {
        HashMap::new()
    }
}

struct StubChecker;

#[async_trait]
impl BlockchainChecker for StubChecker {
    async fn find_blocks_for_timestamp(
        &self,
        _timestamp: Timestamp,
        _layer: BlockchainLayer,
    ) -> Result<BlockInformation, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_land_access(
        &self,
        _eth_address: &str,
        _parcels: &[(i32, i32)],
        _timestamp: Timestamp,
    ) -> Result<Vec<bool>, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_names_ownership(
        &self,
        _eth_address: &str,
        _names: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_items_ownership(
        &self,
        _eth_address: &str,
        _urns: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_collection_access(
        &self,
        _eth_address: &str,
        _contract_address: &str,
        _item_id: &str,
        _entity: &Entity,
        _timestamp: Timestamp,
        _layer: BlockchainLayer,
    ) -> Result<bool, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_third_party_access(
        &self,
        _asset_urn: &str,
        _entity: &Entity,
        _deployment: &DeploymentToValidate,
        _timestamp: Timestamp,
    ) -> Result<bool, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    async fn check_third_party_items(
        &self,
        _eth_address: &str,
        _item_urns: &[String],
        _block: u64,
    ) -> Result<Vec<bool>, ValidatorError> {
        unreachable!("access checks are not exercised by these tests")
    }

    fn is_address_owned_by_decentraland(&self, _address: &str) -> bool {
        false
    }
}

fn validator(external: StubExternalCalls) -> ContentValidator<StubExternalCalls, StubChecker> {
    ContentValidator::new(external, StubChecker, true)
}

fn deployment(
    entity_type: EntityType,
    content: Vec<ContentMapping>,
    files: HashMap<String, Vec<u8>>,
    metadata: Option<serde_json::Value>,
) -> DeploymentToValidate {
    DeploymentToValidate {
        entity: Entity {
            id: ENTITY_HASH.to_string(),
            entity_type,
            pointers: vec!["0,0".to_string()],
            timestamp: adr_timestamps::ADR_244,
            content,
            version: "v3".to_string(),
            metadata,
        },
        files,
        audit_info: DeploymentAuditInfo {
            auth_chain: Vec::new(),
        },
    }
}

fn mapping(file: &str, hash: &str) -> ContentMapping {
    ContentMapping {
        file: file.to_string(),
        hash: hash.to_string(),
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    buf.extend_from_slice(&13u32.to_be_bytes());
    buf.extend_from_slice(b"IHDR");
    buf.extend_from_slice(&width.to_be_bytes());
    buf.extend_from_slice(&height.to_be_bytes());
    buf.extend_from_slice(&[8, 6, 0, 0, 0]);
    buf.extend_from_slice(&[0, 0, 0, 0]);
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(b"IEND");
    buf.extend_from_slice(&[0, 0, 0, 0]);
    buf
}

fn face_metadata(hash: &str) -> serde_json::Value {
    serde_json::json!({
        "avatars": [{ "avatar": { "snapshots": { "face256": hash } } }]
    })
}

#[tokio::test]
async fn an_unreadable_store_never_reports_referenced_content_as_missing() {
    let v = validator(StubExternalCalls::faulting_exist());
    let d = deployment(
        EntityType::Scene,
        vec![mapping("scene.glb", CONTENT_HASH)],
        HashMap::new(),
        None,
    );

    let result = v.validate_content(&d).await;

    assert!(
        result.is_unavailable(),
        "a store fault must not become a rejection: {result}"
    );
    let errors = result.errors().unwrap();
    assert!(
        errors[0].contains("could not read its own content store") && errors[0].contains(FAULT),
        "unexpected error: {}",
        errors[0]
    );
    assert!(
        !errors[0].contains("was not uploaded or previously available"),
        "a fault must never be phrased as the depositor's missing upload"
    );
}

#[tokio::test]
async fn a_provable_miss_is_still_the_depositors_rejection() {
    let v = validator(StubExternalCalls::with_stored(CONTENT_HASH, false));
    let d = deployment(
        EntityType::Scene,
        vec![mapping("scene.glb", CONTENT_HASH)],
        HashMap::new(),
        None,
    );

    let result = v.validate_content(&d).await;

    assert!(!result.is_unavailable(), "a plain miss is not a node fault");
    assert!(result.errors().unwrap()[0].contains("was not uploaded or previously available"));
}

#[tokio::test]
async fn content_that_came_with_the_deployment_is_never_probed() {
    let external = StubExternalCalls::faulting_exist();
    let v = validator(external);
    let d = deployment(
        EntityType::Scene,
        vec![mapping("scene.glb", CONTENT_HASH)],
        HashMap::from([(CONTENT_HASH.to_string(), b"bytes".to_vec())]),
        None,
    );

    let result = v.validate_content(&d).await;

    assert!(
        result.is_ok(),
        "an uploaded file makes the store's answer irrelevant, so damage elsewhere must not \
         block the deployment: {result}"
    );
    assert!(
        v.external_calls.exist_queries.lock().unwrap().is_empty(),
        "no probe is owed for a hash whose bytes are in hand"
    );
}

#[tokio::test]
async fn only_the_hashes_whose_answer_decides_the_verdict_are_probed() {
    let other_hash = "bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy";
    let v = validator(StubExternalCalls::with_stored(other_hash, true));
    let d = deployment(
        EntityType::Scene,
        vec![
            mapping("scene.glb", CONTENT_HASH),
            mapping("extra.glb", other_hash),
            mapping("alias.glb", other_hash),
        ],
        HashMap::from([(CONTENT_HASH.to_string(), b"bytes".to_vec())]),
        None,
    );

    let result = v.validate_content(&d).await;

    assert!(result.is_ok(), "{result}");
    let queries = v.external_calls.exist_queries.lock().unwrap().clone();
    assert_eq!(queries, vec![vec![other_hash.to_string()]]);
}

#[tokio::test]
async fn an_unreadable_store_never_reports_a_wearable_thumbnail_as_missing() {
    let v = validator(StubExternalCalls::faulting_exist());
    let d = deployment(
        EntityType::Wearable,
        vec![mapping("thumbnail.png", CONTENT_HASH)],
        HashMap::new(),
        Some(serde_json::json!({ "thumbnail": "thumbnail.png" })),
    );

    let result = v.validate_item_thumbnail(&d).await;

    assert!(result.is_unavailable(), "{result}");
    assert!(!result
        .errors()
        .unwrap()
        .iter()
        .any(|e| e.contains("Couldn't find thumbnail file with hash")));
}

#[tokio::test]
async fn an_unreadable_store_never_reports_a_face_thumbnail_as_missing() {
    let v = validator(StubExternalCalls::faulting_exist());
    let d = deployment(
        EntityType::Profile,
        vec![mapping("face256.png", CONTENT_HASH)],
        HashMap::new(),
        Some(face_metadata(CONTENT_HASH)),
    );

    let result = v.validate_face_thumbnail(&d).await;

    assert!(result.is_unavailable(), "{result}");
}

#[tokio::test]
async fn a_face_thumbnail_in_hand_is_checked_rather_than_blocked_by_a_store_fault() {
    let v = validator(StubExternalCalls::faulting_exist());
    let d = deployment(
        EntityType::Profile,
        vec![mapping("face256.png", CONTENT_HASH)],
        HashMap::from([(CONTENT_HASH.to_string(), png(64, 64))]),
        Some(face_metadata(CONTENT_HASH)),
    );

    let result = v.validate_face_thumbnail(&d).await;

    assert!(
        !result.is_unavailable(),
        "the probe only decides whether re-checking may be skipped, so a fault must not block \
         a thumbnail whose bytes are in hand: {result}"
    );
    assert_eq!(
        result.errors().unwrap()[0],
        "Invalid face256 thumbnail image size (width = 64 / height = 64)"
    );
}

#[tokio::test]
async fn an_unreadable_store_never_reports_a_referenced_size_as_unfetchable() {
    let external = StubExternalCalls {
        fault_size: true,
        ..Default::default()
    };
    let v = validator(external);
    let d = deployment(
        EntityType::Scene,
        vec![mapping("scene.glb", CONTENT_HASH)],
        HashMap::new(),
        None,
    );

    let result = v.validate_size(&d).await;

    assert!(result.is_unavailable(), "{result}");
    assert!(!result
        .errors()
        .unwrap()
        .iter()
        .any(|e| e.contains("Couldn't fetch content file with hash")));
}

#[tokio::test]
async fn a_size_the_store_provably_lacks_is_still_the_depositors_rejection() {
    let v = validator(StubExternalCalls::default());
    let d = deployment(
        EntityType::Scene,
        vec![mapping("scene.glb", CONTENT_HASH)],
        HashMap::new(),
        None,
    );

    let result = v.validate_size(&d).await;

    assert!(!result.is_unavailable(), "a plain miss is not a node fault");
    assert!(result.errors().unwrap()[0].contains("Couldn't fetch content file with hash"));
}
