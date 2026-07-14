use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

pub use catalyrst_types::{ApiErrorBody, HttpError, InvalidParameterError};

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
}

impl<T> ApiOk<T> {
    pub fn new(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Common(#[from] catalyrst_types::ApiError),

    #[error(transparent)]
    InvalidParameter(#[from] InvalidParameterError),
}

impl ApiError {
    pub fn http(status: u16, msg: impl Into<String>) -> Self {
        ApiError::Common(catalyrst_types::ApiError::http(status, msg))
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::http(400, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::http(404, msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::http(401, msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::http(403, msg)
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::http(501, msg)
    }
    pub fn gone(msg: impl Into<String>) -> Self {
        Self::http(410, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError::Common(catalyrst_types::ApiError::internal(msg))
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
            ApiError::Common(e) => e.into_response(),
            ApiError::InvalidParameter(e) => (
                StatusCode::BAD_REQUEST,
                Json(ApiErrorBody::new(e.to_string())),
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_api_ok_envelope() {
        let new = serde_json::to_value(ApiOk::new(vec![1, 2])).unwrap();
        assert_eq!(new, json!({ "ok": true, "data": [1, 2] }));

        let doc = json!({ "name": "x", "description": null });
        let new = serde_json::to_value(ApiOk::new(doc.clone())).unwrap();
        assert_eq!(new, json!({ "ok": true, "data": doc }));
    }
}
