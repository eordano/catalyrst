//! Signed-fetch (ADR-44) auth for the rentals service, delegated to the shared
//! `catalyrst_crypto::signed_fetch` path.
//!
//! This module used to carry a private fork of the whole signed-fetch module
//! (chain types, extraction, freshness, verification). The fork verified the
//! same payloads but skipped the shared extractor's structural checks (SIGNER
//! only at index 0, first link must be SIGNER, non-first links must carry a
//! signature, chain-length overflow rejected), so consolidating onto the shared
//! path is strictly tightening for malformed chains and byte-identical for
//! well-formed ones.
//!
//! The one wire-visible surface this service adds on top is [`wire_message`]:
//! the fork's error `Display` included the `MalformedChain`/`InvalidSignature`
//! detail fields that the shared `Display` omits, and the 401 bodies built from
//! those strings are pinned here so consolidation does not change them.

use axum::http::HeaderMap;

pub use catalyrst_crypto::signed_fetch::AuthChainError;

/// The exact error text this service has always put in its 401 bodies.
///
/// The shared `AuthChainError` `Display` drops the detail fields; the local
/// fork's `Display` included them. Callers building responses must go through
/// this instead of `to_string()`.
pub fn wire_message(err: &AuthChainError) -> String {
    match err {
        AuthChainError::MalformedChain { detail } => format!("Invalid Auth Chain: {detail}"),
        AuthChainError::InvalidSignature(detail) => format!("Invalid signature: {detail}"),
        other => other.to_string(),
    }
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    expiration_secs: i64,
) -> Result<String, AuthChainError> {
    catalyrst_crypto::signed_fetch::verify_signed_fetch(headers, method, path, expiration_secs)
        .await
        .map(|signer| signer.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
    use catalyrst_crypto::signed_fetch::{
        build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    };

    const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
    const FIVE_MINUTES: i64 = 5 * 60;

    fn signed_headers(wallet: &Wallet, method: &str, path: &str, timestamp_ms: i64) -> HeaderMap {
        let payload = build_payload(method, path, &timestamp_ms.to_string(), "{}");
        let chain = create_simple_auth_chain(wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp_ms.to_string()).unwrap(),
        );
        headers.insert(AUTH_METADATA_HEADER, HeaderValue::from_static("{}"));
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

    #[tokio::test]
    async fn require_signer_recovers_the_wallet_address() {
        let wallet = Wallet::from_hex(TEST_KEY).unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let headers = signed_headers(&wallet, "post", "/v1/rentals-listings", now_ms);
        let signer = require_signer(&headers, "post", "/v1/rentals-listings", FIVE_MINUTES)
            .await
            .unwrap();
        assert_eq!(signer, wallet.address().to_lowercase());
    }

    #[tokio::test]
    async fn require_signer_rejects_a_rebound_path() {
        let wallet = Wallet::from_hex(TEST_KEY).unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let headers = signed_headers(&wallet, "post", "/v1/rentals-listings", now_ms);
        let err = require_signer(&headers, "post", "/v1/other", FIVE_MINUTES)
            .await
            .unwrap_err();
        assert!(matches!(err, AuthChainError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn require_signer_rejects_outside_the_symmetric_window() {
        let wallet = Wallet::from_hex(TEST_KEY).unwrap();
        for skew_ms in [-(FIVE_MINUTES + 60) * 1000, (FIVE_MINUTES + 60) * 1000] {
            let ts = chrono::Utc::now().timestamp_millis() + skew_ms;
            let headers = signed_headers(&wallet, "post", "/v1/rentals-listings", ts);
            let err = require_signer(&headers, "post", "/v1/rentals-listings", FIVE_MINUTES)
                .await
                .unwrap_err();
            assert!(matches!(err, AuthChainError::Expired { .. }));
        }
    }

    /// The 401 text emitted before consolidation, byte for byte.
    #[test]
    fn wire_message_preserves_the_pre_consolidation_text() {
        assert_eq!(
            wire_message(&AuthChainError::MalformedChain {
                detail: "unexpected token".into()
            }),
            "Invalid Auth Chain: unexpected token"
        );
        assert_eq!(
            wire_message(&AuthChainError::InvalidSignature("recovery failed".into())),
            "Invalid signature: recovery failed"
        );
        assert_eq!(
            wire_message(&AuthChainError::InsufficientLinks),
            "Invalid Auth Chain"
        );
        assert_eq!(
            wire_message(&AuthChainError::MissingTimestamp),
            "Missing timestamp"
        );
        assert_eq!(
            wire_message(&AuthChainError::Expired {
                signed_at: 0,
                now: 100,
                window_secs: 60
            }),
            "Expired signature"
        );
        assert_eq!(
            wire_message(&AuthChainError::EipNotImplemented),
            "EIP-1654 not implemented"
        );
    }
}
