use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use serde::Serialize;
use thiserror::Error;

use crate::auth_chain::AuthChainError;

const ADR44_ERROR: &str = "Invalid Auth Chain";
const ADR44_MESSAGE: &str = "This endpoint requires a signed fetch request. See ADR-44.";

#[derive(Debug, Serialize)]
pub struct AuthChainErrorBody {
    pub error: String,
    pub message: String,
}

impl AuthChainErrorBody {
    pub fn adr44() -> Self {
        Self {
            error: ADR44_ERROR.to_string(),
            message: ADR44_MESSAGE.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("invalid auth chain: {0}")]
    InvalidAuthChain(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    PaymentRequired(String),

    #[error("{0}")]
    Unprocessable(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("{0}")]
    ServiceUnavailable(String),

    #[error("{0}")]
    BadGateway(String),

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
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
    pub fn payment_required(msg: impl Into<String>) -> Self {
        Self::PaymentRequired(msg.into())
    }
    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self::Unprocessable(msg.into())
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::BadGateway(msg.into())
    }
}

impl From<AuthChainError> for ApiError {
    fn from(e: AuthChainError) -> Self {
        ApiError::InvalidAuthChain(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::InvalidAuthChain(reason) = &self {
            tracing::debug!(reason = %reason, "signed-fetch auth chain rejected");
            return (StatusCode::BAD_REQUEST, Json(AuthChainErrorBody::adr44())).into_response();
        }
        let (code, message) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone()),
            ApiError::Unauthorized(m) => (401, m.clone()),
            ApiError::InvalidAuthChain(m) => (400, m.clone()),
            ApiError::Forbidden(m) => (403, m.clone()),
            ApiError::NotFound(m) => (404, m.clone()),
            ApiError::Conflict(m) => (409, m.clone()),
            ApiError::PaymentRequired(m) => (402, m.clone()),
            ApiError::Unprocessable(m) => (422, m.clone()),
            ApiError::NotImplemented(m) => (501, m.clone()),
            ApiError::ServiceUnavailable(m) => (503, m.clone()),
            ApiError::BadGateway(m) => (502, m.clone()),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Internal(m) => (500, m.clone()),
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
        let resp = ApiError::payment_required("insufficient credits").into_response();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "insufficient credits", "message": "insufficient credits" })
        );
    }
}
