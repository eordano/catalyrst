//! ADR-44: a scene must not act as a user's identity on the privileged social surfaces. The
//! explorer sets `signer: decentraland-kernel-scene` in the signed-fetch metadata when it signs on a
//! scene's behalf; the HTTP routes and the WS RPC handshake both refuse it (upstream #440).

/// The `signer` value an explorer stamps on an auth chain signed on a scene's behalf.
pub const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// Whether parsed signed-fetch metadata declares the scene signer.
///
/// The signer is client-supplied and the signing payload is lowercased before the signature is
/// checked, so a re-cased or padded spelling rides a byte-identical, still-valid signature
/// (upstream #465/#484). Trim and lowercase before comparing so `Decentraland-Kernel-Scene` and
/// `  decentraland-kernel-scene ` cannot slip past.
pub fn is_scene_signer(metadata: &serde_json::Value) -> bool {
    metadata
        .get("signer")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_lowercase() == SCENE_SIGNER)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_scene_signer_is_rejected() {
        assert!(is_scene_signer(&json!({ "signer": SCENE_SIGNER })));
    }

    #[test]
    fn recased_and_padded_scene_signer_is_rejected() {
        assert!(is_scene_signer(
            &json!({ "signer": "Decentraland-Kernel-Scene" })
        ));
        assert!(is_scene_signer(
            &json!({ "signer": "  decentraland-kernel-scene\t" })
        ));
    }

    #[test]
    fn other_or_absent_signers_pass() {
        assert!(!is_scene_signer(&json!({ "signer": "dcl:explorer" })));
        assert!(!is_scene_signer(&json!({})));
        assert!(!is_scene_signer(&serde_json::Value::Null));
        assert!(!is_scene_signer(&json!({ "signer": 42 })));
    }
}
