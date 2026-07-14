use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// Optional signer for the read paths that widen visibility when authenticated. Uses the
/// metadata-carrying verify so a scene-signed chain (ADR-44, upstream #440) is treated as no signer
/// rather than a valid identity — otherwise a scene could ride an anonymous caller's chain into the
/// member-only view.
pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    let (signer, metadata) =
        signed_fetch::verify_signed_fetch_meta(headers, method, path, FIVE_MINUTES)
            .await
            .ok()?;
    if crate::scene_signer::is_scene_signer(&metadata) {
        return None;
    }
    Some(signer)
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    match signed_fetch::verify_signed_fetch_meta(headers, method, path, FIVE_MINUTES).await {
        Ok((signer, metadata)) => {
            if crate::scene_signer::is_scene_signer(&metadata) {
                tracing::warn!(%method, %path, "signed-fetch rejected: scene signer");
                return Err(AuthChainError::SceneSignerRejected);
            }
            Ok(signer)
        }
        Err(e) => {
            tracing::warn!(error = ?e, %method, %path, "signed-fetch rejected");
            Err(e)
        }
    }
}
