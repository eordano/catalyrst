use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;
use catalyrst_types::AuthLinkType;

use crate::http::response::ApiError;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthChainError, AuthLink, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// Mirrors @dcl/crypto-middleware ≥5.1.0 (marketplace-server #388): the
/// signed-fetch `verify()` entrypoint rejects, with HTTP 400 and a message
/// prefixed `Invalid chain metadata: `, any request whose `x-identity-metadata`
/// JSON carries a `signer` or `intent` that is not canonical — i.e. differs from
/// its own `trim().to_lowercase()` (mixed case or surrounding whitespace). This
/// fires before any route-specific validator. A request with no `signer`/`intent`
/// (or non-JSON metadata) is unaffected.
///
/// Why it matters: the signed-fetch client lowercases the payload before signing
/// but delivers the metadata header with its original casing, so a mixed-case
/// `signer` produces a signature byte-identical to the canonical spelling's — a
/// scene-signed request (`Decentraland-Kernel-Scene`) could otherwise slip past a
/// case-sensitive service gate as if directly user-signed. Returns the full
/// route-facing 400 message on rejection.
pub fn check_canonical_metadata(metadata: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return Ok(());
    };
    for key in ["signer", "intent"] {
        if let Some(raw) = value.get(key).and_then(serde_json::Value::as_str) {
            if raw != raw.trim().to_lowercase() {
                // Upstream echoes the raw metadata back, truncated at 64 chars.
                let echo: String = metadata.chars().take(64).collect();
                return Err(format!("Invalid chain metadata: {echo}"));
            }
        }
    }
    Ok(())
}

/// Header-facing wrapper for [`check_canonical_metadata`]: reads
/// `x-identity-metadata` (defaulting to `{}`, like the signature path) and
/// surfaces a 400 `ApiError` on a non-canonical `signer`/`intent`.
pub fn require_canonical_metadata(headers: &HeaderMap) -> Result<(), ApiError> {
    let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    check_canonical_metadata(metadata).map_err(ApiError::bad_request)
}

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

pub async fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<Signer, AuthChainError> {
    signed_fetch::validate_signature(chain, payload, timestamp, expiration_secs, now)
        .await
        .map_err(normalize)
}

pub async fn verify_with_address(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
    expected_address: &str,
) -> Result<Signer, AuthChainError> {
    let recovered = validate_signature(chain, payload, timestamp, expiration_secs, now).await?;
    if recovered.as_str() != expected_address.to_lowercase() {
        return Err(AuthChainError::AddressMismatch {
            expected: expected_address.to_lowercase(),
            recovered: recovered.as_str().to_string(),
        });
    }
    Ok(recovered)
}

pub async fn require_signer(
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
    validate_signature(&chain, &payload, &ts, FIVE_MINUTES, now).await
}

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

pub async fn optional_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Option<String>, ApiError> {
    let first_link = format!("{AUTH_CHAIN_HEADER_PREFIX}0");
    if !headers.contains_key(first_link.as_str()) {
        return Ok(None);
    }
    require_canonical_metadata(headers)?;
    require_signer(headers, method, path)
        .await
        .map(|s| Some(s.as_str().to_string()))
        .map_err(auth_chain_error_to_api)
}

#[cfg(test)]
mod canonical_metadata_tests {
    use super::check_canonical_metadata;

    /// The rejection matrix documented by marketplace-server's
    /// `signed-fetch-authentication.spec.ts`: a mixed-case or whitespace-padded
    /// `signer`/`intent` is rejected before service authorization.
    #[test]
    fn rejects_non_canonical_signer_and_intent() {
        for meta in [
            r#"{"signer":"Dcl:Marketplace","intent":"dcl:marketplace:add-pick"}"#,
            r#"{"signer":" dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#,
            r#"{"signer":"dcl:marketplace","intent":"Dcl:Marketplace:Add-Pick"}"#,
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick "}"#,
            // The exploit this closes: a mixed-case kernel-scene signer.
            r#"{"origin":"https://play.decentraland.org","signer":"Decentraland-Kernel-Scene"}"#,
        ] {
            let err = check_canonical_metadata(meta).expect_err(meta);
            assert!(
                err.starts_with("Invalid chain metadata: "),
                "message must match upstream prefix, got: {err}"
            );
        }
    }

    #[test]
    fn accepts_canonical_and_absent_metadata() {
        // Canonical signer + intent.
        assert!(check_canonical_metadata(
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#
        )
        .is_ok());
        // The canonical kernel-scene spelling is not a canonicalization failure
        // (the scene-signer policy is a separate, route-level concern).
        assert!(check_canonical_metadata(r#"{"signer":"decentraland-kernel-scene"}"#).is_ok());
        // No signer at all is unaffected.
        assert!(check_canonical_metadata(r#"{"intent":"dcl:marketplace:remove-pick"}"#).is_ok());
        assert!(check_canonical_metadata("{}").is_ok());
        // Non-JSON metadata is not a canonicalization question here.
        assert!(check_canonical_metadata("not json").is_ok());
    }
}
