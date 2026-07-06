use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth_chain::require_signer;
use crate::http::{get_first, get_pagination_params, ApiError, Paginated};
use crate::AppState;

pub async fn get_moderation_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = require_signer(&headers, "get", "/v1/moderation/communities")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    if !state
        .global_moderators
        .iter()
        .any(|m| m.eq_ignore_ascii_case(&signer))
    {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            403,
            "Access denied. Global moderator privileges required.",
        )));
    }
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search");
    let (rows, total) = state.moderation.all(search.as_deref(), &pagination).await?;
    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
