use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppError, AppResult, InvalidRequestError, NotFoundError};
use crate::state::AppState;

pub async fn get_audit(
    State(state): State<Arc<AppState>>,
    Path((entity_type, entity_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let normalized = {
        let mut s = entity_type.trim().to_lowercase();
        if s.ends_with('s') {
            s.pop();
        }
        s
    };

    let valid_types = ["scene", "profile", "wearable", "store", "emote"];
    if !valid_types.contains(&normalized.as_str()) {
        return Err(InvalidRequestError::new(format!("Unrecognized type: {}", entity_type)).into());
    }

    let audit_info = state
        .database
        .get_audit_info(&normalized, &entity_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| NotFoundError::new("No deployment found"))?;

    Ok(Json(audit_info))
}
