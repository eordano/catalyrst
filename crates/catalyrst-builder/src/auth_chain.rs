use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;
pub const THIRTY_MINUTES: i64 = 30 * 60;

pub fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES)
}

pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, AuthChainError> {
    signed_fetch::verify_signed_fetch(headers, method, path, THIRTY_MINUTES)
}
