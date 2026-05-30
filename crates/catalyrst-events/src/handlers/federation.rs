use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::response::ApiError;

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub since: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_feed(Query(_q): Query<FeedQuery>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"events": [], "partial": false})))
}

pub async fn get_attendance(
    Path(event_id): Path<String>,
    Query(_q): Query<FeedQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"event_id": event_id, "attendances": [], "partial": false})))
}
