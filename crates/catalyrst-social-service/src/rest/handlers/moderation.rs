use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::error::CommError;
use crate::rest::http::{get_first, get_pagination_params, Paginated};
use crate::rest::AppState;

#[utoipa::path(
    get,
    path = "/v1/moderation/communities",
    tag = "moderation",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::rest::handlers::error::SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_moderation_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let signer = require_signer(&headers, "get", "/v1/moderation/communities").await?;
    if !state
        .global_moderators
        .iter()
        .any(|m| m.eq_ignore_ascii_case(signer.as_str()))
    {
        return Err(CommError::status(
            StatusCode::FORBIDDEN,
            "Access denied. Global moderator privileges required.",
        ));
    }
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search");
    let (rows, total) = state.moderation.all(search.as_deref(), &pagination).await?;
    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
