use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::fed::authority::Role;
use crate::handlers::communities::thumbnail_url;
use crate::handlers::permissions::{can_act_on_member, has_permission, is_member, Permission};
use crate::ports::places_api::PlacesError;
use crate::AppState;

pub fn is_federation_envelope(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let Some(obj) = v.as_object() else {
        return false;
    };
    obj.contains_key("domain") && obj.contains_key("message") && obj.contains_key("signature")
}

struct MultipartFields {
    name: Option<String>,
    description: Option<String>,
    privacy: Option<String>,
    visibility: Option<String>,
    place_ids: Vec<String>,
    thumbnail: Option<Vec<u8>>,
}

fn boundary(headers: &HeaderMap) -> Option<String> {
    let ct = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    multer::parse_boundary(ct).ok()
}

async fn parse_multipart(boundary: String, body: Bytes) -> Result<MultipartFields, Response> {
    let stream = futures_util::stream::once(async move { Ok::<Bytes, std::io::Error>(body) });
    let mut mp = multer::Multipart::new(stream, boundary);
    let mut out = MultipartFields {
        name: None,
        description: None,
        privacy: None,
        visibility: None,
        place_ids: Vec::new(),
        thumbnail: None,
    };
    loop {
        let field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    format!("invalid multipart: {}", e),
                ))
            }
        };
        let fname = field.name().unwrap_or("").to_string();
        match fname.as_str() {
            "thumbnail" => {
                let data = field.bytes().await.unwrap_or_default();
                out.thumbnail = if data.is_empty() {
                    None
                } else {
                    Some(data.to_vec())
                };
            }
            "name" => out.name = Some(field.text().await.unwrap_or_default()),
            "description" => out.description = Some(field.text().await.unwrap_or_default()),
            "privacy" => out.privacy = Some(field.text().await.unwrap_or_default().to_lowercase()),
            "visibility" => {
                out.visibility = Some(field.text().await.unwrap_or_default().to_lowercase())
            }
            "placeIds" => {
                let raw = field.text().await.unwrap_or_default();
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&raw) {
                    out.place_ids = parsed;
                } else if !raw.trim().is_empty() {
                    out.place_ids = raw
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    Ok(out)
}

fn err(code: StatusCode, message: impl Into<String>) -> Response {
    (code, Json(json!({ "message": message.into() }))).into_response()
}

fn auth(headers: &HeaderMap, method: &str, path: &str) -> Result<String, Response> {
    require_signer(headers, method, path)
        .map(|s| s.to_lowercase())
        .map_err(|e| err(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e)))
}

fn parse_uuid(s: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(s).map_err(|_| err(StatusCode::BAD_REQUEST, "invalid community id"))
}

fn map_db<T>(r: Result<T, sqlx::Error>) -> Result<T, Response> {
    r.map_err(|e| {
        tracing::error!(error = %e, "communities client-write database error");
        err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })
}

fn map_api(e: crate::http::ApiError) -> Response {
    match e {
        crate::http::ApiError::Http(h) => {
            let code = StatusCode::from_u16(h.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            err(code, h.message)
        }
        other => err(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}

/// Persist uploaded thumbnail bytes to the ContentStore and record the hash +
/// `has_thumbnail` on `community_ranking_metrics`. A store failure is fatal so
/// we never advertise a thumbnail we cannot serve.
async fn store_thumbnail<'a, E>(
    executor: E,
    store: &crate::content_store::ContentStore,
    community_id: Uuid,
    bytes: &[u8],
) -> Result<(), Response>
where
    E: sqlx::PgExecutor<'a>,
{
    let hash = store.put(bytes).await.map_err(|e| match e {
        crate::content_store::ContentError::TooLarge { max } => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("thumbnail exceeds {} bytes", max),
        ),
        other => {
            tracing::error!(error = %other, "failed to store community thumbnail");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to store thumbnail",
            )
        }
    })?;
    map_db(
        sqlx::query(
            "INSERT INTO community_ranking_metrics (community_id, has_thumbnail, thumbnail_hash, updated_at) \
             VALUES ($1, TRUE, $2, now()) \
             ON CONFLICT (community_id) DO UPDATE SET has_thumbnail = TRUE, thumbnail_hash = EXCLUDED.thumbnail_hash, updated_at = now()",
        )
        .bind(community_id)
        .bind(&hash)
        .execute(executor)
        .await,
    )?;
    Ok(())
}

