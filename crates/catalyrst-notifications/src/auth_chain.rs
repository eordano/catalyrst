use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

use crate::http::ApiError;

pub const FIVE_MINUTES: i64 = 5 * 60;

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES).await
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, ApiError> {
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES)
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "signed-fetch rejected");
            ApiError::unauthorized(e.to_string())
        })
}
