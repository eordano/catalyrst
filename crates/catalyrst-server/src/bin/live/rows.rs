use super::*;

#[allow(dead_code)]
#[derive(Serialize)]
struct ContentEntry<'a> {
    key: &'a str,
    hash: &'a str,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct AuditInfoResponse<'a> {
    version: &'a str,
    #[serde(rename = "authChain")]
    auth_chain: &'a Value,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(rename = "overwrittenBy")]
    overwritten_by: &'a Option<String>,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct DeploymentItem<'a> {
    #[serde(rename = "entityType")]
    entity_type: &'a str,
    #[serde(rename = "entityId")]
    entity_id: &'a str,
    #[serde(rename = "entityTimestamp")]
    entity_timestamp: i64,
    pointers: &'a Vec<String>,
    content: Vec<ContentEntry<'a>>,
    #[serde(rename = "deployedBy")]
    deployed_by: &'a str,
    #[serde(rename = "entityVersion")]
    entity_version: &'a str,
    #[serde(rename = "auditInfo")]
    audit_info: AuditInfoResponse<'a>,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct EntityResponse<'a> {
    version: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    entity_type: &'a str,
    timestamp: f64,
    pointers: &'a Vec<String>,
    content: Vec<ContentEntry<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

#[derive(Serialize)]
pub(crate) struct PointerChangeDelta<'a> {
    #[serde(rename = "deploymentId")]
    pub(crate) deployment_id: i64,
    #[serde(rename = "entityType")]
    pub(crate) entity_type: &'a str,
    #[serde(rename = "entityId")]
    pub(crate) entity_id: &'a str,
    pub(crate) pointers: &'a Vec<String>,
    #[serde(rename = "entityTimestamp")]
    pub(crate) entity_timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metadata: Option<&'a Value>,
    #[serde(rename = "deployerAddress")]
    pub(crate) deployer_address: &'a str,
    pub(crate) version: &'a str,
    #[serde(rename = "authChain")]
    pub(crate) auth_chain: &'a Value,
    #[serde(rename = "localTimestamp")]
    pub(crate) local_timestamp: i64,
}

#[derive(Serialize)]
pub(crate) struct DeploymentFiltersResponse<'a> {
    pub(crate) pointers: &'a Vec<String>,
    #[serde(rename = "entityTypes")]
    pub(crate) entity_types: &'a Vec<String>,
    #[serde(rename = "entityIds")]
    pub(crate) entity_ids: &'a Vec<String>,
    #[serde(rename = "from", skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<i64>,
    #[serde(rename = "to", skip_serializing_if = "Option::is_none")]
    pub(crate) to: Option<i64>,
    #[serde(
        rename = "onlyCurrentlyPointed",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) only_currently_pointed: Option<bool>,
    #[serde(rename = "deployedBy", skip_serializing_if = "Vec::is_empty")]
    pub(crate) deployed_by: &'a Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct PointerChangesFiltersResponse<'a> {
    #[serde(rename = "entityTypes")]
    pub(crate) entity_types: &'a Vec<String>,
    #[serde(rename = "from", skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<i64>,
    #[serde(rename = "to", skip_serializing_if = "Option::is_none")]
    pub(crate) to: Option<i64>,
    #[serde(rename = "includeAuthChain")]
    pub(crate) include_auth_chain: bool,
}

const MAX_HISTORY_LIMIT: i64 = 500;

pub(crate) fn curate_limit(limit: Option<i64>) -> i64 {
    match limit {
        Some(l) if l > 0 && l <= MAX_HISTORY_LIMIT => l,
        _ => MAX_HISTORY_LIMIT,
    }
}

pub(crate) fn curate_offset(offset: Option<i64>) -> i64 {
    match offset {
        Some(o) if o >= 0 => o,
        _ => 0,
    }
}

fn deployment_row_to_entity(row: &DeploymentRow) -> Value {
    let content_arr: Vec<Value> = row
        .content
        .iter()
        .map(|(key, hash)| json!({"file": key, "hash": hash}))
        .collect();

    let mut obj = json!({
        "version": &row.version,
        "id": &row.entity_id,
        "type": row.entity_type,
        "timestamp": row.entity_timestamp as i64,
        "pointers": &row.pointers,
        "content": content_arr,
    });

    if let Some(ref m) = row.metadata {
        obj["metadata"] = m.clone();
    }

    obj
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ActiveEntityRow {
    pub(crate) entity_id: String,
    pub(crate) entity_type: String,
    entity_pointers: Vec<String>,
    entity_metadata: Option<Value>,
    entity_timestamp: f64,
    version: String,
    #[allow(dead_code)]
    id: i32,
    content_json: Value,
}

struct DeploymentRow {
    entity_id: String,
    entity_type: &'static str,
    pointers: Vec<String>,
    metadata: Option<Value>,
    entity_timestamp: f64,
    version: String,
    #[allow(dead_code)]
    deployment_id: i32,
    content: Vec<(String, String)>,
}

fn parse_content_json(v: &Value) -> Vec<(String, String)> {
    match v.as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|entry| {
                let key = entry.get("key")?.as_str()?;
                let hash = entry.get("hash")?.as_str()?;
                Some((key.to_string(), hash.to_string()))
            })
            .collect(),
        None => Vec::new(),
    }
}

pub(crate) fn build_entities_from_rows(rows: Vec<ActiveEntityRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|row| {
            let content = parse_content_json(&row.content_json);
            let metadata = row
                .entity_metadata
                .as_ref()
                .and_then(|m| m.get("v").cloned());
            let dr = DeploymentRow {
                entity_id: row.entity_id,
                entity_type: intern_entity_type(&row.entity_type),
                pointers: row.entity_pointers,
                metadata,
                entity_timestamp: row.entity_timestamp,
                version: row.version,
                deployment_id: row.id,
                content,
            };
            deployment_row_to_entity(&dr)
        })
        .collect()
}

pub(crate) fn row_to_cached_entity(row: ActiveEntityRow) -> CachedEntity {
    let content = parse_content_json(&row.content_json);
    let metadata = row
        .entity_metadata
        .as_ref()
        .and_then(|m| m.get("v").cloned());
    let etype = intern_entity_type(&row.entity_type);
    let pointers_lower: Vec<String> = row
        .entity_pointers
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let dr = DeploymentRow {
        entity_id: row.entity_id.clone(),
        entity_type: etype,
        pointers: row.entity_pointers,
        metadata,
        entity_timestamp: row.entity_timestamp,
        version: row.version,
        deployment_id: row.id,
        content,
    };
    let value = deployment_row_to_entity(&dr);
    let bytes = Bytes::from(serde_json::to_vec(&value).unwrap_or_default());
    CachedEntity {
        entity_id: row.entity_id,
        entity_type: etype,
        pointers: pointers_lower,
        bytes,
    }
}
