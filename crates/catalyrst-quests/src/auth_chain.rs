//! Signed DCL auth-chain verification for the quests dcl-rpc WebSocket
//! handshake. Mirrors upstream `decentraland/quests` crates/server/src/rpc/mod.rs
//! which authenticates each connection via
//! `authenticate_dcl_user_with_signed_headers("get", "/", ...)`: the client's
//! first WS frame carries the signed-fetch headers as a JSON object, and the
//! recovered signer becomes the per-connection user address.
//!
//! Identical contract/implementation to catalyrst-social-rpc's handshake — the
//! same EIP-712 ephemeral-key auth chain the explorer mints for any signed
//! WebSocket. REST routes carry the same headers in HTTP form (see
//! `require_signer`).

use axum::http::HeaderMap;
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_crypto::AuthError;
use catalyrst_types::{AuthLink as CryptoAuthLink, AuthLinkType as CryptoAuthLinkType, EthAddress};
use serde_json::Value;
use thiserror::Error;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";

pub const MAX_AUTH_CHAIN_LINKS: usize = 10;

pub const FIVE_MINUTES_SECS: i64 = 5 * 60;

#[derive(Debug, Clone)]
pub struct AuthLink {
    pub kind: CryptoAuthLinkType,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct AuthChain {
    pub links: Vec<AuthLink>,
    pub signer: EthAddress,
}

#[derive(Debug, Error)]
pub enum AuthChainError {
    #[error("invalid auth-chain envelope: not a JSON object")]
    EnvelopeNotObject,
    #[error("invalid auth-chain link {index}: {detail}")]
    MalformedChain { index: usize, detail: String },
    #[error("auth-chain shorter than 2 links")]
    InsufficientLinks,
    #[error("missing {0}")]
    MissingHeader(&'static str),
    #[error("signature older than {window_secs}s window: signed_at={signed_at} now={now}")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },
    #[error("signature did not verify: {0}")]
    InvalidSignature(String),
    #[error("EIP-1654 chains not implemented")]
    EipNotImplemented,
}

pub fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{}:{}:{}:{}", method, path, timestamp, metadata).to_lowercase()
}

/// Behind the front-host proxy, nginx strips the service prefix but the client
/// signs the full external path; it forwards the original in `x-original-path`.
/// Prefer it for the signed-fetch payload, falling back to the route path.
fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn obj_str<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_str())
}

pub fn extract_from_object(
    obj: &serde_json::Map<String, Value>,
) -> Result<AuthChain, AuthChainError> {
    let mut links = Vec::new();

    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, i);
        let Some(raw) = obj_str(obj, &name) else {
            break;
        };
        let link: CryptoAuthLink = serde_json::from_str(raw).map_err(|e| {
            let mut detail = e.to_string();
            if detail.len() > 64 {
                detail.truncate(64);
            }
            AuthChainError::MalformedChain { index: i, detail }
        })?;
        match link.link_type {
            CryptoAuthLinkType::SIGNER => {
                if i != 0 {
                    return Err(AuthChainError::MalformedChain {
                        index: i,
                        detail: "SIGNER link at non-zero index".into(),
                    });
                }
            }
            _ => {
                if i == 0 {
                    return Err(AuthChainError::MalformedChain {
                        index: 0,
                        detail: "first link must be SIGNER".into(),
                    });
                }
                if link.signature.as_deref().unwrap_or("").is_empty() {
                    return Err(AuthChainError::MalformedChain {
                        index: i,
                        detail: "missing signature".into(),
                    });
                }
            }
        }
        links.push(AuthLink {
            kind: link.link_type,
            payload: link.payload,
            signature: link.signature.unwrap_or_default(),
        });
    }

    let overflow = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, MAX_AUTH_CHAIN_LINKS);
    if obj_str(obj, &overflow).is_some() {
        return Err(AuthChainError::MalformedChain {
            index: MAX_AUTH_CHAIN_LINKS,
            detail: format!("exceeds max length {MAX_AUTH_CHAIN_LINKS}"),
        });
    }
    if links.len() < 2 {
        return Err(AuthChainError::InsufficientLinks);
    }
    let signer = links[0].payload.to_lowercase();
    Ok(AuthChain { links, signer })
}

pub fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<EthAddress, AuthChainError> {
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthChainError::Expired {
                signed_at,
                now,
                window_secs: expiration_secs,
            });
        }
    }

    let crypto_chain: Vec<CryptoAuthLink> = chain
        .links
        .iter()
        .map(|link| CryptoAuthLink {
            link_type: link.kind,
            payload: link.payload.clone(),
            signature: if link.signature.is_empty() {
                None
            } else {
                Some(link.signature.clone())
            },
        })
        .collect();

    verify_auth_chain(&crypto_chain, payload, Some(now * 1000)).map_err(map_auth_error)?;
    Ok(chain.signer.clone())
}

