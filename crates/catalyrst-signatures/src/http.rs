use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::{ApiErrorBody, ApiOk};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

pub struct Ok2<T: Serialize>(pub StatusCode, pub T);

impl<T: Serialize> IntoResponse for Ok2<T> {
    fn into_response(self) -> Response {
        // Rendering through serde_json::Value keeps the published key order the
        // same whether or not serde_json's preserve_order feature is unified on
        // by the rest of the workspace.
        (self.0, Json(json!(ApiOk::new(self.1)))).into_response()
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Common(#[from] catalyrst_types::ApiError),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl ApiError {
    pub fn http(status: u16, msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::http(status, msg))
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::http(400, msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::http(401, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::http(404, msg)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::http(409, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Common(catalyrst_types::ApiError::internal(msg))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Common(e) => e.into_response(),
            ApiError::Database(e) => {
                tracing::error!(error = %e, "sqlx error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiErrorBody::new("Server error")),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn success_envelope_wire_shape() {
        let data = json!({ "a": 1 });
        let resp = Ok2(StatusCode::OK, data.clone()).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let legacy = serde_json::to_vec(&json!({ "ok": true, "data": data })).unwrap();
        assert_eq!(bytes.as_ref(), legacy.as_slice());
    }

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::not_found("Rental not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Rental not found", "message": "Rental not found" })
        );
    }

    #[tokio::test]
    async fn internal_detail_is_not_published() {
        let resp = ApiError::internal("signer key load failed: /etc/keys/x").into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "internal error", "message": "internal error" })
        );
    }

    #[tokio::test]
    async fn database_detail_is_not_published() {
        let resp = ApiError::from(sqlx::Error::RowNotFound).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Server error", "message": "Server error" })
        );
    }
}
