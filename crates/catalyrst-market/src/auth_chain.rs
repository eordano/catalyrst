use axum::http::HeaderMap;
use thiserror::Error;

use catalyrst_crypto::signed_fetch;
use catalyrst_types::{AuthLinkType, EthAddress};

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

#[derive(Debug, Error)]
pub enum AuthChainError {
    #[error("Invalid Auth Chain")]
    MalformedChain { detail: String },

    #[error("Invalid Auth Chain")]
    InsufficientLinks,

    #[error("Invalid Auth Chain")]
    MissingTimestamp,

    #[error("Expired signature")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },

    #[error("Invalid signature")]
    InvalidSignature(String),

    #[error("Forbidden: address mismatch")]
    AddressMismatch { expected: String, recovered: String },

    #[error("EIP-1654 not implemented")]
    EipNotImplemented,
}

impl AuthChainError {
    pub fn message(&self) -> String {
        match self {
            AuthChainError::AddressMismatch { .. } => "Forbidden: address mismatch".to_string(),
            AuthChainError::Expired { .. } => "Expired signature".to_string(),
            AuthChainError::EipNotImplemented => "EIP-1654 not supported on this route".to_string(),

            _ => "Invalid Auth Chain".to_string(),
        }
    }
}

impl From<signed_fetch::AuthChainError> for AuthChainError {
    fn from(e: signed_fetch::AuthChainError) -> Self {
        match e {
            signed_fetch::AuthChainError::MalformedChain { detail } => {
                AuthChainError::MalformedChain { detail }
            }
            signed_fetch::AuthChainError::InsufficientLinks => AuthChainError::InsufficientLinks,
            signed_fetch::AuthChainError::MissingTimestamp => AuthChainError::MissingTimestamp,
            signed_fetch::AuthChainError::Expired {
                signed_at,
                now,
                window_secs,
            } => AuthChainError::Expired {
                signed_at,
                now,
                window_secs,
            },
            signed_fetch::AuthChainError::InvalidSignature(d) => {
                AuthChainError::InvalidSignature(d)
            }
            signed_fetch::AuthChainError::ForbiddenSigner => {
                AuthChainError::InvalidSignature(e.to_string())
            }
            signed_fetch::AuthChainError::EipNotImplemented => AuthChainError::EipNotImplemented,
        }
    }
}

fn reject_eip_links(chain: &AuthChain) -> Result<(), AuthChainError> {
    for link in &chain.links {
        if matches!(
            link.kind,
            AuthLinkType::EcdsaEip1654Ephemeral | AuthLinkType::EcdsaEip1654SignedEntity
        ) {
            return Err(AuthChainError::EipNotImplemented);
        }
    }
    Ok(())
}

pub fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    let chain = signed_fetch::extract_auth_chain(headers)?;
    reject_eip_links(&chain)?;
    Ok(chain)
}

pub fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<EthAddress, AuthChainError> {
    Ok(signed_fetch::validate_signature(
        chain,
        payload,
        timestamp,
        expiration_secs,
        now,
    )?)
}

pub fn verify_with_address(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
    expected_address: &str,
) -> Result<EthAddress, AuthChainError> {
    let recovered = validate_signature(chain, payload, timestamp, expiration_secs, now)?;
    if recovered.to_lowercase() != expected_address.to_lowercase() {
        return Err(AuthChainError::AddressMismatch {
            expected: expected_address.to_lowercase(),
            recovered,
        });
    }
    Ok(recovered)
}

pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<EthAddress, AuthChainError> {
    let path = signed_fetch::signed_fetch_path(headers, path);
    let path = path.as_ref();
    let chain = extract_auth_chain(headers)?;
    let ts = signed_fetch::header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();
    let payload = build_payload(method, path, &ts, &metadata);
    let now = chrono::Utc::now().timestamp();
    validate_signature(&chain, &payload, &ts, FIVE_MINUTES, now)
}
