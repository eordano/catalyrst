use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequest;
use axum::http::HeaderMap;
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::http::errors::ApiError;
use crate::storage::StorageEntry;

pub const MAX_LIMIT: i64 = 100;
pub const CONFIRM_DELETE_ALL_HEADER: &str = "x-confirm-delete-all";

pub const MAX_KEY_LENGTH: usize = 255;

pub const BODY_ENVELOPE_SLACK_BYTES: i64 = 1024;

pub fn validate_key(key: &str) -> Result<(), ApiError> {
    if key.is_empty() || (key.len() > MAX_KEY_LENGTH && key.chars().count() > MAX_KEY_LENGTH) {
        return Err(ApiError::bad_request(format!(
            "Key must be between 1 and {MAX_KEY_LENGTH} characters"
        )));
    }
    Ok(())
}

pub fn check_content_length(headers: &HeaderMap, max_value_size: i64) -> Result<(), ApiError> {
    let content_length = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 0);
    let Some(content_length) = content_length else {
        return Err(ApiError::LengthRequired(
            "Requests with a body must include a valid Content-Length header".to_string(),
        ));
    };
    let max_body_size = max_value_size + BODY_ENVELOPE_SLACK_BYTES;
    if content_length > max_body_size {
        return Err(ApiError::PayloadTooLarge(format!(
            "Request body exceeds the maximum allowed size ({max_body_size} bytes)"
        )));
    }
    Ok(())
}

pub fn reject_nul_characters(serialized: &str) -> Result<(), ApiError> {
    let bytes = serialized.as_bytes();
    let needle = b"\\u0000";
    let mut from = 0;
    while let Some(pos) = find_from(bytes, needle, from) {
        let backslashes_before = bytes[..pos]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        if backslashes_before % 2 == 0 {
            return Err(ApiError::bad_request(
                "Values must not contain the \\u0000 (NUL) character",
            ));
        }
        from = pos + 1;
    }
    Ok(())
}

fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub prefix: Option<String>,
}

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

