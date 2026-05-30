//! Shared handler helpers: pagination/prefix parsing, the bulk-delete
//! confirmation header, and the upsert body shape.

use std::collections::HashMap;

use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;

use crate::http::errors::ApiError;

pub const DEFAULT_LIMIT: i64 = 100;
pub const MAX_LIMIT: i64 = 1000;
pub const CONFIRM_DELETE_ALL_HEADER: &str = "x-confirm-delete-all";

#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub prefix: Option<String>,
}

/// Parse `limit`/`offset`/`prefix` query params with the upstream defaults
/// (limit defaults to 100, capped at 1000; offset defaults to 0).
pub fn parse_pagination(params: &HashMap<String, String>) -> Result<Pagination, ApiError> {
    let limit = match params.get("limit") {
        Some(s) => {
            let v: i64 = s
                .parse()
                .map_err(|_| ApiError::bad_request("invalid limit parameter"))?;
            if v <= 0 {
                return Err(ApiError::bad_request("limit must be a positive integer"));
            }
            v.min(MAX_LIMIT)
        }
        None => DEFAULT_LIMIT,
    };
    let offset = match params.get("offset") {
        Some(s) => {
            let v: i64 = s
                .parse()
                .map_err(|_| ApiError::bad_request("invalid offset parameter"))?;
            if v < 0 {
                return Err(ApiError::bad_request("offset must be a non-negative integer"));
            }
            v
        }
        None => 0,
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

#[cfg(test)]
mod tests {
    use super::is_js_falsy;
    use serde_json::json;

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
