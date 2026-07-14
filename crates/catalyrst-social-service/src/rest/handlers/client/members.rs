use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::rest::community_membership_authority::{
    CommunityBanAuthority, CommunityMembershipTier, CommunityUnbanAuthority,
};
use crate::rest::handlers::permissions::{
    can_act_on_member, has_permission, is_member, Permission,
};
use crate::rest::AppState;

use super::{
    auth, err, load_client_standing, map_db, parse_uuid, refusal_response,
    role_text_as_written_into_the_community_members_table, verified_wallet_of_the_caller,
};

pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members", id);
    let signer = match auth(&headers, "post", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let community: Option<(bool, bool)> = match map_db(
        sqlx::query_as("SELECT active, private FROM communities WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&state.pool)
            .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (active, private) = match community {
        Some(v) => v,
        None => {
            return err(
                StatusCode::NOT_FOUND,
                format!("Community not found: {}", uuid),
            )
        }
    };
    if !active {
        return err(StatusCode::BAD_REQUEST, "Community is not active");
    }
    if private {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "Cannot join private community {} directly; a join request or invite is required",
                uuid
            ),
        );
    }
    let banned: Option<bool> = match map_db(
        sqlx::query_scalar(
            "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
        )
        .bind(uuid)
        .bind(signer.as_str())
        .fetch_optional(&state.pool)
        .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if banned.unwrap_or(false) {
        return err(
            StatusCode::FORBIDDEN,
            "The member is banned from this community",
        );
    }
    let ins = sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role, joined_at) \
         VALUES ($1, $2, 'member', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
    )
    .bind(uuid)
    .bind(signer.as_str())
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(ins) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
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
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let signer = match auth(&headers, "delete", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();
    if target == signer {
        let standing = match load_client_standing(&state, uuid, signer.as_str()).await {
            Ok(s) => s,
            Err(e) => return e,
        };
        if standing.tier() == CommunityMembershipTier::OwnerOfThisCommunity {
            return err(
                StatusCode::UNAUTHORIZED,
                format!("The owner cannot leave the community {}", uuid),
            );
        }
    } else {
        let kicker_standing = match load_client_standing(&state, uuid, signer.as_str()).await {
            Ok(s) => s,
            Err(e) => return e,
        };
        let target_standing = match load_client_standing(&state, uuid, &target).await {
            Ok(s) => s,
            Err(e) => return e,
        };
        // An absent target ranks as the lowest actionable tier (upstream #477): nothing can act on
        // a non-member, so ranking against the real tier refused every caller and turned the
        // always-204 no-op kick into a 401 -- and 204-vs-401 is what reveals who is in the
        // community. An owner or moderator passes and the kick stays the no-op it was; anyone else
        // gets the same refusal they would get for a member.
        let target_tier = target_standing.tier();
        let effective_target_tier = if is_member(target_tier) {
            target_tier
        } else {
            CommunityMembershipTier::OrdinaryMemberOfThisCommunity
        };
        if !can_act_on_member(kicker_standing.tier(), effective_target_tier) {
            return err(
                StatusCode::UNAUTHORIZED,
                format!(
                    "The user {} doesn't have permission to kick {} from community {}",
                    signer, target, uuid
                ),
            );
        }
    }
    let del = sqlx::query(
        "DELETE FROM community_members WHERE community_id = $1 AND member_address = $2",
    )
    .bind(uuid)
    .bind(&target)
    .execute(&state.pool)
    .await;
    let removed = match map_db(del) {
        Ok(r) => r.rows_affected() > 0,
        Err(e) => return e,
    };
    if removed {
        crate::rest::events::note_member_left(&uuid.to_string(), &target);
        // Kick branch only: upstream's leaveCommunity never touches the voice room, its
        // kickMember does.
        if target != signer.as_str() {
            state
                .evict_from_private_community_voice(uuid, &target)
                .await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct RoleBody {
    pub role: String,
}

pub async fn update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let signer = match auth(&headers, "patch", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();
    let parsed: RoleBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let new_tier =
        match CommunityMembershipTier::parse_role_text_supplied_in_a_request(&parsed.role) {
            Some(t)
                if t != CommunityMembershipTier::NotAMemberOfThisCommunity
                    && t != CommunityMembershipTier::BannedFromThisCommunity =>
            {
                t
            }
            _ => return err(StatusCode::BAD_REQUEST, "invalid role"),
        };

    if signer == target {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} cannot update their own role in community {}",
                signer, uuid
            ),
        );
    }
    if new_tier == CommunityMembershipTier::OwnerOfThisCommunity {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to assign roles in community {}",
                signer, uuid
            ),
        );
    }
    let updater_standing = match load_client_standing(&state, uuid, signer.as_str()).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target_standing = match load_client_standing(&state, uuid, &target).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    if !has_permission(updater_standing.tier(), Permission::AssignRoles)
        || !can_act_on_member(updater_standing.tier(), target_standing.tier())
    {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to assign roles in community {}",
                signer, uuid
            ),
        );
    }
    let stored = role_text_as_written_into_the_community_members_table(new_tier);
    let upd = sqlx::query(
        "UPDATE community_members SET role = $3 WHERE community_id = $1 AND member_address = $2",
    )
    .bind(uuid)
    .bind(&target)
    .bind(stored)
    .execute(&state.pool)
    .await;
    match map_db(upd) {
        Ok(r) if r.rows_affected() == 0 => {
            return err(StatusCode::NOT_FOUND, "member not found in community")
        }
        Ok(_) => {}
        Err(e) => return e,
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let signer = match auth(&headers, "post", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();

    if let Err(refusal) = CommunityBanAuthority::resolve_from_end_user_signed_fetch_on_this_node(
        &state.pool,
        &verified_wallet_of_the_caller(&signer),
        uuid,
        &target,
    )
    .await
    {
        return refusal_response(&refusal, StatusCode::UNAUTHORIZED, || {
            format!(
                "The user {} doesn't have permission to ban {} from community {}",
                signer, target, uuid
            )
        });
    }
    let mut tx = match map_db(state.pool.begin().await) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if let Err(e) =
        sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND member_address = $2")
            .bind(uuid)
            .bind(&target)
            .execute(&mut *tx)
            .await
    {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Err(e) = sqlx::query(
        "DELETE FROM community_requests \
         WHERE community_id = $1 AND member_address = $2 AND status = 'pending'",
    )
    .bind(uuid)
    .bind(&target)
    .execute(&mut *tx)
    .await
    {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Err(e) = sqlx::query(
        "INSERT INTO community_bans (community_id, banned_address, banned_by, active, banned_at) \
         VALUES ($1,$2,$3,TRUE, now()) \
         ON CONFLICT (community_id, banned_address) DO UPDATE \
           SET active = TRUE, banned_by = EXCLUDED.banned_by, banned_at = now(), unbanned_by = NULL, unbanned_at = NULL",
    )
    .bind(uuid)
    .bind(&target)
    .bind(signer.as_str())
    .execute(&mut *tx)
    .await
    {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Err(e) = map_db(tx.commit().await) {
        return e;
    }
    // Outside the membership guard, exactly as upstream: a pre-emptive ban of a non-member can
    // still be holding a seat in the room.
    state
        .evict_from_private_community_voice(uuid, &target)
        .await;
    StatusCode::NO_CONTENT.into_response()
}

pub async fn unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let signer = match auth(&headers, "delete", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();

    if let Err(refusal) = CommunityUnbanAuthority::resolve_from_end_user_signed_fetch_on_this_node(
        &state.pool,
        &verified_wallet_of_the_caller(&signer),
        uuid,
        &target,
    )
    .await
    {
        return refusal_response(&refusal, StatusCode::UNAUTHORIZED, || {
            format!(
                "The user {} doesn't have permission to unban {} from community {}",
                signer, target, uuid
            )
        });
    }
    let upd = sqlx::query(
        "UPDATE community_bans SET active = FALSE, unbanned_by = $3, unbanned_at = now() \
          WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(uuid)
    .bind(&target)
    .bind(signer.as_str())
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}
