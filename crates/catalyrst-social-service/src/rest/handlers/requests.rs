use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::rest::auth_chain::require_signer;
use crate::rest::community_membership_authority::load_standing_from_community_members;
use crate::rest::handlers::communities::thumbnail_url;
use crate::rest::handlers::enrich::enrich_with_profiles;
use crate::rest::handlers::error::CommError;
use crate::rest::handlers::permissions::Permission;
use crate::rest::http::{get_first, get_pagination_params, Paginated};
use crate::rest::AppState;

fn parse_type(v: Option<&str>) -> Option<&'static str> {
    match v {
        Some("invite") => Some("invite"),
        Some("request_to_join") => Some("request_to_join"),
        _ => None,
    }
}

#[utoipa::path(
    get,
    path = "/v1/communities/{id}/requests",
    tag = "requests",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::rest::handlers::error::SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_community_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v1/communities/{}/requests", id_str);
    let signer = require_signer(&headers, "get", &path).await?;

    let standing = load_standing_from_community_members(&state.pool, id, signer.as_str()).await?;
    if !standing.holds_capability_within_this_community(Permission::ViewRequests) {
        return Err(CommError::not_authorized(format!(
            "The user {} doesn't have permission to view requests",
            signer
        )));
    }

    let pagination = get_pagination_params(&pairs);
    let type_filter = parse_type(get_first(&pairs, "type").as_deref());
    let (rows, total) = state
        .requests
        .list_by_community(id, type_filter, &pagination)
        .await?;

    let mut json_rows = rows
        .into_iter()
        .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    enrich_with_profiles(&state.profiles, &mut json_rows, "memberAddress").await;

    let paginated = Paginated::new(json_rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

#[utoipa::path(
    get,
    path = "/v2/communities/{id}/requests",
    tag = "requests",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::rest::handlers::error::SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_community_requests_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v2/communities/{}/requests", id_str);
    let signer = require_signer(&headers, "get", &path).await?;

    let standing = load_standing_from_community_members(&state.pool, id, signer.as_str()).await?;
    if !standing.holds_capability_within_this_community(Permission::ViewRequests) {
        return Err(CommError::not_authorized(format!(
            "The user {} doesn't have permission to view requests",
            signer
        )));
    }

    let pagination = get_pagination_params(&pairs);
    let type_filter = parse_type(get_first(&pairs, "type").as_deref());
    let (rows, total) = state
        .requests
        .list_by_community(id, type_filter, &pagination)
        .await?;

    let mut json_rows = rows
        .into_iter()
        .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
        .collect::<Vec<_>>();
    crate::rest::handlers::members::enrich_with_friendship_status(
        &state,
        Some(signer.as_str()),
        &mut json_rows,
    )
    .await;

    let paginated = Paginated::new(json_rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

#[utoipa::path(
    get,
    path = "/v1/members/{address}/requests",
    tag = "requests",
    params(("address" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::rest::handlers::error::SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_member_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v1/members/{}/requests", address);
    let signer = require_signer(&headers, "get", &path).await?;
    if !signer.as_str().eq_ignore_ascii_case(&address) {
        return Err(CommError::not_authorized(
            "You are not authorized to get requests for this member",
        ));
    }
    let pagination = get_pagination_params(&pairs);
    let type_filter = parse_type(get_first(&pairs, "type").as_deref());
    let (mut rows, total) = state
        .requests
        .list_aggregated_by_member(&address, type_filter, &pagination)
        .await?;

    let owner_addrs: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            r.get("ownerAddress")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    let owner_names = state.profiles.get_owner_names(&owner_addrs).await;

    for row in rows.iter_mut() {
        if let Some(map) = row.as_object_mut() {
            let id = map
                .get("communityId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_default();
            // Upstream builds thumbnailUrl unconditionally for request rows
            // (requests.ts: buildThumbnailUrl), so the key is always present.
            map.remove("_hasThumbnail");
            map.insert(
                "thumbnailUrl".to_string(),
                serde_json::Value::String(thumbnail_url(&state.cdn_url, &id)),
            );
            let owner = map
                .get("ownerAddress")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let owner_name = owner_names.get(&owner).cloned().unwrap_or_default();
            map.insert(
                "ownerName".to_string(),
                serde_json::Value::String(owner_name),
            );
        }
    }

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

#[utoipa::path(
    get,
    path = "/v2/members/{address}/requests",
    tag = "requests",
    params(("address" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::rest::handlers::error::SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_member_requests_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v2/members/{}/requests", address);
    let signer = require_signer(&headers, "get", &path).await?;
    if !signer.as_str().eq_ignore_ascii_case(&address) {
        return Err(CommError::not_authorized(
            "You are not authorized to get requests for this member",
        ));
    }
    let pagination = get_pagination_params(&pairs);
    let type_filter = parse_type(get_first(&pairs, "type").as_deref());
    let (mut rows, total) = state
        .requests
        .list_aggregated_by_member(&address, type_filter, &pagination)
        .await?;

    for row in rows.iter_mut() {
        if let Some(map) = row.as_object_mut() {
            let community_id = map
                .get("communityId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_default();
            map.remove("_hasThumbnail");
            map.insert(
                "thumbnailUrl".to_string(),
                serde_json::Value::String(thumbnail_url(&state.cdn_url, &community_id)),
            );
        }
    }

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}
