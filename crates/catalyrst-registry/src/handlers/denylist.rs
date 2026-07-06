use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::handlers::status::require_signed_fetch;
use crate::http::auth::has_valid_bearer;
use crate::http::errors::{ApiError, ApiResult};
use crate::ports::registry::DenylistEntry;
use crate::AppState;

#[derive(Debug, serde::Deserialize, Default)]
pub struct DenylistBody {
    #[serde(default)]
    pub reason: Option<String>,
}

pub async fn get_denylist(State(state): State<AppState>) -> ApiResult<Json<Vec<DenylistEntry>>> {
    let rows = state.registry.denylist_rows().await?;
    Ok(Json(rows))
}

pub async fn add_denylist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
    body: Option<Json<DenylistBody>>,
) -> ApiResult<Response> {
    let signer = if has_valid_bearer(&state, &headers) {
        "admin-bearer".to_string()
    } else {
        let signer = require_signed_fetch(&headers, "post", &format!("/denylist/{entity_id}"))?;
        require_moderator(&state, &signer)?;
        signer
    };
    if !state.registry.enabled() {
        return Err(ApiError::not_implemented(
            "denylist persistence requires the ab_registry DB",
        ));
    }
    let reason = body.and_then(|b| b.0.reason);
    let entry = state
        .registry
        .add_to_denylist(&entity_id, &signer, reason.as_deref())
        .await?;
    state.manifests.invalidate_all();
    Ok((StatusCode::CREATED, Json(entry)).into_response())
}

pub async fn remove_denylist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    if !has_valid_bearer(&state, &headers) {
        let signer = require_signed_fetch(&headers, "delete", &format!("/denylist/{entity_id}"))?;
        require_moderator(&state, &signer)?;
    }
    if !state.registry.enabled() {
        return Err(ApiError::not_implemented(
            "denylist persistence requires the ab_registry DB",
        ));
    }
    let deleted = state.registry.remove_from_denylist(&entity_id).await?;
    if !deleted {
        return Err(ApiError::not_found("Entity ID not found in denylist"));
    }

    state.manifests.invalidate_all();
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn require_moderator(state: &AppState, signer: &str) -> Result<(), ApiError> {
    if state.denylist_moderators.iter().any(|m| m == signer) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Forbidden: signer is not an authorized moderator",
        ))
    }
}
