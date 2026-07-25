use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_types::EthAddress;

pub use catalyrst_crypto::signed_fetch::AuthChainError;

pub const FIVE_MINUTES: i64 = 5 * 60;

pub const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

#[derive(Debug, Clone)]
pub struct VerifiedAuth {
    pub signer: EthAddress,
    pub metadata: serde_json::Value,
}

impl VerifiedAuth {
    pub fn secret(&self) -> Option<String> {
        self.metadata
            .get("secret")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

pub fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedAuth, AuthChainError> {
    let (signer, metadata) =
        signed_fetch::verify_signed_fetch_meta(headers, method, path, FIVE_MINUTES)?;

    if metadata
        .get("signer")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case(KERNEL_SCENE_SIGNER))
        .unwrap_or(false)
    {
        return Err(AuthChainError::ForbiddenSigner);
    }

    Ok(VerifiedAuth { signer, metadata })
}
