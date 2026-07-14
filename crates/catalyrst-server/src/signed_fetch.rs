use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate};

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChainError, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

pub const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// DELETE /content/scenes/{coord} reads nothing off the metadata but the gate's key. ui3 and
/// dcl-one-sdk still mint the folded payload, so the fallback stays on behind exactly this key
/// until they ship the 6.x one. Empty the list then.
pub const SCENE_METADATA_KEYS: &[&str] = &["signer"];

#[derive(Debug, Clone)]
pub struct VerifiedAuth {
    pub signer: Signer,
    pub metadata: serde_json::Value,
}

fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| {
        reject_if_signer(&[KERNEL_SCENE_SIGNER]).expect("KERNEL_SCENE_SIGNER is canonical")
    })
}

pub async fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedAuth, AuthChainError> {
    let (signer, metadata) = signed_fetch::verify_signed_fetch_meta_with_legacy_fallback(
        headers,
        method,
        path,
        FIVE_MINUTES,
        SCENE_METADATA_KEYS,
        Some(scene_signer_gate()),
    )
    .await?;

    Ok(VerifiedAuth { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
    use catalyrst_crypto::signed_fetch::build_payload_v6;
    use serde_json::json;

    const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
    const METHOD: &str = "delete";
    const PATH: &str = "/content/scenes/52,-52";
    const CASED_METADATA: &str = r#"{"signer":"dcl:explorer","sceneId":"bafkreiAbC123"}"#;

    fn test_wallet() -> Wallet {
        Wallet::from_hex(TEST_KEY).unwrap()
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn headers_for(wallet: &Wallet, timestamp_ms: i64, metadata: &str, payload: &str) -> HeaderMap {
        let chain = create_simple_auth_chain(wallet, payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp_ms.to_string()).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
            headers.insert(
                axum::http::HeaderName::from_bytes(
                    format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes(),
                )
                .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            );
        }
        headers
    }

    fn signed_headers(wallet: &Wallet, method: &str, path: &str, timestamp_ms: i64) -> HeaderMap {
        signed_headers_with_metadata(wallet, method, path, timestamp_ms, "{}")
    }

    fn signed_headers_with_metadata(
        wallet: &Wallet,
        method: &str,
        path: &str,
        timestamp_ms: i64,
        metadata: &str,
    ) -> HeaderMap {
        let payload = build_payload(method, path, &timestamp_ms.to_string(), metadata);
        headers_for(wallet, timestamp_ms, metadata, &payload)
    }

    fn v6_signed_headers_with_metadata(
        wallet: &Wallet,
        method: &str,
        path: &str,
        timestamp_ms: i64,
        metadata: &str,
    ) -> HeaderMap {
        let payload = build_payload_v6(method, path, &timestamp_ms.to_string(), metadata);
        headers_for(wallet, timestamp_ms, metadata, &payload)
    }

    #[tokio::test]
    async fn fresh_signed_fetch_verifies_and_recovers_signer() {
        let wallet = test_wallet();
        let headers = signed_headers(&wallet, METHOD, PATH, now_ms());
        let auth = require_verified(&headers, METHOD, PATH).await.unwrap();
        assert_eq!(auth.signer, wallet.address().to_lowercase());
    }

    #[tokio::test]
    async fn stale_signature_is_rejected() {
        let wallet = test_wallet();
        let old_ms = now_ms() - (FIVE_MINUTES + 60) * 1000;
        let headers = signed_headers(&wallet, METHOD, PATH, old_ms);
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    #[tokio::test]
    async fn wrong_path_signature_is_rejected() {
        let wallet = test_wallet();
        let headers = signed_headers(&wallet, METHOD, "/content/scenes/0,0", now_ms());
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(matches!(err, AuthChainError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn missing_chain_is_rejected() {
        let headers = HeaderMap::new();
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(matches!(err, AuthChainError::InsufficientLinks));
    }

    #[tokio::test]
    async fn v6_payload_verifies_without_the_fallback() {
        let wallet = test_wallet();
        let headers =
            v6_signed_headers_with_metadata(&wallet, METHOD, PATH, now_ms(), CASED_METADATA);
        let auth = require_verified(&headers, METHOD, PATH).await.unwrap();
        assert_eq!(auth.signer, wallet.address().to_lowercase());
        assert_eq!(auth.metadata["sceneId"], "bafkreiAbC123");
    }

    #[tokio::test]
    async fn legacy_payload_verifies_through_the_fallback() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(&wallet, METHOD, PATH, now_ms(), CASED_METADATA);
        let auth = require_verified(&headers, METHOD, PATH).await.unwrap();
        assert_eq!(auth.signer, wallet.address().to_lowercase());
        assert_eq!(auth.metadata["sceneId"], "bafkreiAbC123");
    }

    #[test]
    fn scene_gate_refuses_folded_signer_keys() {
        for metadata in [
            json!({ "Signer": KERNEL_SCENE_SIGNER }),
            json!({ "SIGNER": KERNEL_SCENE_SIGNER }),
            json!({ "signer": "dcl:explorer", "Signer": KERNEL_SCENE_SIGNER }),
            json!({ "Signer": "dcl:explorer" }),
        ] {
            assert!(!scene_signer_gate().permits(&metadata), "{metadata}");
        }
    }

    #[test]
    fn scene_gate_refuses_non_canonical_or_non_string_signers() {
        for metadata in [
            json!({ "signer": KERNEL_SCENE_SIGNER }),
            json!({ "signer": " decentraland-kernel-scene" }),
            json!({ "signer": "decentraland-kernel-scene " }),
            json!({ "signer": "Decentraland-Kernel-Scene" }),
            json!({ "signer": "\u{FEFF}decentraland-kernel-scene" }),
            json!({ "signer": 42 }),
            json!({ "signer": null }),
            json!({ "signer": [KERNEL_SCENE_SIGNER] }),
        ] {
            assert!(!scene_signer_gate().permits(&metadata), "{metadata}");
        }
    }

    #[test]
    fn scene_gate_passes_exactly_spelled_metadata_without_the_kernel_signer() {
        for metadata in [
            json!({}),
            json!({ "signer": "dcl:explorer" }),
            json!({ "intent": "dcl:explorer:comms-handshake" }),
            json!({ "Intent": "dcl:explorer:comms-handshake" }),
        ] {
            assert!(scene_signer_gate().permits(&metadata), "{metadata}");
        }
    }

    #[tokio::test]
    async fn kernel_scene_signer_is_rejected() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(
            &wallet,
            METHOD,
            PATH,
            now_ms(),
            r#"{"signer":"decentraland-kernel-scene"}"#,
        );
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn re_cased_signer_key_is_rejected() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(
            &wallet,
            METHOD,
            PATH,
            now_ms(),
            r#"{"Signer":"decentraland-kernel-scene"}"#,
        );
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn bom_padded_kernel_signer_is_rejected() {
        let wallet = test_wallet();
        let headers = v6_signed_headers_with_metadata(
            &wallet,
            METHOD,
            PATH,
            now_ms(),
            r#"{"signer":"\ufeffdecentraland-kernel-scene"}"#,
        );
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn non_object_metadata_is_malformed_rather_than_verified() {
        let wallet = test_wallet();
        for metadata in [
            r#""decentraland-kernel-scene""#,
            r#"["decentraland-kernel-scene"]"#,
            "42",
            "{not json",
        ] {
            let headers = signed_headers_with_metadata(&wallet, METHOD, PATH, now_ms(), metadata);
            let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
            assert!(
                matches!(err, AuthChainError::MalformedChain { .. }),
                "{metadata}: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn null_metadata_reads_as_an_empty_object() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(&wallet, METHOD, PATH, now_ms(), "null");
        let auth = require_verified(&headers, METHOD, PATH).await.unwrap();
        assert_eq!(auth.metadata, json!({}));
    }

    #[tokio::test]
    async fn gate_runs_before_signature_verification() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(
            &wallet,
            METHOD,
            "/content/scenes/0,0",
            now_ms(),
            r#"{"signer":"decentraland-kernel-scene"}"#,
        );
        let err = require_verified(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn explorer_signer_metadata_is_accepted() {
        let wallet = test_wallet();
        let headers = signed_headers_with_metadata(
            &wallet,
            METHOD,
            PATH,
            now_ms(),
            r#"{"signer":"dcl:explorer"}"#,
        );
        let auth = require_verified(&headers, METHOD, PATH).await.unwrap();
        assert_eq!(auth.metadata["signer"], "dcl:explorer");
    }
}
