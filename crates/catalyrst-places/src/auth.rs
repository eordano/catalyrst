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
        Some(token) if timing_safe_eq(&token, expected) => Ok(()),
        _ => Err(crate::http::errors::ApiError::unauthorized("Invalid authentication")),
    }
}

/// Constant-time string comparison (mirrors catalyrst-comms
/// `moderator::timing_safe_eq`). Avoids leaking the admin token via the
/// early-exit timing of `==`.
pub fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Gate for the LATER admin-console routes (moderation queue, place
/// disable/soft-delete, POI CRUD). **Fails closed:** when the admin token env
/// is unset, `expected` is `None` and every call returns 403 (default-safe per
/// admin-console.md §4). Compares the presented `Authorization: Bearer <token>`
/// in constant time.
pub fn require_admin_bearer(
    headers: &HeaderMap,
    expected: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    let expected = expected
        .ok_or_else(|| crate::http::errors::ApiError::forbidden("Admin token not configured"))?;
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(&token, expected) => Ok(()),
        _ => Err(crate::http::errors::ApiError::forbidden(
            "Invalid admin credentials",
        )),
    }
}
