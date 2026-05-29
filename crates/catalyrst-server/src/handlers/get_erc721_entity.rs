use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;

use catalyrst_validator::erc721::format_erc721_entity;
use catalyrst_validator::types::{ContentMapping, Entity as ValidatorEntity, EntityType as VEntityType};

use crate::errors::{AppError, AppResult, InvalidRequestError, NotFoundError};
use crate::state::AppState;

fn get_urn_protocol(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        1 => Some("ethereum"),
        3 => Some("ropsten"),
        4 => Some("rinkeby"),
        5 => Some("goerli"),
        137 => Some("matic"),
        80001 => Some("mumbai"),
        80002 => Some("amoy"),
        11155111 => Some("sepolia"),
        _ => None,
    }
}

fn build_urn(protocol: &str, contract: &str, option: &str) -> String {
    let version = if contract.starts_with("0x") { "v2" } else { "v1" };
    format!(
        "urn:decentraland:{}:collections-{}:{}:{}",
        protocol, version, contract, option
    )
}

fn checked_f64_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    if v < i64::MIN as f64 || v > i64::MAX as f64 {
        return None;
    }
    Some(v as i64)
}

fn value_to_validator_entity(value: &serde_json::Value) -> Option<ValidatorEntity> {
    let id = value.get("id")?.as_str()?.to_string();
    let entity_type_str = value.get("type")?.as_str()?;
    let entity_type = VEntityType::parse(entity_type_str)?;
    let pointers: Vec<String> = value
        .get("pointers")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    let timestamp = checked_f64_to_i64(value.get("timestamp")?.as_f64()?)?;
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("v3")
        .to_string();
    let metadata = value.get("metadata").cloned();

    let content: Vec<ContentMapping> = value
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| {
                    let file = entry.get("file").or_else(|| entry.get("key"))?.as_str()?.to_string();
                    let hash = entry.get("hash")?.as_str()?.to_string();
                    Some(ContentMapping { file, hash })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(ValidatorEntity {
        id,
        entity_type,
        pointers,
        timestamp,
        content,
        version,
        metadata,
    })
}

pub async fn get_erc721_entity(
    State(state): State<Arc<AppState>>,
    Path(params): Path<Erc721Params>,
) -> AppResult<impl IntoResponse> {
    let chain_id: u64 = params
        .chain_id
        .parse()
        .map_err(|_| InvalidRequestError::new(format!("Invalid chainId '{}'", params.chain_id)))?;

    let protocol = get_urn_protocol(chain_id)
        .ok_or_else(|| InvalidRequestError::new(format!("Invalid chainId '{}'", params.chain_id)))?;

    let pointer = build_urn(protocol, &params.contract, &params.option);

    let entity_value = state
        .database
        .find_entity_by_pointer(&pointer)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| NotFoundError::new("Entity does not exist"))?;

    let metadata = entity_value
        .get("metadata")
        .ok_or_else(|| NotFoundError::new("Entity does not exist"))?;

    if metadata.get("rarity").is_none() {
        return Err(InvalidRequestError::new("Wearable is not standard.").into());
    }

    let entity = value_to_validator_entity(&entity_value)
        .ok_or_else(|| AppError::Internal("Failed to parse entity from DB".to_string()))?;

    let emission = params.emission.as_deref();
    let result = format_erc721_entity(&pointer, &entity, &state.content_server_address, emission);

    Ok(Json(result))
}

#[derive(Debug, serde::Deserialize)]
pub struct Erc721Params {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    pub contract: String,
    pub option: String,
    #[serde(default)]
    pub emission: Option<String>,
}
