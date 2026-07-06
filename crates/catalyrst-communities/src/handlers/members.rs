use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::Json;
use uuid::Uuid;

use crate::auth_chain::try_extract_signer;
use crate::handlers::error::CommError;
use crate::handlers::friendship::{friendship_status, friendship_statuses};
use crate::http::{get_first, get_pagination_params, ApiError, Paginated};
use crate::ports::members::{CommunityMember, CommunityMemberV2Wire, CommunityMemberWire};
use crate::AppState;

async fn connected_peers(_state: &AppState) -> Vec<String> {
    Vec::new()
}

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
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
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
        let online = connected_peers(&state).await;
        state.members.list_online(id, &online, &pagination).await?
    } else {
        state.members.list(id, &pagination).await?
    };

    let rows = to_member_wire_rows(&state, signer.as_deref(), members).await;

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

pub async fn get_members_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| CommError::not_found(format!("Community not found: {}", id_str)))?;
    let path = format!("/v2/communities/{}/members", id_str);
    let signer = try_extract_signer(&headers, "get", &path);
    let bypass_privacy = admin_bearer(&state, &headers);

    let only_public = signer.is_none() && !bypass_privacy;
    if !state.communities.community_exists(id, only_public).await? {
        return Err(CommError::not_found(format!(
            "Community not found: {}",
            id_str
        )));
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
        let online = connected_peers(&state).await;
        state.members.list_online(id, &online, &pagination).await?
    } else {
        state.members.list(id, &pagination).await?
    };

    let rows = to_member_v2_wire_rows(&state, signer.as_deref(), members).await;

    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

async fn member_friendship_statuses(
    state: &AppState,
    signer: Option<&str>,
    members: &[CommunityMember],
) -> std::collections::HashMap<String, i32> {
    match (signer, state.mutes_pool.as_ref()) {
        (Some(user), Some(social)) if !members.is_empty() => {
            let user = user.to_lowercase();
            let addresses: Vec<String> = members
                .iter()
                .map(|m| m.member_address.to_lowercase())
                .collect();
            friendship_statuses(social, &user, &addresses).await
        }
        _ => std::collections::HashMap::new(),
    }
}

async fn to_member_wire_rows(
    state: &AppState,
    signer: Option<&str>,
    members: Vec<CommunityMember>,
) -> Vec<CommunityMemberWire> {
    let addresses: Vec<String> = members.iter().map(|m| m.member_address.clone()).collect();
    let profiles = state.profiles.get_profiles(&addresses).await;
    let statuses = member_friendship_statuses(state, signer, &members).await;

    members
        .into_iter()
        .map(|m| {
            let addr = m.member_address.to_lowercase();
            let info = profiles.get(&addr);
            let (name, profile_picture_url, has_claimed_name) = match info {
                Some(info) => (
                    info.name.clone(),
                    info.profile_picture_url.clone(),
                    info.has_claimed_name,
                ),
                None => (String::new(), String::new(), false),
            };
            let name_color = info.and_then(|i| i.name_color.clone());
            let friendship_status = statuses
                .get(&addr)
                .copied()
                .unwrap_or(friendship_status::NONE);
            CommunityMemberWire {
                base: m,
                name,
                profile_picture_url,
                has_claimed_name,
                name_color,
                friendship_status,
            }
        })
        .collect()
}

async fn to_member_v2_wire_rows(
    state: &AppState,
    signer: Option<&str>,
    members: Vec<CommunityMember>,
) -> Vec<CommunityMemberV2Wire> {
    let statuses = member_friendship_statuses(state, signer, &members).await;
    members
        .into_iter()
        .map(|m| {
            let friendship_status = statuses
                .get(&m.member_address.to_lowercase())
                .copied()
                .unwrap_or(friendship_status::NONE);
            CommunityMemberV2Wire {
                base: m,
                friendship_status,
            }
        })
        .collect()
}

pub(crate) async fn enrich_with_friendship_status(
    state: &AppState,
    signer: Option<&str>,
    rows: &mut [serde_json::Value],
) {
    if rows.is_empty() {
        return;
    }
    let statuses = match (signer, state.mutes_pool.as_ref()) {
        (Some(user), Some(social)) => {
            let user = user.to_lowercase();
            let addresses: Vec<String> = rows
                .iter()
                .filter_map(|r| {
                    r.get("memberAddress")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_lowercase())
                })
                .collect();
            friendship_statuses(social, &user, &addresses).await
        }
        _ => std::collections::HashMap::new(),
    };

    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        let addr = obj
            .get("memberAddress")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        let status = statuses
            .get(&addr)
            .copied()
            .unwrap_or(friendship_status::NONE);
        obj.insert(
            "friendshipStatus".to_string(),
            serde_json::Value::Number(status.into()),
        );
    }
}

