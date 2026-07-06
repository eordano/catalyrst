use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::handlers::friendship::community_friends;
use crate::http::{
    get_all, get_bool, get_first, get_pagination_params, ApiError, HttpError, Paginated, Pagination,
};
use crate::AppState;

const MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE: usize = 3;
const MAX_LIMIT_FOR_MINIMAL_RESPONSE: i64 = 50;

#[derive(Debug, PartialEq, Eq)]
enum MinimalError {
    Unauthorized,
    TooShort,
}

fn plan_minimal_search(
    search: Option<&str>,
    signer_present: bool,
    requested_limit: i64,
) -> Result<i64, MinimalError> {
    if !signer_present {
        return Err(MinimalError::Unauthorized);
    }
    if let Some(s) = search {
        if s.chars().count() < MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE {
            return Err(MinimalError::TooShort);
        }
    }
    Ok(requested_limit.min(MAX_LIMIT_FOR_MINIMAL_RESPONSE))
}

pub fn thumbnail_url(cdn: &str, id: &str) -> String {
    format!("{}/social/communities/{}/raw-thumbnail.png", cdn, id)
}

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

pub fn apply_thumbnail(cdn_url: &str, obj: &mut serde_json::Value) {
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
        thumbnail_url(cdn_url, &id)
    } else {
        "N/A".to_string()
    };
    map.insert("thumbnailUrl".to_string(), serde_json::Value::String(thumb));
}

