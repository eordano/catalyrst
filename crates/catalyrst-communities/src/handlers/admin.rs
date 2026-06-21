//! Admin console controls owned by catalyrst-communities
//! (docs/admin-console.md §4): community suspend/unsuspend and a filtered admin
//! community list.
//!
//! Every route here is gated by a bearer token compared in constant time
//! against the crate's existing admin token env (`API_ADMIN_TOKEN`, surfaced as
//! `AppStateInner::admin_token`). When that env is unset the gate fails closed
//! (403) so a default deploy exposes no admin surface. These controls act on
//! local catalyst state only; they are NOT minted as EIP-712 federation actions.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::http::{get_first, get_pagination_params, Paginated};
use crate::AppState;

/// Constant-time compare, mirroring `catalyrst-comms` `timing_safe_eq` /
/// `authorize_moderator`. Length is intentionally allowed to leak (same as
/// upstream) — only the byte comparison is made non-short-circuiting.
pub(crate) fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
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

/// Returns an identity string for the authenticated admin, or a 403 response.
/// Fails closed (403) when no admin token is configured.
fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                json!({ "ok": false, "message": "admin controls disabled (API_ADMIN_TOKEN unset)" }),
            ),
        ));
    };
    match bearer_token(headers) {
        Some(got) if timing_safe_eq(got.as_bytes(), expected.as_bytes()) => {
            Ok("admin-token".to_string())
        }
        _ => Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "ok": false, "message": "admin bearer token required" })),
        )),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SuspendBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /v1/admin/communities/{id}/suspend` — bearer-gated. Marks the community
/// suspended (hidden from public/member reads). Optional body `{ "reason": ".." }`.
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

/// `POST /v1/admin/communities/{id}/unsuspend` — bearer-gated. Clears the
/// suspension, restoring the community's prior visibility.
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
            Json(json!({ "ok": false, "message": "invalid community id" })),
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
            Json(json!({ "ok": false, "message": format!("Community not found: {}", id_str) })),
        ),
        Err(e) => {
            tracing::error!(error = %e, community_id = %id, "admin set_suspended failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "ok": false, "message": "database error" })),
            )
        }
    }
}

/// `GET /v1/admin/communities` — bearer-gated. Filtered admin list over ALL
/// communities (any privacy/visibility/suspension/active state). Query params:
/// `status` (all|active|suspended|inactive, default all), `owner` (0x address),
/// `search` (name ILIKE), plus `limit`/`offset` pagination.
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
                Json(json!({ "ok": false, "message": "database error" })),
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
