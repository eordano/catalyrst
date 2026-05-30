use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::fed::authority::{can_grant, Role};
use crate::handlers::communities::thumbnail_url;
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
    has_thumbnail: bool,
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
        has_thumbnail: false,
    };
    loop {
        let field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return Err(err(StatusCode::BAD_REQUEST, format!("invalid multipart: {}", e))),
        };
        let fname = field.name().unwrap_or("").to_string();
        match fname.as_str() {
            "thumbnail" => {
                let data = field.bytes().await.unwrap_or_default();
                out.has_thumbnail = !data.is_empty();
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

async fn community_active(state: &AppState, id: Uuid) -> Result<bool, Response> {
    let active: Option<bool> =
        map_db(sqlx::query_scalar("SELECT active FROM communities WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await)?;
    match active {
        Some(a) => Ok(a),
        None => Err(err(StatusCode::NOT_FOUND, format!("Community not found: {}", id))),
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
    let has_thumbnail = fields.has_thumbnail;

    if let Err(e) = crate::validate::validate_name(&name) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description(&description) {
        return err(StatusCode::BAD_REQUEST, e);
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
    if has_thumbnail {
        let _ = sqlx::query(
            "INSERT INTO community_ranking_metrics (community_id, has_thumbnail, updated_at) \
             VALUES ($1, TRUE, now()) ON CONFLICT (community_id) DO UPDATE SET has_thumbnail = TRUE, updated_at = now()",
        )
        .bind(id)
        .execute(&mut *tx)
        .await;
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Admin).await {
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
    let has_thumbnail = fields.has_thumbnail;

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
    if has_thumbnail {
        let _ = sqlx::query(
            "INSERT INTO community_ranking_metrics (community_id, has_thumbnail, updated_at) \
             VALUES ($1, TRUE, now()) ON CONFLICT (community_id) DO UPDATE SET has_thumbnail = TRUE, updated_at = now()",
        )
        .bind(uuid)
        .execute(&state.pool)
        .await;
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
    let parsed: PatchBody = serde_json::from_slice(&body).unwrap_or(PatchBody { editors_choice: None });
    if let Some(ec) = parsed.editors_choice {
        let upd = sqlx::query("UPDATE communities SET editors_choice = $2, updated_at = now() WHERE id = $1")
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Owner).await {
        return e;
    }
    let upd = sqlx::query("UPDATE communities SET active = FALSE, updated_at = now() WHERE id = $1")
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
        sqlx::query_scalar("SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2")
            .bind(uuid)
            .bind(&signer)
            .fetch_optional(&state.pool)
            .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if banned.unwrap_or(false) {
        return err(StatusCode::FORBIDDEN, "The member is banned from this community");
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
    if target != signer {
        let actor = match require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
            Ok(r) => r,
            Err(e) => return e,
        };
        let target_role = load_role_uuid(&state, uuid, &target).await;
        if target_role >= actor {
            return err(StatusCode::FORBIDDEN, "cannot remove a peer or superior");
        }
    }
    let del = sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND member_address = $2")
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
    // HAZARD FIX (prior review: latent `role='admin'`): this is the LEGACY,
    // unsigned write path — it mutates `community_members.role` directly without
    // appending a signed `CommunityRole` to the federation log. Per
    // `docs/federation/communities.md §4`, every role transition (especially
    // elevations to `admin`/`owner`) MUST be a signed action so peers can
    // replay the authority chain deterministically. Letting an unsigned legacy
    // request mint an `admin` would create a role that exists in
    // `community_members` but not in `community_role_current` / the signed log,
    // diverging the two stores and granting authority that no peer can verify.
    // We therefore cap unsigned grants at `mod`; admin/owner grants must use the
    // signed federation endpoint (writes::update_member_role with a
    // Signed<CommunityRole> envelope), which records the log entry + projects
    // authoritatively.
    if matches!(new_role, Role::Admin | Role::Owner) {
        return err(
            StatusCode::FORBIDDEN,
            "granting admin/owner requires a signed CommunityRole action (federation envelope); \
             the unsigned endpoint can only grant mod/member (see docs/federation/communities.md §4)",
        );
    }
    let actor = match require_min_role_uuid(&state, uuid, &signer, Role::Admin).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    if !can_grant(actor, new_role) {
        return err(StatusCode::FORBIDDEN, "actor cannot grant this role");
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
    let actor = match require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
        Ok(r) => r,
        Err(e) => return e,
    };
    let target_role = load_role_uuid(&state, uuid, &target).await;
    if target_role >= actor {
        return err(StatusCode::FORBIDDEN, "cannot ban a peer or superior");
    }
    let mut tx = match map_db(state.pool.begin().await) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if let Err(e) = sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND member_address = $2")
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
        return e;
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Admin).await {
        return e;
    }
    let parsed: PlacesBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    if parsed.place_ids.is_empty() {
        return err(StatusCode::BAD_REQUEST, "placeIds is required");
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Admin).await {
        return e;
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Member).await {
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
        sqlx::query_scalar("SELECT author_address FROM community_posts WHERE id = $1 AND community_id = $2")
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
    if !author.eq_ignore_ascii_case(&signer) {
        if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
            return e;
        }
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Member).await {
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
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Member).await {
        return e;
    }
    let del = sqlx::query("DELETE FROM community_post_likes WHERE post_id = $1 AND user_address = $2")
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
        other => return err(StatusCode::BAD_REQUEST, format!("invalid intention: {}", other)),
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

    let self_caller = member_address.eq_ignore_ascii_case(&signer);
    let needs_mod = matches!(
        (kind.as_str(), status),
        ("request_to_join", "accepted") | ("request_to_join", "rejected") | ("invite", "cancelled")
    );
    if needs_mod {
        if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
            return e;
        }
    } else if !self_caller {
        if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Mod).await {
            return e;
        }
    }

    let upd = sqlx::query("UPDATE community_requests SET status = $2, updated_at = now() WHERE id = $1")
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
        Role::Admin => "admin",
        Role::Mod => "moderator",
        Role::Member => "member",
        Role::Banned => "banned",
        Role::None => "none",
    }
}

async fn load_role_uuid(state: &AppState, community_id: Uuid, member: &str) -> Role {
    let row: Option<String> =
        sqlx::query_scalar("SELECT role FROM community_members WHERE community_id = $1 AND member_address = $2")
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
    let banned: Option<bool> =
        sqlx::query_scalar("SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2")
            .bind(community_id)
            .bind(signer.to_lowercase())
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if banned.unwrap_or(false) {
        return Err(err(StatusCode::FORBIDDEN, "Forbidden: banned from this community"));
    }
    let actual = load_role_uuid(state, community_id, signer).await;
    if actual < min {
        return Err(err(
            StatusCode::FORBIDDEN,
            format!("Forbidden: signer role {} below required {}", actual.as_str(), min.as_str()),
        ));
    }
    Ok(actual)
}
