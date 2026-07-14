use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::json_error;
use crate::state::AppState;

pub async fn get_snapshots(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.snapshot_generator.get_current_snapshots() {
        Some(metadata) => (StatusCode::OK, Json(metadata)).into_response(),
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "New Snapshots not yet created",
        ),
    }
}
