use axum::http::HeaderMap;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";

pub fn auth_address_optional(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(format!("{AUTH_CHAIN_HEADER_PREFIX}0"))
        .and_then(|v| v.to_str().ok())?;
    let link: serde_json::Value = serde_json::from_str(raw).ok()?;
    let addr = link.get("payload").and_then(|p| p.as_str())?;
    if addr.starts_with("0x") && addr.len() == 42 {
        Some(addr.to_lowercase())
    } else {
        None
    }
}

pub fn auth_address_required(headers: &HeaderMap) -> Result<String, crate::http::errors::ApiError> {
    auth_address_optional(headers)
        .ok_or_else(|| crate::http::errors::ApiError::unauthorized("Invalid authentication"))
}

/// Authoritative auth for write paths: verify the full signed-fetch auth chain
/// (signature + ephemeral + timestamp) over `method:path:ts:metadata` and return
/// the *verified* lowercased signer. Upstream places gates favorites/likes/report
/// behind `withAuth` (decentraland-gatsby signed-fetch); reading the unverified
/// SIGNER address would let any wallet forge actions as any user.
pub fn auth_address_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, crate::http::errors::ApiError> {
    crate::auth_chain::require_signer(headers, method, path).map_err(|e| {
        tracing::debug!(error = %e, "signed-fetch verification failed");
        crate::http::errors::ApiError::unauthorized("Invalid authentication")
    })
}

pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("authorization").and_then(|v| v.to_str().ok())?;
    let trimmed = raw.trim();
    let token = trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub fn require_bearer_token(
    headers: &HeaderMap,
    expected: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    let expected =
        expected.ok_or_else(|| crate::http::errors::ApiError::unauthorized("Invalid authentication"))?;
    match bearer_token(headers) {
        Some(token) if token == expected => Ok(()),
        _ => Err(crate::http::errors::ApiError::unauthorized("Invalid authentication")),
    }
}
