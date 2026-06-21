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
        let (code, label, message): (u16, &str, Option<String>) = match &self {
            ApiError::BadRequest(m) => (400, "Bad request", Some(m.clone())),
            ApiError::NotAuthorized(m) => (401, "Not Authorized", Some(m.clone())),
            ApiError::NotFound(m) => (404, "Not Found", Some(m.clone())),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "Internal Server Error", None)
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (500, "Internal Server Error", None)
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = match message {
            Some(m) => json!({ "error": label, "message": m }),
            None => json!({ "error": label }),
        };
        (status, Json(body)).into_response()
    }
}
