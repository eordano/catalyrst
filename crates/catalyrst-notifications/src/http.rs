use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

use crate::auth_chain::AuthChainError;

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
}

impl From<AuthChainError> for ApiError {
    fn from(e: AuthChainError) -> Self {
        tracing::warn!(error = ?e, "signed-fetch rejected");
        ApiError::Unauthorized(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone()),
            ApiError::Unauthorized(m) => (401, m.clone()),
            ApiError::Forbidden(m) => (403, m.clone()),
            ApiError::NotFound(m) => (404, m.clone()),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Internal(m) => (500, m.clone()),
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = json!({ "ok": false, "message": message });
        (status, Json(body)).into_response()
    }
}
