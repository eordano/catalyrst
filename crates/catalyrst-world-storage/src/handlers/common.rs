use std::collections::HashMap;

use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequest;
use axum::http::HeaderMap;
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::errors::ApiError;

pub const MAX_LIMIT: i64 = 100;
pub const CONFIRM_DELETE_ALL_HEADER: &str = "x-confirm-delete-all";

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

pub fn get_value_response(value: Option<Value>) -> Result<Json<Value>, ApiError> {
    match value {
        Some(v) => Ok(Json(json!({ "value": v }))),
        None => Err(ApiError::not_found("Value not found")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_value_response, is_eth_address, parse_pagination, UpsertBody, UpsertEnvBody,
        ValidatedJson, MAX_LIMIT,
    };
    use crate::http::errors::ApiError;
    use axum::extract::FromRequest;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
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
            let body = get_value_response(Some(v.clone()))
                .expect("stored falsy value should yield a 200 body")
                .0;
            assert_eq!(body, json!({ "value": v }));
        }
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
