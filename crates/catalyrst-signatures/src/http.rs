use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;

/// Upstream signatures-server wraps every response in `{ ok: boolean, ... }`.
/// Success: `{ ok: true, data: <T> }`. Error: `{ ok: false, message, data? }`.
pub struct Ok2<T: Serialize>(pub StatusCode, pub T);

impl<T: Serialize> IntoResponse for Ok2<T> {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "ok": true, "data": self.1 }))).into_response()
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    /// 501 — endpoint depends on subgraph/cron machinery not wired locally.
    #[error("{0}")]
    NotImplemented(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Error envelope with an attached `data` object, matching upstream's
    /// typed-error responses (e.g. RentalAlreadyExists carries contractAddress).
    pub fn with_data(self, data: Value) -> ApiErrorWithData {
        ApiErrorWithData { error: self, data }
    }
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            ApiError::Database(_) | ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                "Server error".to_string()
            }
            other => other.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = json!({ "ok": false, "message": self.message() });
        (status, Json(body)).into_response()
    }
}

pub struct ApiErrorWithData {
    error: ApiError,
    data: Value,
}

impl IntoResponse for ApiErrorWithData {
    fn into_response(self) -> Response {
        let status = self.error.status();
        let body = json!({ "ok": false, "message": self.error.message(), "data": self.data });
        (status, Json(body)).into_response()
    }
}
