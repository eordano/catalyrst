use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate};

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

use crate::http::ApiError;

pub const FIVE_MINUTES: i64 = 5 * 60;

/// The `signer` an explorer sets on an auth chain it signed on a scene's behalf.
pub const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// The only metadata field this crate authorizes on, so the only one pinned to
/// its declared spelling when a legacy-payload request is accepted. The
/// pre-6.0.0 payload lowercased the metadata before signing, so
/// `{"Signer":"decentraland-kernel-scene"}` carries a signature identical to the
/// canonical spelling while a gate reading `get("signer")` sees it as absent.
/// Every in-tree signer still mints that folded payload
/// (`signed_fetch::build_payload`), so the fallback stays on behind the guard;
/// drop this to `&[]` once they mint 6.x.
const CANONICAL_METADATA_KEYS: &[&str] = &["signer"];

/// Upstream notifications-workers wires `rejectIfSigner('decentraland-kernel-scene')`
/// as the `metadataValidator` of every signed-fetch route
/// (`inbox/src/controllers/routes.ts` `createSignedFetchMiddleware`), so a request a
/// scene runtime signed on a visiting player's behalf is refused rather than served
/// as that player.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical"))
}

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    verify(headers, method, path).await.ok()
}

async fn verify(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, signed_fetch::AuthChainError> {
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

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, ApiError> {
    verify(headers, method, path).await.map_err(|e| {
        tracing::warn!(error = ?e, "signed-fetch rejected");
        ApiError::unauthorized(e.to_string())
    })
}

#[cfg(test)]
mod scene_signer_gate_tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use catalyrst_crypto::signed_fetch::build_payload_v6;
    use catalyrst_crypto::{create_simple_auth_chain, Wallet};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
    const METHOD: &str = "put";
    const PATH: &str = "/notifications/read";

    fn signed(metadata: &str, v6: bool) -> HeaderMap {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let payload = if v6 {
            build_payload_v6(METHOD, PATH, &timestamp, metadata)
        } else {
            build_payload(METHOD, PATH, &timestamp, metadata)
        };
        let chain = create_simple_auth_chain(&wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
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
        let metadata = r#"{"signer":"dcl:explorer"}"#;
        for v6 in [true, false] {
            let signer = verify(&signed(metadata, v6), METHOD, PATH)
                .await
                .unwrap_or_else(|e| panic!("v6={v6}: {e}"));
            assert_eq!(
                signer.as_str().to_lowercase(),
                Wallet::from_hex(KEY).unwrap().address().to_lowercase()
            );
        }
    }

    /// Upstream refuses this before any signature is checked; ours served it as the
    /// visiting user until the gate was wired in.
    #[tokio::test]
    async fn a_scene_signed_request_is_refused() {
        for metadata in [
            r#"{"signer":"decentraland-kernel-scene"}"#,
            r#"{"Signer":"decentraland-kernel-scene"}"#,
        ] {
            for v6 in [true, false] {
                let err = verify(&signed(metadata, v6), METHOD, PATH)
                    .await
                    .expect_err("a scene-signed request must not be served");
                assert_eq!(
                    err.http_status_and_message().0,
                    400,
                    "v6={v6} {metadata}: {err}"
                );
                assert!(
                    try_extract_signer(&signed(metadata, v6), METHOD, PATH)
                        .await
                        .is_none(),
                    "v6={v6} {metadata}: the optional path must read a refused request as anonymous"
                );
            }
        }
    }
}
