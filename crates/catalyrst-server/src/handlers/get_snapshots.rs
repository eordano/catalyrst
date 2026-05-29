use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::state::AppState;

pub async fn get_snapshots(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.snapshot_generator.get_current_snapshots() {
        Some(metadata) => (StatusCode::OK, Json(metadata)).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "New Snapshots not yet created" })),
        )
            .into_response(),
    }
}
