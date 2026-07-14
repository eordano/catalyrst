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
use serde_json::Value;

/// Pure wrapper around `(status, Json(body)).into_response()` — no status or shape change,
/// just a name for the triple repeated 10+ times across `blocklist.rs` and
/// `feature_flags.rs`.
pub(crate) fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}
