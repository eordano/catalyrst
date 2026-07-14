use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct InvalidRequestError {
    pub message: String,
}

impl InvalidRequestError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct NotFoundError {
    pub message: String,
}

impl NotFoundError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidRequest(#[from] InvalidRequestError),

    #[error("{0}")]
    NotFound(#[from] NotFoundError),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::InvalidRequest(e) => (StatusCode::BAD_REQUEST, e.message.clone()),
            AppError::NotFound(e) => (StatusCode::NOT_FOUND, e.message.clone()),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_string(),
            ),
        };

        let body = serde_json::to_string(&ErrorBody {
            error: message,
            message: None,
        })
        .unwrap_or_else(|_| r#"{"error":"Internal Server Error"}"#.to_string());

        (
            status,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response()
    }
}

/// Detect the Postgres "NUL byte in text" failure from a stringified DB error.
/// Postgres cannot store `\0` in text and surfaces it as SQLSTATE 22021 / a
/// message naming the `0x00` byte; sqlx may also reject it at encode time.
fn is_nul_byte_db_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("0x00")
        || m.contains("22021")
        || m.contains("nul byte")
        || m.contains("nul character")
        || m.contains("null byte")
        || (m.contains("invalid") && m.contains("utf") && m.contains("00"))
}

/// Request-body backstop to `nul_guard` (which covers the URL): a JSON
/// `"\0"` only becomes a real NUL after deserialization, past any URL check,
/// then Postgres rejects the bound value. Classify that as a client error (400)
/// rather than an opaque server error (500); everything else stays Internal.
impl From<crate::state::DatabaseError> for AppError {
    fn from(e: crate::state::DatabaseError) -> Self {
        if is_nul_byte_db_error(&e.to_string()) {
            InvalidRequestError::new("a request value contains an invalid NUL byte").into()
        } else {
            AppError::Internal(e.to_string())
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

pub fn bad_request(message: &str) -> Response {
    json_response(
        StatusCode::BAD_REQUEST,
        json!({ "error": "Bad request", "message": message }),
    )
}

pub fn not_found(message: &str) -> Response {
    json_response(
        StatusCode::NOT_FOUND,
        json!({ "error": "Not Found", "message": message }),
    )
}

pub fn internal_server_error() -> Response {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "Internal Server Error" }),
    )
}

/// JSON error body with a caller-chosen status; used to convert axum
/// extractor rejections (Json/Multipart) from their default text/plain
/// responses into the `{"error":...}` JSON contract.
pub fn json_error(status: StatusCode, message: &str) -> Response {
    json_response(status, json!({ "error": message }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nul_byte_db_errors_are_recognized() {
        assert!(is_nul_byte_db_error(
            "sqlx error: error returned from database: invalid byte sequence for encoding \"UTF8\": 0x00"
        ));
        assert!(is_nul_byte_db_error("encode error: unexpected NUL byte in string"));
        assert!(is_nul_byte_db_error("SQLSTATE 22021"));
    }

    #[test]
    fn ordinary_db_errors_are_not_nul() {
        assert!(!is_nul_byte_db_error("sqlx error: pool timed out"));
        assert!(!is_nul_byte_db_error("connection refused"));
        assert!(!is_nul_byte_db_error("relation \"deployments\" does not exist"));
    }
}
