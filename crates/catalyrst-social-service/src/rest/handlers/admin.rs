use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::rest::http::{get_first, get_pagination_params, Paginated};
use crate::rest::AppState;

fn err_body(message: impl Into<String>) -> serde_json::Value {
    json!(catalyrst_types::ApiErrorBody::new(message))
}

pub(crate) fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    // Hash both to a fixed 32 bytes before comparing so the timing never depends
    // on input length: a bare length-mismatch early return would leak the
    // expected token's length. Matches upstream's SHA-256 digest compare (#461).
    use sha2::{Digest, Sha256};
    let da = Sha256::digest(a);
    let db = Sha256::digest(b);
    let mut diff = 0u8;
    for (x, y) in da.iter().zip(db.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(err_body("admin controls disabled (API_ADMIN_TOKEN unset)")),
        ));
    };
    match bearer_token(headers) {
        Some(got) if timing_safe_eq(got.as_bytes(), expected.as_bytes()) => {
            Ok("admin-token".to_string())
        }
        _ => Err((
            StatusCode::FORBIDDEN,
            Json(err_body("admin bearer token required")),
        )),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SuspendBody {
    #[serde(default)]
    pub reason: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/admin/communities/{id}/suspend",
    tag = "admin",
    params(("id" = String, Path)),
    request_body(content = serde_json::Value, description = "{ reason? }"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn suspend_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    body: Option<Json<SuspendBody>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    set_suspension(
        &state,
        &id_str,
        true,
        &actor,
        body.and_then(|Json(b)| b.reason),
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/admin/communities/{id}/unsuspend",
    tag = "admin",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn unsuspend_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    set_suspension(&state, &id_str, false, &actor, None).await
}

async fn set_suspension(
    state: &AppState,
    id_str: &str,
    suspended: bool,
    actor: &str,
    reason: Option<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Ok(id) = Uuid::parse_str(id_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(err_body("invalid community id")),
        );
    };
    match state
        .communities
        .set_suspended(id, suspended, actor, reason.as_deref())
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "ok": true, "id": id, "suspended": suspended })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(err_body(format!("Community not found: {}", id_str))),
        ),
        Err(e) => {
            tracing::error!(error = %e, community_id = %id, "admin set_suspended failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err_body("database error")),
            )
        }
    }
}

#[utoipa::path(
    get,
    path = "/v1/admin/communities",
    tag = "admin",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn list_communities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let pagination = get_pagination_params(&pairs);
    let status = get_first(&pairs, "status").unwrap_or_else(|| "all".to_string());
    let owner = get_first(&pairs, "owner");
    let search = get_first(&pairs, "search");

    match state
        .communities
        .admin_list(&pagination, &status, owner.as_deref(), search.as_deref())
        .await
    {
        Ok((results, total)) => {
            let paginated = Paginated::new(results, total, &pagination);
            (StatusCode::OK, Json(json!({ "data": paginated })))
        }
        Err(e) => {
            tracing::error!(error = %e, "admin list_communities failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(err_body("database error")),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_safe_eq_matches_and_mismatches() {
        assert!(timing_safe_eq(b"secret", b"secret"));
        assert!(!timing_safe_eq(b"secret", b"secreT"));
        assert!(!timing_safe_eq(b"secret", b"secret-longer"));
        assert!(!timing_safe_eq(b"", b"x"));
    }

    #[test]
    fn bearer_token_parses_prefix() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc123"));

        let mut h2 = HeaderMap::new();
        h2.insert("authorization", "Basic abc123".parse().unwrap());
        assert_eq!(bearer_token(&h2), None);

        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }
}
