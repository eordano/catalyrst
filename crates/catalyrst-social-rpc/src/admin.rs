//! Operator admin surface (admin-console.md §4, catalyrst-social-rpc LATER
//! tranche): admin reads (presence, friendship graph, active voice calls) plus
//! force-disconnect / force-presence-broadcast / reset-settings mutations.
//!
//! Every route here is gated by a bearer token compared in constant time
//! against `CATALYRST_SOCIAL_RPC_ADMIN_TOKEN`. When that env is unset the gate
//! fails closed (403). The token is read from the environment lazily at request
//! time so a test/process can set it after `Router` construction; in practice
//! the value is fixed for the lifetime of the process.
//!
//! These routes are additive: they do NOT change the WS handshake auth or any
//! existing HTTP route. They are meant to live behind the loopback/tailnet edge
//! (the public nginx config 404s `/admin`).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::proto::v2::ConnectivityStatus;
use crate::AppState;

/// Env var holding the admin bearer token. social-rpc has no pre-existing
/// admin/moderator token of its own (the comms gatekeeper token it forwards is
/// a *downstream* credential, not an operator gate), so we introduce the
/// workspace-standard name.
const ADMIN_TOKEN_ENV: &str = "CATALYRST_SOCIAL_RPC_ADMIN_TOKEN";

/// Constant-time string compare, mirroring catalyrst-comms `timing_safe_eq`.
fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Fail-closed bearer gate. Returns `Err(403)` when the token env is unset/empty
/// or the presented bearer does not match in constant time.
fn authorize_admin(headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = std::env::var(ADMIN_TOKEN_ENV)
        .ok()
        .filter(|s| !s.is_empty());
    match expected {
        Some(expected) => match bearer_token(headers) {
            Some(tok) if timing_safe_eq(&tok, &expected) => Ok(()),
            _ => Err(StatusCode::FORBIDDEN),
        },
        None => Err(StatusCode::FORBIDDEN),
    }
}

/// Routes nested under `/admin/social` by the binary's router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/presence", get(get_presence))
        .route("/voice-calls", get(get_voice_calls))
        .route("/friendships/{address}", get(get_friendships))
        .route("/disconnect", post(post_disconnect))
        .route("/force-presence", post(post_force_presence))
        .route("/reset-settings", post(post_reset_settings))
}

// ---- reads ----------------------------------------------------------------

async fn get_presence(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let mut users: Vec<Value> = state
        .ctx
        .presence_snapshot()
        .into_iter()
        .map(|(address, connections)| json!({ "address": address, "connections": connections }))
        .collect();
    users.sort_by(|a, b| a["address"].as_str().cmp(&b["address"].as_str()));
    Ok(Json(json!({
        "online": users.len(),
        "users": users,
    })))
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn get_voice_calls(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let calls = state
        .ctx
        .db()
        .list_active_private_voice_chats(limit)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "admin: list active voice calls failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let calls: Vec<Value> = calls
        .into_iter()
        .map(|(id, caller, callee, created_at, expires_at)| {
            json!({
                "id": id.to_string(),
                "caller": caller,
                "callee": callee,
                "created_at": created_at.to_rfc3339(),
                "expires_at": expires_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(json!({ "total": calls.len(), "calls": calls })))
}

#[derive(Debug, Deserialize)]
struct GraphQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_friendships(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(q): Query<GraphQuery>,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let db = state.ctx.db();
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);

    let friends = db.get_friends(&address, limit, offset).await.map_err(|e| {
        tracing::error!(error = %e, "admin: get_friends failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let friend_count = db.count_friends(&address).await.map_err(|e| {
        tracing::error!(error = %e, "admin: count_friends failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (blocked, blocked_by) = db.get_blocking_status(&address).await.map_err(|e| {
        tracing::error!(error = %e, "admin: get_blocking_status failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "address": address.to_lowercase(),
        "online": state.ctx.is_online(&address),
        "friend_count": friend_count,
        "friends": friends,
        "blocked": blocked,
        "blocked_by": blocked_by,
    })))
}

// ---- mutations ------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AddressBody {
    address: String,
}

async fn post_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddressBody>,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let kicked = state.ctx.disconnect_address(&body.address);
    Ok(Json(json!({
        "address": body.address.to_lowercase(),
        "disconnected": kicked,
    })))
}

#[derive(Debug, Deserialize)]
struct ForcePresenceBody {
    address: String,
    /// "online" | "offline" — the connectivity status to broadcast.
    status: String,
}

async fn post_force_presence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ForcePresenceBody>,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let status = match body.status.to_ascii_lowercase().as_str() {
        "online" => ConnectivityStatus::Online,
        "offline" | "away" => ConnectivityStatus::Offline,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    // Force-presence is an advisory broadcast: it re-fans the requested
    // connectivity status to the user's friends + communities (the same path a
    // real connect/disconnect uses). It does NOT fabricate a transport, so the
    // in-memory connection counter is left untouched — use `disconnect` to drop
    // real sockets. This is a real, well-defined operator effect (nudge clients
    // to refresh a stuck presence row) and never lies about an actual session.
    state.ctx.fan_connectivity(&body.address, status).await;
    Ok(Json(json!({
        "address": body.address.to_lowercase(),
        "broadcast": body.status.to_ascii_lowercase(),
    })))
}

async fn post_reset_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddressBody>,
) -> Result<Json<Value>, StatusCode> {
    authorize_admin(&headers)?;
    let existed = state
        .ctx
        .db()
        .reset_social_settings(&body.address)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "admin: reset_social_settings failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(json!({
        "address": body.address.to_lowercase(),
        "reset": true,
        "had_custom_settings": existed,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::AUTHORIZATION;

    fn headers_with(bearer: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(b) = bearer {
            h.insert(AUTHORIZATION, format!("Bearer {b}").parse().unwrap());
        }
        h
    }

    // One serialized test exercises every gate branch: env mutation is
    // process-global, so splitting these would race under the default parallel
    // test runner.
    #[test]
    fn admin_gate_fails_closed_and_compares_constant_time() {
        // 1. Token env unset => fail closed even with a bearer present.
        std::env::remove_var(ADMIN_TOKEN_ENV);
        assert_eq!(
            authorize_admin(&headers_with(Some("anything"))),
            Err(StatusCode::FORBIDDEN),
            "unset token env must 403"
        );

        // 2. Token configured.
        std::env::set_var(ADMIN_TOKEN_ENV, "s3cret-token");

        // no header => 403
        assert_eq!(
            authorize_admin(&headers_with(None)),
            Err(StatusCode::FORBIDDEN),
            "missing bearer must 403"
        );
        // wrong token => 403
        assert_eq!(
            authorize_admin(&headers_with(Some("wrong"))),
            Err(StatusCode::FORBIDDEN),
            "wrong bearer must 403"
        );
        // correct token => ok
        assert_eq!(
            authorize_admin(&headers_with(Some("s3cret-token"))),
            Ok(()),
            "correct bearer must pass"
        );

        // 3. Empty token env is treated as unset (fail closed).
        std::env::set_var(ADMIN_TOKEN_ENV, "");
        assert_eq!(
            authorize_admin(&headers_with(Some(""))),
            Err(StatusCode::FORBIDDEN),
            "empty token env must 403"
        );

        std::env::remove_var(ADMIN_TOKEN_ENV);
    }

    #[test]
    fn timing_safe_eq_basic() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "abcd"));
        assert!(!timing_safe_eq("", "x"));
        assert!(timing_safe_eq("", ""));
    }
}
