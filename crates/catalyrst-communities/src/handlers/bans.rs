use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::handlers::enrich::enrich_with_profiles;
use crate::handlers::error::CommError;
use crate::handlers::roles::has_moderation_permission;
use crate::http::{get_pagination_params, Paginated};
use crate::AppState;

pub async fn get_bans(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v1/communities/{}/bans", id_str);
    let signer = require_signer(&headers, "get", &path)?;

    if !state.communities.community_exists(id, false).await? {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    }

    let role = state.communities.member_role(id, &signer).await?;
    if !has_moderation_permission(role.as_deref()) {
        return Err(CommError::not_authorized(format!(
            "The user {} doesn't have permission to get banned members from the community",
            signer
        )));
    }

    let pagination = get_pagination_params(&pairs);
    let (bans, total) = state.bans.list(id, &pagination).await?;

    let mut rows = bans
        .into_iter()
        .map(|b| serde_json::to_value(b).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    enrich_with_profiles(&state.profiles, &mut rows, "memberAddress").await;

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

pub async fn get_bans_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v2/communities/{}/bans", id_str);
    let signer = require_signer(&headers, "get", &path)?;

    if !state.communities.community_exists(id, false).await? {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    }

    let role = state.communities.member_role(id, &signer).await?;
    if !has_moderation_permission(role.as_deref()) {
        return Err(CommError::not_authorized(format!(
            "The user {} doesn't have permission to get banned members from the community",
            signer
        )));
    }

    let pagination = get_pagination_params(&pairs);
    let (bans, total) = state.bans.list(id, &pagination).await?;

    let mut rows = bans
        .into_iter()
        .map(|b| serde_json::to_value(b).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    crate::handlers::members::enrich_with_friendship_status(&state, Some(&signer), &mut rows).await;

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
