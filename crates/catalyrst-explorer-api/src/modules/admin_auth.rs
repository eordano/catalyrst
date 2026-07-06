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
