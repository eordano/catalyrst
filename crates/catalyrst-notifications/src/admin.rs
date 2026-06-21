//! Bearer-token gate for the admin broadcast route.
//!
//! Mirrors the timing-safe compare used by catalyrst-comms `authorize_moderator`.
//! The token is read from `CATALYRST_NOTIFICATIONS_ADMIN_TOKEN`; when it is unset
//! the route fails closed (403) so the broadcast surface can never be reached
//! without an explicitly configured secret.

use axum::http::HeaderMap;

use crate::http::ApiError;
use crate::AppState;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

/// Constant-time string comparison (no early return on first mismatch).
pub fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Authorize an admin request. Returns 403 when the token env is unset (fail
/// closed) or when the presented bearer token does not match.
pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    check_admin(state.admin_token.as_deref(), bearer_token(headers))
}

/// Core gate, independent of `AppState`/`HeaderMap` so it can be unit-tested.
/// Fails closed: `expected == None` is always rejected.
fn check_admin(expected: Option<&str>, presented: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Err(ApiError::Forbidden(
            "admin broadcast disabled: CATALYRST_NOTIFICATIONS_ADMIN_TOKEN is not set".to_string(),
        ));
    };
    match presented {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::Forbidden(
            "You are not authorized to access this resource".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_forbidden(r: Result<(), ApiError>) -> bool {
        matches!(r, Err(ApiError::Forbidden(_)))
    }

    #[test]
    fn fails_closed_when_token_unset() {
        // Even a "correct-looking" bearer is rejected when no token configured.
        assert!(is_forbidden(check_admin(None, Some("anything"))));
        assert!(is_forbidden(check_admin(None, None)));
    }

    #[test]
    fn rejects_missing_or_wrong_token() {
        assert!(is_forbidden(check_admin(Some("secret"), None)));
        assert!(is_forbidden(check_admin(Some("secret"), Some("nope"))));
        assert!(is_forbidden(check_admin(Some("secret"), Some("secre"))));
    }

    #[test]
    fn accepts_matching_token() {
        assert!(check_admin(Some("secret"), Some("secret")).is_ok());
    }

    #[test]
    fn timing_safe_eq_basics() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "ab"));
        assert!(!timing_safe_eq("", "x"));
        assert!(timing_safe_eq("", ""));
    }

    #[test]
    fn parses_bearer_prefix() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer tok123".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("tok123"));

        let mut h2 = HeaderMap::new();
        h2.insert("authorization", "Basic tok123".parse().unwrap());
        assert_eq!(bearer_token(&h2), None);
    }
}
