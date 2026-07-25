use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

use crate::auth_chain::AuthChainError;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{message}")]
    Http { status: u16, message: String },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ApiError {
    fn http(status: u16, message: impl Into<String>) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::http(400, msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::http(401, msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::http(403, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::http(404, msg)
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::http(503, msg)
    }
}

impl From<AuthChainError> for ApiError {
    fn from(e: AuthChainError) -> Self {
        ApiError::http(401, format!("Unauthenticated: {e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match self {
            ApiError::Http { status, message } => (status, message),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(ApiErrorBody::new(message))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::not_found("Not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Not found", "message": "Not found" })
        );
    }
}
