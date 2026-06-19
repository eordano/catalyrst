//! Bearer-token gate for admin (`/admin/*`) mutation + introspection routes.
//!
//! Mirrors the constant-time compare in `catalyrst-comms::moderator`. The token
//! is read from `CATALYRST_EXPLORER_API_ADMIN_TOKEN`. This crate has no existing
//! admin/moderator token env, so a new one is introduced.
//!
//! Fail-closed: if the env var is unset (or empty) every admin route returns 403,
//! so the console is read-only until an operator deliberately provisions a token.

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::env;

pub const ADMIN_TOKEN_ENV: &str = "CATALYRST_EXPLORER_API_ADMIN_TOKEN";

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time string compare (same shape as `catalyrst-comms::moderator::timing_safe_eq`).
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

/// Returns `Ok(())` when a valid bearer token is present, otherwise a `403` response.
///
/// Default-safe: when `CATALYRST_EXPLORER_API_ADMIN_TOKEN` is unset or empty,
/// this always fails closed (403) regardless of the request.
pub fn require_admin(headers: &HeaderMap) -> Result<(), Response> {
    let expected = match env::var(ADMIN_TOKEN_ENV) {
        Ok(v) if !v.is_empty() => v,
        _ => return Err(forbidden("admin token not configured")),
    };
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(&token, &expected) => Ok(()),
        _ => Err(forbidden("invalid or missing bearer token")),
    }
}

fn forbidden(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "forbidden", "message": msg })),
    )
        .into_response()
}
