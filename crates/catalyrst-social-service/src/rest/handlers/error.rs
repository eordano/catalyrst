use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;

use crate::rest::auth_chain::AuthChainError;
use crate::rest::http::ApiError;

#[derive(Debug)]
pub enum CommError {
    BadRequest(String),
    NotAuthorized(String),
    NotFound(String),
    Status(StatusCode, String),
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
    pub fn status(code: StatusCode, msg: impl Into<String>) -> Self {
        CommError::Status(code, msg.into())
    }
}

impl IntoResponse for CommError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            CommError::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                ApiErrorBody::labeled("Bad request", message),
            ),
            CommError::NotAuthorized(message) => (
                StatusCode::UNAUTHORIZED,
                ApiErrorBody::labeled("Not Authorized", message),
            ),
            CommError::NotFound(message) => (
                StatusCode::NOT_FOUND,
                ApiErrorBody::labeled("Not Found", message),
            ),
            CommError::Status(code, message) => (code, ApiErrorBody::new(message)),
            CommError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorBody::new("Internal Server Error"),
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
                code => match StatusCode::from_u16(code) {
                    Ok(status) => CommError::Status(status, h.message),
                    Err(_) => {
                        tracing::error!(code, message = %h.message, "upstream error");
                        CommError::Internal
                    }
                },
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = CommError::not_found("community not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Not Found", "message": "community not found" })
        );
    }

    #[tokio::test]
    async fn status_envelope_wire_shape() {
        let resp = CommError::status(StatusCode::SERVICE_UNAVAILABLE, "friends unavailable")
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "friends unavailable", "message": "friends unavailable" })
        );
    }
}
