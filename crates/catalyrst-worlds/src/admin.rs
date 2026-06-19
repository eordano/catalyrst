//! Admin authentication for the LATER admin-console tranche
//! (docs/admin-console.md §4 — catalyrst-worlds row).
//!
//! Every `/admin/*` route is gated by a bearer token compared in constant time
//! against `CATALYRST_WORLDS_ADMIN_TOKEN` (`Config::admin_token`). When the env
//! var is unset the gate fails closed (403), matching the console's default-safe
//! invariant. The compare mirrors `catalyrst-comms` `moderator::timing_safe_eq`.

use axum::http::HeaderMap;

use crate::http::ApiError;
use crate::AppState;

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time string compare (length-independent of content match).
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

/// Pure authorization decision: true iff `expected` is configured and the
/// request carries a matching bearer token. Fails closed when `expected` is
/// `None` (admin token env unset).
pub(crate) fn admin_authorized(expected: Option<&str>, headers: &HeaderMap) -> bool {
    match (expected, bearer_token(headers)) {
        (Some(expected), Some(token)) => timing_safe_eq(&token, expected),
        _ => false,
    }
}

/// Authorize an admin request. Fails closed (403) when the admin token env is
/// unset, when no `Authorization: Bearer` header is present, or when the token
/// does not match.
pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if admin_authorized(state.cfg.admin_token.as_deref(), headers) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "You are not authorized to access this resource",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{admin_authorized, timing_safe_eq};
    use axum::http::HeaderMap;

    fn with_auth(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("authorization", value.parse().unwrap());
        h
    }

    #[test]
    fn timing_safe_eq_matches() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "abcd"));
        assert!(!timing_safe_eq("", "x"));
        assert!(timing_safe_eq("", ""));
    }

    #[test]
    fn fails_closed_when_token_unset() {
        // No admin token configured ⇒ unauthorized even with a bearer present.
        assert!(!admin_authorized(None, &with_auth("Bearer anything")));
        assert!(!admin_authorized(None, &HeaderMap::new()));
    }

    #[test]
    fn rejects_missing_or_wrong_bearer() {
        let expected = Some("s3cret");
        assert!(!admin_authorized(expected, &HeaderMap::new()));
        assert!(!admin_authorized(expected, &with_auth("Bearer wrong")));
        assert!(!admin_authorized(expected, &with_auth("s3cret"))); // missing "Bearer "
        assert!(!admin_authorized(expected, &with_auth("Basic s3cret")));
    }

    #[test]
    fn accepts_matching_bearer() {
        assert!(admin_authorized(Some("s3cret"), &with_auth("Bearer s3cret")));
    }
}