pub fn require_confirm_delete_all(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get(CONFIRM_DELETE_ALL_HEADER).is_none() {
        return Err(ApiError::bad_request(
            "Missing required header: X-Confirm-Delete-All",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertBody {
    pub value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertEnvBody {
    pub value: String,
}

pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(
        req: axum::http::Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection: JsonRejection| ApiError::bad_request(rejection.body_text()))?;
        Ok(ValidatedJson(value))
    }
}

pub fn normalize_player(address: &str) -> Result<String, ApiError> {
    let t = address.trim();
    if t.is_empty() {
        return Err(ApiError::bad_request("player_address is required"));
    }
    Ok(t.to_ascii_lowercase())
}

pub fn is_eth_address(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug)]
pub struct RawJson(pub String);

impl axum::response::IntoResponse for RawJson {
    fn into_response(self) -> axum::response::Response {
        (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            self.0,
        )
            .into_response()
    }
}

pub fn raw_value_response(serialized: &str) -> RawJson {
    RawJson(format!("{{\"value\":{serialized}}}"))
}

pub fn raw_paginated_response(
    entries: &[StorageEntry],
    limit: i64,
    offset: i64,
    total: i64,
) -> RawJson {
    let mut data = String::from("[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            data.push(',');
        }
        data.push_str("{\"key\":");
        data.push_str(&serde_json::to_string(&e.key).expect("strings always serialize"));
        data.push_str(",\"value\":");
        data.push_str(&e.value);
        data.push('}');
    }
    data.push(']');
    RawJson(format!(
        "{{\"data\":{data},\"pagination\":{{\"limit\":{limit},\"offset\":{offset},\"total\":{total}}}}}"
    ))
}

pub fn get_value_response(value: Option<Arc<str>>) -> Result<RawJson, ApiError> {
    match value {
        Some(v) => Ok(raw_value_response(&v)),
        None => Err(ApiError::not_found("Value not found")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_content_length, get_value_response, is_eth_address, parse_pagination,
        raw_paginated_response, raw_value_response, reject_nul_characters, validate_key,
        UpsertBody, UpsertEnvBody, ValidatedJson, BODY_ENVELOPE_SLACK_BYTES, MAX_LIMIT,
    };
    use crate::http::errors::ApiError;
    use crate::storage::StorageEntry;
    use axum::extract::FromRequest;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn key_length_is_validated_in_code_points() {
        assert!(validate_key("k").is_ok());
        assert!(validate_key(&"k".repeat(255)).is_ok());
        assert!(
            validate_key(&"é".repeat(255)).is_ok(),
            "255 code points, 510 bytes"
        );
        assert!(matches!(validate_key(""), Err(ApiError::BadRequest(_))));
        assert!(matches!(
            validate_key(&"k".repeat(256)),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            validate_key(&"é".repeat(256)),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn nul_characters_are_rejected_but_escaped_backslash_text_is_not() {
        let with_nul = serde_json::to_string(&json!({ "a": "b\u{0}c" })).unwrap();
        assert!(matches!(
            reject_nul_characters(&with_nul),
            Err(ApiError::BadRequest(_))
        ));

        let literal_text = serde_json::to_string(&json!({ "a": "\\u0000" })).unwrap();
        assert!(reject_nul_characters(&literal_text).is_ok());

        let backslash_then_nul = serde_json::to_string(&json!({ "a": "\\\u{0}" })).unwrap();
        assert!(reject_nul_characters(&backslash_then_nul).is_err());

        assert!(reject_nul_characters(r#"{"a":"plain"}"#).is_ok());
    }

    #[test]
    fn content_length_precheck_maps_to_411_and_413() {
        let mut headers = HeaderMap::new();
        let err = check_content_length(&headers, 100).unwrap_err();
        assert!(matches!(err, ApiError::LengthRequired(_)));
        assert_eq!(err.into_response().status(), StatusCode::LENGTH_REQUIRED);

        headers.insert("content-length", "not-a-number".parse().unwrap());
        assert!(matches!(
            check_content_length(&headers, 100),
            Err(ApiError::LengthRequired(_))
        ));

        headers.insert(
            "content-length",
            (100 + BODY_ENVELOPE_SLACK_BYTES)
                .to_string()
                .parse()
                .unwrap(),
        );
        assert!(check_content_length(&headers, 100).is_ok());

        headers.insert(
            "content-length",
            (101 + BODY_ENVELOPE_SLACK_BYTES)
                .to_string()
                .parse()
                .unwrap(),
        );
        let err = check_content_length(&headers, 100).unwrap_err();
        assert!(matches!(err, ApiError::PayloadTooLarge(_)));
        assert_eq!(err.into_response().status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn pagination_caps_and_coerces_like_upstream() {
        let p = parse_pagination(&params(&[])).unwrap();
        assert_eq!((p.limit, p.offset), (MAX_LIMIT, 0));

        let p = parse_pagination(&params(&[("limit", "500")])).unwrap();
        assert_eq!(p.limit, MAX_LIMIT);

        let p = parse_pagination(&params(&[("limit", "50"), ("offset", "10")])).unwrap();
        assert_eq!((p.limit, p.offset), (50, 10));

        for bad in ["abc", "0", "-1"] {
            assert_eq!(
                parse_pagination(&params(&[("limit", bad)])).unwrap().limit,
                MAX_LIMIT
            );
        }

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
    fn stored_falsy_value_is_200_not_404() {
        for v in [json!(0), json!(false), json!(""), json!(null)] {
            let serialized: Arc<str> = Arc::from(serde_json::to_string(&v).unwrap());
            let body = get_value_response(Some(serialized))
                .expect("stored falsy value should yield a 200 body")
                .0;
            assert_eq!(body, serde_json::to_string(&json!({ "value": v })).unwrap());
        }
    }

    fn gnarly_values() -> Vec<Value> {
        vec![
            json!({"quotes":"a\"b","backslash":"c\\d","nl":"e\nf","tab":"g\th"}),
            json!({"unicode":"héllo 🌍 \u{2028}","empty":{},"nested":{"n":[1,2.5,null,true],"s":""}}),
            json!([1, "two", {"three": 3}, []]),
            json!("\\u0041 literal escape text"),
            json!(-0.5),
        ]
    }

    #[test]
    fn raw_value_splice_is_byte_identical_to_the_decoded_path() {
        for v in gnarly_values() {
            let serialized = serde_json::to_string(&v).unwrap();
            assert_eq!(
                raw_value_response(&serialized).0,
                serde_json::to_string(&json!({ "value": v })).unwrap()
            );
        }
    }

    #[test]
    fn raw_paginated_splice_is_byte_identical_to_the_decoded_path() {
        let keys = ["plain", "with\"quote", "back\\slash", "uni🔑\n"];
        let entries: Vec<StorageEntry> = keys
            .iter()
            .zip(gnarly_values())
            .map(|(k, v)| StorageEntry {
                key: k.to_string(),
                value: serde_json::to_string(&v).unwrap(),
            })
            .collect();
        let decoded: Vec<Value> = keys
            .iter()
            .zip(gnarly_values())
            .map(|(k, v)| json!({ "key": k, "value": v }))
            .collect();
        assert_eq!(
            raw_paginated_response(&entries, 100, 0, 4).0,
            serde_json::to_string(&json!({
                "data": decoded,
                "pagination": { "limit": 100, "offset": 0, "total": 4 }
            }))
            .unwrap()
        );

        assert_eq!(
            raw_paginated_response(&[], 10, 5, 0).0,
            r#"{"data":[],"pagination":{"limit":10,"offset":5,"total":0}}"#
        );
    }

    #[test]
    fn raw_value_splice_passes_postgres_spaced_text_verbatim() {
        let pg_text = "{\"a\": 1, \"b\": [1, 2]}";
        let body = raw_value_response(pg_text).0;
        assert!(body.contains(pg_text));
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            json!({ "value": { "a": 1, "b": [1, 2] } })
        );
    }

    #[test]
    fn absent_key_is_404() {
        let err = get_value_response(None).expect_err("absent key should be an error");
        assert!(matches!(err, ApiError::NotFound(_)));
        assert_eq!(err.into_response().status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn upsert_body_schema_matches_ajv() {
        assert!(serde_json::from_str::<UpsertBody>(r#"{"value": {"a": 1}}"#).is_ok());
        assert!(serde_json::from_str::<UpsertBody>(r#"{"value": null}"#).is_ok());
        assert!(serde_json::from_str::<UpsertBody>(r#"{"value": [1,2,3]}"#).is_ok());
        assert!(serde_json::from_str::<UpsertBody>(r#"{"value": 0}"#).is_ok());

        assert!(serde_json::from_str::<UpsertBody>(r#"{}"#).is_err());

        assert!(serde_json::from_str::<UpsertBody>(r#"{"value": 1, "extra": 2}"#).is_err());
    }

    #[test]
    fn upsert_env_body_requires_string_value() {
        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{"value": "secret"}"#).is_ok());
        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{"value": 5}"#).is_err());
        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{"value": true}"#).is_err());
        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{"value": null}"#).is_err());

        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<UpsertEnvBody>(r#"{"value": "x", "extra": 1}"#).is_err());
    }

    fn json_put(body: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method("PUT")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn validated_json_rejects_bad_body_with_400_bad_request() {
        let res =
            ValidatedJson::<UpsertBody>::from_request(json_put(r#"{"value":1,"extra":2}"#), &())
                .await;
        let err = res.err().expect("extra property should be rejected");
        assert!(matches!(err, ApiError::BadRequest(_)));
        assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);

        let res = ValidatedJson::<UpsertBody>::from_request(json_put(r#"{}"#), &()).await;
        assert!(matches!(res.err(), Some(ApiError::BadRequest(_))));

        let res =
            ValidatedJson::<UpsertEnvBody>::from_request(json_put(r#"{"value":5}"#), &()).await;
        assert!(matches!(res.err(), Some(ApiError::BadRequest(_))));
    }

    #[tokio::test]
    async fn validated_json_accepts_valid_body() {
        let ValidatedJson(body) =
            ValidatedJson::<UpsertBody>::from_request(json_put(r#"{"value":{"a":1}}"#), &())
                .await
                .expect("a valid body should extract");
        assert_eq!(body.value, json!({"a": 1}));
    }
}
