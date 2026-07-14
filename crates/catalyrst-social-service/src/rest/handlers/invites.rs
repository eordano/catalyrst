use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::error::CommError;
use crate::rest::AppState;

#[utoipa::path(
    get,
    path = "/v1/members/{address}/invites",
    tag = "requests",
    params(("address" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_invites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invitee): Path<String>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v1/members/{}/invites", invitee);
    let inviter = require_signer(&headers, "get", &path).await?;
    if inviter.as_str().eq_ignore_ascii_case(&invitee) {
        return Err(CommError::bad_request("Users cannot invite themselves"));
    }
    let invites = state.invites.list(inviter.as_str(), &invitee).await?;
    Ok(Json(serde_json::json!({ "data": invites })))
}
