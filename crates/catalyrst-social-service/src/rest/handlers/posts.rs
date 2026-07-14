use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::rest::auth_chain::try_extract_signer;
use crate::rest::handlers::enrich::enrich_posts_with_authors;
use crate::rest::handlers::error::CommError;
use crate::rest::http::get_pagination_params;
use crate::rest::AppState;

#[utoipa::path(
    get,
    path = "/v1/communities/{id}/posts",
    tag = "posts",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id =
        Uuid::parse_str(&id_str).map_err(|_| CommError::bad_request("invalid community id"))?;
    let path = format!("/v1/communities/{}/posts", id_str);
    let signer = try_extract_signer(&headers, "get", &path).await;

    if !state
        .communities
        .community_exists(id, signer.is_none())
        .await?
    {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    }
    let pagination = get_pagination_params(&pairs);
    let (posts, total) = state
        .posts
        .list(
            id,
            &pagination,
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
        )
        .await?;

    let mut rows = posts
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    enrich_posts_with_authors(&state.profiles, &mut rows, "authorAddress").await;

    Ok(Json(serde_json::json!({
        "data": { "posts": rows, "total": total }
    })))
}

#[utoipa::path(
    get,
    path = "/v2/communities/{id}/posts",
    tag = "posts",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_posts_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id =
        Uuid::parse_str(&id_str).map_err(|_| CommError::bad_request("invalid community id"))?;
    let path = format!("/v2/communities/{}/posts", id_str);
    let signer = try_extract_signer(&headers, "get", &path).await;

    if !state
        .communities
        .community_exists(id, signer.is_none())
        .await?
    {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    }
    let pagination = get_pagination_params(&pairs);
    let (posts, total) = state
        .posts
        .list(
            id,
            &pagination,
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
        )
        .await?;

    let rows = posts
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<serde_json::Value>>();

    Ok(Json(serde_json::json!({
        "data": { "posts": rows, "total": total }
    })))
}
