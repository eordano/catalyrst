use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::state::AppState;

pub async fn get_challenge(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let challenge_text = state.challenge_supervisor.get_challenge_text();
    Json(json!({ "challengeText": challenge_text }))
}
