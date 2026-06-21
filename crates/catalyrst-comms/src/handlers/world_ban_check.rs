use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::http::{unauthorized, ApiError};
use crate::AppState;

pub async fn world_ban_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((world_name, address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(expected) = state.gatekeeper_auth_token.as_deref() {
        let ok = crate::moderator::bearer_token(&headers)
            .map(|t| crate::moderator::timing_safe_eq(&t, expected))
            .unwrap_or(false);
        if !ok {
            return Err(unauthorized("Invalid authorization header"));
        }
    }
    let is_banned = state
        .scene_bans
        .is_banned(&world_name, &address)
        .await
        .unwrap_or(false);
    Ok(Json(serde_json::json!({ "isBanned": is_banned })))
}
