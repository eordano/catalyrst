use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("not implemented (federation): {0}")]
    NotImplemented(String),

    #[error("{0}")]
    ServiceUnavailable(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }
}

const FED_ADR_URL: &str = "./docs/federation/places.md";

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message, extra) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone(), None),
            ApiError::NotFound(m) => (404, m.clone(), None),
            ApiError::Unauthorized(m) => (401, m.clone(), None),
            ApiError::Forbidden(m) => (403, m.clone(), None),
            ApiError::NotImplemented(m) => (
                501,
                m.clone(),
                Some(json!({ "federation_adr": FED_ADR_URL })),
            ),
            ApiError::ServiceUnavailable(m) => (
                503,
                m.clone(),
                Some(json!({ "federation_adr": FED_ADR_URL })),
            ),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string(), None)
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut body = json!({ "ok": false, "message": message });
        if let Some(extra) = extra {
            for (k, v) in extra.as_object().unwrap() {
                body[k] = v.clone();
            }
        }
        (status, Json(body)).into_response()
    }
}
