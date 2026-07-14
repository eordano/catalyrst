//! ADR-44: a scene must not act as a user's identity on the privileged social surfaces. The
//! explorer sets `signer: decentraland-kernel-scene` in the signed-fetch metadata when it signs on a
//! scene's behalf; the HTTP routes and the WS RPC handshake both refuse it (upstream #440).

use std::sync::OnceLock;

use catalyrst_crypto::{reject_if_signer, SignerGate};

/// The `signer` value an explorer stamps on an auth chain signed on a scene's behalf.
pub const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// The shared gate rather than a crate-local predicate: this is an authorization decision, and a
/// second implementation of it is only somewhere for the two to drift apart. `reject_if_signer`
/// rejects a non-canonical constant, so a mis-spelled `SCENE_SIGNER` is a startup panic instead of
/// a gate that quietly never fires.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical"))
}

/// Whether the "this surface is not for scenes" gate refuses this signed-fetch metadata.
///
/// Mirrors `rejectIfSigner(SCENE_SIGNER)` (upstream #492). A `signer` that is present but not
/// already trimmed and lowercase is refused outright rather than folded and compared: folding bases
/// the decision on a value the handler never sees, and comparing without folding would let
/// `Decentraland-Kernel-Scene` read as "not a scene" and walk through. A present non-string is
/// refused for the same reason -- it is not the form the gate needs either. Metadata declaring no
/// `signer` passes: it is not claiming to be one.
pub fn is_refused_signer(metadata: &serde_json::Value) -> bool {
    !scene_signer_gate().permits(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_scene_signer_is_refused() {
        assert!(is_refused_signer(&json!({ "signer": SCENE_SIGNER })));
    }

    #[test]
    fn non_canonical_signer_values_are_refused() {
        assert!(is_refused_signer(
            &json!({ "signer": "Decentraland-Kernel-Scene" })
        ));
        assert!(is_refused_signer(
            &json!({ "signer": "DECENTRALAND-KERNEL-SCENE" })
        ));
        assert!(is_refused_signer(
            &json!({ "signer": "  decentraland-kernel-scene\t" })
        ));
        assert!(is_refused_signer(&json!({ "signer": "DCL:Explorer" })));
    }

    #[test]
    fn a_zero_width_no_break_space_does_not_read_as_canonical() {
        // Upstream folds with `String.prototype.trim`, whose WhiteSpace set includes U+FEFF while
        // Rust's `str::trim` does not; the shared gate follows upstream.
        let padded = format!("\u{FEFF}{SCENE_SIGNER}");
        assert!(is_refused_signer(&json!({ "signer": padded })));
    }

    #[test]
    fn a_present_non_string_signer_is_refused() {
        assert!(is_refused_signer(&json!({ "signer": 42 })));
        assert!(is_refused_signer(&json!({ "signer": true })));
        assert!(is_refused_signer(&json!({ "signer": null })));
        assert!(is_refused_signer(&json!({ "signer": [SCENE_SIGNER] })));
        assert!(is_refused_signer(
            &json!({ "signer": { "value": SCENE_SIGNER } })
        ));
    }

    #[test]
    fn canonical_other_and_absent_signers_pass() {
        assert!(!is_refused_signer(&json!({ "signer": "dcl:explorer" })));
        assert!(!is_refused_signer(&json!({ "signer": "" })));
        assert!(!is_refused_signer(&json!({})));
        assert!(!is_refused_signer(&serde_json::Value::Null));
        assert!(!is_refused_signer(&json!({ "sceneId": "BafkreiAbcDef" })));
    }

    #[test]
    fn a_re_cased_signer_key_is_left_to_the_signature() {
        // Upstream #492 leaves key spelling to the signed payload, which binds the metadata bytes
        // verbatim under the 6.x payload this crate now verifies against: `{"Signer":...}` no longer
        // shares a signature with `{"signer":...}`, so it cannot read as absent and stay authentic.
        assert!(!is_refused_signer(&json!({ "Signer": SCENE_SIGNER })));
    }
}
