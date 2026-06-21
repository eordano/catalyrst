pub mod admin;
pub mod captcha;
pub mod ping;
pub mod seasons;
pub mod users;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use axum::http::HeaderMap;

pub fn signer_from(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    require_signer(headers, method, path).map_err(ApiError::from)
}
