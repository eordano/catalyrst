use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::AuthChainError;

pub const FIVE_MINUTES: i64 = 5 * 60;

pub const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

#[derive(Debug, Clone)]
pub struct VerifiedAuth {
    pub signer: Signer,
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

/// Upstream refuses a non-canonical `signer` in the middleware before this
/// comparison ever runs, so padding never reaches its `!==`. Ours runs the
/// comparison itself, and the folded payload preserves whitespace, so a value
/// padded at signing time verifies as delivered: canonicalize both sides or
/// ` decentraland-kernel-scene` reads as "not a scene" and is served.
fn claims_kernel_scene(metadata: &serde_json::Value) -> bool {
    metadata
        .get("signer")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().eq_ignore_ascii_case(KERNEL_SCENE_SIGNER))
        .unwrap_or(false)
}

pub async fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedAuth, AuthChainError> {
    let (signer, metadata) =
        signed_fetch::verify_signed_fetch_meta(headers, method, path, FIVE_MINUTES).await?;

    if claims_kernel_scene(&metadata) {
        return Err(AuthChainError::ForbiddenSigner);
    }

    Ok(VerifiedAuth { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scene_signer_is_refused_however_it_is_spelled() {
        for spelling in [
            "decentraland-kernel-scene",
            "Decentraland-Kernel-Scene",
            " decentraland-kernel-scene",
            "decentraland-kernel-scene ",
            "\tDECENTRALAND-KERNEL-SCENE\n",
        ] {
            assert!(
                claims_kernel_scene(&json!({ "signer": spelling })),
                "{spelling:?} must not read as a user-signed request"
            );
        }
    }

    #[test]
    fn other_signers_are_untouched() {
        assert!(!claims_kernel_scene(&json!({ "signer": "dcl:explorer" })));
        assert!(!claims_kernel_scene(&json!({ "signer": "0xAbC" })));
        assert!(!claims_kernel_scene(&json!({ "signer": 7 })));
        assert!(!claims_kernel_scene(&json!({})));
    }
}
