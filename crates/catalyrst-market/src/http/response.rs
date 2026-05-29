//! Direct port of `marketplace-server/src/logic/http/response.ts`.
//!
//! Handlers return `Result<Json<T>, ApiError>` where `ApiError::into_response`
//! produces the same `{ok: false, message}` body the Node server emits at
//! the matching status code.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

use super::errors::{HttpError, InvalidParameterError};

/// Single error type that every handler converts into.
#[derive(Debug, thiserror::Error)]
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
    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError::Internal(msg.into())
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
        let body = json!({ "ok": false, "message": message });
        (status, Json(body)).into_response()
    }
}

/// `{ data, total }` — the response shape `/v1/contracts`, `/v1/collections`,
/// `/v1/owners`, etc. wrap their results in.
#[derive(Debug, Serialize)]
pub struct DataTotal<T> {
    pub data: Vec<T>,
    pub total: i64,
}

/// `PaginatedResponse<T>` — the response shape `/v1/catalog`, `/v1/nfts`, etc.
/// use. Mirrors `types.ts:PaginatedResponse`.
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub results: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub pages: i64,
    pub limit: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(results: Vec<T>, total: i64, limit: i64, offset: i64) -> Self {
        let page = if limit > 0 { offset / limit } else { 0 };
        let pages = if limit > 0 {
            (total + limit - 1) / limit
        } else {
            0
        };
        Self {
            results,
            total,
            page,
            pages,
            limit,
        }
    }
}
