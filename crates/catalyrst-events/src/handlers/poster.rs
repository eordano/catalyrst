use axum::Json;
use serde_json::Value;

use crate::http::response::ApiError;

pub async fn upload_poster() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /api/poster requires S3; not provisioned for catalyrst-events. Federation events use the per-catalyst image cache (see docs/federation/events.md)",
    ))
}

pub async fn upload_poster_vertical() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /api/poster-vertical requires S3; not provisioned for catalyrst-events. See TODO.md",
    ))
}
