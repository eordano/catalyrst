use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::rest::auth_chain::AuthChainError;
use crate::rest::http::ApiError;

#[derive(Debug)]
pub enum CommError {
    BadRequest(String),
    NotAuthorized(String),
    NotFound(String),
    Internal,
}

impl CommError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        CommError::BadRequest(msg.into())
    }
    pub fn not_authorized(msg: impl Into<String>) -> Self {
        CommError::NotAuthorized(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        CommError::NotFound(msg.into())
    }
}

impl IntoResponse for CommError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            CommError::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                json!({ "error": "Bad request", "message": message }),
            ),
            CommError::NotAuthorized(message) => (
                StatusCode::UNAUTHORIZED,
                json!({ "error": "Not Authorized", "message": message }),
            ),
            CommError::NotFound(message) => (
                StatusCode::NOT_FOUND,
                json!({ "error": "Not Found", "message": message }),
            ),
            CommError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "Internal Server Error" }),
            ),
        };
        (status, Json(body)).into_response()
    }
}

impl From<AuthChainError> for CommError {
    fn from(e: AuthChainError) -> Self {
        CommError::NotAuthorized(e.to_string())
    }
}

impl From<sqlx::Error> for CommError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "sqlx error");
        CommError::Internal
    }
}

impl From<ApiError> for CommError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Http(h) => match h.code {
                400 => CommError::BadRequest(h.message),
                401 => CommError::NotAuthorized(h.message),
                404 => CommError::NotFound(h.message),
                _ => {
                    tracing::error!(code = h.code, message = %h.message, "upstream error");
                    CommError::Internal
                }
            },
            ApiError::InvalidParameter(p) => CommError::BadRequest(p.to_string()),
            ApiError::Database(db) => {
                tracing::error!(error = %db, "sqlx error");
                CommError::Internal
            }
            ApiError::Internal(s) => {
                tracing::error!(message = %s, "internal error");
                CommError::Internal
            }
        }
    }
}
