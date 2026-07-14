pub mod admin_auth;
pub mod auth_api;
pub mod blocklist;
pub mod builder_api;
pub mod feature_flags;
pub mod onboarding;
pub mod ping;
pub mod realm_provider;
pub mod runtime_config;
pub mod worlds_content_server;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

pub(crate) fn json_response<T: Serialize>(status: StatusCode, body: T) -> Response {
    (status, Json(body)).into_response()
}

#[derive(Serialize)]
pub(crate) struct ErrorMessage {
    pub error: String,
}
