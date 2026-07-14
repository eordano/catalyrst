use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

pub const SIGNED_FETCH_MESSAGE: &str = "This endpoint requires a signed fetch request. See ADR-44.";

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotAuthorized(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    LengthRequired(String),

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),

    #[error("{error}")]
    SignedFetch { status: u16, error: String },
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
        if let ApiError::SignedFetch { status, error } = &self {
            let sc = StatusCode::from_u16(*status).unwrap_or(StatusCode::UNAUTHORIZED);
            let body = ApiErrorBody::labeled(error, SIGNED_FETCH_MESSAGE);
            return (sc, Json(body)).into_response();
        }
        let (code, body) = match &self {
            ApiError::BadRequest(m) => (400, ApiErrorBody::labeled("Bad request", m)),
            ApiError::NotAuthorized(m) => (401, ApiErrorBody::labeled("Not Authorized", m)),
            ApiError::NotFound(m) => (404, ApiErrorBody::labeled("Not Found", m)),
            ApiError::LengthRequired(m) => (411, ApiErrorBody::labeled("Length Required", m)),
            ApiError::PayloadTooLarge(m) => (413, ApiErrorBody::labeled("Payload Too Large", m)),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, ApiErrorBody::new("Internal Server Error"))
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (500, ApiErrorBody::new("Internal Server Error"))
            }
            ApiError::SignedFetch { .. } => unreachable!("handled above"),
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::not_found("value not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Not Found", "message": "value not found" })
        );
    }
}
