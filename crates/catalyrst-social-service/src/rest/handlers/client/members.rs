use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::rest::fed::authority::Role;
use crate::rest::handlers::permissions::{
    can_act_on_member, has_permission, is_member, Permission,
};
use crate::rest::AppState;

use super::{auth, err, load_role_uuid, map_db, parse_uuid, stored_role};

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
    let signer = match auth(&headers, "post", &path) {
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
        .bind(&signer)
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
    .bind(&signer)
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
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();
    if target == signer {
        let role = load_role_uuid(&state, uuid, &signer).await;
        if role == Role::Owner {
            return err(
                StatusCode::UNAUTHORIZED,
                format!("The owner cannot leave the community {}", uuid),
            );
        }
    } else {
        let kicker_role = load_role_uuid(&state, uuid, &signer).await;
        let target_role = load_role_uuid(&state, uuid, &target).await;
        if !can_act_on_member(kicker_role, target_role) {
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
    let signer = match auth(&headers, "patch", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();
    let parsed: RoleBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let new_role = match Role::parse(&parsed.role) {
        Some(r) if r != Role::None && r != Role::Banned => r,
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
    if new_role == Role::Owner {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to assign roles in community {}",
                signer, uuid
            ),
        );
    }
    let updater_role = load_role_uuid(&state, uuid, &signer).await;
    let target_role = load_role_uuid(&state, uuid, &target).await;
    if !has_permission(updater_role, Permission::AssignRoles)
        || !can_act_on_member(updater_role, target_role)
    {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to assign roles in community {}",
                signer, uuid
            ),
        );
    }
    let stored = stored_role(new_role);
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
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();

    let banner_role = load_role_uuid(&state, uuid, &signer).await;
    let target_role = load_role_uuid(&state, uuid, &target).await;
    if !has_permission(banner_role, Permission::BanPlayers)
        || (!can_act_on_member(banner_role, target_role) && is_member(target_role))
    {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to ban {} from community {}",
                signer, target, uuid
            ),
        );
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
    .bind(&signer)
    .execute(&mut *tx)
    .await
    {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Err(e) = map_db(tx.commit().await) {
        return e;
    }
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
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let target = address.to_lowercase();

    let unbanner_role = load_role_uuid(&state, uuid, &signer).await;
    let target_role = load_role_uuid(&state, uuid, &target).await;
    if !has_permission(unbanner_role, Permission::BanPlayers)
        || (!can_act_on_member(unbanner_role, target_role) && is_member(target_role))
    {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to unban {} from community {}",
                signer, target, uuid
            ),
        );
    }
    let upd = sqlx::query(
        "UPDATE community_bans SET active = FALSE, unbanned_by = $3, unbanned_at = now() \
          WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(uuid)
    .bind(&target)
    .bind(&signer)
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}
