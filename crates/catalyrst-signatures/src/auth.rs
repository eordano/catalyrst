//! Auth-chain extraction + verification, mirroring catalyrst-camera-reel's
//! auth_chain.rs. Ports decentraland-crypto-middleware.wellKnownComponents:
//! the POST /v1/rentals-listings route requires a valid auth-chain whose
//! recovered signer becomes the lessor.

use axum::http::HeaderMap;
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_crypto::AuthError;
use catalyrst_types::{AuthLink as CryptoAuthLink, AuthLinkType as CryptoAuthLinkType, EthAddress};
use thiserror::Error;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";
pub const MAX_AUTH_CHAIN_LINKS: usize = 10;

#[derive(Debug, Clone)]
struct AuthLink {
    kind: CryptoAuthLinkType,
    payload: String,
    signature: String,
}

#[derive(Debug, Clone)]
struct AuthChain {
    links: Vec<AuthLink>,
    signer: EthAddress,
}

#[derive(Debug, Error)]
pub enum AuthChainError {
    #[error("Invalid Auth Chain: {detail}")]
    MalformedChain { detail: String },
    #[error("Invalid Auth Chain")]
    InsufficientLinks,
    #[error("Missing timestamp")]
    MissingTimestamp,
    #[error("Expired signature")]
    Expired,
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("EIP-1654 not implemented")]
    EipNotImplemented,
}

fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{}:{}:{}:{}", method, path, timestamp, metadata).to_lowercase()
}

/// Behind the front-host proxy, nginx strips the service prefix before
/// proxying but the client signs the full external path (incl. prefix). nginx
/// forwards the original path in `x-original-path`; prefer it for signed-fetch
/// payload reconstruction so it matches what the client signed. Falls back to the
/// hardcoded route path for direct/loopback requests (no header).
fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    let mut links = Vec::new();
    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, i);
        let Some(raw) = header_str(headers, &name) else {
            break;
        };
        let link: CryptoAuthLink = serde_json::from_str(raw).map_err(|e| {
            let mut detail = e.to_string();
            detail.truncate(detail.len().min(64));
            AuthChainError::MalformedChain { detail }
        })?;
        links.push(AuthLink {
            kind: link.link_type,
            payload: link.payload,
            signature: link.signature.unwrap_or_default(),
        });
    }
    if links.len() < 2 {
        return Err(AuthChainError::InsufficientLinks);
    }
    let signer = links[0].payload.to_lowercase();
    Ok(AuthChain { links, signer })
}

fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<EthAddress, AuthChainError> {
    // Freshness window: read the timestamp from the authoritative
    // `x-identity-timestamp` header value (`timestamp`), NOT by splitting the
    // payload on ':'. Paths containing ':' (URNs) would shift the field and
    // silently skip the freshness check.
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthChainError::Expired);
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
        AuthError::MalformedChain(d) | AuthError::InvalidEphemeralPayload(d) => {
            AuthChainError::MalformedChain { detail: d }
        }
        AuthError::MissingSignature { .. } => AuthChainError::MalformedChain {
            detail: "missing signature".to_string(),
        },
        AuthError::RecoveryFailed(d) => AuthChainError::InvalidSignature(d),
        AuthError::SignerMismatch { .. } | AuthError::FinalAuthorityMismatch { .. } => {
            AuthChainError::InvalidSignature("signer mismatch".to_string())
        }
        AuthError::EphemeralExpired { .. } => AuthChainError::Expired,
        AuthError::Eip1654NotImplemented
        | AuthError::Eip1654ValidationFailed(_)
        | AuthError::Eip1654Rejected { .. } => AuthChainError::EipNotImplemented,
    }
}

/// Verify the auth-chain over `<method>:<path>:<ts>:<metadata>` and return the
/// lowercased signer address.
pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    expiration_secs: i64,
) -> Result<String, AuthChainError> {
    let path = signed_fetch_path(headers, path);
    let path = path.as_ref();
    let chain = extract_auth_chain(headers)?;
    let ts = header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    let metadata = header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();
    let payload = build_payload(method, path, &ts, &metadata);
    let now = chrono::Utc::now().timestamp();
    validate_signature(&chain, &payload, &ts, expiration_secs, now)
}
