use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::http::{get_all, get_bool, get_first, get_pagination_params, ApiError, Paginated};
use crate::AppState;

pub fn thumbnail_url(cdn: &str, id: &str) -> String {
    format!("{}/social/communities/{}/raw-thumbnail.png", cdn, id)
}

fn enrich_community(
    state: &AppState,
    obj: &mut serde_json::Value,
    owner_names: &HashMap<String, String>,
) {
    let Some(map) = obj.as_object_mut() else { return };

    let id = map
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_default();

    let has_thumbnail = map
        .remove("_hasThumbnail")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let thumb = if has_thumbnail {
        thumbnail_url(&state.cdn_url, &id)
    } else {
        "N/A".to_string()
    };
    map.insert("thumbnailUrl".to_string(), serde_json::Value::String(thumb));

    if let Some(owner) = map.get("ownerAddress").and_then(|v| v.as_str()) {
        let name = owner_names
            .get(&owner.to_lowercase())
            .cloned()
            .unwrap_or_default();
        map.insert("ownerName".to_string(), serde_json::Value::String(name));
    }
}

fn owner_addresses(objs: &[serde_json::Value]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for o in objs {
        if let Some(a) = o.get("ownerAddress").and_then(|v| v.as_str()) {
            let lc = a.to_lowercase();
            if !seen.contains(&lc) {
                seen.push(lc);
            }
        }
    }
    seen
}

pub async fn get_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search");
    let only_member_of = get_bool(&pairs, "onlyMemberOf");
    let only_with_active_voice_chat = get_bool(&pairs, "onlyWithActiveVoiceChat");
    let roles = get_all(&pairs, "roles")
        .into_iter()
        .filter(|r| matches!(r.as_str(), "owner" | "moderator" | "member"))
        .collect::<Vec<_>>();

    let signer = try_extract_signer(&headers, "get", "/v1/communities");

    let (mut results, total) = state
        .communities
        .list(
            &pagination,
            search.as_deref(),
            signer.as_deref(),
            only_member_of,
            only_with_active_voice_chat,
            &roles,
        )
        .await?;

    let owner_names = state
        .profiles
        .get_owner_names(&owner_addresses(&results))
        .await;
    for obj in results.iter_mut() {
        enrich_community(&state, obj, &owner_names);
    }

    let paginated = Paginated::new(results, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

pub async fn get_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiError::bad_request("invalid community id"))?;
    let path = format!("/v1/communities/{}", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    let data = state.communities.get_by_id(id, signer.as_deref()).await?;
    let Some(mut obj) = data else {
        return Err(ApiError::not_found(format!("Community not found: {}", id_str)));
    };
    let owner_names = state
        .profiles
        .get_owner_names(&owner_addresses(std::slice::from_ref(&obj)))
        .await;
    enrich_community(&state, &mut obj, &owner_names);
    Ok(Json(serde_json::json!({ "data": obj })))
}
