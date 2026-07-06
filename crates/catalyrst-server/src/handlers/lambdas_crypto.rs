use std::sync::{Arc, OnceLock};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use serde::Deserialize;
use serde_json::json;

use catalyrst_crypto::verify::verify_auth_chain_async;
use catalyrst_crypto::{AuthLink, Eip1654Validator, RpcEip1654Validator, ValidationCache};

const MAX_AUTH_CHAIN_LENGTH: usize = 10;

fn eth_rpc_url() -> String {
    std::env::var("RPC_ENDPOINT_ETH")
        .unwrap_or_else(|_| "https://rpc.decentraland.org/mainnet".to_string())
}

fn crypto_validator() -> &'static Arc<dyn Eip1654Validator> {
    static V: OnceLock<Arc<dyn Eip1654Validator>> = OnceLock::new();
    V.get_or_init(|| {
        let rpc = RpcEip1654Validator::new(eth_rpc_url());
        Arc::new(ValidationCache::new(Arc::new(rpc))) as Arc<dyn Eip1654Validator>
    })
}

#[derive(Debug, Deserialize)]
struct ValidateSignatureBody {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default, rename = "signedMessage")]
    signed_message: Option<String>,
    #[serde(default, rename = "authChain")]
    auth_chain: Option<Vec<AuthLink>>,
}

#[derive(Debug, PartialEq)]
struct PreparedValidation {
    chain: Vec<AuthLink>,
    final_authority: String,
    owner: String,
}

fn prepare_validation(
    auth_chain: Option<Vec<AuthLink>>,
    signed_message: Option<String>,
    timestamp: Option<String>,
) -> Result<PreparedValidation, String> {
    let chain = auth_chain.ok_or_else(|| "Invalid request. Missing 'authChain'".to_string())?;

    if chain.is_empty() || chain.len() > MAX_AUTH_CHAIN_LENGTH {
        return Err(format!(
            "'authChain' length must be between 1 and {MAX_AUTH_CHAIN_LENGTH}"
        ));
    }

    let final_authority = signed_message.or(timestamp).unwrap_or_default();
    if final_authority.is_empty() {
        return Err("Expected 'signedMessage' property to be set".to_string());
    }

    let owner = chain.first().map(|l| l.payload.clone()).unwrap_or_default();

    Ok(PreparedValidation {
        chain,
        final_authority,
        owner,
    })
}

pub async fn validate_signature(body: Bytes) -> Response {
    let parsed: ValidateSignatureBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return crate::errors::bad_request(&format!("Invalid JSON body: {e}")),
    };

    let prepared =
        match prepare_validation(parsed.auth_chain, parsed.signed_message, parsed.timestamp) {
            Ok(p) => p,
            Err(msg) => return crate::errors::bad_request(&msg),
        };

    let validator: &dyn Eip1654Validator = &**crypto_validator();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let result = verify_auth_chain_async(
        &prepared.chain,
        &prepared.final_authority,
        Some(now_ms),
        Some(validator),
    )
    .await;

    let body = match result {
        Ok(()) => json!({ "valid": true, "ownerAddress": prepared.owner }),
        Err(e) => json!({ "valid": false, "error": e.to_string() }),
    };

    (StatusCode::OK, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_crypto::AuthLinkType;

    fn signer_link(addr: &str) -> AuthLink {
        AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: addr.to_string(),
            signature: None,
        }
    }

    #[test]
    fn prepare_rejects_missing_auth_chain() {
        let err = prepare_validation(None, Some("msg".into()), None).unwrap_err();
        assert_eq!(err, "Invalid request. Missing 'authChain'");
    }

    #[test]
    fn prepare_rejects_empty_chain() {
        let err = prepare_validation(Some(vec![]), Some("msg".into()), None).unwrap_err();
        assert_eq!(err, "'authChain' length must be between 1 and 10");
    }

    #[test]
    fn prepare_rejects_chain_over_max_length() {
        let chain: Vec<AuthLink> = (0..=MAX_AUTH_CHAIN_LENGTH)
            .map(|i| signer_link(&format!("0x{i}")))
            .collect();
        assert_eq!(chain.len(), MAX_AUTH_CHAIN_LENGTH + 1);
        let err = prepare_validation(Some(chain), Some("msg".into()), None).unwrap_err();
        assert_eq!(err, "'authChain' length must be between 1 and 10");
    }

    #[test]
    fn prepare_accepts_chain_at_max_length() {
        let chain: Vec<AuthLink> = (0..MAX_AUTH_CHAIN_LENGTH)
            .map(|i| signer_link(&format!("0x{i}")))
            .collect();
        assert!(prepare_validation(Some(chain), Some("msg".into()), None).is_ok());
    }

    #[test]
    fn prepare_rejects_missing_final_authority() {
        let err = prepare_validation(Some(vec![signer_link("0xabc")]), None, None).unwrap_err();
        assert_eq!(err, "Expected 'signedMessage' property to be set");
    }

    #[test]
    fn prepare_rejects_empty_final_authority() {
        let err = prepare_validation(Some(vec![signer_link("0xabc")]), Some(String::new()), None)
            .unwrap_err();
        assert_eq!(err, "Expected 'signedMessage' property to be set");
    }

    #[test]
    fn prepare_prefers_signed_message_over_timestamp() {
        let p = prepare_validation(
            Some(vec![signer_link("0xabc")]),
            Some("the-message".into()),
            Some("12345".into()),
        )
        .unwrap();
        assert_eq!(p.final_authority, "the-message");
        assert_eq!(p.owner, "0xabc");
    }

    #[test]
    fn prepare_falls_back_to_timestamp() {
        let p = prepare_validation(Some(vec![signer_link("0xABC")]), None, Some("12345".into()))
            .unwrap();
        assert_eq!(p.final_authority, "12345");
        assert_eq!(p.owner, "0xABC");
    }

    #[tokio::test]
    async fn validate_signature_owner_is_signer_for_simple_chain() {
        let addr = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
        let body = json!({
            "signedMessage": addr,
            "authChain": [ { "type": "SIGNER", "payload": addr } ],
        })
        .to_string();

        let resp = validate_signature(Bytes::from(body)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
