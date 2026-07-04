use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use crate::AppState;

pub async fn get_invites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invitee): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = format!("/v1/members/{}/invites", invitee);
    let inviter = require_signer(&headers, "get", &path)
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    if inviter.eq_ignore_ascii_case(&invitee) {
        return Err(ApiError::bad_request("Users cannot invite themselves"));
    }
    let invites = state.invites.list(&inviter, &invitee).await?;
    Ok(Json(serde_json::json!({ "data": invites })))
}