async fn community_active(state: &AppState, id: Uuid) -> Result<bool, Response> {
    let active: Option<bool> = map_db(
        sqlx::query_scalar("SELECT active FROM communities WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await,
    )?;
    match active {
        Some(a) => Ok(a),
        None => Err(err(
            StatusCode::NOT_FOUND,
            format!("Community not found: {}", id),
        )),
    }
}

pub async fn create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let signer = match auth(&headers, "post", "/v1/communities") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some(b) = boundary(&headers) else {
        return err(StatusCode::BAD_REQUEST, "expected multipart/form-data");
    };
    let fields = match parse_multipart(b, body).await {
        Ok(f) => f,
        Err(e) => return e,
    };

    let name = fields.name.unwrap_or_default();
    let description = fields.description.unwrap_or_default();
    let privacy = fields.privacy.unwrap_or_else(|| "public".to_string());
    let visibility = fields.visibility.unwrap_or_else(|| "all".to_string());
    let place_ids = fields.place_ids;
    let thumbnail = fields.thumbnail;
    let has_thumbnail = thumbnail.is_some();

    if let Err(e) = crate::validate::validate_name(&name) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description(&description) {
        return err(StatusCode::BAD_REQUEST, e);
    }

    // Upstream gate: the owner must hold at least one claimed DCL name
    // (`catalystClient.getOwnedNames(owner).length === 0 -> NotAuthorized`).
    // Fail closed only when the name oracle answers "no name"; if the content DB
    // is unavailable the gate is skipped (no oracle to consult).
    if let Some(false) = state.profiles.has_owned_name(&signer).await {
        return err(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have any names", signer),
        );
    }

    // Upstream `communityPlaces.validateOwnership(placeIds, owner)` — the owner
    // must own every place they associate at creation time.
    if let Err(e) = validate_places_ownership(&state, &place_ids, &signer).await {
        return e;
    }

    let private = privacy == "private";
    let unlisted = visibility == "unlisted";
    let id = Uuid::new_v4();

    let mut tx = match map_db(state.pool.begin().await) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let ins = sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted, created_at, updated_at) \
         VALUES ($1,$2,$3,$4,$5,TRUE,$6,now(),now())",
    )
    .bind(id)
    .bind(&name)
    .bind(&description)
    .bind(&signer)
    .bind(private)
    .bind(unlisted)
    .execute(&mut *tx)
    .await;
    if let Err(e) = ins {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    let memb = sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role, joined_at) \
         VALUES ($1,$2,'owner', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
    )
    .bind(id)
    .bind(&signer)
    .execute(&mut *tx)
    .await;
    if let Err(e) = memb {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = store_thumbnail(&mut *tx, &state.content_store, id, bytes).await {
            return e;
        }
    }
    for pid in &place_ids {
        let _ = sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(id)
        .bind(&signer)
        .execute(&mut *tx)
        .await;
    }
    if let Err(e) = map_db(tx.commit().await) {
        return e;
    }

    let privacy_out = if private { "private" } else { "public" };
    let visibility_out = if unlisted { "unlisted" } else { "all" };
    let thumb = if has_thumbnail {
        thumbnail_url(&state.cdn_url, &id.to_string())
    } else {
        "N/A".to_string()
    };
    let data = json!({
        "id": id,
        "name": name,
        "description": description,
        "ownerAddress": signer,
        "privacy": privacy_out,
        "visibility": visibility_out,
        "thumbnailUrl": thumb,
        "active": true,
        "role": "owner",
        "membersCount": 1,
    });
    (
        StatusCode::CREATED,
        Json(json!({ "message": "Community created successfully", "data": data })),
    )
        .into_response()
}

