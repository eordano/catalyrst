use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate};

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, validate_signature, AuthChain, AuthChainError, AuthLink,
    AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// The `signer` an explorer sets on an auth chain it signed on a scene's behalf.
pub const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// The only metadata field this crate authorizes on, so the only one pinned to
/// its declared spelling when a legacy-payload request is accepted. Declaring it
/// is what keeps the fold acceptable: the pre-6.0.0 payload lowercased the
/// metadata before signing, so `{"Signer":"decentraland-kernel-scene"}` carries a
/// signature identical to the canonical spelling while a gate reading
/// `get("signer")` sees it as absent. Every in-tree signer still mints that
/// folded payload (`signed_fetch::build_payload`), so the fallback stays on here
/// where upstream gatsby is already 6.x-only; drop this to `&[]` once they mint
/// 6.x.
const CANONICAL_METADATA_KEYS: &[&str] = &["signer"];

/// Upstream decentraland-gatsby makes this the DEFAULT `metadataValidator` on
/// every authed route (`src/entities/Auth/utils.ts` `verifySigner` ->
/// `rejectIfSigner(SCENE_SIGNER)`, defaulted in `resolveVerifyOptions`), so a
/// request a scene runtime signed on a visiting player's behalf is refused
/// rather than served as that player.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical"))
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    signed_fetch::verify_signed_fetch_with_legacy_fallback(
        headers,
        method,
        path,
        FIVE_MINUTES,
        CANONICAL_METADATA_KEYS,
        Some(scene_signer_gate()),
    )
    .await
}

/// Upstream's optional path (`routes/withDecentralandAuth.ts`) swallows every
/// refusal - the scene gate's included - to ANONYMOUS, never to an authenticated
/// caller, so the gate costs an optional route nothing and a scene-signed request
/// reads as a signed-out visitor.
pub async fn optional_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    headers.get(format!("{AUTH_CHAIN_HEADER_PREFIX}0"))?;
    match require_signer(headers, method, path).await {
        Ok(signer) => Some(signer),
        Err(e) => {
            tracing::debug!(error = %e, "optional signed-fetch verification failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use catalyrst_crypto::signed_fetch::build_payload_v6;
    use catalyrst_crypto::{create_simple_auth_chain, Wallet};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
    const PATH: &str = "/api/places/1/likes";

    fn signed(payload_metadata: &str, delivered_metadata: &str, v6: bool) -> HeaderMap {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let payload = if v6 {
            build_payload_v6("patch", PATH, &timestamp, payload_metadata)
        } else {
            build_payload("patch", PATH, &timestamp, payload_metadata)
        };
        let chain = create_simple_auth_chain(&wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(delivered_metadata).unwrap(),
        );
        for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
            headers.insert(
                HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                    .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            );
        }
        headers
    }

    #[tokio::test]
    async fn an_ordinary_signer_still_verifies_under_either_payload_shape() {
        let metadata = r#"{"signer":"dcl:explorer","intent":"dcl:places:like"}"#;
        for v6 in [true, false] {
            let signer = require_signer(&signed(metadata, metadata, v6), "patch", PATH)
                .await
                .unwrap_or_else(|e| panic!("v6={v6}: {e}"));
            assert_eq!(
                signer.as_str().to_lowercase(),
                Wallet::from_hex(KEY).unwrap().address().to_lowercase()
            );
        }
    }

    /// Upstream gatsby refuses this on every authed route; ours served it as the
    /// visiting user until the gate was wired in.
    #[tokio::test]
    async fn a_scene_signed_request_is_refused_and_reads_as_anonymous() {
        let metadata = r#"{"signer":"decentraland-kernel-scene"}"#;
        for v6 in [true, false] {
            let err = require_signer(&signed(metadata, metadata, v6), "patch", PATH)
                .await
                .expect_err("a scene-signed request must not be served");
            assert_eq!(err.http_status_and_message().0, 400, "v6={v6}: {err}");
            assert!(
                optional_signer(&signed(metadata, metadata, v6), "patch", PATH)
                    .await
                    .is_none(),
                "v6={v6}: the optional path must read a refused request as anonymous"
            );
        }
    }

    /// The fold is why the gate refuses a non-canonical spelling instead of
    /// comparing it: the legacy payload lowercases the metadata before signing, so
    /// `{"Signer":...}` carries a signature identical to the canonical spelling.
    #[tokio::test]
    async fn a_folded_scene_signer_key_is_refused_on_both_paths() {
        let metadata = r#"{"Signer":"decentraland-kernel-scene"}"#;
        for v6 in [true, false] {
            let err = require_signer(&signed(metadata, metadata, v6), "patch", PATH)
                .await
                .expect_err("a re-spelled scene signer must not be served");
            assert_eq!(err.http_status_and_message().0, 400, "v6={v6}: {err}");
            assert!(
                optional_signer(&signed(metadata, metadata, v6), "patch", PATH)
                    .await
                    .is_none(),
                "v6={v6}: the optional path must read a refused request as anonymous"
            );
        }
    }

    #[tokio::test]
    async fn stale_timestamp_rejected_even_with_colon_in_path() {
        let chain = AuthChain {
            links: vec![],
            signer: String::new(),
        };
        let payload = "get:/world/urn:decentraland:foo:1000000000000:{}";
        let stale_ts = "1000000000000";
        let now = 2_000_000_000_i64;
        let r = validate_signature(&chain, payload, stale_ts, FIVE_MINUTES, now).await;
        assert!(
            matches!(r, Err(AuthChainError::Expired { .. })),
            "stale ts must be Expired"
        );
    }
}
