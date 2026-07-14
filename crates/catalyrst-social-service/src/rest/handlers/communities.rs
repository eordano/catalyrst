use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::rest::auth_chain::try_extract_signer;
use crate::rest::handlers::dto::{CommunityDetail, CommunityListItem};
use crate::rest::handlers::error::CommError;
use crate::rest::handlers::friendship::community_friends;
use crate::rest::http::{
    get_all, get_bool, get_first, get_pagination_params, EnvelopeData, Paginated, Pagination,
};
use crate::rest::AppState;

const MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE: usize = 3;
const MAX_LIMIT_FOR_MINIMAL_RESPONSE: i64 = 50;

#[derive(Debug, PartialEq, Eq)]
enum MinimalError {
    Unauthorized,
    TooShort,
}

// The term is required, not just bounded: without one the query returns every community, which
// turns name search into a walkable directory (upstream #466).
fn plan_minimal_search(
    search: Option<&str>,
    signer_present: bool,
    requested_limit: i64,
) -> Result<(&str, i64), MinimalError> {
    if !signer_present {
        return Err(MinimalError::Unauthorized);
    }
    let Some(s) = search else {
        return Err(MinimalError::TooShort);
    };
    if s.chars().count() < MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE {
        return Err(MinimalError::TooShort);
    }
    Ok((s, requested_limit.min(MAX_LIMIT_FOR_MINIMAL_RESPONSE)))
}

const VALID_MEMBERSHIP_ROLES: &[&str] = &["owner", "moderator", "member", "none"];

#[derive(Debug, PartialEq, Eq)]
enum MembershipFilterError {
    Unauthorized,
    AllRolesUnknown(Vec<String>),
}

// Both filters describe the caller's OWN relationship to a community, so neither can be answered
// without an identity -- answering with the unfiltered listing turns "communities I moderate" into
// the whole directory (upstream #481). Dropping some unrecognized roles narrows the answer, which
// is safe; dropping every one widens it to everything, so that case is refused. Empty values are
// how clients serialize an unset field, so they read as absent rather than invalid.
fn plan_membership_filters(
    raw_roles: Vec<String>,
    only_member_of: bool,
    signer_present: bool,
) -> Result<Vec<String>, MembershipFilterError> {
    let requested: Vec<String> = raw_roles
        .into_iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    let roles: Vec<String> = requested
        .iter()
        .filter(|r| VALID_MEMBERSHIP_ROLES.contains(&r.as_str()))
        .cloned()
        .collect();
    if !requested.is_empty() && roles.is_empty() {
        return Err(MembershipFilterError::AllRolesUnknown(requested));
    }
    if !signer_present && (!roles.is_empty() || only_member_of) {
        return Err(MembershipFilterError::Unauthorized);
    }
    Ok(roles)
}

fn membership_filter_error(e: MembershipFilterError) -> CommError {
    match e {
        MembershipFilterError::Unauthorized => CommError::not_authorized(
            "Authentication required to filter communities by your own membership",
        ),
        MembershipFilterError::AllRolesUnknown(requested) => CommError::bad_request(format!(
            "Unknown community role: {}. Valid roles are {}",
            requested.join(", "),
            VALID_MEMBERSHIP_ROLES.join(", ")
        )),
    }
}

pub fn thumbnail_url(cdn: &str, id: &str) -> String {
    format!("{}/social/communities/{}/raw-thumbnail.png", cdn, id)
}

#[utoipa::path(
    get,
    path = "/social/communities/{id}/raw-thumbnail.png",
    tag = "communities",
    params(("id" = String, Path)),
    responses(
        (status = 200, description = "PNG thumbnail", content_type = "image/png", body = String),
        (status = 404, description = "thumbnail not found", content_type = "text/plain", body = String),
        (status = 500, description = "server error", content_type = "text/plain", body = String)
    )
)]
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
            // Serve the media type the bytes actually are, not a hardcoded one. New uploads are
            // signature-validated on write; a legacy blob with no recognised signature falls back
            // to image/png, matching the previous behaviour.
            let content_type = crate::rest::thumbnail_signature::detect_image_mime_type(&bytes)
                .map(|m| m.as_str())
                .unwrap_or("image/png");
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
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

