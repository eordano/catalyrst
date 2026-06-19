//! Bearer-token gate for the admin price-override routes.
//!
//! Mirrors the timing-safe compare used elsewhere in the workspace (e.g.
//! `catalyrst-comms` `authorize_moderator` / `timing_safe_eq`). Fails closed:
//! if `CATALYRST_PRICE_ADMIN_TOKEN` is unset the gate always rejects (403).

use axum::http::HeaderMap;

use crate::AppState;

/// Constant-time string compare (avoids leaking the token via timing).
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

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Pure gate logic, factored out for testing without a DB-backed `AppState`.
///
/// Default-safe: when no admin token is configured (`expected = None`) this
/// always returns `false`, so every admin route 403s until an operator opts in
/// by setting `CATALYRST_PRICE_ADMIN_TOKEN`.
fn check(expected: Option<&str>, presented: Option<&str>) -> bool {
    match (expected, presented) {
        (Some(expected), Some(token)) => timing_safe_eq(token, expected),
        _ => false,
    }
}

/// Returns `true` iff the request carries a valid admin bearer token.
pub(crate) fn is_admin(state: &AppState, headers: &HeaderMap) -> bool {
    check(state.admin_token.as_deref(), bearer_token(headers).as_deref())
}

#[cfg(test)]
mod tests {
    use super::{check, timing_safe_eq};

    #[test]
    fn eq_matches_only_identical() {
        assert!(timing_safe_eq("secret", "secret"));
        assert!(!timing_safe_eq("secret", "secreT"));
        assert!(!timing_safe_eq("secret", "secret-longer"));
        assert!(!timing_safe_eq("", "x"));
    }

    #[test]
    fn gate_fails_closed_when_unconfigured() {
        // No admin token env ⇒ even a presented token is rejected (403).
        assert!(!check(None, Some("anything")));
        assert!(!check(None, None));
    }

    #[test]
    fn gate_requires_matching_bearer() {
        assert!(check(Some("s3cret"), Some("s3cret")));
        assert!(!check(Some("s3cret"), Some("wrong")));
        assert!(!check(Some("s3cret"), None));
    }
}
