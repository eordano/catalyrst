use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use thiserror::Error;

use crate::auth_chain::AuthChainError;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{message}")]
    Http {
        status: u16,
        message: String,
        data: Value,
    },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ApiError {
    fn http(status: u16, message: impl Into<String>, data: Value) -> Self {
        Self::Http {
            status,
            message: message.into(),
            data,
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::http(400, msg, json!({}))
    }
    pub fn bad_request_with(msg: impl Into<String>, data: Value) -> Self {
        Self::http(400, msg, data)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::http(401, msg, json!({}))
    }
    pub fn unauthorized_with(msg: impl Into<String>, data: Value) -> Self {
        Self::http(401, msg, data)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::http(403, msg, json!({}))
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::http(404, msg, json!({}))
    }
    pub fn not_found_with(msg: impl Into<String>, data: Value) -> Self {
        Self::http(404, msg, data)
    }
}

impl From<AuthChainError> for ApiError {
    fn from(e: AuthChainError) -> Self {
        ApiError::http(401, "Unauthenticated", json!({ "message": e.to_string() }))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message, data) = match self {
            ApiError::Http {
                status,
                message,
                data,
            } => (status, message, data),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string(), json!({}))
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            Json(json!({ "ok": false, "data": data, "error": message })),
        )
            .into_response()
    }
}
