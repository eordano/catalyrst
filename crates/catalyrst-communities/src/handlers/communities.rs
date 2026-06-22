use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::handlers::friendship::community_friends;
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
    cdn_url: &str,
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
        thumbnail_url(cdn_url, &id)
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
        enrich_community(&state.cdn_url, obj, &owner_names);
    }

    if let Some(user) = signer.as_deref() {
        enrich_with_friends(&state, &user.to_lowercase(), &mut results).await;
    }

    let paginated = Paginated::new(results, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

/// Populate each community's `friends` array with the requesting user's friends
/// who are members of that community (max 3, ordered by address), each rendered
/// as a `FriendProfile` (`address`, `name`, `hasClaimedName`,
/// `profilePictureUrl`). Mirrors upstream `getCommunities` +
/// `parseProfilesToFriends`. Friends whose profile cannot be resolved are
/// dropped, exactly as `mapMembersWithProfiles` filters profile-less entries.
async fn enrich_with_friends(state: &AppState, user: &str, results: &mut [serde_json::Value]) {
    let ids: Vec<uuid::Uuid> = results
        .iter()
        .filter_map(|o| o.get("id").and_then(|v| v.as_str()))
        .filter_map(|s| uuid::Uuid::parse_str(s).ok())
        .collect();
    if ids.is_empty() {
        return;
    }

    let by_community = community_friends(&state.pool, user, &ids).await;
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

/// Render a resolved friend as the wire `FriendProfile`
/// (decentraland/social_service/v2: `address`, `name`, `has_claimed_name`,
/// `profile_picture_url`), exactly as upstream `parseProfileToFriend` emits it.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::profiles::ProfileInfo;

    fn sample_listed_community() -> serde_json::Value {
        // Mirrors the json built by CommunitiesComponent::list for one row.
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
        // GetUserCommunitiesData.CommunityData fields the converter reads.
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
        // _hasThumbnail is an internal flag and must not leak to the wire.
        assert!(!m.contains_key("_hasThumbnail"));
        assert_eq!(m["ownerName"], "OwnerName");
        assert_eq!(
            m["thumbnailUrl"],
            "https://cdn.example/social/communities/11111111-1111-1111-1111-111111111111/raw-thumbnail.png"
        );
        assert!(m["friends"].is_array());
    }

    #[test]
    fn missing_thumbnail_renders_na() {
        let mut obj = sample_listed_community();
        obj["_hasThumbnail"] = serde_json::Value::Bool(false);
        enrich_community("https://cdn.example", &mut obj, &HashMap::new());
        assert_eq!(obj["thumbnailUrl"], "N/A");
        // Unknown owner -> empty string, never absent (Unity reads ownerName).
        assert_eq!(obj["ownerName"], "");
    }

    #[test]
    fn friend_profile_matches_friendprofile_wire_shape() {
        let info = ProfileInfo {
            name: "Alice".to_string(),
            profile_picture_url: "https://content/contents/QmFace".to_string(),
            has_claimed_name: true,
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
