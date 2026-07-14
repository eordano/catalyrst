pub mod images;
pub mod places;
pub mod users;

use axum::http::HeaderMap;

use crate::auth_chain::{require_signer, try_extract_signer};
use crate::http::ApiError;

pub async fn require_auth(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, ApiError> {
    require_signer(headers, method, path).await.map_err(|e| {
        tracing::debug!(error = %e, "auth chain verification failed");
        ApiError::Unauthorized
    })
}

pub async fn optional_auth(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Option<catalyrst_crypto::Signer> {
    try_extract_signer(headers, method, path).await
}

pub fn default_offset() -> u64 {
    0
}
pub fn default_limit() -> u64 {
    20
}
pub const MAX_LIMIT: u64 = 100;
pub fn default_compact() -> bool {
    false
}
