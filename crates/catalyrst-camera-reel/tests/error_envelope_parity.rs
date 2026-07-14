use axum::body::to_bytes;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use serde_json::{json, Value};

use catalyrst_camera_reel::http::{ApiError, ForbiddenError, ForbiddenReason};

async fn parts(err: ApiError) -> (StatusCode, String, Value) {
    let resp = err.into_response();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    (status, content_type, value)
}

fn assert_json_content_type(content_type: &str) {
    assert!(
        content_type.starts_with("application/json"),
        "content-type was {content_type}"
    );
}

fn key_set(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("body is a json object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn max_limit_reached_matches_upstream_forbidden_error() {
    let message = "you have reached the limit of 500 max images";
    let (status, content_type, value) = parts(ApiError::MaxLimitReached(message.to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_json_content_type(&content_type);
    assert_eq!(
        value,
        json!({ "reason": "maxLimitReached", "message": message })
    );
    assert_eq!(key_set(&value), vec!["message", "reason"]);
}

#[tokio::test]
async fn max_limit_reached_roundtrips_through_typed_struct() {
    let message = "you have reached the limit of 500 max images";
    let (_status, _content_type, value) =
        parts(ApiError::MaxLimitReached(message.to_string())).await;
    let typed: ForbiddenError = serde_json::from_value(value).unwrap();
    assert_eq!(typed.reason, ForbiddenReason::MaxLimitReached);
    assert_eq!(typed.message, message);
}

#[tokio::test]
async fn bad_request_matches_upstream_response_error() {
    let (status, content_type, value) =
        parts(ApiError::BadRequest("invalid metadata".to_string())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "invalid metadata" }));
    assert_eq!(key_set(&value), vec!["message"]);
}

#[tokio::test]
async fn unauthorized_matches_bare_message_envelope() {
    let (status, content_type, value) = parts(ApiError::Unauthorized).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "Unauthorized" }));
    assert_eq!(key_set(&value), vec!["message"]);
}

#[tokio::test]
async fn forbidden_matches_upstream_response_error() {
    let (status, content_type, value) = parts(ApiError::Forbidden("forbidden".to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "forbidden" }));
    assert_eq!(key_set(&value), vec!["message"]);
}

#[tokio::test]
async fn not_found_matches_upstream_response_error() {
    let (status, content_type, value) =
        parts(ApiError::NotFound("image not found".to_string())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "image not found" }));
    assert_eq!(key_set(&value), vec!["message"]);
}

#[tokio::test]
async fn bad_gateway_matches_upstream_response_error() {
    let (status, content_type, value) = parts(ApiError::BadGateway(
        "failed to resolve world name".to_string(),
    ))
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "failed to resolve world name" }));
    assert_eq!(key_set(&value), vec!["message"]);
}

#[tokio::test]
async fn internal_matches_upstream_response_error() {
    let (status, content_type, value) =
        parts(ApiError::Internal("database error".to_string())).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_json_content_type(&content_type);
    assert_eq!(value, json!({ "message": "database error" }));
    assert_eq!(key_set(&value), vec!["message"]);
}
