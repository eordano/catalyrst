use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch::handshake;
use catalyrst_crypto::Signer as VerifiedSigner;

pub use catalyrst_crypto::signed_fetch::handshake::{
    extract_from_object, validate_signature, verify_handshake, AuthChainError,
};
pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES_SECS: i64 = 5 * 60;

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedSigner, AuthChainError> {
    handshake::require_signer(headers, method, path, FIVE_MINUTES_SECS).await
}

pub async fn optional_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Option<VerifiedSigner> {
    handshake::optional_signer(headers, method, path, FIVE_MINUTES_SECS).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::{local::PrivateKeySigner, Signer};

    async fn make_chain(method: &str, path: &str, ts_ms: i64) -> (String, String) {
        let root: PrivateKeySigner =
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .unwrap();
        let root_address = format!("{:#x}", root.address());

        let ephemeral: PrivateKeySigner =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let ephemeral_address = format!("{:#x}", ephemeral.address());

        let ephemeral_payload = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
            ephemeral_address
        );
        let ephemeral_sig = root
            .sign_message(ephemeral_payload.as_bytes())
            .await
            .unwrap();

        let metadata = "{}";
        let payload = build_payload(method, path, &ts_ms.to_string(), metadata);
        let entity_sig = ephemeral.sign_message(payload.as_bytes()).await.unwrap();

        let frame = serde_json::json!({
            "x-identity-auth-chain-0": serde_json::json!({
                "type": "SIGNER",
                "payload": root_address,
                "signature": ""
            }).to_string(),
            "x-identity-auth-chain-1": serde_json::json!({
                "type": "ECDSA_EPHEMERAL",
                "payload": ephemeral_payload,
                "signature": ephemeral_sig.to_string()
            }).to_string(),
            "x-identity-auth-chain-2": serde_json::json!({
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": payload,
                "signature": entity_sig.to_string()
            }).to_string(),
            "x-identity-timestamp": ts_ms.to_string(),
            "x-identity-metadata": metadata
        });
        (root_address, frame.to_string())
    }

    #[tokio::test]
    async fn verify_handshake_accepts_valid_chain() {
        let now_secs = 1_700_000_000;
        let (expected_signer, frame) = make_chain("get", "/", now_secs * 1000).await;
        let signer = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect("valid chain must verify");
        assert_eq!(signer, expected_signer.to_lowercase());
    }

    #[tokio::test]
    async fn verify_handshake_rejects_expired() {
        let signed_secs = 1_700_000_000;
        let now_secs = signed_secs + 10 * 60;
        let (_, frame) = make_chain("get", "/", signed_secs * 1000).await;
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect_err("expired chain must be rejected");
        assert!(matches!(err, AuthChainError::Expired { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn verify_handshake_rejects_malformed_envelope() {
        let err = verify_handshake("not json", "get", "/", FIVE_MINUTES_SECS, 0)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
        let err2 = verify_handshake("[]", "get", "/", FIVE_MINUTES_SECS, 0)
            .await
            .unwrap_err();
        assert!(
            matches!(err2, AuthChainError::EnvelopeNotObject),
            "{err2:?}"
        );
    }
}
