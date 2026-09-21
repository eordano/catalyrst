use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;

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

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    Conflict(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = self.status_and_message();
        (status, Json(ApiErrorBody::new(message))).into_response()
    }
}

impl AppError {
    fn status_and_message(&self) -> (StatusCode, String) {
        match self {
            AppError::InvalidRequest(e) => (StatusCode::BAD_REQUEST, e.message.clone()),
            AppError::NotFound(e) => (StatusCode::NOT_FOUND, e.message.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_string(),
            ),
        }
    }
}

#[derive(Debug)]
pub struct ContentQueryError(AppError);

impl From<AppError> for ContentQueryError {
    fn from(error: AppError) -> Self {
        Self(error)
    }
}

impl From<NotFoundError> for ContentQueryError {
    fn from(error: NotFoundError) -> Self {
        Self(error.into())
    }
}

impl From<InvalidRequestError> for ContentQueryError {
    fn from(error: InvalidRequestError) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for ContentQueryError {
    fn into_response(self) -> Response {
        let (status, message) = self.0.status_and_message();
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

fn is_nul_byte_db_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("0x00")
        || m.contains("22021")
        || m.contains("nul byte")
        || m.contains("nul character")
        || m.contains("null byte")
        || (m.contains("invalid") && m.contains("utf") && m.contains("00"))
}

impl From<crate::state::DatabaseError> for AppError {
    fn from(e: crate::state::DatabaseError) -> Self {
        if is_nul_byte_db_error(&e.to_string()) {
            InvalidRequestError::new("a request value contains an invalid NUL byte").into()
        } else {
            AppError::Internal(e.to_string())
        }
    }
}

impl From<catalyrst_storage::StorageError> for AppError {
    fn from(e: catalyrst_storage::StorageError) -> Self {
        AppError::Internal(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod content_wire_contract {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn query_routes_preserve_upstream_error_bodies() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let router = crate::routes::build_router(state);
        for (path, status, message) in [
            (
                "/queries/items/missing/thumbnail",
                StatusCode::NOT_FOUND,
                "Entity not found.",
            ),
            (
                "/queries/items/missing/image",
                StatusCode::NOT_FOUND,
                "Entity not found.",
            ),
            (
                "/queries/erc721/137/0x123/0",
                StatusCode::NOT_FOUND,
                "Entity does not exist",
            ),
            (
                "/queries/erc721/137/0x123/0/1",
                StatusCode::NOT_FOUND,
                "Entity does not exist",
            ),
            (
                "/queries/erc721/invalid/0x123/0",
                StatusCode::BAD_REQUEST,
                "Invalid chainId 'invalid'",
            ),
        ] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{path}");
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            assert_eq!(
                body.as_ref(),
                serde_json::to_vec(&serde_json::json!({"error": message})).unwrap(),
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn status_preserves_dao_acronym_on_the_wire() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let response = crate::routes::build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sync = &body["synchronizationStatus"];
        assert!(sync["lastSyncWithDAO"].is_i64());
        assert!(sync.get("lastSyncWithDao").is_none());
    }

    #[tokio::test]
    async fn query_internal_errors_do_not_expose_details() {
        let response = ContentQueryError(AppError::Internal("private database details".into()))
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"error":"Internal Server Error"}"#);
    }
}

#[derive(serde::Serialize)]
struct UpstreamErrorBody {
    error: String,
    message: String,
}

impl UpstreamErrorBody {
    fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

pub fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(UpstreamErrorBody::new("Bad request", message)),
    )
        .into_response()
}

pub fn not_found(message: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(UpstreamErrorBody::new("Not Found", message)),
    )
        .into_response()
}

pub fn service_unavailable(message: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(UpstreamErrorBody::new("Service Unavailable", message)),
    )
        .into_response()
}

pub fn internal_server_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(UpstreamErrorBody::new(
            "Internal Server Error",
            "Internal Server Error",
        )),
    )
        .into_response()
}

pub fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(ApiErrorBody::new(message))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = not_found("Entity not found");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "error": "Not Found", "message": "Entity not found" })
        );
    }

    #[test]
    fn nul_byte_db_errors_are_recognized() {
        assert!(is_nul_byte_db_error(
            "sqlx error: error returned from database: invalid byte sequence for encoding \"UTF8\": 0x00"
        ));
        assert!(is_nul_byte_db_error(
            "encode error: unexpected NUL byte in string"
        ));
        assert!(is_nul_byte_db_error("SQLSTATE 22021"));
    }

    #[test]
    fn ordinary_db_errors_are_not_nul() {
        assert!(!is_nul_byte_db_error("sqlx error: pool timed out"));
        assert!(!is_nul_byte_db_error("connection refused"));
        assert!(!is_nul_byte_db_error(
            "relation \"deployments\" does not exist"
        ));
    }
}
