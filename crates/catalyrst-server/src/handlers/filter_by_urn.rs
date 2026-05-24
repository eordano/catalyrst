use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::query_params::{parse_pagination, parse_query_string};
use crate::state::AppState;

fn is_hex_address(s: &str) -> bool {
    let rest = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(r) => r,
        None => return false,
    };
    rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_valid_collection_urn(urn: &str) -> bool {
    let p: Vec<&str> = urn.split(':').collect();
    if p.len() < 5 || p[0] != "urn" || p[1] != "decentraland" {
        return false;
    }

    if p[2] == "off-chain" {
        return (p[3] == "base-avatars" || p[3] == "base-emotes") && !p[4].is_empty();
    }

    match p[3] {
        "collections-v1" => !p[4].is_empty(),
        "collections-v2" => is_hex_address(p[4]),

        "collections-thirdparty" => p.len() == 5 && !p[4].is_empty(),
        _ => false,
    }
}

pub async fn get_entities_by_collection(
    State(state): State<Arc<AppState>>,
    Path(collection_urn): Path<String>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    if !is_valid_collection_urn(&collection_urn) {
        return Err(InvalidRequestError::new(format!(
            "Invalid collection urn param, it must be a valid urn prefix of a collection \
             or a third party id, instead: '{}'",
            collection_urn
        ))
        .into());
    }

    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);
    let pagination = parse_pagination(&params, 1000).map_err(InvalidRequestError::new)?;

    let result = state
        .database
        .active_entities_by_prefix(&collection_urn, pagination.offset, pagination.limit)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "total": result.total,
        "entities": result.entities,
    })))
}
