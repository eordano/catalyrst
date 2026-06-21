use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::http::{get_all, get_bool, get_first, get_pagination_params, ApiError, Paginated};
use crate::AppState;

pub fn thumbnail_url(cdn: &str, id: &str) -> String {
    format!("{}/social/communities/{}/raw-thumbnail.png", cdn, id)
}

/// `GET /social/communities/{id}/raw-thumbnail.png` — serve the locally-stored
/// thumbnail bytes for a community.
///
/// The unity-explorer client fetches this on the assets-cdn host; after nginx
/// strips `/assets-cdn/` it routes to this (SOCIAL) bundle as
/// `/social/communities/{id}/raw-thumbnail.png`. The bytes were persisted to
/// the local ContentStore on community create/update; we resolve
/// community_id -> content hash (community_ranking_metrics.thumbnail_hash) and
/// stream them back as image/png. 404 if the community has no stored thumbnail.
pub async fn get_raw_thumbnail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let Ok(id) = Uuid::parse_str(&id_str) else {
        return (StatusCode::NOT_FOUND, "thumbnail not found").into_response();
    };

    let hash: Option<String> = match sqlx::query_scalar(
        "SELECT thumbnail_hash FROM community_ranking_metrics \
         WHERE community_id = $1 AND has_thumbnail",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(h) => h.flatten(),
        Err(e) => {
            tracing::error!(error = %e, "raw-thumbnail: db error");
            return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let Some(hash) = hash else {
        return (StatusCode::NOT_FOUND, "thumbnail not found").into_response();
    };

    match state.content_store.get(&hash).await {
        Ok(Some(bytes)) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
            if let Ok(v) = HeaderValue::from_str(&bytes.len().to_string()) {
                headers.insert(header::CONTENT_LENGTH, v);
            }
            (StatusCode::OK, headers, bytes).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "thumbnail not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, hash = %hash, "raw-thumbnail: content store error");
            (StatusCode::INTERNAL_SERVER_ERROR, "content store error").into_response()
        }
    }
}

fn enrich_community(
    state: &AppState,
    obj: &mut serde_json::Value,
    owner_names: &HashMap<String, String>,
) {
    let Some(map) = obj.as_object_mut() else {
        return;
    };

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
    let id = Uuid::parse_str(&id_str).map_err(|_| ApiError::bad_request("invalid community id"))?;
    let path = format!("/v1/communities/{}", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    let data = state.communities.get_by_id(id, signer.as_deref()).await?;
    let Some(mut obj) = data else {
        return Err(ApiError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    };
    let owner_names = state
        .profiles
        .get_owner_names(&owner_addresses(std::slice::from_ref(&obj)))
        .await;
    enrich_community(&state, &mut obj, &owner_names);
    Ok(Json(serde_json::json!({ "data": obj })))
}
