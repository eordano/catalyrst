use axum::http::HeaderMap;
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_crypto::AuthError;
use catalyrst_types::{AuthLink as CryptoAuthLink, AuthLinkType as CryptoAuthLinkType, EthAddress};
use thiserror::Error;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";

pub const MAX_AUTH_CHAIN_LINKS: usize = 10;
pub const FIVE_MINUTES: i64 = 5 * 60;

pub const KERNEL_SCENE_SIGNER: &str = "decentraland-kernel-scene";

#[derive(Debug, Clone)]
pub struct AuthLink {
    pub kind: CryptoAuthLinkType,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct VerifiedAuth {
    pub signer: EthAddress,
    pub metadata: serde_json::Value,
}

impl VerifiedAuth {
    pub fn secret(&self) -> Option<String> {
        self.metadata
            .get("secret")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

#[derive(Debug, Error)]
pub enum AuthChainError {
    #[error("Invalid Auth Chain")]
    MalformedChain { detail: String },
    #[error("Invalid Auth Chain")]
    InsufficientLinks,
    #[error("Missing timestamp")]
    MissingTimestamp,
    #[error("Expired signature")]
    Expired,
    #[error("Invalid signature")]
    InvalidSignature(String),
    #[error("Access denied, invalid signer")]
    ForbiddenSigner,
    #[error("EIP-1654 not implemented")]
    EipNotImplemented,
}

pub fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{}:{}:{}:{}", method, path, timestamp, metadata).to_lowercase()
}

fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn extract_auth_chain(headers: &HeaderMap) -> Result<(Vec<AuthLink>, EthAddress), AuthChainError> {
    let mut links = Vec::new();

    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, i);
        let Some(raw) = header_str(headers, &name) else {
            break;
        };

        let link: CryptoAuthLink = serde_json::from_str(raw).map_err(|e| {
            let mut detail = e.to_string();
            if detail.len() > 64 {
                detail.truncate(64);
            }
            AuthChainError::MalformedChain { detail }
        })?;

        match link.link_type {
            CryptoAuthLinkType::SIGNER => {
                if i != 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("SIGNER link at non-zero index {}", i),
                    });
                }
            }
            _ => {
                if i == 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: "first link must be SIGNER".to_string(),
                    });
                }
                if link.signature.as_deref().unwrap_or("").is_empty() {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("missing signature on link {}", i),
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
    if header_str(headers, &overflow).is_some() {
        return Err(AuthChainError::MalformedChain {
            detail: format!("exceeds max length of {}", MAX_AUTH_CHAIN_LINKS),
        });
    }
    if links.len() < 2 {
        return Err(AuthChainError::InsufficientLinks);
    }
    let signer = links[0].payload.to_lowercase();
    Ok((links, signer))
}

fn validate_signature(
    links: &[AuthLink],
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthChainError> {
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthChainError::Expired);
        }
    }

    let crypto_chain: Vec<CryptoAuthLink> = links
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
    Ok(())
}

fn map_auth_error(err: AuthError) -> AuthChainError {
    match err {
        AuthError::MalformedChain(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::MissingSignature { .. } => AuthChainError::MalformedChain {
            detail: err.to_string(),
        },
        AuthError::RecoveryFailed(d) => AuthChainError::InvalidSignature(d),
        AuthError::SignerMismatch { .. } | AuthError::FinalAuthorityMismatch { .. } => {
            AuthChainError::InvalidSignature(err.to_string())
        }
        AuthError::EphemeralExpired { .. } => AuthChainError::Expired,
        AuthError::InvalidEphemeralPayload(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::Eip1654NotImplemented
        | AuthError::Eip1654ValidationFailed(_)
        | AuthError::Eip1654Rejected { .. } => AuthChainError::EipNotImplemented,
    }
}

pub fn require_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<VerifiedAuth, AuthChainError> {
    let path = signed_fetch_path(headers, path);
    let path = path.as_ref();
    let (links, signer) = extract_auth_chain(headers)?;
    let ts = header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    let metadata_raw = header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();
    let payload = build_payload(method, path, &ts, &metadata_raw);
    let now = chrono::Utc::now().timestamp();
    validate_signature(&links, &payload, &ts, FIVE_MINUTES, now)?;

    let metadata: serde_json::Value =
        serde_json::from_str(&metadata_raw).unwrap_or(serde_json::Value::Null);

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
