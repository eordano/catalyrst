use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::handlers::enrich::enrich_posts_with_authors;
use crate::http::{get_pagination_params, ApiError};
use crate::AppState;

pub async fn get_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| ApiError::bad_request("invalid community id"))?;
    let path = format!("/v1/communities/{}/posts", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    // Upstream gates posts on the community existing (and being public when
    // unauthenticated): a missing/soft-deleted/anon-on-private community 404s
    // rather than returning an empty list. Matches get_members.
    if !state
        .communities
        .community_exists(id, signer.is_none())
        .await?
    {
        return Err(ApiError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    }
    let pagination = get_pagination_params(&pairs);
    let (posts, total) = state.posts.list(id, &pagination, signer.as_deref()).await?;

    let mut rows = posts
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    enrich_posts_with_authors(&state.profiles, &mut rows, "authorAddress").await;

    Ok(Json(serde_json::json!({
        "data": { "posts": rows, "total": total }
    })))
}
