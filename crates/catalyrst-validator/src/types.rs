use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use catalyrst_types::{ContentMapping, EntityType};

pub type EntityId = String;

pub type Pointer = String;

pub type ContentFileHash = String;

pub type EthAddress = String;

pub type Timestamp = i64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    #[serde(default)]
    pub id: EntityId,

    #[serde(rename = "type")]
    pub entity_type: EntityType,

    pub pointers: Vec<Pointer>,

    pub timestamp: Timestamp,

    pub content: Vec<ContentMapping>,

    #[serde(default)]
    pub version: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthLink {
    #[serde(rename = "type")]
    pub link_type: String,
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

pub type AuthChain = Vec<AuthLink>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentAuditInfo {
    #[serde(rename = "authChain")]
    pub auth_chain: AuthChain,
}

#[derive(Debug, Clone)]
pub struct DeploymentToValidate {
    pub entity: Entity,
    pub files: HashMap<String, Vec<u8>>,
    pub audit_info: DeploymentAuditInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentContext {
    Local,
    Synced,
    SyncedLegacyEntity,
    FixAttempt,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockInformation {
    pub block_at_deployment: Option<u64>,
    pub block_five_min_before: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContractType {
    #[serde(rename = "erc721")]
    Erc721,
    #[serde(rename = "erc1155")]
    Erc1155,
    Unknown,
}

pub const L1_NETWORKS: &[&str] = &["mainnet", "kovan", "rinkeby", "goerli", "sepolia"];

pub const L2_NETWORKS: &[&str] = &["matic", "mumbai", "amoy"];

pub fn is_l1_network(network: &str) -> bool {
    L1_NETWORKS.contains(&network) || network == "ethereum"
}

pub fn is_l2_network(network: &str) -> bool {
    L2_NETWORKS.contains(&network)
}

pub fn max_size_mb(entity_type: EntityType) -> Option<u64> {
    match entity_type {
        EntityType::Scene => Some(15),
        EntityType::Profile => Some(2),
        EntityType::Wearable => Some(3),
        EntityType::Store => Some(1),
        EntityType::Emote => Some(3),
        EntityType::Outfits => Some(1),
    }
}

pub const SKIN_MAX_SIZE_MB: u64 = 9;

pub const THUMBNAIL_MAX_SIZE_MB: u64 = 1;

pub mod adr_timestamps {
    use super::Timestamp;

    pub const ADR_45: Timestamp = 1_652_191_200_000;

    pub const ADR_74: Timestamp = 1_662_987_600_000;

    pub const ADR_75: Timestamp = 1_658_275_200_000;

    pub const ADR_158: Timestamp = 1_674_576_000_000;

    pub const ADR_173: Timestamp = 1_673_967_600_000;

    pub const ADR_232: Timestamp = 1_686_571_200_000;

    pub const ADR_236: Timestamp = 1_684_497_600_000;

    pub const ADR_244: Timestamp = 1_710_428_400_000;

    pub const ADR_290_OPTIONAL: Timestamp = 1_762_743_600_000;

    pub const ADR_290_REJECTED: Timestamp = ADR_290_OPTIONAL + 3 * 30 * 24 * 60 * 60 * 1000;

    pub const LEGACY_CONTENT_MIGRATION: Timestamp = 1_582_167_600_000;
}
