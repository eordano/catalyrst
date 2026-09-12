use axum::http::HeaderMap;

use crate::http::ApiError;
use crate::AppState;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

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

pub fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    check_admin(state.admin_token.as_deref(), bearer_token(headers))
}

fn check_admin(expected: Option<&str>, presented: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected.filter(|s| !s.is_empty()) else {
        return Err(ApiError::forbidden(
            "admin broadcast disabled: CATALYRST_NOTIFICATIONS_ADMIN_TOKEN is not set",
        ));
    };
    match presented {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::forbidden(
            "You are not authorized to access this resource",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_forbidden(r: Result<(), ApiError>) -> bool {
        matches!(r, Err(ApiError::Http { status: 403, .. }))
    }

    #[test]
    fn fails_closed_when_token_unset() {
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

    #[test]
    fn probe_empty_bearer_header_parses_to_empty_token() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer ".parse().unwrap());
        assert_eq!(bearer_token(&h), Some(""));
    }

    #[test]
    fn probe_empty_presented_token_with_set_secret_rejected() {
        assert!(is_forbidden(check_admin(Some("secret"), Some(""))));
        assert!(is_forbidden(check_admin(Some("secret"), None)));
    }

    #[test]
    fn probe_fails_closed_when_secret_empty() {
        assert!(is_forbidden(check_admin(Some(""), Some(""))));
        assert!(is_forbidden(check_admin(Some(""), None)));
    }
}
