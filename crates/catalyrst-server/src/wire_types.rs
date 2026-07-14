use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentContent {
    pub key: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditInfo {
    pub version: String,
    pub auth_chain: Value,
    pub local_timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerDeployment {
    pub entity_version: String,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_timestamp: i64,
    pub deployed_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<DeploymentContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_info: Option<AuditInfo>,
    pub local_timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPagination {
    pub offset: i64,
    pub limit: i64,
    pub more_data: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentsFilters {
    pub pointers: Vec<String>,
    pub entity_types: Vec<String>,
    pub entity_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_currently_pointed: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deployed_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentsResponse {
    pub deployments: Vec<ControllerDeployment>,
    pub filters: DeploymentsFilters,
    pub pagination: HistoryPagination,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerChangeDelta {
    pub deployment_id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub pointers: Vec<String>,
    pub entity_timestamp: i64,
    pub deployer_address: String,
    pub version: String,
    pub auth_chain: Value,
    pub local_timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerChangesFilters {
    pub entity_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    pub include_auth_chain: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PointerChangesResponse {
    pub deltas: Vec<PointerChangeDelta>,
    pub filters: PointerChangesFilters,
    pub pagination: HistoryPagination,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynchronizationStatus {
    pub last_sync_with_dao: i64,
    pub synchronization_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_frontier: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up: Option<bool>,
    #[serde(flatten)]
    pub cluster_extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentStatusResponse {
    pub version: String,
    pub commit_hash: String,
    pub eth_network: String,
    pub synchronization_status: SynchronizationStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeResponse {
    pub challenge_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvailableContentItem {
    pub cid: String,
    pub available: bool,
}
