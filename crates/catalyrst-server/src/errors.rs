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
