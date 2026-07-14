use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::rpc::proto::v2::ConnectivityStatus;
use crate::rpc::AppState;

const ADMIN_TOKEN_ENV: &str = "CATALYRST_SOCIAL_RPC_ADMIN_TOKEN";

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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/presence", get(get_presence))
        .route("/voice-calls", get(get_voice_calls))
        .route("/friendships/{address}", get(get_friendships))
        .route("/disconnect", post(post_disconnect))
        .route("/force-presence", post(post_force_presence))
        .route("/reset-settings", post(post_reset_settings))
}

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
        .list_active_private_voice_chats(limit, state.ctx.cfg().private_voice_chat_expiration_ms)
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

    #[test]
    fn admin_gate_fails_closed_and_compares_constant_time() {
        std::env::remove_var(ADMIN_TOKEN_ENV);
        assert_eq!(
            authorize_admin(&headers_with(Some("anything"))),
            Err(StatusCode::FORBIDDEN),
            "unset token env must 403"
        );

        std::env::set_var(ADMIN_TOKEN_ENV, "s3cret-token");

        assert_eq!(
            authorize_admin(&headers_with(None)),
            Err(StatusCode::FORBIDDEN),
            "missing bearer must 403"
        );

        assert_eq!(
            authorize_admin(&headers_with(Some("wrong"))),
            Err(StatusCode::FORBIDDEN),
            "wrong bearer must 403"
        );

        assert_eq!(
            authorize_admin(&headers_with(Some("s3cret-token"))),
            Ok(()),
            "correct bearer must pass"
        );

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
