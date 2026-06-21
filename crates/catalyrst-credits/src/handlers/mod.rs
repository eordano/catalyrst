pub mod admin;
pub mod captcha;
pub mod cart;
pub mod packs;
pub mod ping;
pub mod prices;
pub mod seasons;
pub mod stripe;
pub mod topup;
pub mod users;
pub mod wallet;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use axum::http::HeaderMap;

pub fn signer_from(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    require_signer(headers, method, path).map_err(ApiError::from)
}