pub async fn update_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "put", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Upstream `validatePermissionToEditCommunity` -> `edit_info` (owner +
    // moderator), NOT an owner/admin-only gate.
    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::EditInfo,
        "edit the community",
    )
    .await
    {
        return e;
    }
    let Some(b) = boundary(&headers) else {
        return err(StatusCode::BAD_REQUEST, "expected multipart/form-data");
    };
    let fields = match parse_multipart(b, body).await {
        Ok(f) => f,
        Err(e) => return e,
    };
    let name = fields.name;
    let description = fields.description;
    if let Err(e) = crate::validate::validate_name_opt(name.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description_opt(description.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    let privacy: Option<bool> = fields.privacy.map(|p| p == "private");
    let visibility: Option<bool> = fields.visibility.map(|v| v == "unlisted");
    let thumbnail = fields.thumbnail;

    let upd = sqlx::query(
        "UPDATE communities SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description), \
            private = COALESCE($4, private), \
            unlisted = COALESCE($5, unlisted), \
            updated_at = now() \
          WHERE id = $1",
    )
    .bind(uuid)
    .bind(name.as_deref())
    .bind(description.as_deref())
    .bind(privacy)
    .bind(visibility)
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = store_thumbnail(&state.pool, &state.content_store, uuid, bytes).await {
            return e;
        }
    }

    let data = match state.communities.get_by_id(uuid, Some(&signer)).await {
        Ok(Some(mut obj)) => {
            let has_thumb = obj
                .as_object_mut()
                .and_then(|m| m.remove("_hasThumbnail"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if let Some(m) = obj.as_object_mut() {
                let thumb = if has_thumb {
                    thumbnail_url(&state.cdn_url, &uuid.to_string())
                } else {
                    "N/A".to_string()
                };
                m.insert("thumbnailUrl".to_string(), serde_json::Value::String(thumb));
            }
            obj
        }
        Ok(None) => return err(StatusCode::NOT_FOUND, "Community not found"),
        Err(e) => return map_api(e),
    };
    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct PatchBody {
    #[serde(rename = "editorsChoice", default)]
    pub editors_choice: Option<bool>,
}

pub async fn update_community_partially(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "patch", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Owner).await {
        return e;
    }
    let parsed: PatchBody = serde_json::from_slice(&body).unwrap_or(PatchBody {
        editors_choice: None,
    });
    if let Some(ec) = parsed.editors_choice {
        let upd = sqlx::query(
            "UPDATE communities SET editors_choice = $2, updated_at = now() WHERE id = $1",
        )
        .bind(uuid)
        .bind(ec)
        .execute(&state.pool)
        .await;
        if let Err(e) = map_db(upd) {
            return e;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Upstream `validatePermissionToDeleteCommunity` -> `delete_community`
    // (owner-only in the matrix).
    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::DeleteCommunity,
        "delete the community",
    )
    .await
    {
        return e;
    }
    let upd =
        sqlx::query("UPDATE communities SET active = FALSE, updated_at = now() WHERE id = $1")
            .bind(uuid)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}

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
    match community_active(&state, uuid).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::BAD_REQUEST, "Community is not active"),
        Err(e) => return e,
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
        // Self-removal == leave. Upstream `validatePermissionToLeaveCommunity`:
        // an owner cannot leave their own community.
        let role = load_role_uuid(&state, uuid, &signer).await;
        if role == Role::Owner {
            return err(
                StatusCode::UNAUTHORIZED,
                format!("The owner cannot leave the community {}", uuid),
            );
        }
    } else {
        // Kicking another member: `validatePermissionToKickMemberFromCommunity`
        // -> `canActOnMember(kickerRole, targetRole)`.
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
    if let Err(e) = map_db(del) {
        return e;
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

    // Upstream `validatePermissionToUpdateMemberRole`:
    //   - a user cannot update their own role
    //   - updater must hold `assign_roles` (owner-only)
    //   - `canActOnMember(updaterRole, targetRole)`
    //   - the new role can never be Owner (ownership transfer is a separate path)
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
    // Upstream `validatePermissionToBanMemberFromCommunity`: banner needs
    // `ban_players` and, if the target is a real member, must be able to act on
    // them (`canActOnMember`). Non-members can be pre-emptively banned.
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
    // Upstream `validatePermissionToUnbanMemberFromCommunity`: needs
    // `ban_players` and (since a banned user is not a current member) the
    // member-transition clause is vacuous.
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

#[derive(Debug, Deserialize)]
pub struct PlacesBody {
    #[serde(rename = "placeIds", default)]
    pub place_ids: Vec<String>,
}

pub async fn add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places", id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Upstream `validatePermissionToAddPlacesToCommunity` -> `add_places`
    // (owner + moderator).
    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::AddPlaces,
        "add places to the community",
    )
    .await
    {
        return e;
    }
    let parsed: PlacesBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    if parsed.place_ids.is_empty() {
        return err(StatusCode::BAD_REQUEST, "placeIds is required");
    }
    // Upstream `communityPlaces.validateOwnership(placeIds, placesOwner)`.
    if let Err(e) = validate_places_ownership(&state, &parsed.place_ids, &signer).await {
        return e;
    }
    for pid in &parsed.place_ids {
        let ins = sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(uuid)
        .bind(&signer)
        .execute(&state.pool)
        .await;
        if let Err(e) = map_db(ins) {
            return e;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct PathIdPlace {
    pub id: String,
    #[serde(rename = "placeId")]
    pub place_id: String,
}

pub async fn remove_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPlace { id, place_id }): Path<PathIdPlace>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places/{}", id, place_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Upstream `removePlace`: validate the caller owns the place, then — unless
    // they are the community owner — require the `remove_places` permission.
    if let Err(e) =
        validate_places_ownership(&state, std::slice::from_ref(&place_id), &signer).await
    {
        return e;
    }
    let member_role = load_role_uuid(&state, uuid, &signer).await;
    if member_role != Role::Owner {
        if let Err(e) = require_permission_uuid(
            &state,
            uuid,
            &signer,
            Permission::RemovePlaces,
            "remove places from the community",
        )
        .await
        {
            return e;
        }
    }
    let del = sqlx::query("DELETE FROM community_places WHERE id = $1 AND community_id = $2")
        .bind(&place_id)
        .bind(uuid)
        .execute(&state.pool)
        .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct PostBody {
    pub content: String,
}

pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts", id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    // Upstream `validatePermissionToCreatePost` -> `create_posts` (owner +
    // moderator; members carry no permissions in the matrix).
    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::CreatePosts,
        "create posts in the community",
    )
    .await
    {
        return e;
    }
    let parsed: PostBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let content = parsed.content.trim().to_string();
    if content.is_empty() {
        return err(StatusCode::BAD_REQUEST, "content is required");
    }
    let post_id = Uuid::new_v4();
    let ins = sqlx::query_as::<_, (Uuid, chrono::NaiveDateTime)>(
        "INSERT INTO community_posts (id, community_id, author_address, content, created_at) \
         VALUES ($1, $2, $3, $4, now()) RETURNING id, created_at",
    )
    .bind(post_id)
    .bind(uuid)
    .bind(&signer)
    .bind(&content)
    .fetch_one(&state.pool)
    .await;
    let (post_id, created_at) = match map_db(ins) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let data = json!({
        "id": post_id,
        "communityId": uuid,
        "authorAddress": signer,
        "content": content,
        "createdAt": created_at,
    });
    (StatusCode::CREATED, Json(json!({ "data": data }))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct PathIdPost {
    pub id: String,
    #[serde(rename = "postId")]
    pub post_id: String,
}

pub async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}", id, post_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let author: Option<String> = match map_db(
        sqlx::query_scalar(
            "SELECT author_address FROM community_posts WHERE id = $1 AND community_id = $2",
        )
        .bind(post_uuid)
        .bind(uuid)
        .fetch_optional(&state.pool)
        .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(author) = author else {
        return err(StatusCode::NOT_FOUND, "Post not found");
    };
    // Upstream `validatePermissionToDeletePost`: needs `delete_posts` (owner +
    // moderator); a moderator may only delete their own posts.
    let role = load_role_uuid(&state, uuid, &signer).await;
    let is_author = author.eq_ignore_ascii_case(&signer);
    if !has_permission(role, Permission::DeletePosts) || (role == Role::Mod && !is_author) {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to delete posts from the community",
                signer
            ),
        );
    }
    let del = sqlx::query("DELETE FROM community_posts WHERE id = $1 AND community_id = $2")
        .bind(post_uuid)
        .bind(uuid)
        .execute(&state.pool)
        .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = validate_like_unlike_access(&state, uuid, &signer).await {
        return e;
    }
    let exists: Option<bool> = match map_db(
        sqlx::query_scalar("SELECT TRUE FROM community_posts WHERE id = $1 AND community_id = $2")
            .bind(post_uuid)
            .bind(uuid)
            .fetch_optional(&state.pool)
            .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if exists.is_none() {
        return err(StatusCode::NOT_FOUND, "Post not found");
    }
    let ins = sqlx::query(
        "INSERT INTO community_post_likes (post_id, user_address, liked_at) \
         VALUES ($1, $2, now()) ON CONFLICT (post_id, user_address) DO NOTHING",
    )
    .bind(post_uuid)
    .bind(&signer)
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(ins) {
        return e;
    }
    StatusCode::CREATED.into_response()
}

