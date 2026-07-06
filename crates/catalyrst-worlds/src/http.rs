use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{message}")]
    TooManyRequests { message: String, retry_after: u64 },

    #[error("{0}")]
    ServiceUnavailable(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn too_many(msg: impl Into<String>, retry_after: u64) -> Self {
        Self::TooManyRequests {
            message: msg.into(),
            retry_after,
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(m) => err(400, m, None),
            ApiError::Unauthorized(m) => err(401, m, None),
            ApiError::Forbidden(m) => err(403, m, None),
            ApiError::NotFound(m) => err(404, m, None),
            ApiError::TooManyRequests {
                message,
                retry_after,
            } => err(429, message, Some(retry_after)),
            ApiError::ServiceUnavailable(m) => err(503, m, None),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                err(500, "database error".to_string(), None)
            }
            ApiError::Internal(m) => {
                tracing::error!(message = %m, "internal error");
                err(500, m, None)
            }
        }
    }
}

fn err(code: u16, message: String, retry_after: Option<u64>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = Json(json!({ "error": message }));
    match retry_after {
        Some(secs) => (status, [("Retry-After", secs.to_string())], body).into_response(),
        None => (status, body).into_response(),
    }
}
