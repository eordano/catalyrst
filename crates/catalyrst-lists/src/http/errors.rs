use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500u16, "database error".to_string())
            }
            ApiError::Internal(m) => (500, m.clone()),
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "ok": false, "message": message }))).into_response()
    }
}
