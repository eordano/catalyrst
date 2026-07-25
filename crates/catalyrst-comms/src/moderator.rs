use axum::http::HeaderMap;

use crate::auth_chain::{try_extract_signer, AUTH_METADATA_HEADER};
use crate::http::{unauthorized, ApiError};
use crate::AppState;

const MAX_MODERATOR_NAME_LENGTH: usize = 100;
const SCENE_SIGNER: &str = "decentraland-kernel-scene";

pub enum ModeratorMode {
    Read,
    Write,
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub(crate) fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn sanitize_moderator_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MODERATOR_NAME_LENGTH {
        return None;
    }
    let ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.');
    if ok {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn authorize_moderator(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    mode: ModeratorMode,
    moderator_query: Option<&str>,
) -> Result<String, ApiError> {
    if let Some(expected) = state.moderator_token.as_deref() {
        if let Some(token) = bearer_token(headers) {
            if timing_safe_eq(&token, expected) {
                return match mode {
                    ModeratorMode::Write => {
                        let raw = moderator_query.ok_or_else(|| {
                            ApiError::bad_request("Missing moderator query parameter")
                        })?;
                        sanitize_moderator_name(raw).ok_or_else(|| {
                            ApiError::bad_request(
                                "Invalid moderator query parameter. Must be alphanumeric (spaces, hyphens, underscores, and dots allowed) and at most 100 characters",
                            )
                        })
                    }
                    ModeratorMode::Read => Ok("moderator-token".to_string()),
                };
            }
        }
    }

    let meta_signer = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|m| m.get("signer").and_then(|v| v.as_str()).map(str::to_string));
    if meta_signer.as_deref() == Some(SCENE_SIGNER) {
        return Err(unauthorized(
            "You are not authorized to access this resource",
        ));
    }

    let signer = try_extract_signer(headers, method, path)
        .ok_or_else(|| unauthorized("You are not authorized to access this resource"))?
        .to_lowercase();

    if state.moderator_addresses.iter().any(|a| a == &signer) {
        Ok(signer)
    } else {
        Err(unauthorized(
            "You are not authorized to access this resource",
        ))
    }
}
