pub mod admin;
pub mod broker;
pub mod contracts;
pub mod escrow;
pub mod health;
pub mod names;
pub mod payments;
pub mod ping;
pub mod transactions;

use axum::http::HeaderMap;

use crate::http::errors::ApiError;
use crate::AppState;

pub(crate) fn idempotency_key(headers: &HeaderMap, body_key: Option<&str>) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .or(body_key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn require_broadcast_enabled(state: &AppState, what: &str) -> Result<(), ApiError> {
    if state.runtime.relayer_enabled() {
        return Ok(());
    }
    Err(ApiError::RelayerUnavailable(format!(
        "Broadcasting is paused by the operator (relayer toggle is OFF); {what} are not broadcast while paused."
    )))
}
