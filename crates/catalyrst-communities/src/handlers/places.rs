use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::http::{get_pagination_params, ApiError, Paginated};
use crate::AppState;

pub async fn get_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiError::bad_request("invalid community id"))?;
    // Upstream getPlaces: communityExists(onlyPublic: !userAddress) -> 404 for a
    // missing/soft-deleted community, and hides a PRIVATE community's places from
    // unauthenticated callers. Previously this endpoint had no auth and listed any
    // community's places (nonexistent -> empty 200).
    let path = format!("/v1/communities/{}/places", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    if !state.communities.community_exists(id, signer.is_none()).await? {
        return Err(ApiError::not_found(format!("Community not found: {}", id_str)));
    }
    let pagination = get_pagination_params(&pairs);
    let (places, total) = state.places.list(id, &pagination).await?;
    let paginated = Paginated::new(places, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
