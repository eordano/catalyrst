use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Common(#[from] catalyrst_types::ApiError),

    #[error("{message}")]
    TooManyRequests { message: String, retry_after: u64 },

    /// 503 with `Retry-After: 5`: shed by the shared multipart upload limiter.
    #[error("{0}")]
    UploadShed(String),

    /// 408: multipart receive or processing deadline exceeded.
    #[error("{0}")]
    RequestTimeout(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::bad_request(msg))
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::unauthorized(msg))
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::forbidden(msg))
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::not_found(msg))
    }
    /// 409: a concurrent deploy changed the overlapping-scene set out from under a
    /// scoped replacement authorization.
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::http(409, msg))
    }
    pub fn too_many(msg: impl Into<String>, retry_after: u64) -> Self {
        Self::TooManyRequests {
            message: msg.into(),
            retry_after,
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::internal(msg))
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::service_unavailable(msg))
    }
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            ApiError::Common(catalyrst_types::ApiError::Http { status: 409, .. })
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Common(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Common(inner) => inner.into_response(),
            ApiError::TooManyRequests {
                message,
                retry_after,
            } => err(429, message, Some(retry_after)),
            ApiError::UploadShed(m) => crate::upload_limits::shed_response(&m),
            ApiError::RequestTimeout(m) => crate::upload_limits::timeout_response(&m),
        }
    }
}

fn err(code: u16, message: String, retry_after: Option<u64>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = Json(ApiErrorBody::new(message));
    match retry_after {
        Some(secs) => (status, [("Retry-After", secs.to_string())], body).into_response(),
        None => (status, body).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::not_found("world not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "world not found", "message": "world not found" })
        );
    }

    #[tokio::test]
    async fn internal_detail_is_not_published() {
        let resp = ApiError::internal("local content open: /srv/worlds/secret.bin").into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "internal error", "message": "internal error" })
        );
    }
}
