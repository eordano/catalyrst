use std::collections::BTreeSet;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;

use dcl_contents::errors::{ApiError, ApiResult};
use dcl_contents::registry::EntitySource;

use crate::http::auth::timing_safe_eq;
use crate::AppState;

pub async fn post_registry(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<serde_json::Value>>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&state, &headers)?;

    let unique_entity_ids: BTreeSet<String> = body
        .as_ref()
        .and_then(|b| b.0.get("entityIds"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    if unique_entity_ids.is_empty() {
        return Err(ApiError::bad_request("No entity ids provided"));
    }

    let mut failures: Vec<serde_json::Value> = Vec::new();
    let successes: Vec<serde_json::Value> = Vec::new();

    for entity_id in &unique_entity_ids {
        match state.content.resolve_one(entity_id).await {
            Ok(Some(_entity)) => {
                let _ = state.manifests.get(entity_id).await;
            }
            Ok(None) => {
                failures.push(json!({
                    "entityId": entity_id,
                    "error": "Entity not found in catalyst",
                }));
            }
            Err(err) => {
                tracing::error!(error = %err, entity_id = %entity_id, "error persisting entity");
                failures.push(json!({
                    "entityId": entity_id,
                    "error": err.to_string(),
                }));
            }
        }
    }

    Ok(Json(json!({
        "failures": failures,
        "successes": successes,
    })))
}

pub async fn flush_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&state, &headers)?;
    state.manifests.invalidate_all();
    Ok(Json(json!({ "ok": true, "message": "Cache flushed" })))
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = &state.admin_token else {
        return Err(ApiError::forbidden(
            "admin controls disabled (API_ADMIN_TOKEN unset)",
        ));
    };
    let header = headers.get("authorization").and_then(|v| v.to_str().ok());
    let Some(header) = header else {
        return Err(ApiError::unauthorized("Authorization header is missing"));
    };
    let mut parts = header.splitn(2, ' ');
    let scheme = parts.next().unwrap_or("");
    let value = parts.next().unwrap_or("");
    if scheme != "Bearer" || !timing_safe_eq(value, expected) {
        return Err(ApiError::unauthorized("Invalid authorization header"));
    }
    Ok(())
}
