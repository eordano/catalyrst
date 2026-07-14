use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;
use catalyrst_types::AuthLinkType;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthChainError, AuthLink, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// Route-facing message per error, matching the upstream marketplace-server
/// wording (everything not explicitly special-cased is "Invalid Auth Chain").
pub trait AuthChainErrorExt {
    fn message(&self) -> String;
}

impl AuthChainErrorExt for AuthChainError {
    fn message(&self) -> String {
        match self {
            AuthChainError::AddressMismatch { .. } => "Forbidden: address mismatch".to_string(),
            AuthChainError::Expired { .. } => "Expired signature".to_string(),
            AuthChainError::EipNotImplemented => "EIP-1654 not supported on this route".to_string(),

            _ => "Invalid Auth Chain".to_string(),
        }
    }
}

/// market never surfaces ForbiddenSigner: it is folded into InvalidSignature,
/// preserving the pre-consolidation route behavior (401, not a 400 fallthrough).
fn normalize(e: AuthChainError) -> AuthChainError {
    match e {
        AuthChainError::ForbiddenSigner => AuthChainError::InvalidSignature(e.to_string()),
        other => other,
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
    let chain = signed_fetch::extract_auth_chain(headers).map_err(normalize)?;
    reject_eip_links(&chain)?;
    Ok(chain)
}

pub fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<Signer, AuthChainError> {
    signed_fetch::validate_signature(chain, payload, timestamp, expiration_secs, now)
        .map_err(normalize)
}

pub fn verify_with_address(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
    expected_address: &str,
) -> Result<Signer, AuthChainError> {
    let recovered = validate_signature(chain, payload, timestamp, expiration_secs, now)?;
    if recovered.as_str() != expected_address.to_lowercase() {
        return Err(AuthChainError::AddressMismatch {
            expected: expected_address.to_lowercase(),
            recovered: recovered.as_str().to_string(),
        });
    }
    Ok(recovered)
}

pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
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
