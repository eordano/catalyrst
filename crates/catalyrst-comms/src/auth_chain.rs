use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, header_str, signed_fetch_path, try_extract,
    validate_signature, AuthChain, AuthChainError, AuthLink, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;
pub const DEFAULT_EXPIRATION_SECS: i64 = 60;

#[derive(Debug)]
pub struct SignedFetchError {
    pub status: u16,
    pub message: String,
}

impl SignedFetchError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

pub struct SignedFetch {
    pub signer: String,
    pub metadata: serde_json::Value,
}

pub fn verify_signed_fetch(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
) -> Result<SignedFetch, SignedFetchError> {
    let path = signed_fetch_path(headers, path);
    let path = path.as_ref();
    let chain = extract_auth_chain(headers).map_err(map_chain_error)?;

    let raw_metadata = header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata: serde_json::Value = serde_json::from_str(raw_metadata).map_err(|_| {
        SignedFetchError::new(400, format!("Invalid chain metadata: \"{raw_metadata}\""))
    })?;

    if !allowed_signers.is_empty() {
        let signer_field = metadata
            .get("signer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !allowed_signers.contains(&signer_field) {
            return Err(SignedFetchError::new(
                400,
                format!("Invalid metadata content: {raw_metadata}"),
            ));
        }
    }

    let ts = header_str(headers, AUTH_TIMESTAMP_HEADER)
        .unwrap_or("0")
        .to_string();
    let payload = build_payload(method, path, &ts, raw_metadata);
    let now = chrono::Utc::now().timestamp();

    let signer =
        validate_signature(&chain, &payload, &ts, DEFAULT_EXPIRATION_SECS, now).map_err(|e| {
            tracing::warn!(error = ?e, %payload, signer = %chain.signer, "signed-fetch rejected");
            match e {
                AuthChainError::Expired { .. } => SignedFetchError::new(401, "Expired signature"),
                AuthChainError::InvalidSignature(d) => {
                    SignedFetchError::new(401, format!("Invalid signature: {d}"))
                }
                AuthChainError::EipNotImplemented => {
                    SignedFetchError::new(503, "EIP-1654 validation unavailable")
                }
                other => SignedFetchError::new(400, other.to_string()),
            }
        })?;

    Ok(SignedFetch { signer, metadata })
}

fn map_chain_error(e: AuthChainError) -> SignedFetchError {
    match e {
        AuthChainError::MalformedChain { detail } => {
            SignedFetchError::new(400, format!("Invalid chain format: {detail}"))
        }
        AuthChainError::InsufficientLinks => SignedFetchError::new(400, "Invalid Auth Chain"),
        other => SignedFetchError::new(400, other.to_string()),
    }
}

pub fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES)
}

pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, AuthChainError> {
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES)
}
