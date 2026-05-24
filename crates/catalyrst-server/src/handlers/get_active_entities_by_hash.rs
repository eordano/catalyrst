use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppError, AppResult, NotFoundError};
use crate::state::AppState;

pub async fn get_active_entities_by_hash(
    State(state): State<Arc<AppState>>,
    Path(hash_id): Path<String>,
) -> AppResult<impl IntoResponse> {

    let mut entity_ids = state
        .database
        .active_entity_ids_by_content_hash(&hash_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    entity_ids.retain(|id| !state.denylist.is_denylisted(id));

    if entity_ids.is_empty() {
        return Err(NotFoundError::new("The entity was not found").into());
    }

    Ok(Json(entity_ids))
}
