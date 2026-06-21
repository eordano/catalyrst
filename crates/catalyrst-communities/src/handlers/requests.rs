use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::handlers::communities::thumbnail_url;
use crate::handlers::enrich::enrich_with_profiles;
use crate::handlers::error::CommError;
use crate::handlers::roles::has_moderation_permission;
use crate::http::{get_first, get_pagination_params, Paginated};
use crate::AppState;

fn parse_type(v: Option<&str>) -> Option<&'static str> {
    match v {
        Some("invite") => Some("invite"),
        Some("request_to_join") => Some("request_to_join"),
        _ => None,
    }
}

pub async fn get_community_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v1/communities/{}/requests", id_str);
    let signer = require_signer(&headers, "get", &path)?;

    let role = state.communities.member_role(id, &signer).await?;
    if !has_moderation_permission(role.as_deref()) {
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

pub async fn get_member_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v1/members/{}/requests", address);
    let signer = require_signer(&headers, "get", &path)?;
    if !signer.eq_ignore_ascii_case(&address) {
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
            let has_thumb = map
                .remove("_hasThumbnail")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let thumb = if has_thumb {
                thumbnail_url(&state.cdn_url, &id)
            } else {
                "N/A".to_string()
            };
            map.insert("thumbnailUrl".to_string(), serde_json::Value::String(thumb));
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