// thumbnailUrl is always present: upstream substitutes the literal "N/A" when the
// community has no thumbnail (communities.ts: getThumbnail(...) || 'N/A').
fn set_thumbnail_list(cdn_url: &str, item: &mut CommunityListItem) {
    item.thumbnail_url = Some(if item.has_thumbnail {
        thumbnail_url(cdn_url, &item.id.to_string())
    } else {
        "N/A".to_string()
    });
}

fn set_thumbnail_detail(cdn_url: &str, item: &mut CommunityDetail) {
    item.thumbnail_url = Some(if item.has_thumbnail {
        thumbnail_url(cdn_url, &item.id.to_string())
    } else {
        "N/A".to_string()
    });
}

fn enrich_list_v1(
    cdn_url: &str,
    items: &mut [CommunityListItem],
    owner_names: &HashMap<String, String>,
) {
    for item in items.iter_mut() {
        set_thumbnail_list(cdn_url, item);
        let name = owner_names
            .get(&item.owner_address.to_lowercase())
            .cloned()
            .unwrap_or_default();
        item.owner_name = Some(name);
    }
}

fn owner_addresses(items: &[CommunityListItem]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for o in items {
        let lc = o.owner_address.to_lowercase();
        if !seen.contains(&lc) {
            seen.push(lc);
        }
    }
    seen
}

