use axum::http::HeaderMap;

use crate::http::ApiError;
use crate::AppState;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

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

pub(super) fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    authorize_with_token(state.admin_token.as_deref(), headers)
}

fn authorize_with_token(expected: Option<&str>, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Err(ApiError::forbidden(
            "admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)",
        ));
    };
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::forbidden("invalid admin token")),
    }
}

fn clean_actor(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

pub(super) fn admin_actor(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_actor)
}

pub(super) fn validate_idempotency_key(raw: &Option<String>) -> Result<Option<String>, ApiError> {
    match raw {
        None => Ok(None),
        Some(k) => {
            let t = k.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 200 {
                Err(ApiError::bad_request("idempotencyKey too long (max 200)"))
            } else if !t.chars().all(|c| c.is_ascii_graphic()) {
                Err(ApiError::bad_request(
                    "idempotencyKey must be printable ASCII",
                ))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

pub(super) fn normalize_address(raw: &str) -> Result<String, ApiError> {
    let a = raw.trim().to_lowercase();
    let ok = a.len() == 42 && a.starts_with("0x") && a[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(a)
    } else {
        Err(ApiError::bad_request("invalid wallet address"))
    }
}

pub(crate) fn validate_positive_amount(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(ApiError::bad_request("invalid amount"));
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    let mut any_nonzero = false;
    for c in s.chars() {
        match c {
            '0'..='9' => {
                any_digit = true;
                if c != '0' {
                    any_nonzero = true;
                }
            }
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(ApiError::bad_request("invalid amount")),
        }
    }
    if !any_digit || !any_nonzero {
        return Err(ApiError::bad_request("amount must be a positive number"));
    }
    Ok(s.to_string())
}

pub(super) fn validate_max_mana(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(ApiError::bad_request("invalid maxMana"));
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    for c in s.chars() {
        match c {
            '0'..='9' => any_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(ApiError::bad_request("invalid maxMana")),
        }
    }
    if !any_digit {
        return Err(ApiError::bad_request("invalid maxMana"));
    }
    Ok(s.to_string())
}

const VALID_SEASON_STATES: [&str; 3] = ["NOT_STARTED", "IN_PROGRESS", "FINISHED"];

pub(super) fn validate_season_state(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_uppercase();
    if VALID_SEASON_STATES.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(ApiError::bad_request(
            "state must be NOT_STARTED, IN_PROGRESS, or FINISHED",
        ))
    }
}

pub(super) fn validated_reason(reason: &Option<String>) -> Result<Option<String>, ApiError> {
    match reason {
        None => Ok(None),
        Some(r) => {
            let t = r.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 500 {
                Err(ApiError::bad_request("reason too long (max 500)"))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

pub(super) fn validate_sku(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 100 {
        return Err(ApiError::bad_request("invalid sku"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid sku"));
    }
    Ok(s.to_string())
}

pub(super) fn validate_escrow_ref(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 200 {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    Ok(s.to_string())
}

pub(super) fn validate_price_cents(v: i64) -> Result<i64, ApiError> {
    if v < 0 {
        return Err(ApiError::bad_request("priceCents must be >= 0"));
    }
    Ok(v)
}

pub(super) fn validate_currency(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() || s.len() > 10 || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ApiError::bad_request("invalid currency"));
    }
    Ok(s)
}

pub(super) fn paginate(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert("authorization", HeaderValue::from_str(a).unwrap());
        }
        h
    }

    #[test]
    fn unset_token_fails_closed() {
        let err = authorize_with_token(None, &headers_with(Some("Bearer anything"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn missing_bearer_is_forbidden() {
        let err = authorize_with_token(Some("secret"), &headers_with(None)).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn wrong_token_is_forbidden() {
        let err =
            authorize_with_token(Some("secret"), &headers_with(Some("Bearer nope"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn correct_token_authorizes() {
        assert!(authorize_with_token(Some("secret"), &headers_with(Some("Bearer secret"))).is_ok());
    }

    #[test]
    fn raw_token_without_bearer_prefix_is_forbidden() {
        let err = authorize_with_token(Some("secret"), &headers_with(Some("secret"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn validates_address() {
        assert!(normalize_address("0x1234567890abcdef1234567890abcdef12345678").is_ok());
        assert_eq!(
            normalize_address("0xABCDEF1234567890ABCDEF1234567890ABCDEF12").unwrap(),
            "0xabcdef1234567890abcdef1234567890abcdef12"
        );
        assert!(normalize_address("notanaddress").is_err());
        assert!(normalize_address("0x123").is_err());
    }

    #[test]
    fn validates_positive_amount() {
        assert_eq!(validate_positive_amount("100").unwrap(), "100");
        assert_eq!(validate_positive_amount(" 12.5 ").unwrap(), "12.5");
        assert!(validate_positive_amount("0").is_err());
        assert!(validate_positive_amount("0.0").is_err());
        assert!(validate_positive_amount("-5").is_err());
        assert!(validate_positive_amount("1e9").is_err());
        assert!(validate_positive_amount("").is_err());
    }

    #[test]
    fn validates_idempotency_key() {
        assert_eq!(validate_idempotency_key(&None).unwrap(), None);
        assert_eq!(validate_idempotency_key(&Some("  ".into())).unwrap(), None);
        assert_eq!(
            validate_idempotency_key(&Some(" grant-2026-001 ".into())).unwrap(),
            Some("grant-2026-001".to_string())
        );
        assert!(validate_idempotency_key(&Some("x".repeat(201))).is_err());
        assert!(validate_idempotency_key(&Some("bad key".into())).is_err());
        assert!(validate_idempotency_key(&Some("bad\nkey".into())).is_err());
    }

    #[test]
    fn header_actor_resolves() {
        let mut h = HeaderMap::new();
        assert_eq!(admin_actor(&h), None);
        h.insert("x-catalyrst-admin", HeaderValue::from_static("  alice  "));
        assert_eq!(admin_actor(&h).as_deref(), Some("alice"));
    }

    #[test]
    fn validates_season_state() {
        assert_eq!(validate_season_state("in_progress").unwrap(), "IN_PROGRESS");
        assert!(validate_season_state("BOGUS").is_err());
    }

    #[test]
    fn validates_sku_phase8() {
        assert_eq!(validate_sku(" pack_100 ").unwrap(), "pack_100");
        assert!(validate_sku("").is_err());
        assert!(validate_sku("a/b").is_err());
        assert!(validate_sku("a\\b").is_err());
        assert!(validate_sku(&"x".repeat(101)).is_err());
    }

    #[test]
    fn validates_escrow_ref() {
        assert_eq!(validate_escrow_ref(" 0xdeadBEEF ").unwrap(), "0xdeadBEEF");
        assert!(validate_escrow_ref("").is_err());
        assert!(validate_escrow_ref("a/b").is_err());
        assert!(validate_escrow_ref(&"x".repeat(201)).is_err());
    }

    #[test]
    fn validates_price_cents() {
        assert_eq!(validate_price_cents(0).unwrap(), 0);
        assert_eq!(validate_price_cents(999).unwrap(), 999);
        assert!(validate_price_cents(-1).is_err());
    }

    #[test]
    fn validates_currency() {
        assert_eq!(validate_currency(" USD ").unwrap(), "usd");
        assert_eq!(validate_currency("eur").unwrap(), "eur");
        assert!(validate_currency("").is_err());
        assert!(validate_currency("us1").is_err());
        assert!(validate_currency(&"a".repeat(11)).is_err());
    }

    #[test]
    fn paginates_with_bounds() {
        assert_eq!(paginate(None, None), (50, 0));
        assert_eq!(paginate(Some(10), Some(5)), (10, 5));
        assert_eq!(paginate(Some(0), Some(-3)), (1, 0));
        assert_eq!(paginate(Some(9999), None), (200, 0));
    }
}
