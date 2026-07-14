use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::fed::apply;
use crate::fed::authority::{
    community_exists, community_is_private, load_role, require_min_role, Role,
};
use crate::fed::ids::community_uuid_from_hex;
use crate::fed::messages::{
    CommunityBan, CommunityJoin, CommunityLeave, CommunityRole, CommunityUnban,
};
use crate::handlers::permissions::{can_act_on_member, has_permission, is_member, Permission};
use crate::AppState;

use super::{emit_gossip, err_json, into_resp, map_apply_err, ok_json, preflight, uuid_from_path};

pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::add_member(State(state), headers, Path(id)).await;
    }
    into_resp(fed_add_member(State(state), headers, Path(id), body).await)
}

async fn fed_add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members", id);
    let (signed, signer) =
        match preflight::<CommunityJoin>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match community_exists(&state.pool, &signed.message.community_id).await {
        Ok(true) => {}
        Ok(false) => return err_json(StatusCode::NOT_FOUND, "community not found"),
        Err(e) => return map_apply_err(e),
    }
    match community_is_private(&state.pool, &signed.message.community_id).await {
        Ok(Some(true)) => {
            return err_json(
                StatusCode::UNAUTHORIZED,
                format!(
                "Cannot join private community {} directly; a join request or invite is required",
                id
            ),
            )
        }
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(Role::Banned) => return err_json(StatusCode::FORBIDDEN, "banned from community"),
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_join(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdAddr {
    pub id: String,
    pub address: String,
}

pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::remove_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(
        fed_remove_member(
            State(state),
            headers,
            Path(PathIdAddr { id, address }),
            body,
        )
        .await,
    )
}

async fn fed_remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let (signed, signer) =
        match preflight::<CommunityLeave>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.member.eq_ignore_ascii_case(&signer) {
        return err_json(StatusCode::FORBIDDEN, "may only leave on behalf of self");
    }
    if !address.eq_ignore_ascii_case(&signer) {
        return err_json(StatusCode::FORBIDDEN, "path address must match signer");
    }

    match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(Role::Owner) => {
            return err_json(
                StatusCode::UNAUTHORIZED,
                format!("The owner cannot leave the community {}", id),
            )
        }
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_leave(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            crate::events::note_member_left(&uuid.to_string(), &signer.to_ascii_lowercase());
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::update_member_role(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
            body,
        )
        .await;
    }
    into_resp(
        fed_update_member_role(
            State(state),
            headers,
            Path(PathIdAddr { id, address }),
            body,
        )
        .await,
    )
}

async fn fed_update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let (signed, signer) =
        match preflight::<CommunityRole>(&state, &headers, "patch", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }

    if !matches!(
        Role::parse(&signed.message.role),
        Some(Role::Member) | Some(Role::Mod)
    ) {
        return err_json(StatusCode::BAD_REQUEST, "invalid role");
    }
    if signed.message.target.eq_ignore_ascii_case(&signer) {
        return err_json(StatusCode::FORBIDDEN, "a user cannot update their own role");
    }
    let actor_role = match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    let target_role = match load_role(
        &state.pool,
        &signed.message.community_id,
        &signed.message.target,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if !has_permission(actor_role, Permission::AssignRoles)
        || !can_act_on_member(actor_role, target_role)
    {
        return err_json(
            StatusCode::FORBIDDEN,
            "actor cannot assign roles for this member",
        );
    }
    match apply::apply_role(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::ban_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(
        fed_ban_member(
            State(state),
            headers,
            Path(PathIdAddr { id, address }),
            body,
        )
        .await,
    )
}

async fn fed_ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let (signed, signer) =
        match preflight::<CommunityBan>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }
    let actor = match require_min_role(
        &state.pool,
        &signed.message.community_id,
        &signer,
        Role::Mod,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    let target_role = match load_role(
        &state.pool,
        &signed.message.community_id,
        &signed.message.target,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if target_role >= actor {
        return err_json(StatusCode::FORBIDDEN, "cannot ban a peer or superior");
    }
    match apply::apply_ban(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::unban_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(
        fed_unban_member(
            State(state),
            headers,
            Path(PathIdAddr { id, address }),
            body,
        )
        .await,
    )
}

async fn fed_unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let (signed, signer) =
        match preflight::<CommunityUnban>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }

    let actor_role = match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    let target_role = match load_role(
        &state.pool,
        &signed.message.community_id,
        &signed.message.target,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if !has_permission(actor_role, Permission::BanPlayers)
        || (!can_act_on_member(actor_role, target_role) && is_member(target_role))
    {
        return err_json(
            StatusCode::FORBIDDEN,
            format!(
                "The user {} doesn't have permission to unban {} from community {}",
                signer, signed.message.target, id
            ),
        );
    }
    match apply::apply_unban(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Deserialize)]
pub struct MemberCommunitiesByIdsBody {
    #[serde(rename = "communityIds", default)]
    community_ids: Vec<String>,
}

pub async fn member_communities_by_ids(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    body: Option<Json<MemberCommunitiesByIdsBody>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    match (&state.admin_token, bearer) {
        (Some(expected), Some(got))
            if crate::handlers::admin::timing_safe_eq(expected.as_bytes(), got.as_bytes()) => {}
        _ => return err_json(StatusCode::UNAUTHORIZED, "admin bearer token required"),
    }

    let community_ids = body.map(|Json(b)| b.community_ids).unwrap_or_default();
    let uuids: Vec<Uuid> = community_ids
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();

    let visible = match state
        .communities
        .visible_communities_by_ids(&uuids, &address)
        .await
    {
        Ok(v) => v,
        Err(e) => return map_apply_err(e),
    };

    let communities: Vec<serde_json::Value> = visible
        .into_iter()
        .map(|id| json!({ "id": id.to_string() }))
        .collect();

    (
        StatusCode::OK,
        Json(json!({ "data": { "communities": communities } })),
    )
}
