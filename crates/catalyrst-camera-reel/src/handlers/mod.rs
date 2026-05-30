pub mod images;
pub mod places;
pub mod users;

use axum::http::HeaderMap;

use crate::auth_chain::{require_signer, try_extract_signer};
use crate::http::ApiError;

pub fn require_auth(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    require_signer(headers, method, path)
        .map(|a| a.to_lowercase())
        .map_err(|e| {
            tracing::debug!(error = %e, "auth chain verification failed");
            ApiError::Unauthorized
        })
}

pub fn optional_auth(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    try_extract_signer(headers, method, path).map(|a| a.to_lowercase())
}

pub fn default_offset() -> u64 {
    0
}
pub fn default_limit() -> u64 {
    20
}
pub fn default_compact() -> bool {
    false
}
