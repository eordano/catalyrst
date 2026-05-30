use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

/// Error taxonomy mirroring the upstream `@dcl/http-commons` error classes
/// (InvalidRequestError → 400, NotAuthorizedError → 401, NotFoundError → 404).
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotAuthorized(String),

    #[error("{0}")]
    NotFound(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_authorized(msg: impl Into<String>) -> Self {
        Self::NotAuthorized(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone()),
            ApiError::NotAuthorized(m) => (401, m.clone()),
            ApiError::NotFound(m) => (404, m.clone()),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (500, m.clone())
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "error": message }))).into_response()
    }
}