fn map_auth_error(err: AuthError) -> AuthChainError {
    match err {
        AuthError::MalformedChain(d) => AuthChainError::MalformedChain {
            index: 0,
            detail: d,
        },
        AuthError::MissingSignature { .. } => AuthChainError::MalformedChain {
            index: 0,
            detail: err.to_string(),
        },
        AuthError::RecoveryFailed(d) => AuthChainError::InvalidSignature(d),
        AuthError::SignerMismatch { .. } | AuthError::FinalAuthorityMismatch { .. } => {
            AuthChainError::InvalidSignature(err.to_string())
        }
        AuthError::EphemeralExpired {
            expiration_ms,
            now_ms,
        } => AuthChainError::Expired {
            signed_at: expiration_ms / 1000,
            now: now_ms / 1000,
            window_secs: 0,
        },
        AuthError::InvalidEphemeralPayload(d) => AuthChainError::MalformedChain {
            index: 0,
            detail: d,
        },
        AuthError::Eip1654NotImplemented
        | AuthError::Eip1654ValidationFailed(_)
        | AuthError::Eip1654Rejected { .. } => AuthChainError::EipNotImplemented,
    }
}

/// Verify the WS handshake frame (JSON object of signed-fetch headers) and
/// return the recovered signer (lowercased). Mirrors upstream's
/// `authenticate_dcl_user_with_signed_headers("get", "/", ...)`.
pub fn verify_handshake(
    frame_json: &str,
    method: &str,
    path: &str,
    expiration_secs: i64,
    now_secs: i64,
) -> Result<EthAddress, AuthChainError> {
    let value: Value =
        serde_json::from_str(frame_json).map_err(|e| AuthChainError::MalformedChain {
            index: 0,
            detail: format!("frame not JSON: {e}"),
        })?;
    let obj = value.as_object().ok_or(AuthChainError::EnvelopeNotObject)?;

    let chain = extract_from_object(obj)?;
    let timestamp = obj_str(obj, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingHeader(AUTH_TIMESTAMP_HEADER))?;
    let metadata = obj_str(obj, AUTH_METADATA_HEADER).unwrap_or("{}");
    let payload = build_payload(method, path, timestamp, metadata);
    validate_signature(&chain, &payload, timestamp, expiration_secs, now_secs)
}

/// Recover the signer of an HTTP signed-fetch request from its headers
/// (lowercased). Used by REST routes that gate on quest creator identity
/// (GET /quests/{id} definition, /quests/{id}/instances, instance state, ...).
pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, AuthChainError> {
    let path = signed_fetch_path(headers, path);
    let path = path.as_ref();
    let mut value = serde_json::Map::new();
    for (name, val) in headers.iter() {
        if let Ok(s) = val.to_str() {
            value.insert(name.as_str().to_string(), Value::String(s.to_string()));
        }
    }
    let chain = extract_from_object(&value)?;
    let timestamp = obj_str(&value, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingHeader(AUTH_TIMESTAMP_HEADER))?
        .to_string();
    let metadata = obj_str(&value, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();
    let payload = build_payload(method, path, &timestamp, &metadata);
    let now = chrono::Utc::now().timestamp();
    validate_signature(&chain, &payload, &timestamp, FIVE_MINUTES_SECS, now)
}

/// Optional signed-fetch: returns the signer if present and valid, else None.
/// Mirrors upstream's `OptionalAuthUser` extractor used by the read routes
/// (e.g. GET /quests/{id}, GET /creators/{addr}/quests) which return the
/// decoded definition only when the authed signer is the creator.
pub fn optional_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    // Only attempt verification when a chain is actually present.
    if !headers.keys().any(|k| {
        k.as_str()
            .eq_ignore_ascii_case(&format!("{}0", AUTH_CHAIN_HEADER_PREFIX))
    }) {
        return None;
    }
    require_signer(headers, method, path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers_signers::{LocalWallet, Signer};

    async fn make_chain(method: &str, path: &str, ts_ms: i64) -> (String, String) {
        let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
        let root_address = format!("{:#x}", root.address());

        let ephemeral: LocalWallet =
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
                "signature": format!("0x{}", ephemeral_sig)
            }).to_string(),
            "x-identity-auth-chain-2": serde_json::json!({
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": payload,
                "signature": format!("0x{}", entity_sig)
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
            .expect("valid chain must verify");
        assert_eq!(signer, expected_signer.to_lowercase());
    }

    #[tokio::test]
    async fn verify_handshake_rejects_expired() {
        let signed_secs = 1_700_000_000;
        let now_secs = signed_secs + 10 * 60;
        let (_, frame) = make_chain("get", "/", signed_secs * 1000).await;
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .expect_err("expired chain must be rejected");
        assert!(matches!(err, AuthChainError::Expired { .. }), "{err:?}");
    }

    #[test]
    fn verify_handshake_rejects_malformed_envelope() {
        let err = verify_handshake("not json", "get", "/", FIVE_MINUTES_SECS, 0).unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
        let err2 = verify_handshake("[]", "get", "/", FIVE_MINUTES_SECS, 0).unwrap_err();
        assert!(
            matches!(err2, AuthChainError::EnvelopeNotObject),
            "{err2:?}"
        );
    }
}
