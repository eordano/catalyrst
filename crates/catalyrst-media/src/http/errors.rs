use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("translation backend error: {0}")]
    Backend(String),

    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match &self {
            ApiError::BadRequest(m) => (400u16, m.clone()),
            ApiError::Backend(m) => {
                tracing::error!(error = %m, "translation backend error");
                (502, "translation backend error".to_string())
            }
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (500, "database error".to_string())
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (500, "internal error".to_string())
            }
        };
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(ApiErrorBody::new(message))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::bad_request("missing q parameter").into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "missing q parameter", "message": "missing q parameter" })
        );
    }
}