pub async fn get_member_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, CommError> {
    let path = format!("/v1/members/{}/communities", address);
    let signer = try_extract_signer(&headers, "get", &path);
    let only_public_visible = signer
        .as_deref()
        .is_none_or(|s| !s.eq_ignore_ascii_case(&address));
    let pagination = get_pagination_params(&pairs);
    let (rows, total) = state
        .communities
        .member_communities(&address, &pagination, None, only_public_visible)
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
        .member_communities(&address, &pagination, Some(roles), false)
        .await?;
    let paginated = Paginated::new(rows, total, &pagination);
    Ok(Json(serde_json::json!({ "data": paginated })))
}

#[cfg(test)]
mod tests {
    use crate::handlers::friendship::friendship_status;
    use crate::http::{Paginated, Pagination};
    use crate::ports::members::{CommunityMember, CommunityMemberV2Wire, CommunityMemberWire};
    use crate::ports::profiles::NameColor;
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn sample_member() -> CommunityMember {
        CommunityMember {
            community_id: Uuid::nil(),
            member_address: "0xabc0000000000000000000000000000000000001".to_string(),
            role: "member".to_string(),
            joined_at: NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        }
    }

    fn envelope<T: serde::Serialize>(rows: Vec<T>) -> String {
        let pagination = Pagination {
            limit: 100,
            offset: 0,
        };
        let paginated = Paginated::new(rows, 1, &pagination);
        serde_json::to_string(&serde_json::json!({ "data": paginated })).unwrap()
    }

    fn legacy_patched_row(
        profile: Option<(&str, &str, bool, Option<NameColor>)>,
        friendship_status: i32,
    ) -> serde_json::Value {
        let mut v = serde_json::to_value(sample_member()).unwrap();
        let obj = v.as_object_mut().unwrap();
        if let Some((name, pic, claimed, name_color)) = profile {
            obj.insert("name".into(), serde_json::Value::String(name.into()));
            obj.insert(
                "profilePictureUrl".into(),
                serde_json::Value::String(pic.into()),
            );
            obj.insert("hasClaimedName".into(), serde_json::Value::Bool(claimed));
            if let Some(nc) = &name_color {
                obj.insert("nameColor".into(), serde_json::to_value(nc).unwrap());
            }
        }
        obj.insert(
            "friendshipStatus".into(),
            serde_json::Value::Number(friendship_status.into()),
        );
        v
    }

    #[test]
    fn v1_wire_row_matches_legacy_value_patching_byte_for_byte() {
        let nc = NameColor {
            r: 0.5,
            g: 0.25,
            b: 1.0,
        };
        let legacy = envelope(vec![legacy_patched_row(
            Some((
                "Alice",
                "https://content/contents/xyz",
                true,
                Some(nc.clone()),
            )),
            friendship_status::ACCEPTED,
        )]);
        let wire = envelope(vec![CommunityMemberWire {
            base: sample_member(),
            name: "Alice".to_string(),
            profile_picture_url: "https://content/contents/xyz".to_string(),
            has_claimed_name: true,
            name_color: Some(nc),
            friendship_status: friendship_status::ACCEPTED,
        }]);
        assert_eq!(legacy, wire);
    }

    #[test]
    fn v1_wire_row_unresolved_profile_matches_legacy_placeholders() {
        let legacy = envelope(vec![legacy_patched_row(
            Some(("", "", false, None)),
            friendship_status::NONE,
        )]);
        let wire = envelope(vec![CommunityMemberWire {
            base: sample_member(),
            name: String::new(),
            profile_picture_url: String::new(),
            has_claimed_name: false,
            name_color: None,
            friendship_status: friendship_status::NONE,
        }]);
        assert_eq!(legacy, wire);
        assert!(
            !wire.contains("nameColor"),
            "absent nameColor must not serialize (never null)"
        );
    }

    #[test]
    fn v2_wire_row_matches_legacy_value_patching_byte_for_byte() {
        let legacy = envelope(vec![legacy_patched_row(
            None,
            friendship_status::REQUEST_SENT,
        )]);
        let wire = envelope(vec![CommunityMemberV2Wire {
            base: sample_member(),
            friendship_status: friendship_status::REQUEST_SENT,
        }]);
        assert_eq!(legacy, wire);
    }
}
