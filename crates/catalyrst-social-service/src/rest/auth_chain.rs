use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// No canonical metadata keys, so the pre-6.0.0 folded payload is never accepted here: this surface
/// verifies the 6.x payload only, which binds the metadata bytes and so closes the re-cased-key
/// bypass of the scene gate. Every first-party social client signs `{}` metadata, which folds to
/// itself, so the two payload shapes are byte-identical for real traffic; the only uppercase-bearing
/// metadata in the ecosystem is what an SDK `signedFetch` attaches on a scene's behalf, and those
/// requests are refused here either way. Name a key here only for a signer that cannot be shipped
/// ahead of this service.
const CANONICAL_METADATA_KEYS: &[&str] = &[];

/// Why a signed-fetch request was turned down at the REST gate.
///
/// The metadata gate answers before signature verification (upstream #492), so it has no
/// [`AuthChainError`] to report: it carries the metadata it read instead, which the response echoes
/// back.
#[derive(Debug)]
pub enum SignedFetchRejection {
    Chain(AuthChainError),
    RefusedMetadata(String),
}

impl From<AuthChainError> for SignedFetchRejection {
    fn from(e: AuthChainError) -> Self {
        SignedFetchRejection::Chain(e)
    }
}

/// The metadata of a request the "not for scenes" gate refuses, as the gate read it.
///
/// Read straight off the header so the gate can answer before any crypto: a refused request is a
/// 400 that costs no catalyst round-trip (upstream #492).
fn refused_metadata(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())?;
    let metadata: serde_json::Value = serde_json::from_str(raw).ok()?;
    crate::scene_signer::is_refused_signer(&metadata).then(|| metadata.to_string())
}

/// Optional signer for the read paths that widen visibility when authenticated. A scene-signed
/// chain (ADR-44, upstream #440) is treated as no signer rather than a valid identity -- otherwise a
/// scene could ride an anonymous caller's chain into the member-only view.
pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    if refused_metadata(headers).is_some() {
        return None;
    }
    verify(headers, method, path).await.ok()
}

