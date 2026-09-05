use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::rest::community_membership_authority::{
    load_standing_from_community_role_current, CommunityBanAuthority, CommunityMembershipTier,
    CommunityUnbanAuthority,
};
use crate::rest::fed::apply;
use crate::rest::fed::authority::{community_exists, community_is_private};
use crate::rest::fed::ids::community_uuid_from_hex;
use crate::rest::fed::messages::{
    CommunityBan, CommunityJoin, CommunityLeave, CommunityRole, CommunityUnban,
};
use crate::rest::handlers::permissions::{can_act_on_member, has_permission, Permission};
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, map_refusal_using_its_own_detail, ok_json,
    preflight, uuid_from_path, verified_wallet_of_the_caller,
};

#[utoipa::path(
    post,
    path = "/v1/communities/{id}/members",
    tag = "members",
    params(("id" = String, Path)),
    request_body(content = serde_json::Value, description = "empty for client join, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::add_member(State(state), headers, Path(id)).await;
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
    match load_standing_from_community_role_current(
        &state.pool,
        &signed.message.community_id,
        signer.as_str(),
    )
    .await
    {
        Ok(standing) if standing.tier() == CommunityMembershipTier::BannedFromThisCommunity => {
            return err_json(StatusCode::FORBIDDEN, "banned from community")
        }
        Ok(_) => {}
        Err(refusal) => return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN),
    }
    match apply::apply_join(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
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

#[utoipa::path(
    delete,
    path = "/v1/communities/{id}/members/{address}",
    tag = "members",
    params(("id" = String, Path), ("address" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::remove_member(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdAddr { id, address }),
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
    if !signed.message.member.eq_ignore_ascii_case(signer.as_str()) {
        return err_json(StatusCode::FORBIDDEN, "may only leave on behalf of self");
    }
    if !address.eq_ignore_ascii_case(signer.as_str()) {
        return err_json(StatusCode::FORBIDDEN, "path address must match signer");
    }

    match load_standing_from_community_role_current(
        &state.pool,
        &signed.message.community_id,
        signer.as_str(),
    )
    .await
    {
        Ok(standing) if standing.tier() == CommunityMembershipTier::OwnerOfThisCommunity => {
            return err_json(
                StatusCode::UNAUTHORIZED,
                format!("The owner cannot leave the community {}", id),
            )
        }
        Ok(_) => {}
        Err(refusal) => return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN),
    }
    match apply::apply_leave(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
            crate::rest::events::note_member_left(&uuid.to_string(), signer.as_str());
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    patch,
    path = "/v1/communities/{id}/members/{address}",
    tag = "members",
    params(("id" = String, Path), ("address" = String, Path)),
    request_body(content = serde_json::Value, description = "{ role } for client, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::update_member_role(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdAddr { id, address }),
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
        CommunityMembershipTier::parse_role_text_supplied_in_a_request(&signed.message.role),
        Some(CommunityMembershipTier::OrdinaryMemberOfThisCommunity)
            | Some(CommunityMembershipTier::ModeratorOfThisCommunity)
    ) {
        return err_json(StatusCode::BAD_REQUEST, "invalid role");
    }
    if signed.message.target.eq_ignore_ascii_case(signer.as_str()) {
        return err_json(StatusCode::FORBIDDEN, "a user cannot update their own role");
    }
    let actor_standing = match load_standing_from_community_role_current(
        &state.pool,
        &signed.message.community_id,
        signer.as_str(),
    )
    .await
    {
        Ok(s) => s,
        Err(refusal) => return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN),
    };
    let target_standing = match load_standing_from_community_role_current(
        &state.pool,
        &signed.message.community_id,
        &signed.message.target,
    )
    .await
    {
        Ok(s) => s,
        Err(refusal) => return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN),
    };
    if !has_permission(actor_standing.tier(), Permission::AssignRoles)
        || !can_act_on_member(actor_standing.tier(), target_standing.tier())
    {
        return err_json(
            StatusCode::FORBIDDEN,
            "actor cannot assign roles for this member",
        );
    }
    match apply::apply_role(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/communities/{id}/members/{address}/bans",
    tag = "bans",
    params(("id" = String, Path), ("address" = String, Path)),
    request_body(content = serde_json::Value, description = "empty for client ban, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::ban_member(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdAddr { id, address }),
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
    if let Err(refusal) =
        CommunityBanAuthority::resolve_from_federation_envelope_signed_by_the_originating_wallet(
            &state.pool,
            &verified_wallet_of_the_caller(&signer),
            &signed.message.community_id,
            &signed.message.target,
        )
        .await
    {
        // The three refusals this used to spell separately -- "banned from this
        // community", "signer role X below required mod", and "cannot ban a peer or
        // superior" -- merge into the authority's own detail, which names whichever one
        // actually fired. The status is unchanged at 403; only the body text moves.
        return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN);
    }
    match apply::apply_ban(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
            state
                .evict_from_private_community_voice(uuid, &signed.message.target.to_lowercase())
                .await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    delete,
    path = "/v1/communities/{id}/members/{address}/bans",
    tag = "bans",
    params(("id" = String, Path), ("address" = String, Path)),
    request_body(content = serde_json::Value, description = "empty for client unban, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::unban_member(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdAddr { id, address }),
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

    if let Err(refusal) =
        CommunityUnbanAuthority::resolve_from_federation_envelope_signed_by_the_originating_wallet(
            &state.pool,
            &verified_wallet_of_the_caller(&signer),
            &signed.message.community_id,
            &signed.message.target,
        )
        .await
    {
        // Same merge as the ban path above; status unchanged at 403.
        return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN);
    }
    match apply::apply_unban(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
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

#[utoipa::path(
    post,
    path = "/v1/members/{address}/communities",
    tag = "members",
    params(("address" = String, Path)),
    request_body(content = serde_json::Value, description = "{ communityIds }"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
            if crate::rest::handlers::admin::timing_safe_eq(
                expected.as_bytes(),
                got.as_bytes(),
            ) => {}
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
