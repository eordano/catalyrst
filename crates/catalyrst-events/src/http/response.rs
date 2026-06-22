use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

pub use catalyrst_types::{HttpError, InvalidParameterError};

#[derive(Debug, Serialize)]
pub struct ApiOk<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

impl<T: Serialize> ApiOk<T> {
    pub fn new(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    InvalidParameter(#[from] InvalidParameterError),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(400, msg))
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(404, msg))
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(401, msg))
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(403, msg))
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(501, msg))
    }
    pub fn gone(msg: impl Into<String>) -> Self {
        ApiError::Http(HttpError::new(410, msg))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::Http(HttpError { code, message }) => (*code, message.clone()),
            ApiError::InvalidParameter(e) => (400u16, e.to_string()),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Internal(s) => (500, s.clone()),
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = json!({ "ok": false, "error": message });
        (status, Json(body)).into_response()
    }
}
