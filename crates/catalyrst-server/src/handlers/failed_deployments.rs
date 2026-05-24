use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppError, AppResult};
use crate::state::AppState;

pub async fn get_failed_deployments(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let failed = state
        .database
        .get_failed_deployments()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(failed))
}
