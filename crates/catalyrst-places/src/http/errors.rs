use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
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

fn code_for_status(status: u16) -> &'static str {
    match status {
        400 => "bad_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        429 => "too_many_requests",
        500 => "internal_server_error",
        501 => "not_implemented",
        503 => "service_unavailable",
        _ => "error",
    }
}

fn common_status_message(e: catalyrst_types::ApiError) -> (u16, String) {
    match e {
        catalyrst_types::ApiError::Http { status, message } => (status, message),
        catalyrst_types::ApiError::Database(err) => {
            tracing::error!(error = %err, "sqlx error");
            (500, "database error".to_string())
        }
        catalyrst_types::ApiError::Internal(m) => (500, m),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message, federation_adr) = match self {
            ApiError::Common(e) => {
                let (status, message) = common_status_message(e);
                (status, message, None)
            }
            ApiError::NotImplemented(m) => (501, m, Some(FED_ADR_URL)),
            ApiError::ServiceUnavailable(m) => (503, m, Some(FED_ADR_URL)),
        };
        let status_code = axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let mut body = json!({
            "ok": false,
            "error": message,
            "message": message,
            "code": code_for_status(status),
        });
        if let Some(adr) = federation_adr {
            body["federation_adr"] = json!(adr);
        }
        (status_code, Json(body)).into_response()
    }
}
