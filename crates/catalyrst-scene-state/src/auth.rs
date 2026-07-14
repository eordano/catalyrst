use std::collections::HashMap;

use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::{AuthLink, AuthLinkType};
use thiserror::Error;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, signed_fetch_path, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("auth frame is not valid JSON: {0}")]
    BadJson(String),
    #[error("auth chain malformed: {0}")]
    MalformedChain(String),
    #[error("auth chain has fewer than 2 links")]
    InsufficientLinks,
    #[error("signature expired (signed_at={signed_at}, now={now}, window={window_secs}s)")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },
    #[error("signature rejected: {0}")]
    InvalidSignature(String),
}

#[derive(Debug, Clone)]
pub struct Authenticated {
    pub signer: String,
}

pub fn verify_auth_frame(
    frame_json: &[u8],
    method: &str,
    path: &str,
    now: i64,
) -> Result<Authenticated, AuthError> {
    let headers: HashMap<String, String> =
        serde_json::from_slice(frame_json).map_err(|e| AuthError::BadJson(e.to_string()))?;
    let headers: HashMap<String, String> = headers
        .into_iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect();

    let chain = extract_auth_chain(&headers)?;
    let signer = chain
        .first()
        .map(|l| l.payload.to_ascii_lowercase())
        .ok_or(AuthError::InsufficientLinks)?;

    let ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .cloned()
        .unwrap_or_else(|| "0".into());
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .cloned()
        .unwrap_or_else(|| "{}".into());

    validate_signature_either_payload(&chain, method, path, &ts, &metadata, FIVE_MINUTES, now)?;

    Ok(Authenticated { signer })
}

fn validate_signature_either_payload(
    chain: &[AuthLink],
    method: &str,
    path: &str,
    timestamp: &str,
    metadata: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthError> {
    let v6 = build_payload_v6(method, path, timestamp, metadata);
    let legacy = build_legacy_payload(method, path, timestamp, metadata);
    match validate_signature(chain, &v6, timestamp, expiration_secs, now) {
        Err(AuthError::InvalidSignature(_)) if legacy != v6 => {
            validate_signature(chain, &legacy, timestamp, expiration_secs, now)
        }
        settled => settled,
    }
}

fn extract_auth_chain(headers: &HashMap<String, String>) -> Result<Vec<AuthLink>, AuthError> {
    let mut links: Vec<AuthLink> = Vec::new();
    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{AUTH_CHAIN_HEADER_PREFIX}{i}");
        let Some(raw) = headers.get(&name) else { break };
        let link: AuthLink =
            serde_json::from_str(raw).map_err(|e| AuthError::MalformedChain(e.to_string()))?;
        if link.link_type == AuthLinkType::SIGNER {
            if i != 0 {
                return Err(AuthError::MalformedChain(format!(
                    "SIGNER link at non-zero index {i}"
                )));
            }
        } else {
            if i == 0 {
                return Err(AuthError::MalformedChain(
                    "first link must be SIGNER".into(),
                ));
            }
            if link.signature.as_deref().unwrap_or("").is_empty() {
                return Err(AuthError::MalformedChain(format!(
                    "missing signature on link {i}"
                )));
            }
        }
        links.push(link);
    }
    let overflow = format!("{AUTH_CHAIN_HEADER_PREFIX}{MAX_AUTH_CHAIN_LINKS}");
    if headers.contains_key(&overflow) {
        return Err(AuthError::MalformedChain(format!(
            "exceeds max length of {MAX_AUTH_CHAIN_LINKS}"
        )));
    }
    if links.len() < 2 {
        return Err(AuthError::InsufficientLinks);
    }
    Ok(links)
}

fn validate_signature(
    chain: &[AuthLink],
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthError> {
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthError::Expired {
                signed_at,
                now,
                window_secs: expiration_secs,
            });
        }
    }
    verify_auth_chain(&chain.to_vec(), payload, Some(now * 1000))
        .map_err(|e| AuthError::InvalidSignature(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_json() {
        let err = verify_auth_frame(b"not json", "GET", "/ws/x", 0).unwrap_err();
        assert!(
            matches!(err, AuthError::BadJson(_)),
            "expected BadJson, got {err:?}"
        );
    }

    #[test]
    fn rejects_chain_with_one_link() {
        let body = serde_json::json!({
            "x-identity-auth-chain-0":
                "{\"type\":\"SIGNER\",\"payload\":\"0xabc\",\"signature\":\"\"}"
        })
        .to_string();
        let err = verify_auth_frame(body.as_bytes(), "GET", "/ws/x", 0).unwrap_err();
        assert!(
            matches!(err, AuthError::InsufficientLinks),
            "expected InsufficientLinks, got {err:?}"
        );
    }

    #[test]
    fn rejects_signer_link_at_non_zero_index() {
        let body = serde_json::json!({
            "x-identity-auth-chain-0":
                "{\"type\":\"ECDSA_EPHEMERAL\",\"payload\":\"0xabc\",\"signature\":\"0xsig\"}",
            "x-identity-auth-chain-1":
                "{\"type\":\"SIGNER\",\"payload\":\"0xabc\",\"signature\":\"\"}"
        })
        .to_string();
        let err = verify_auth_frame(body.as_bytes(), "GET", "/ws/x", 0).unwrap_err();
        assert!(
            matches!(err, AuthError::MalformedChain(_)),
            "expected MalformedChain, got {err:?}"
        );
    }

    #[test]
    fn rejects_non_signer_first_link() {
        let body = serde_json::json!({
            "x-identity-auth-chain-0":
                "{\"type\":\"ECDSA_EPHEMERAL\",\"payload\":\"0xabc\",\"signature\":\"0xsig\"}",
            "x-identity-auth-chain-1":
                "{\"type\":\"ECDSA_SIGNED_ENTITY\",\"payload\":\"payload\",\"signature\":\"0xsig\"}"
        })
        .to_string();
        let err = verify_auth_frame(body.as_bytes(), "GET", "/ws/x", 0).unwrap_err();
        assert!(
            matches!(err, AuthError::MalformedChain(_)),
            "expected MalformedChain, got {err:?}"
        );
    }

    #[test]
    fn rejects_missing_signature_on_non_signer_link() {
        let body = serde_json::json!({
            "x-identity-auth-chain-0":
                "{\"type\":\"SIGNER\",\"payload\":\"0xabc\",\"signature\":\"\"}",
            "x-identity-auth-chain-1":
                "{\"type\":\"ECDSA_SIGNED_ENTITY\",\"payload\":\"payload\"}"
        })
        .to_string();
        let err = verify_auth_frame(body.as_bytes(), "GET", "/ws/x", 0).unwrap_err();
        assert!(
            matches!(err, AuthError::MalformedChain(_)),
            "expected MalformedChain, got {err:?}"
        );
    }

    #[test]
    fn payload_is_lowercased() {
        assert_eq!(
            build_payload("GET", "/ws/MyWorld.dcl.eth", "123", "{}"),
            "get:/ws/myworld.dcl.eth:123:{}"
        );
    }
}
