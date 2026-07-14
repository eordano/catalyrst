use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

/// Places-specific API error: generic status+message cases delegate to the
/// shared [`catalyrst_types::ApiError`] envelope; the federation cases keep
/// their `federation_adr` decoration crate-locally.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Common(#[from] catalyrst_types::ApiError),

    #[error("not implemented (federation): {0}")]
    NotImplemented(String),

    #[error("{0}")]
    ServiceUnavailable(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::bad_request(msg))
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::not_found(msg))
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::unauthorized(msg))
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::forbidden(msg))
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Common(e.into())
    }
}

const FED_ADR_URL: &str = "./docs/federation/places.md";

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match self {
            ApiError::Common(e) => return e.into_response(),
            ApiError::NotImplemented(m) => (axum::http::StatusCode::NOT_IMPLEMENTED, m),
            ApiError::ServiceUnavailable(m) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, m),
        };
        let body = ApiErrorBody::new(message).with_federation_adr(FED_ADR_URL);
        (code, Json(body)).into_response()
    }
}
