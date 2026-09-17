//! No kernel-scene `metadataValidator` here, unlike the gatsby-based services
//! (places, events) and the crypto-middleware ones (marketplace-server,
//! builder-server, notifications-workers): credits-server has no public mirror to
//! derive a posture from, and nothing in this crate authorizes on
//! `x-identity-metadata` - every handler takes the address from the recovered
//! signature alone. Wire `reject_if_signer(&["decentraland-kernel-scene"])` into
//! `require_signer` the day a credits-server source appears carrying one, or the
//! day a handler starts reading a metadata field.

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
