//! Shared handler helpers: pagination/prefix parsing, the bulk-delete
//! confirmation header, and the upsert body shape.

use std::collections::HashMap;

use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;

use crate::http::errors::ApiError;

pub const MAX_LIMIT: i64 = 100;
pub const CONFIRM_DELETE_ALL_HEADER: &str = "x-confirm-delete-all";

#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub prefix: Option<String>,
}

/// Parse `limit`/`offset`/`prefix` query params, mirroring upstream
/// `getPaginationParams` (core-libs http-commons): cap at MAX_LIMIT=100 and never
/// throw — absent/invalid/out-of-range limit falls back to 100, invalid/negative
/// offset falls back to 0.
pub fn parse_pagination(params: &HashMap<String, String>) -> Result<Pagination, ApiError> {
    let limit = match params.get("limit").and_then(|s| s.parse::<i64>().ok()) {
        Some(v) if v > 0 && v <= MAX_LIMIT => v,
        _ => MAX_LIMIT,
    };
    let offset = match params.get("offset").and_then(|s| s.parse::<i64>().ok()) {
        Some(v) if v >= 0 => v,
        _ => 0,
    };
    let prefix = params.get("prefix").cloned().filter(|s| !s.is_empty());
    Ok(Pagination {
        limit,
        offset,
        prefix,
    })
}

/// Upstream `X-Confirm-Delete-All` guard for bulk deletes.
pub fn require_confirm_delete_all(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get(CONFIRM_DELETE_ALL_HEADER).is_none() {
        return Err(ApiError::bad_request(
            "Missing required header: X-Confirm-Delete-All",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct UpsertBody {
    pub value: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpsertEnvBody {
    pub value: String,
}

/// Upstream's get handlers gate the 404 on JS truthiness (`if (!value)`), so a
/// stored value that is JS-falsy — `null`, `false`, the number `0`, or the empty
/// string `""` — is reported as "Value not found" even though the row exists.
/// (Note: empty arrays/objects are truthy in JS, hence 200.) This mirrors that.
pub fn is_js_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::String(s) => s.is_empty(),
        Value::Number(n) => n.as_f64().map(|f| f == 0.0).unwrap_or(false),
        Value::Array(_) | Value::Object(_) => false,
    }
}

/// Normalize a player address path param (lowercased, non-empty).
pub fn normalize_player(address: &str) -> Result<String, ApiError> {
    let t = address.trim();
    if t.is_empty() {
        return Err(ApiError::bad_request("player_address is required"));
    }
    Ok(t.to_ascii_lowercase())
}

/// Whether `s` is a 0x-prefixed 40-hex-digit eth address (upstream's
/// `EthAddress.validate`). Checked post-auth so unauthenticated callers get 401.
pub fn is_eth_address(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{is_eth_address, is_js_falsy, parse_pagination, MAX_LIMIT};
    use serde_json::json;
    use std::collections::HashMap;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn pagination_caps_and_coerces_like_upstream() {
        // absent -> 100/0
        let p = parse_pagination(&params(&[])).unwrap();
        assert_eq!((p.limit, p.offset), (MAX_LIMIT, 0));
        // over-max -> capped to 100
        let p = parse_pagination(&params(&[("limit", "500")])).unwrap();
        assert_eq!(p.limit, MAX_LIMIT);
        // valid in-range
        let p = parse_pagination(&params(&[("limit", "50"), ("offset", "10")])).unwrap();
        assert_eq!((p.limit, p.offset), (50, 10));
        // non-numeric/zero/negative limit -> 100 (never 400)
        for bad in ["abc", "0", "-1"] {
            assert_eq!(
                parse_pagination(&params(&[("limit", bad)])).unwrap().limit,
                MAX_LIMIT
            );
        }
        // non-numeric/negative offset -> 0 (never 400)
        for bad in ["abc", "-1"] {
            assert_eq!(
                parse_pagination(&params(&[("offset", bad)]))
                    .unwrap()
                    .offset,
                0
            );
        }
    }

    #[test]
    fn eth_address_matches_upstream_validate() {
        assert!(is_eth_address("0x0000000000000000000000000000000000000000"));
        assert!(is_eth_address("0xAbCdEf0123456789aBcDeF0123456789AbCdEf01"));
        assert!(!is_eth_address(""));
        assert!(!is_eth_address("0x123"));
        assert!(!is_eth_address("not-an-address"));
        assert!(!is_eth_address(
            "0x000000000000000000000000000000000000000g"
        ));
        assert!(!is_eth_address("0000000000000000000000000000000000000000"));
    }

    #[test]
    fn js_falsy_matches_upstream_truthiness() {
        // falsy (upstream `if (!value)` -> 404)
        assert!(is_js_falsy(&json!(null)));
        assert!(is_js_falsy(&json!(false)));
        assert!(is_js_falsy(&json!(0)));
        assert!(is_js_falsy(&json!(0.0)));
        assert!(is_js_falsy(&json!("")));
        // truthy (-> 200), including the JS gotchas: [] and {} are truthy
        assert!(!is_js_falsy(&json!(true)));
        assert!(!is_js_falsy(&json!(1)));
        assert!(!is_js_falsy(&json!(-1)));
        assert!(!is_js_falsy(&json!("false")));
        assert!(!is_js_falsy(&json!([])));
        assert!(!is_js_falsy(&json!({})));
        assert!(!is_js_falsy(&json!([0])));
    }
}
