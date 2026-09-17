//! No kernel-scene `metadataValidator` here, and that is upstream's posture rather
//! than an omission: camera-reel-service verifies with
//! `VerificationOptions::default().accept_legacy_payload(&[])` and declares the
//! empty key list deliberately (`src/api/auth.rs`) because nothing in the service
//! reads `x-identity-metadata` - `AuthUser` carries only the recovered address, as
//! it does here. The union of both payload shapes that `verify_signed_fetch` takes
//! is the same accept set. Revisit if a handler starts authorizing on a metadata
//! field.

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES).await
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES).await
}
