use axum::body::Bytes;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use uuid::Uuid;

use crate::rest::auth_chain::require_signer;
use crate::rest::fed::authority::Role;
use crate::rest::handlers::permissions::{has_permission, Permission};
use crate::rest::ports::places_api::PlacesError;
use crate::rest::AppState;

mod communities;
mod members;
mod places;
mod posts;
mod requests;

pub use communities::{
    create_community, delete_community, update_community, update_community_partially, PatchBody,
};
pub use members::{
    add_member, ban_member, remove_member, unban_member, update_member_role, PathIdAddr, RoleBody,
};
pub use places::{add_places, remove_place, PathIdPlace, PlacesBody};
pub use posts::{create_post, delete_post, like_post, unlike_post, PathIdPost, PostBody};
pub use requests::{update_request_status, PathIdReq, RequestStatusBody};

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

fn map_api(e: crate::rest::http::ApiError) -> Response {
    match e {
        crate::rest::http::ApiError::Http(h) => {
            let code = StatusCode::from_u16(h.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            err(code, h.message)
        }
        other => err(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}

async fn store_thumbnail<'a, E>(
    executor: E,
    store: &crate::rest::content_store::ContentStore,
    community_id: Uuid,
    bytes: &[u8],
) -> Result<(), Response>
where
    E: sqlx::PgExecutor<'a>,
{
    let hash = store.put(bytes).await.map_err(|e| match e {
        crate::rest::content_store::ContentError::TooLarge { max } => err(
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
