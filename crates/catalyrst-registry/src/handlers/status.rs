use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;

use dcl_contents::errors::{ApiError, ApiResult};
use dcl_contents::handlers::status::entity_status_from;
use dcl_contents::types::EntityStatus;

use crate::AppState;

pub async fn service_status() -> Json<serde_json::Value> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Json(json!({
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "currentTime": current_time,
            "commitHash": option_env!("GIT_COMMIT").unwrap_or("unknown"),
        }
    }))
}

pub async fn get_entities_status_signed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<EntityStatus>>> {
    let signer = require_signed_fetch(&headers, "get", "/entities/status")?;

    let ents = state.content.active_entities_by_deployer(&signer).await?;
    let mut out = Vec::with_capacity(ents.len());
    for ent in ents {
        let m = state.manifests.get(&ent.entity_id).await;
        out.push(entity_status_from(&ent.entity_id, &m, ent.is_world()));
    }
    Ok(Json(out))
}

pub(crate) fn require_signed_fetch(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, ApiError> {
    if let Some(signer) = headers
        .get("x-identity-metadata-signer")
        .and_then(|v| v.to_str().ok())
    {
        if signer == "decentraland-kernel-scene" {
            return Err(ApiError::unauthorized(
                "decentraland-kernel-scene signer is not allowed",
            ));
        }
    }

    catalyrst_comms::auth_chain::require_signer(headers, method, path)
        .map(|s| s.to_lowercase())
        .map_err(|e| ApiError::unauthorized(format!("signed-fetch verification failed: {e}")))
}
