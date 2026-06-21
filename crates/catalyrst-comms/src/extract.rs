use serde::de::DeserializeOwned;
use serde_json::json;

use crate::http::ApiError;

pub trait SchemaValidate {
    fn schema_validate(value: &serde_json::Value) -> Result<(), String>;
}

fn is_json_content_type(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let media = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if media == "application/json" {
        return true;
    }
    if let Some((prefix, suffix)) = media.rsplit_once('+') {
        return suffix == "json" && prefix.contains('/') && !prefix.starts_with('/');
    }
    false
}

pub fn validate_body<T>(content_type: Option<&str>, bytes: &[u8]) -> Result<T, ApiError>
where
    T: DeserializeOwned + SchemaValidate,
{
    if !is_json_content_type(content_type) {
        return Err(ApiError::schema(
            415,
            json!({ "ok": false, "message": "Content-Type must be application/json" }),
        ));
    }

    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| ApiError::schema(400, json!({ "ok": false, "message": e.to_string() })))?;

    if let Err(detail) = T::schema_validate(&value) {
        return Err(ApiError::schema(
            400,
            json!({ "ok": false, "message": "Invalid JSON body", "data": detail }),
        ));
    }

    serde_json::from_value(value)
        .map_err(|e| ApiError::schema(400, json!({ "ok": false, "message": e.to_string() })))
}
