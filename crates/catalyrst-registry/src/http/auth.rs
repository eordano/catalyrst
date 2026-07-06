use axum::http::HeaderMap;

use dcl_contents::errors::ApiError;

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

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

pub(crate) fn has_valid_bearer(state: &AppState, headers: &HeaderMap) -> bool {
    bearer_matches(state.admin_token.as_deref(), headers)
}

fn bearer_matches(expected: Option<&str>, headers: &HeaderMap) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    if expected.is_empty() {
        return false;
    }
    match bearer_token(headers) {
        Some(token) => timing_safe_eq(token, expected),
        None => false,
    }
}

pub(crate) fn require_bearer(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if has_valid_bearer(state, headers) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Forbidden: valid admin bearer token required",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn hdrs(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert("authorization", HeaderValue::from_str(a).unwrap());
        }
        h
    }

    #[test]
    fn timing_safe_eq_matches_semantics() {
        assert!(timing_safe_eq("secret", "secret"));
        assert!(!timing_safe_eq("secret", "secres"));
        assert!(!timing_safe_eq("secret", "secret-longer"));
        assert!(!timing_safe_eq("", "x"));
    }

    #[test]
    fn fails_closed_when_token_unset() {
        assert!(!bearer_matches(None, &hdrs(Some("Bearer anything"))));
        assert!(!bearer_matches(Some(""), &hdrs(Some("Bearer "))));
    }

    #[test]
    fn rejects_missing_or_wrong_bearer() {
        let expected = Some("topsecret");
        assert!(!bearer_matches(expected, &hdrs(None)));
        assert!(!bearer_matches(expected, &hdrs(Some("Bearer wrong"))));
        assert!(!bearer_matches(expected, &hdrs(Some("Basic topsecret"))));
        assert!(!bearer_matches(expected, &hdrs(Some("topsecret"))));
    }

    #[test]
    fn accepts_exact_bearer() {
        assert!(bearer_matches(
            Some("topsecret"),
            &hdrs(Some("Bearer topsecret"))
        ));
    }
}
