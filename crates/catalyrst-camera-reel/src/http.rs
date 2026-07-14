use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ForbiddenReason {
    MaxLimitReached,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForbiddenError {
    pub reason: ForbiddenReason,
    pub message: String,
}

impl ForbiddenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            reason: ForbiddenReason::MaxLimitReached,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseError {
    pub message: String,
}

impl ResponseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

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
        match self {
            ApiError::MaxLimitReached(m) => {
                (StatusCode::FORBIDDEN, Json(ForbiddenError::new(m))).into_response()
            }
            ApiError::BadRequest(m) => {
                (StatusCode::BAD_REQUEST, Json(ResponseError::new(m))).into_response()
            }
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(ResponseError::new("Unauthorized")),
            )
                .into_response(),
            ApiError::Forbidden(m) => {
                (StatusCode::FORBIDDEN, Json(ResponseError::new(m))).into_response()
            }
            ApiError::NotFound(m) => {
                (StatusCode::NOT_FOUND, Json(ResponseError::new(m))).into_response()
            }
            ApiError::BadGateway(m) => {
                (StatusCode::BAD_GATEWAY, Json(ResponseError::new(m))).into_response()
            }
            ApiError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ResponseError::new(m)),
            )
                .into_response(),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ResponseError::new("database error")),
                )
                    .into_response()
            }
        }
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
            json!({ "reason": "maxLimitReached", "message": "gallery is full" })
        );
    }
}