#[utoipa::path(
    get,
    path = "/v1/communities",
    tag = "communities",
    responses(
        (status = 200, body = EnvelopeData<Paginated<CommunityListItem>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, CommError> {
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let minimal = get_bool(&pairs, "minimal");

    let signer = try_extract_signer(&headers, "get", "/v1/communities").await;

    if minimal {
        return minimal_search(
            &state,
            &pagination,
            search.as_deref(),
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
        )
        .await;
    }

    let only_member_of = get_bool(&pairs, "onlyMemberOf");
    let only_with_active_voice_chat = get_bool(&pairs, "onlyWithActiveVoiceChat");
    let roles = plan_membership_filters(get_all(&pairs, "roles"), only_member_of, signer.is_some())
        .map_err(membership_filter_error)?;

    let (mut results, total) = state
        .communities
        .list(
            &pagination,
            search.as_deref(),
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
            only_member_of,
            only_with_active_voice_chat,
            &roles,
        )
        .await?;

    let owner_names = state
        .profiles
        .get_owner_names(&owner_addresses(&results))
        .await;
    enrich_list_v1(&state.cdn_url, &mut results, &owner_names);

    if let Some(user) = signer.as_ref().map(catalyrst_crypto::Signer::as_str) {
        enrich_with_friends(&state, &user.to_lowercase(), &mut results).await;
    }

    let data = Paginated::new(results, total, &pagination);
    Ok(Json(EnvelopeData { data }).into_response())
}

async fn minimal_search(
    state: &AppState,
    pagination: &Pagination,
    search: Option<&str>,
    signer: Option<&str>,
) -> Result<Response, CommError> {
    let (term, limit) =
        plan_minimal_search(search, signer.is_some(), pagination.limit).map_err(|e| match e {
            MinimalError::Unauthorized => {
                CommError::not_authorized("Authentication required for minimal community search")
            }
            MinimalError::TooShort => CommError::bad_request(format!(
                "Search query must be at least {} characters when using minimal",
                MIN_SEARCH_LENGTH_FOR_MINIMAL_RESPONSE
            )),
        })?;
    let user = signer.unwrap_or_default();

    let (results, total) = state
        .communities
        .search_communities(term, user, limit, pagination.offset)
        .await?;

    let capped = Pagination {
        limit,
        offset: pagination.offset,
    };
    let data = Paginated::new(results, total, &capped);
    Ok(Json(EnvelopeData { data }).into_response())
}

async fn friends_by_community(
    state: &AppState,
    user: &str,
    items: &[CommunityListItem],
) -> HashMap<Uuid, Vec<String>> {
    let ids: Vec<Uuid> = items.iter().map(|o| o.id).collect();
    if ids.is_empty() {
        return HashMap::new();
    }
    let Some(social) = state.mutes_pool.as_ref() else {
        return HashMap::new();
    };
    community_friends(social, &state.pool, user, &ids).await
}

async fn enrich_with_friends(state: &AppState, user: &str, items: &mut [CommunityListItem]) {
    let by_community = friends_by_community(state, user, items).await;
    if by_community.is_empty() {
        return;
    }

    let all_addresses: Vec<String> = by_community.values().flatten().cloned().collect();
    let profiles = state.profiles.get_profiles(&all_addresses).await;

    for item in items.iter_mut() {
        let Some(addresses) = by_community.get(&item.id) else {
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
            item.friends = Some(friends);
        }
    }
}

fn friend_profile(
    address: &str,
    info: &crate::rest::ports::profiles::ProfileInfo,
) -> serde_json::Value {
    serde_json::json!({
        "address": address,
        "name": info.name,
        "hasClaimedName": info.has_claimed_name,
        "profilePictureUrl": info.profile_picture_url,
    })
}

#[utoipa::path(
    get,
    path = "/v1/communities/{id}",
    tag = "communities",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = EnvelopeData<CommunityDetail>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<Json<EnvelopeData<CommunityDetail>>, CommError> {
    let id =
        Uuid::parse_str(&id_str).map_err(|_| CommError::bad_request("invalid community id"))?;
    let path = format!("/v1/communities/{}", id_str);
    let signer = try_extract_signer(&headers, "get", &path).await;
    let data = state
        .communities
        .get_by_id(id, signer.as_ref().map(catalyrst_crypto::Signer::as_str))
        .await?;
    let Some(mut detail) = data else {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    };
    let owner_names = state
        .profiles
        .get_owner_names(&[detail.owner_address.to_lowercase()])
        .await;
    set_thumbnail_detail(&state.cdn_url, &mut detail);
    let name = owner_names
        .get(&detail.owner_address.to_lowercase())
        .cloned()
        .unwrap_or_default();
    detail.owner_name = Some(name);
    Ok(Json(EnvelopeData { data: detail }))
}

#[utoipa::path(
    get,
    path = "/v2/communities",
    tag = "communities",
    responses(
        (status = 200, body = EnvelopeData<Paginated<CommunityListItem>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_communities_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, CommError> {
    let pagination = get_pagination_params(&pairs);
    let search = get_first(&pairs, "search")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let minimal = get_bool(&pairs, "minimal");
    let only_member_of = get_bool(&pairs, "onlyMemberOf");
    let only_with_active_voice_chat = get_bool(&pairs, "onlyWithActiveVoiceChat");

    let signer = try_extract_signer(&headers, "get", "/v2/communities").await;

    if minimal {
        return minimal_search(
            &state,
            &pagination,
            search.as_deref(),
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
        )
        .await;
    }

    let roles = plan_membership_filters(get_all(&pairs, "roles"), only_member_of, signer.is_some())
        .map_err(membership_filter_error)?;

    let (mut results, total) = state
        .communities
        .list(
            &pagination,
            search.as_deref(),
            signer.as_ref().map(catalyrst_crypto::Signer::as_str),
            only_member_of,
            only_with_active_voice_chat,
            &roles,
        )
        .await?;

    for item in results.iter_mut() {
        set_thumbnail_list(&state.cdn_url, item);
    }

    if let Some(user) = signer.as_ref().map(catalyrst_crypto::Signer::as_str) {
        enrich_with_friend_addresses(&state, &user.to_lowercase(), &mut results).await;
    }

    let data = Paginated::new(results, total, &pagination);
    Ok(Json(EnvelopeData { data }).into_response())
}

async fn enrich_with_friend_addresses(
    state: &AppState,
    user: &str,
    items: &mut [CommunityListItem],
) {
    let by_community = friends_by_community(state, user, items).await;
    if by_community.is_empty() {
        return;
    }

    for item in items.iter_mut() {
        if let Some(addresses) = by_community.get(&item.id) {
            item.friends = Some(
                addresses
                    .iter()
                    .map(|addr| serde_json::Value::String(addr.clone()))
                    .collect(),
            );
        }
    }
}

#[utoipa::path(
    get,
    path = "/v2/communities/{id}",
    tag = "communities",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = EnvelopeData<CommunityDetail>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_community_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<Json<EnvelopeData<CommunityDetail>>, CommError> {
    let id =
        Uuid::parse_str(&id_str).map_err(|_| CommError::bad_request("invalid community id"))?;
    let path = format!("/v2/communities/{}", id_str);
    let signer = try_extract_signer(&headers, "get", &path).await;
    let data = state
        .communities
        .get_by_id(id, signer.as_ref().map(catalyrst_crypto::Signer::as_str))
        .await?;
    let Some(mut detail) = data else {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
    };
    set_thumbnail_detail(&state.cdn_url, &mut detail);
    Ok(Json(EnvelopeData { data: detail }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::ports::profiles::ProfileInfo;

    fn sample_item(has_thumbnail: bool) -> CommunityListItem {
        CommunityListItem {
            id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            name: "Test".to_string(),
            description: "desc".to_string(),
            owner_address: "0xOWNER".to_string(),
            privacy: "public".to_string(),
            active: true,
            unlisted: false,
            members_count: 5,
            created_at: "2026-01-01T00:00:00".parse().unwrap(),
            is_live: false,
            voice_chat_status: crate::rest::handlers::dto::VoiceChatStatus::idle(),
            visibility: Some("all".to_string()),
            role: Some("member".to_string()),
            is_banned: None,
            thumbnail_url: None,
            owner_name: None,
            friends: Some(Vec::new()),
            has_thumbnail,
        }
    }

    #[test]
    fn list_item_carries_all_unity_fields() {
        let mut items = vec![sample_item(true)];
        let mut owners = HashMap::new();
        owners.insert("0xowner".to_string(), "OwnerName".to_string());
        enrich_list_v1("https://cdn.example", &mut items, &owners);

        let v = serde_json::to_value(&items[0]).unwrap();
        let m = v.as_object().unwrap();

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
        let mut item = sample_item(true);
        set_thumbnail_list("https://cdn.example", &mut item);
        let v = serde_json::to_value(&item).unwrap();
        let m = v.as_object().unwrap();

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
    fn missing_thumbnail_reports_na() {
        let mut items = vec![sample_item(false)];
        enrich_list_v1("https://cdn.example", &mut items, &HashMap::new());
        let v = serde_json::to_value(&items[0]).unwrap();
        let m = v.as_object().unwrap();
        assert_eq!(
            m["thumbnailUrl"], "N/A",
            "no-thumbnail community reports the upstream 'N/A' sentinel"
        );
        assert_eq!(m["ownerName"], "");
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
        assert_eq!(plan_minimal_search(Some("abc"), true, 20), Ok(("abc", 20)));
    }

    #[test]
    fn minimal_search_requires_a_term_even_for_an_authenticated_user() {
        assert_eq!(
            plan_minimal_search(None, true, 20),
            Err(MinimalError::TooShort)
        );
    }

    #[test]
    fn minimal_search_caps_limit_to_max() {
        assert_eq!(
            plan_minimal_search(Some("cool"), true, 100),
            Ok(("cool", MAX_LIMIT_FOR_MINIMAL_RESPONSE))
        );
        assert_eq!(
            plan_minimal_search(Some("cool"), true, 10),
            Ok(("cool", 10))
        );
    }

    #[test]
    fn membership_filters_require_an_identity() {
        assert_eq!(
            plan_membership_filters(vec!["owner".into()], false, false),
            Err(MembershipFilterError::Unauthorized)
        );
        assert_eq!(
            plan_membership_filters(vec![], true, false),
            Err(MembershipFilterError::Unauthorized)
        );
        assert_eq!(plan_membership_filters(vec![], false, false), Ok(vec![]));
    }

    #[test]
    fn all_unknown_roles_refused_partially_valid_kept() {
        assert_eq!(
            plan_membership_filters(vec!["onwer".into()], false, true),
            Err(MembershipFilterError::AllRolesUnknown(vec!["onwer".into()]))
        );
        assert_eq!(
            plan_membership_filters(vec!["owner".into(), "invalid".into()], false, true),
            Ok(vec!["owner".into()])
        );
        assert_eq!(
            plan_membership_filters(vec!["".into()], false, false),
            Ok(vec![])
        );
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
