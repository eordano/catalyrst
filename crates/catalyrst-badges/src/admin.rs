//! Bearer-gated authorization for the admin grant/revoke endpoints.
//! Fails closed: when `CATALYRST_BADGES_ADMIN_TOKEN` is unset every admin route
//! returns 403.

use axum::http::HeaderMap;

use crate::http::errors::ApiError;
use crate::AppState;

/// Constant-time compare (length leaks via the early return, as in the comms helper).
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

/// Fails closed: `expected == None` (token unset) ⇒ `Err`.
fn check_admin(expected: Option<&str>, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = expected.ok_or_else(|| ApiError::forbidden("admin token not configured"))?;
    let token = bearer_token(headers)
        .ok_or_else(|| ApiError::forbidden("missing or invalid bearer token"))?;
    if timing_safe_eq(&token, expected) {
        Ok(())
    } else {
        Err(ApiError::forbidden("missing or invalid bearer token"))
    }
}

/// Authorize an admin mutation. `Ok(())` only when a token is configured AND the
/// request carries a matching `Authorization: Bearer` header; 403 otherwise.
pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    check_admin(state.admin_token.as_deref(), headers)
}

/// Audit actor label for an admin mutation. The trustworthy source is the
/// `X-Catalyrst-Admin` header, set server-side by the admin console; absent it
/// falls back to `"admin-token"`.
pub fn admin_actor(headers: &HeaderMap) -> String {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(100).collect())
        .unwrap_or_else(|| "admin-token".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hm(v: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("authorization", v.parse().unwrap());
        h
    }

    #[test]
    fn timing_safe_eq_basic() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "abcd"));
        assert!(!timing_safe_eq("", "x"));
    }

    #[test]
    fn bearer_token_parsing() {
        assert_eq!(bearer_token(&hm("Bearer xyz")).as_deref(), Some("xyz"));
        assert_eq!(bearer_token(&hm("Basic xyz")), None);
        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }

    fn is_forbidden(r: Result<(), ApiError>) -> bool {
        matches!(r, Err(ApiError::Forbidden(_)))
    }

    #[test]
    fn unset_token_fails_closed() {
        assert!(is_forbidden(check_admin(None, &hm("Bearer anything"))));
        assert!(is_forbidden(check_admin(None, &HeaderMap::new())));
    }

    #[test]
    fn missing_or_wrong_bearer_forbidden() {
        assert!(is_forbidden(check_admin(Some("secret"), &HeaderMap::new())));
        assert!(is_forbidden(check_admin(
            Some("secret"),
            &hm("Bearer wrong")
        )));
        assert!(is_forbidden(check_admin(
            Some("secret"),
            &hm("Basic secret")
        )));
    }

    #[test]
    fn correct_bearer_authorizes() {
        assert!(check_admin(Some("secret"), &hm("Bearer secret")).is_ok());
    }
}
