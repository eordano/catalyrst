use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden(String),
    #[error("max limit reached")]
    MaxLimitReached(String),
    #[error("{0}")]
    NotFound(String),
    #[error("bad gateway: {0}")]
    BadGateway(String),
    #[error("{0}")]
    Internal(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, ApiErrorBody::new(m)),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, ApiErrorBody::new("Unauthorized")),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, ApiErrorBody::new(m)),
            ApiError::MaxLimitReached(m) => (
                StatusCode::FORBIDDEN,
                ApiErrorBody::labeled("maxLimitReached", m),
            ),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, ApiErrorBody::new(m)),
            ApiError::BadGateway(m) => (StatusCode::BAD_GATEWAY, ApiErrorBody::new(m)),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, ApiErrorBody::new(m)),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ApiErrorBody::new("database error"),
                )
            }
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::MaxLimitReached("gallery is full".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "maxLimitReached", "message": "gallery is full" })
        );
    }
}