/// The crate-local pre-gate above owns the 400 body, which the crypto crate does not produce, so the
/// gate is passed as `None` here rather than handed to the verifier.
async fn verify(headers: &HeaderMap, method: &str, path: &str) -> Result<Signer, AuthChainError> {
    signed_fetch::verify_signed_fetch_with_legacy_fallback(
        headers,
        method,
        path,
        FIVE_MINUTES,
        CANONICAL_METADATA_KEYS,
        None,
    )
    .await
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, SignedFetchRejection> {
    if let Some(metadata) = refused_metadata(headers) {
        tracing::warn!(%method, %path, "signed-fetch rejected: refused signer");
        return Err(SignedFetchRejection::RefusedMetadata(metadata));
    }
    match verify(headers, method, path).await {
        Ok(signer) => Ok(signer),
        Err(e) => {
            tracing::warn!(error = ?e, %method, %path, "signed-fetch rejected");
            Err(SignedFetchRejection::Chain(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_metadata(raw: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTH_METADATA_HEADER, HeaderValue::from_str(raw).unwrap());
        headers
    }

    #[tokio::test]
    async fn a_refused_signer_answers_before_verification() {
        // No auth chain at all: reaching verification first would report the missing chain instead.
        let headers = headers_with_metadata(r#"{"signer":"decentraland-kernel-scene"}"#);
        let err = require_signer(&headers, "get", "/v1/mutes")
            .await
            .expect_err("a scene-signed request must be refused");
        assert!(
            matches!(&err, SignedFetchRejection::RefusedMetadata(m)
                if m == r#"{"signer":"decentraland-kernel-scene"}"#),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn a_re_cased_signer_value_answers_from_the_metadata_gate() {
        let headers = headers_with_metadata(r#"{"signer":"Decentraland-Kernel-Scene"}"#);
        let err = require_signer(&headers, "get", "/v1/mutes")
            .await
            .unwrap_err();
        assert!(
            matches!(err, SignedFetchRejection::RefusedMetadata(_)),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn ordinary_metadata_falls_through_to_verification() {
        for raw in [r#"{}"#, r#"{"signer":"dcl:explorer"}"#, "not json"] {
            let headers = headers_with_metadata(raw);
            let err = require_signer(&headers, "get", "/v1/mutes")
                .await
                .unwrap_err();
            assert!(
                matches!(err, SignedFetchRejection::Chain(_)),
                "{raw}: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_optional_path_drops_a_refused_signer() {
        let headers = headers_with_metadata(r#"{"signer":42}"#);
        assert!(try_extract_signer(&headers, "get", "/v1/communities")
            .await
            .is_none());
    }

    const SCENE_METADATA: &str = r#"{"signer":"decentraland-kernel-scene"}"#;
    const RECASED_KEY: &str = r#"{"Signer":"decentraland-kernel-scene"}"#;
    const ROOT_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const EPHEMERAL_KEY: &str = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

    /// A genuinely signed chain whose delivered metadata may differ from the metadata that went
    /// into the signed payload -- the shape of the re-cased-key attack, where nothing about the
    /// signature is weakened and only the bytes on the wire are rewritten.
    async fn signed_headers(signed_payload: &str, delivered_metadata: &str) -> (String, HeaderMap) {
        use alloy::signers::{local::PrivateKeySigner, Signer as _};

        let root: PrivateKeySigner = ROOT_KEY.parse().unwrap();
        let ephemeral: PrivateKeySigner = EPHEMERAL_KEY.parse().unwrap();
        let root_address = format!("{:#x}", root.address());
        let ephemeral_payload = format!(
            "Decentraland Login\nEphemeral address: {:#x}\nExpiration: 2099-01-01T00:00:00.000Z",
            ephemeral.address()
        );
        let ephemeral_sig = root
            .sign_message(ephemeral_payload.as_bytes())
            .await
            .unwrap();
        let entity_sig = ephemeral
            .sign_message(signed_payload.as_bytes())
            .await
            .unwrap();

        let link = |kind: &str, payload: &str, signature: &str| {
            HeaderValue::from_str(
                &serde_json::json!({
                    "type": kind,
                    "payload": payload,
                    "signature": signature,
                })
                .to_string(),
            )
            .unwrap()
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-identity-auth-chain-0", link("SIGNER", &root_address, ""));
        headers.insert(
            "x-identity-auth-chain-1",
            link(
                "ECDSA_EPHEMERAL",
                &ephemeral_payload,
                &ephemeral_sig.to_string(),
            ),
        );
        headers.insert(
            "x-identity-auth-chain-2",
            link(
                "ECDSA_SIGNED_ENTITY",
                signed_payload,
                &entity_sig.to_string(),
            ),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(delivered_metadata).unwrap(),
        );
        (root_address.to_lowercase(), headers)
    }

    fn now_ms() -> String {
        chrono::Utc::now().timestamp_millis().to_string()
    }

    fn with_timestamp(mut headers: HeaderMap, ts_ms: &str) -> HeaderMap {
        headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_str(ts_ms).unwrap());
        headers
    }

    #[tokio::test]
    async fn a_re_cased_metadata_key_no_longer_folds_back_into_the_signed_scene_metadata() {
        let ts = now_ms();
        let signed = build_payload("get", "/v1/mutes", &ts, SCENE_METADATA);
        // The premise of the attack: under the pre-6.0.0 folded payload the rewrite was invisible,
        // so the scene's own signature verified over the metadata the gate had just read as absent.
        assert_eq!(build_payload("get", "/v1/mutes", &ts, RECASED_KEY), signed);
        let (_, headers) = signed_headers(&signed, RECASED_KEY).await;
        let err = require_signer(&with_timestamp(headers, &ts), "get", "/v1/mutes")
            .await
            .expect_err("a scene-signed chain must not be served as a user identity");
        assert!(
            matches!(
                err,
                SignedFetchRejection::Chain(AuthChainError::InvalidSignature(_))
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn the_optional_path_drops_a_re_cased_metadata_key() {
        let ts = now_ms();
        let signed = build_payload("get", "/v1/communities", &ts, SCENE_METADATA);
        let (_, headers) = signed_headers(&signed, RECASED_KEY).await;
        assert!(
            try_extract_signer(&with_timestamp(headers, &ts), "get", "/v1/communities")
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_metadata_field_this_service_never_reads_is_still_bound_to_the_signature() {
        let ts = now_ms();
        let signed = build_payload("get", "/v1/mutes", &ts, r#"{"sceneId":"bafkreiabcdef"}"#);
        let (_, headers) = signed_headers(&signed, r#"{"sceneId":"BAFKREIABCDEF"}"#).await;
        let err = require_signer(&with_timestamp(headers, &ts), "get", "/v1/mutes")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                SignedFetchRejection::Chain(AuthChainError::InvalidSignature(_))
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn empty_metadata_verifies_under_either_payload_shape() {
        // Every first-party social client signs `{}`, which folds to itself, so the legacy bytes a
        // deployed client mints are identical to the 6.x bytes this gate now verifies.
        let ts = now_ms();
        let legacy = build_payload("get", "/v1/mutes", &ts, "{}");
        let v6 = catalyrst_crypto::signed_fetch::build_payload_v6("get", "/v1/mutes", &ts, "{}");
        assert_eq!(legacy, v6);
        let (expected, headers) = signed_headers(&legacy, "{}").await;
        let signer = require_signer(&with_timestamp(headers, &ts), "get", "/v1/mutes")
            .await
            .expect("an empty-metadata chain must still authenticate");
        assert_eq!(signer.as_str(), expected);
    }

    #[tokio::test]
    async fn mixed_case_metadata_delivered_untouched_authenticates() {
        let ts = now_ms();
        let signed = catalyrst_crypto::signed_fetch::build_payload_v6(
            "get",
            "/v1/mutes",
            &ts,
            r#"{"sceneId":"BafkreiAbcDef"}"#,
        );
        let (expected, headers) = signed_headers(&signed, r#"{"sceneId":"BafkreiAbcDef"}"#).await;
        let signer = require_signer(&with_timestamp(headers, &ts), "get", "/v1/mutes")
            .await
            .expect("mixed-case metadata is ordinary traffic when it is delivered as signed");
        assert_eq!(signer.as_str(), expected);
    }

    #[tokio::test]
    async fn unparseable_and_non_object_metadata_are_refused_as_malformed() {
        let ts = now_ms();
        for raw in ["not json", "[]", "42", "\"scene\""] {
            let signed = build_payload("get", "/v1/mutes", &ts, raw);
            let (_, headers) = signed_headers(&signed, raw).await;
            let err = require_signer(&with_timestamp(headers, &ts), "get", "/v1/mutes")
                .await
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    SignedFetchRejection::Chain(AuthChainError::MalformedChain { .. })
                ),
                "{raw}: {err:?}"
            );
        }
    }
}
