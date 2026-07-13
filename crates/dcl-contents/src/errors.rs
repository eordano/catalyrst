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
    NotFound(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[cfg(feature = "pg")]
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("upstream error: {0}")]
    Upstream(String),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }
    pub fn upstream(msg: impl Into<String>) -> Self {
        Self::Upstream(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone()),
            ApiError::NotFound(m) => (404, m.clone()),
            ApiError::Unauthorized(m) => (401, m.clone()),
            ApiError::Forbidden(m) => (403, m.clone()),
            ApiError::NotImplemented(m) => (501, m.clone()),
            #[cfg(feature = "pg")]
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Upstream(m) => {
                tracing::warn!(error = %m, "upstream catalyst error");
                (502, "upstream catalyst error".to_string())
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (500, m.clone())
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "ok": false, "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
