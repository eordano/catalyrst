//! Bearer-token gate for the admin moderation routes
//! (`POST /api/events`, `PATCH /api/events/{id}`).
//!
//! Mirrors `catalyrst-comms::moderator` (constant-time compare, `Bearer`
//! extraction) and the admin-console design (docs/admin-console.md §4): a single
//! bearer token (`CATALYRST_EVENTS_ADMIN_TOKEN`) gates the operator. When the
//! token env is unset the gate fails closed — every admin route returns 403.

use axum::http::HeaderMap;

use crate::http::response::ApiError;
use crate::AppState;

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time string compare (length leak only), same as
/// `catalyrst-comms::moderator::timing_safe_eq`.
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

/// Pure gate decision, independent of `AppState`/axum so it is unit-testable.
/// `expected` is the configured admin token (`None` ⇒ disabled).
pub(crate) fn admin_ok(expected: Option<&str>, headers: &HeaderMap) -> bool {
    match expected {
        Some(exp) => bearer_token(headers)
            .map(|t| timing_safe_eq(&t, exp))
            .unwrap_or(false),
        None => false,
    }
}

/// Gate an admin route. Returns `Ok(())` only when the configured admin token is
/// present *and* the request carries a matching `Authorization: Bearer <token>`.
/// Fails closed with 403 when the token env is unset or the bearer is missing /
/// mismatched.
pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if admin_ok(state.admin_token.as_deref(), headers) {
        Ok(())
    } else if state.admin_token.is_none() {
        Err(ApiError::forbidden("Admin operations are disabled"))
    } else {
        Err(ApiError::forbidden(
            "You are not authorized to access this resource",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn hdrs(bearer: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(b) = bearer {
            h.insert("authorization", format!("Bearer {b}").parse().unwrap());
        }
        h
    }

    #[test]
    fn timing_safe_eq_matches_exact() {
        assert!(timing_safe_eq("secret", "secret"));
        assert!(!timing_safe_eq("secret", "secreT"));
        assert!(!timing_safe_eq("secret", "secret-longer"));
        assert!(!timing_safe_eq("", "x"));
    }

    #[test]
    fn fails_closed_when_token_unset() {
        // No configured token ⇒ even a "correct-looking" bearer is rejected.
        assert!(!admin_ok(None, &hdrs(Some("anything"))));
        assert!(!admin_ok(None, &hdrs(None)));
    }

    #[test]
    fn rejects_missing_or_wrong_bearer() {
        assert!(!admin_ok(Some("topsecret"), &hdrs(None)));
        assert!(!admin_ok(Some("topsecret"), &hdrs(Some("wrong"))));
    }

    #[test]
    fn accepts_matching_bearer() {
        assert!(admin_ok(Some("topsecret"), &hdrs(Some("topsecret"))));
    }
}
