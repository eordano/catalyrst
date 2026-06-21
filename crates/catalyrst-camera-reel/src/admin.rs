//! Bearer-token gate for the moderator `/admin/*` routes.
//!
//! Mirrors the timing-safe compare used elsewhere in the workspace
//! (`catalyrst-comms::moderator::authorize_moderator`). Fails closed (403) when
//! the `CATALYRST_CAMERA_REEL_ADMIN_TOKEN` env (surfaced as `config.admin_token`)
//! is unset.

use axum::http::HeaderMap;

use crate::http::ApiError;
use crate::AppState;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

/// Constant-time string comparison (matches comms `timing_safe_eq`).
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

/// Returns `Ok(())` only when a valid admin bearer token is presented.
///
/// Fails closed with `Forbidden` when the admin token env is unset, and with
/// `Unauthorized` when a token is configured but the request did not present a
/// matching one.
pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    check_admin(state.config.admin_token.as_deref(), headers)
}

/// Pure token gate, separated from `AppState` so it can be unit-tested.
pub(crate) fn check_admin(expected: Option<&str>, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        // Fail closed: no token configured means no admin access.
        return Err(ApiError::Forbidden(
            "admin token not configured".to_string(),
        ));
    };
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
}

#[cfg(test)]
mod tests {
    use super::{check_admin, timing_safe_eq};
    use crate::http::ApiError;
    use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue};

    fn headers_with_bearer(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        h
    }

    #[test]
    fn check_admin_fails_closed_when_token_unset() {
        // No configured token -> 403 even with a bearer header present.
        let err = check_admin(None, &headers_with_bearer("anything")).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn check_admin_rejects_missing_bearer() {
        let err = check_admin(Some("expected"), &HeaderMap::new()).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized));
    }

    #[test]
    fn check_admin_rejects_wrong_token() {
        let err = check_admin(Some("expected"), &headers_with_bearer("wrong")).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized));
    }

    #[test]
    fn check_admin_accepts_matching_token() {
        assert!(check_admin(Some("expected"), &headers_with_bearer("expected")).is_ok());
    }

    #[test]
    fn timing_safe_eq_matches_equal_strings() {
        assert!(timing_safe_eq("s3cr3t-token", "s3cr3t-token"));
    }

    #[test]
    fn timing_safe_eq_rejects_different_strings() {
        assert!(!timing_safe_eq("s3cr3t-token", "s3cr3t-toker"));
    }

    #[test]
    fn timing_safe_eq_rejects_different_lengths() {
        assert!(!timing_safe_eq("short", "longer-token"));
        assert!(!timing_safe_eq("", "x"));
    }
}