fn enrich_community(
    cdn_url: &str,
    obj: &mut serde_json::Value,
    owner_names: &HashMap<String, String>,
) {
    apply_thumbnail(cdn_url, obj);

    let Some(map) = obj.as_object_mut() else {
        return;
    };

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
    let search = get_first(&pairs, "search")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let minimal = get_bool(&pairs, "minimal");

    let signer = try_extract_signer(&headers, "get", "/v1/communities");

    if minimal {
        return minimal_search(&state, &pagination, search.as_deref(), signer.as_deref()).await;
    }

    let only_member_of = get_bool(&pairs, "onlyMemberOf");
    let only_with_active_voice_chat = get_bool(&pairs, "onlyWithActiveVoiceChat");
    let roles = get_all(&pairs, "roles")
        .into_iter()
        .filter(|r| matches!(r.as_str(), "owner" | "moderator" | "member"))
        .collect::<Vec<_>>();

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
        enrich_community(&state.cdn_url, obj, &owner_names);
    }

    if let Some(user) = signer.as_deref() {
        enrich_with_friends(&state, &user.to_lowercase(), &mut results).await;
    }

    let paginated = Paginated::new(results, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

async fn minimal_search(
    state: &AppState,
    pagination: &Pagination,
    search: Option<&str>,
    signer: Option<&str>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit =
        plan_minimal_search(search, signer.is_some(), pagination.limit).map_err(|e| match e {
            MinimalError::Unauthorized => ApiError::Http(HttpError::new(
                401,
                "Authentication required for minimal community search",
            )),
            MinimalError::TooShort => ApiError::bad_request(format!(
                "Search query must be at least {} characters when using minimal",
                MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE
            )),
        })?;
    let user = signer.unwrap_or_default();

    let (results, total) = state
        .communities
        .search_communities(search.unwrap_or(""), user, limit, pagination.offset)
        .await?;

    let capped = Pagination {
        limit,
        offset: pagination.offset,
    };
    let paginated = Paginated::new(results, total, &capped);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

async fn enrich_with_friends(state: &AppState, user: &str, results: &mut [serde_json::Value]) {
    let ids: Vec<uuid::Uuid> = results
        .iter()
        .filter_map(|o| o.get("id").and_then(|v| v.as_str()))
        .filter_map(|s| uuid::Uuid::parse_str(s).ok())
        .collect();
    if ids.is_empty() {
        return;
    }

    let Some(social) = state.mutes_pool.as_ref() else {
        return;
    };
    let by_community = community_friends(social, &state.pool, user, &ids).await;
    if by_community.is_empty() {
        return;
    }

    let all_addresses: Vec<String> = by_community.values().flatten().cloned().collect();
    let profiles = state.profiles.get_profiles(&all_addresses).await;

    for obj in results.iter_mut() {
        let Some(map) = obj.as_object_mut() else {
            continue;
        };
        let Some(id) = map
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
        else {
            continue;
        };
        let Some(addresses) = by_community.get(&id) else {
            continue;
        };
        let friends: Vec<serde_json::Value> = addresses
            .iter()
            .filter_map(|addr| {
                profiles
                    .get(&addr.to_lowercase())
                    .map(|info| friend_profile(addr, info))
            })
            .collect();
        if !friends.is_empty() {
            map.insert("friends".to_string(), serde_json::Value::Array(friends));
        }
    }
}

fn friend_profile(address: &str, info: &crate::ports::profiles::ProfileInfo) -> serde_json::Value {
    serde_json::json!({
        "address": address,
        "name": info.name,
        "hasClaimedName": info.has_claimed_name,
        "profilePictureUrl": info.profile_picture_url,
    })
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
    enrich_community(&state.cdn_url, &mut obj, &owner_names);
    Ok(Json(serde_json::json!({ "data": obj })))
}

pub async fn get_communities_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let minimal = get_bool(&pairs, "minimal");
    let only_member_of = get_bool(&pairs, "onlyMemberOf");
    let only_with_active_voice_chat = get_bool(&pairs, "onlyWithActiveVoiceChat");
    let roles = get_all(&pairs, "roles")
        .into_iter()
        .filter(|r| matches!(r.as_str(), "owner" | "moderator" | "member"))
        .collect::<Vec<_>>();

    let signer = try_extract_signer(&headers, "get", "/v2/communities");

    if minimal {
        return minimal_search(&state, &pagination, search.as_deref(), signer.as_deref()).await;
    }

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

    for obj in results.iter_mut() {
        apply_thumbnail(&state.cdn_url, obj);
    }

    if let Some(user) = signer.as_deref() {
        enrich_with_friend_addresses(&state, &user.to_lowercase(), &mut results).await;
    }

    let paginated = Paginated::new(results, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

async fn enrich_with_friend_addresses(
    state: &AppState,
    user: &str,
    results: &mut [serde_json::Value],
) {
    let ids: Vec<uuid::Uuid> = results
        .iter()
        .filter_map(|o| o.get("id").and_then(|v| v.as_str()))
        .filter_map(|s| uuid::Uuid::parse_str(s).ok())
        .collect();
    if ids.is_empty() {
        return;
    }

    let Some(social) = state.mutes_pool.as_ref() else {
        return;
    };
    let by_community = community_friends(social, &state.pool, user, &ids).await;
    if by_community.is_empty() {
        return;
    }

    for obj in results.iter_mut() {
        let Some(map) = obj.as_object_mut() else {
            continue;
        };
        let Some(id) = map
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
        else {
            continue;
        };
        let Some(addresses) = by_community.get(&id) else {
            continue;
        };
        let friends: Vec<serde_json::Value> = addresses
            .iter()
            .map(|addr| serde_json::Value::String(addr.clone()))
            .collect();
        map.insert("friends".to_string(), serde_json::Value::Array(friends));
    }
}

pub async fn get_community_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| ApiError::bad_request("invalid community id"))?;
    let path = format!("/v2/communities/{}", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    let data = state.communities.get_by_id(id, signer.as_deref()).await?;
    let Some(mut obj) = data else {
        return Err(ApiError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    };
    apply_thumbnail(&state.cdn_url, &mut obj);
    Ok(Json(serde_json::json!({ "data": obj })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::profiles::ProfileInfo;

    fn sample_listed_community() -> serde_json::Value {
        serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "name": "Test",
            "description": "desc",
            "ownerAddress": "0xOWNER",
            "privacy": "public",
            "visibility": "all",
            "role": "member",
            "active": true,
            "unlisted": false,
            "membersCount": 5,
            "isLive": false,
            "friends": [],
            "voiceChatStatus": serde_json::Value::Null,
            "_hasThumbnail": true,
        })
    }

    #[test]
    fn list_item_carries_all_unity_fields() {
        let mut obj = sample_listed_community();
        let mut owners = HashMap::new();
        owners.insert("0xowner".to_string(), "OwnerName".to_string());
        enrich_community("https://cdn.example", &mut obj, &owners);

        let m = obj.as_object().unwrap();

        for key in [
            "id",
            "name",
            "description",
            "ownerAddress",
            "ownerName",
            "thumbnailUrl",
            "privacy",
            "visibility",
            "role",
            "membersCount",
            "friends",
            "voiceChatStatus",
        ] {
            assert!(m.contains_key(key), "missing list field {key}");
        }

        assert!(!m.contains_key("_hasThumbnail"));
        assert_eq!(m["ownerName"], "OwnerName");
        assert_eq!(
            m["thumbnailUrl"],
            "https://cdn.example/social/communities/11111111-1111-1111-1111-111111111111/raw-thumbnail.png"
        );
        assert!(m["friends"].is_array());
    }

    #[test]
    fn apply_thumbnail_is_address_only_no_owner_name() {
        let mut obj = sample_listed_community();
        apply_thumbnail("https://cdn.example", &mut obj);
        let m = obj.as_object().unwrap();

        assert!(m.contains_key("ownerAddress"), "address is preserved");
        assert!(
            !m.contains_key("ownerName"),
            "v2 shaping must not resolve/add ownerName"
        );
        assert!(!m.contains_key("_hasThumbnail"), "internal flag stripped");
        assert_eq!(
            m["thumbnailUrl"],
            "https://cdn.example/social/communities/11111111-1111-1111-1111-111111111111/raw-thumbnail.png"
        );
    }

    #[test]
    fn missing_thumbnail_renders_na() {
        let mut obj = sample_listed_community();
        obj["_hasThumbnail"] = serde_json::Value::Bool(false);
        enrich_community("https://cdn.example", &mut obj, &HashMap::new());
        assert_eq!(obj["thumbnailUrl"], "N/A");

        assert_eq!(obj["ownerName"], "");
    }

    #[test]
    fn minimal_search_requires_authentication() {
        assert_eq!(
            plan_minimal_search(Some("cool"), false, 20),
            Err(MinimalError::Unauthorized)
        );
        assert_eq!(
            plan_minimal_search(Some("a"), false, 20),
            Err(MinimalError::Unauthorized)
        );
    }

    #[test]
    fn minimal_search_enforces_min_length_when_search_present() {
        assert_eq!(
            plan_minimal_search(Some("ab"), true, 20),
            Err(MinimalError::TooShort)
        );
        assert_eq!(plan_minimal_search(Some("abc"), true, 20), Ok(20));
    }

    #[test]
    fn minimal_search_allows_empty_search_for_authenticated_user() {
        assert_eq!(plan_minimal_search(None, true, 20), Ok(20));
    }

    #[test]
    fn minimal_search_caps_limit_to_max() {
        assert_eq!(
            plan_minimal_search(Some("cool"), true, 100),
            Ok(MAX_LIMIT_FOR_MINIMAL_RESPONSE)
        );
        assert_eq!(plan_minimal_search(Some("cool"), true, 10), Ok(10));
    }

    #[test]
    fn friend_profile_matches_friendprofile_wire_shape() {
        let info = ProfileInfo {
            name: "Alice".to_string(),
            profile_picture_url: "https://content/contents/QmFace".to_string(),
            has_claimed_name: true,
            name_color: None,
        };
        let f = friend_profile("0xabc", &info);
        let m = f.as_object().unwrap();
        assert_eq!(m.len(), 4, "FriendProfile has exactly 4 fields");
        assert_eq!(m["address"], "0xabc");
        assert_eq!(m["name"], "Alice");
        assert_eq!(m["hasClaimedName"], true);
        assert_eq!(m["profilePictureUrl"], "https://content/contents/QmFace");
    }
}