pub async fn unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = validate_like_unlike_access(&state, uuid, &signer).await {
        return e;
    }
    let del =
        sqlx::query("DELETE FROM community_post_likes WHERE post_id = $1 AND user_address = $2")
            .bind(post_uuid)
            .bind(&signer)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct PathIdReq {
    pub id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestStatusBody {
    pub intention: String,
}

pub async fn update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let req_uuid = match Uuid::parse_str(&request_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid request id"),
    };
    let path = format!("/v1/communities/{}/requests/{}", id, request_id);
    let signer = match auth(&headers, "patch", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let parsed: RequestStatusBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let status = match parsed.intention.to_lowercase().as_str() {
        "accepted" => "accepted",
        "rejected" => "rejected",
        "cancelled" => "cancelled",
        other => {
            return err(
                StatusCode::BAD_REQUEST,
                format!("invalid intention: {}", other),
            )
        }
    };

    let row: Option<(String, String, String)> = match map_db(
        sqlx::query_as(
            "SELECT member_address, type, status FROM community_requests WHERE id = $1 AND community_id = $2",
        )
        .bind(req_uuid)
        .bind(uuid)
        .fetch_optional(&state.pool)
        .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some((member_address, kind, _cur)) = row else {
        return err(StatusCode::NOT_FOUND, "Request not found");
    };

    // Port of upstream `validateInvitePermissions` / `validateJoinPermissions`.
    // `accept_requests`+`reject_requests` map to the manager permission (owner +
    // moderator); the request subject is `member_address`.
    let self_caller = member_address.eq_ignore_ascii_case(&signer);
    let manager_check = || async {
        let role = load_role_uuid(&state, uuid, &signer).await;
        if has_permission(role, Permission::AcceptRequests)
            && has_permission(role, Permission::RejectRequests)
        {
            Ok(())
        } else {
            Err(err(
                StatusCode::UNAUTHORIZED,
                format!(
                    "The user {} doesn't have permission to accept and reject requests",
                    signer
                ),
            ))
        }
    };
    let auth_err: Option<Response> = match (kind.as_str(), status) {
        ("invite", "cancelled") => {
            if self_caller {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Invited user cannot cancel their invite",
                ))
            } else {
                manager_check().await.err()
            }
        }
        ("invite", _) => {
            if self_caller {
                None
            } else {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Only invited user can accept or reject invites",
                ))
            }
        }
        ("request_to_join", "cancelled") => {
            if self_caller {
                None
            } else {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Only requesting user can cancel their request",
                ))
            }
        }
        ("request_to_join", _) => {
            if self_caller {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Requesting user cannot accept or reject their own request",
                ))
            } else {
                manager_check().await.err()
            }
        }
        _ => None,
    };
    if let Some(e) = auth_err {
        return e;
    }

    let upd =
        sqlx::query("UPDATE community_requests SET status = $2, updated_at = now() WHERE id = $1")
            .bind(req_uuid)
            .bind(status)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(upd) {
        return e;
    }

    if status == "accepted" {
        let banned: Option<bool> = sqlx::query_scalar(
            "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
        )
        .bind(uuid)
        .bind(&member_address)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        if !banned.unwrap_or(false) {
            let _ = sqlx::query(
                "INSERT INTO community_members (community_id, member_address, role, joined_at) \
                 VALUES ($1, $2, 'member', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
            )
            .bind(uuid)
            .bind(&member_address)
            .execute(&state.pool)
            .await;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

fn stored_role(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Mod => "moderator",
        Role::Member => "member",
        Role::Banned => "banned",
        Role::None => "none",
    }
}

async fn load_role_uuid(state: &AppState, community_id: Uuid, member: &str) -> Role {
    let row: Option<String> = sqlx::query_scalar(
        "SELECT role FROM community_members WHERE community_id = $1 AND member_address = $2",
    )
    .bind(community_id)
    .bind(member.to_lowercase())
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    row.and_then(|r| Role::parse(&r)).unwrap_or(Role::None)
}

async fn require_min_role_uuid(
    state: &AppState,
    community_id: Uuid,
    signer: &str,
    min: Role,
) -> Result<Role, Response> {
    let banned: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(community_id)
    .bind(signer.to_lowercase())
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    if banned.unwrap_or(false) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "Forbidden: banned from this community",
        ));
    }
    let actual = load_role_uuid(state, community_id, signer).await;
    if actual < min {
        return Err(err(
            StatusCode::FORBIDDEN,
            format!(
                "Forbidden: signer role {} below required {}",
                actual.as_str(),
                min.as_str()
            ),
        ));
    }
    Ok(actual)
}

