use axum::Json;
use serde_json::Value;

use crate::http::response::ApiError;

pub async fn get_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /api/profiles/subscriptions is deprecated upstream (web-push); not ported",
    ))
}

pub async fn create_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /api/profiles/subscriptions is deprecated upstream (web-push); not ported",
    ))
}

pub async fn delete_profile_subscription() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "DELETE /api/profiles/subscriptions is deprecated upstream (web-push); not ported",
    ))
}
