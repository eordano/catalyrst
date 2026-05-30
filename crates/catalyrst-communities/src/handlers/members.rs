use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::{require_signer, try_extract_signer};
use crate::handlers::enrich::enrich_with_profiles;
use crate::handlers::error::CommError;
use crate::http::{get_first, get_pagination_params, ApiError, Paginated};
use crate::AppState;

fn admin_bearer(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = state.admin_token.as_deref() else {
        return false;
    };
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        == Some(expected)
}

pub async fn get_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v1/communities/{}/members", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    let bypass_privacy = admin_bearer(&state, &headers);

    let only_public = signer.is_none() && !bypass_privacy;
    if !state.communities.community_exists(id, only_public).await? {
        return Err(CommError::not_found(format!("Community not found: {}", id_str)));
    }

    if !bypass_privacy {
        if let Some(addr) = signer.as_deref() {
            if state.communities.is_private(id).await? {
                let role = state.communities.member_role(id, addr).await?;
                if role.is_none() {
                    return Err(CommError::not_authorized(
                        "The user doesn't have permission to get community members",
                    ));
                }
            }
        }
    }

    let pagination = get_pagination_params(&pairs);
    let only_online = get_first(&pairs, "onlyOnline")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (members, total) = if only_online {
        (Vec::new(), 0)
    } else {
        state.members.list(id, &pagination).await?
    };

    let mut rows = members
        .into_iter()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    enrich_with_profiles(&state.profiles, &mut rows, "memberAddress").await;

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

pub async fn get_member_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v1/members/{}/communities", address);
    let signer = require_signer(&headers, "get", &path)?;
    if !signer.eq_ignore_ascii_case(&address) {
        return Err(CommError::not_authorized(
            "You are not authorized to get communities for this member",
        ));
    }
    let pagination = get_pagination_params(&pairs);
    let (rows, total) = state
        .communities
        .member_communities(&address, &pagination, None)
        .await?;
    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

pub async fn get_managed_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            404,
            "Not found",
        )));
    };
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    if bearer != Some(expected) {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            401,
            "Access denied, invalid token",
        )));
    }

    let pagination = get_pagination_params(&pairs);
    let roles: &[&str] = &["owner", "moderator"];
    let (rows, total) = state
        .communities
        .member_communities(&address, &pagination, Some(roles))
        .await?;
    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