/// Upstream `validatePermission(permission, action)` — resolves the signer's
/// role and enforces the permission matrix (roles.ts). Banned signers are denied
/// up front; the matrix grants nothing to `none`/`banned`/`member`.
async fn require_permission_uuid(
    state: &AppState,
    community_id: Uuid,
    signer: &str,
    permission: Permission,
    action: &str,
) -> Result<Role, Response> {
    let banned: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(community_id)
    .bind(signer.to_lowercase())
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    if banned.unwrap_or(false) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "Forbidden: banned from this community",
        ));
    }
    let role = load_role_uuid(state, community_id, signer).await;
    if !has_permission(role, permission) {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have permission to {}", signer, action),
        ));
    }
    Ok(role)
}

/// Port of `validatePermissionsToLikeAndUnlikePost`: in a private community the
/// signer must be a member (role != None); in any community the signer must not
/// be banned. Public communities allow non-members to (un)like.
async fn validate_like_unlike_access(
    state: &AppState,
    community_id: Uuid,
    signer: &str,
) -> Result<(), Response> {
    let private = state
        .communities
        .is_private(community_id)
        .await
        .unwrap_or(false);
    let role = load_role_uuid(state, community_id, signer).await;
    if private && role == Role::None {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            format!(
                "{} is not a member of private community {}. You need to be a member to like/unlike posts in this community.",
                signer, community_id
            ),
        ));
    }
    let banned: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(community_id)
    .bind(signer.to_lowercase())
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    if banned.unwrap_or(false) {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            format!(
                "{} is banned from community {}. You cannot like/unlike posts in this community.",
                signer, community_id
            ),
        ));
    }
    Ok(())
}

/// Port of `communityPlaces.validateOwnership`: every place must resolve to a
/// place the signer owns. When `PLACES_API_URL` is unconfigured the check is
/// skipped (the deployment carries no places API), matching a deployment that
/// has no ownership oracle wired rather than fabricating a result.
async fn validate_places_ownership(
    state: &AppState,
    place_ids: &[String],
    signer: &str,
) -> Result<(), Response> {
    if place_ids.is_empty() || !state.places_api.is_configured() {
        return Ok(());
    }
    match state.places_api.validate_ownership(place_ids, signer).await {
        Ok(_) => Ok(()),
        Err(PlacesError::NotOwner(msg)) => Err(err(StatusCode::UNAUTHORIZED, msg)),
        Err(PlacesError::Unconfigured) => Ok(()),
        Err(PlacesError::Upstream(msg)) => {
            tracing::error!(error = %msg, "places ownership validation failed");
            Err(err(
                StatusCode::BAD_GATEWAY,
                "failed to validate place ownership",
            ))
        }
    }
}
