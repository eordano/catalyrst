use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate};

pub use catalyrst_crypto::signed_fetch::AuthChainError;

pub const FIVE_MINUTES: i64 = 5 * 60;

pub const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// The explorer comms handshakes. The js explorers still sign the folded payload
/// with camelCase metadata; bevy-explorer mints the 6.x payload, which the
/// guarded fallback already tries first. So the fallback stays on behind exactly
/// the keys upstream pins; `intent` is not read here but is declared so a re-spelled
/// `Intent` is refused on the same requests upstream refuses it. Empty the list
/// once every explorer signs the 6.x payload.
pub const EXPLORER_METADATA_KEYS: &[&str] = &["signer", "intent", "secret"];

/// `POST /world/:name/permissions/:permission` only: creator-hub still signs the
/// folded payload here and `post_permissions` reads every one of these. A key
/// list pins spellings, never values, which is why this reaches one route.
pub const PERMISSIONS_METADATA_KEYS: &[&str] =
    &["signer", "type", "secret", "wallets", "communities", "nft"];

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

/// Refuses a `signer` that is not already canonical instead of folding it, and
/// refuses a key that merely folds to `signer`: either would otherwise read as
/// "not a scene" on a request whose delivered bytes are exactly what it signed.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| {
        reject_if_signer(&[KERNEL_SCENE_SIGNER]).expect("KERNEL_SCENE_SIGNER is canonical")
    })
}

/// `canonical_metadata_keys` is the switch for the folded-payload fallback: a
/// route that names no keys verifies the 6.x payload only. The owner-only
/// routes (scene delete, undeploy, settings, per-address permissions) name
/// none, as upstream keeps them strict; the gate still answers there.
pub async fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    canonical_metadata_keys: &[&str],
) -> Result<VerifiedAuth, AuthChainError> {
    let (signer, metadata) = signed_fetch::verify_signed_fetch_meta_with_legacy_fallback(
        headers,
        method,
        path,
        FIVE_MINUTES,
        canonical_metadata_keys,
        Some(scene_signer_gate()),
    )
    .await?;

    Ok(VerifiedAuth { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_crypto::metadata_gate::assert_canonical_metadata_keys;
    use serde_json::json;

    fn permits(metadata: serde_json::Value) -> bool {
        scene_signer_gate().permits(&metadata)
    }

    #[test]
    fn every_route_policy_pins_the_gate_key_and_is_well_formed() {
        for keys in [EXPLORER_METADATA_KEYS, PERMISSIONS_METADATA_KEYS] {
            assert!(keys.contains(&"signer"), "{keys:?} must pin the gate's key");
            assert!(assert_canonical_metadata_keys(keys).is_ok());
        }
    }

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
                !permits(json!({ "signer": spelling })),
                "{spelling:?} must not read as a user-signed request"
            );
        }
    }

    #[test]
    fn a_key_that_folds_to_signer_is_refused_rather_than_read_as_absent() {
        for key in ["Signer", "SIGNER", "sIgNeR"] {
            assert!(
                !permits(json!({ key: KERNEL_SCENE_SIGNER })),
                "{key:?} must not walk past the gate"
            );
        }
        assert!(!permits(json!({
            "signer": "dcl:explorer",
            "Signer": KERNEL_SCENE_SIGNER,
        })));
    }

    #[test]
    fn other_signers_are_untouched() {
        assert!(permits(json!({ "signer": "dcl:explorer" })));
        assert!(permits(json!({ "signer": "0xabc" })));
        assert!(permits(json!({ "intent": "dcl:explorer:comms-handshake" })));
        assert!(permits(json!({})));
    }
}
