use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
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

impl From<ApiError> for catalyrst_types::ApiError {
    fn from(e: ApiError) -> Self {
        use catalyrst_types::ApiError as Common;
        match e {
            ApiError::BadRequest(m) | ApiError::InvalidAuthChain(m) => Common::http(400, m),
            ApiError::Unauthorized(m) => Common::http(401, m),
            ApiError::PaymentRequired(m) => Common::http(402, m),
            ApiError::Forbidden(m) => Common::http(403, m),
            ApiError::NotFound(m) => Common::http(404, m),
            ApiError::Conflict(m) => Common::http(409, m),
            ApiError::Unprocessable(m) => Common::http(422, m),
            ApiError::NotImplemented(m) => Common::http(501, m),
            ApiError::BadGateway(m) => Common::http(502, m),
            ApiError::ServiceUnavailable(m) => Common::http(503, m),
            ApiError::Database(e) => Common::Database(e),
            ApiError::Internal(m) => Common::Internal(m),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::InvalidAuthChain(reason) = &self {
            tracing::debug!(reason = %reason, "signed-fetch auth chain rejected");
            return (StatusCode::BAD_REQUEST, Json(AuthChainErrorBody::adr44())).into_response();
        }
        catalyrst_types::ApiError::from(self).into_response()
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

    #[test]
    fn every_variant_keeps_its_status() {
        for (err, want) in [
            (ApiError::bad_request("m"), 400u16),
            (ApiError::unauthorized("m"), 401),
            (ApiError::payment_required("m"), 402),
            (ApiError::forbidden("m"), 403),
            (ApiError::not_found("m"), 404),
            (ApiError::conflict("m"), 409),
            (ApiError::unprocessable("m"), 422),
            (ApiError::not_implemented("m"), 501),
            (ApiError::bad_gateway("m"), 502),
            (ApiError::service_unavailable("m"), 503),
            (ApiError::Internal("m".into()), 500),
            (ApiError::Database(sqlx::Error::RowNotFound), 500),
        ] {
            let label = err.to_string();
            assert_eq!(
                err.into_response().status().as_u16(),
                want,
                "wrong status for {label}"
            );
        }
    }

    #[tokio::test]
    async fn internal_detail_is_not_published() {
        let resp =
            ApiError::Internal("connect to 10.0.0.7:5434 refused".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "internal error", "message": "internal error" })
        );
    }
}
