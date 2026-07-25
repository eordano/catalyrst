use axum::http::HeaderMap;

use crate::AppState;

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

fn check(expected: Option<&str>, presented: Option<&str>) -> bool {
    match (expected, presented) {
        (Some(expected), Some(token)) => timing_safe_eq(token, expected),
        _ => false,
    }
}

pub(crate) fn is_admin(state: &AppState, headers: &HeaderMap) -> bool {
    check(
        state.admin_token.as_deref(),
        bearer_token(headers).as_deref(),
    )
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
