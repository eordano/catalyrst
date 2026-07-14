use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChainError, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

#[derive(Debug, Clone)]
pub struct VerifiedAuth {
    pub signer: Signer,
    pub metadata: serde_json::Value,
}

pub async fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedAuth, AuthChainError> {
    let (signer, metadata) =
        signed_fetch::verify_signed_fetch_meta(headers, method, path, FIVE_MINUTES).await?;

    if metadata
        .get("signer")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case(KERNEL_SCENE_SIGNER))
        .unwrap_or(false)
    {
        return Err(AuthChainError::ForbiddenSigner);
    }

    Ok(VerifiedAuth { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};

    const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

    fn test_wallet() -> Wallet {
        Wallet::from_hex(TEST_KEY).unwrap()
    }

    fn signed_headers(wallet: &Wallet, method: &str, path: &str, timestamp_ms: i64) -> HeaderMap {
        let metadata = "{}";
        let payload = build_payload(method, path, &timestamp_ms.to_string(), metadata);
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
    async fn fresh_signed_fetch_verifies_and_recovers_signer() {
        let wallet = test_wallet();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let headers = signed_headers(&wallet, "delete", "/content/scenes/52,-52", now_ms);
        let auth = require_verified(&headers, "delete", "/content/scenes/52,-52")
            .await
            .unwrap();
        assert_eq!(auth.signer, wallet.address().to_lowercase());
    }

    #[tokio::test]
    async fn stale_signature_is_rejected() {
        let wallet = test_wallet();
        let old_ms = chrono::Utc::now().timestamp_millis() - (FIVE_MINUTES + 60) * 1000;
        let headers = signed_headers(&wallet, "delete", "/content/scenes/52,-52", old_ms);
        let err = require_verified(&headers, "delete", "/content/scenes/52,-52")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    #[tokio::test]
    async fn wrong_path_signature_is_rejected() {
        let wallet = test_wallet();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let headers = signed_headers(&wallet, "delete", "/content/scenes/0,0", now_ms);
        let err = require_verified(&headers, "delete", "/content/scenes/52,-52")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthChainError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn missing_chain_is_rejected() {
        let headers = HeaderMap::new();
        let err = require_verified(&headers, "delete", "/content/scenes/52,-52")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthChainError::InsufficientLinks));
    }
}
