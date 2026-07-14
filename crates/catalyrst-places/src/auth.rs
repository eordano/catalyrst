use axum::http::HeaderMap;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";

pub fn auth_address_optional(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(format!("{AUTH_CHAIN_HEADER_PREFIX}0"))
        .and_then(|v| v.to_str().ok())?;
    let link: serde_json::Value = serde_json::from_str(raw).ok()?;
    let addr = link.get("payload").and_then(|p| p.as_str())?;
    if catalyrst_types::is_eth_address(addr) {
        Some(addr.to_lowercase())
    } else {
        None
    }
}

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
    let expected = expected
        .ok_or_else(|| crate::http::errors::ApiError::unauthorized("Invalid authentication"))?;
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(&token, expected) => Ok(()),
        _ => Err(crate::http::errors::ApiError::unauthorized(
            "Invalid authentication",
        )),
    }
}

pub fn require_ranking_token(
    headers: &HeaderMap,
    data_team: Option<&str>,
    admin: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    let token = bearer_token(headers)
        .ok_or_else(|| crate::http::errors::ApiError::unauthorized("Invalid authentication"))?;
    if [data_team, admin]
        .into_iter()
        .flatten()
        .any(|expected| timing_safe_eq(&token, expected))
    {
        Ok(())
    } else {
        Err(crate::http::errors::ApiError::unauthorized(
            "Invalid authentication",
        ))
    }
}

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

#[cfg(test)]
mod auth_address_tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_signer(payload: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let link = serde_json::json!({ "type": "SIGNER", "payload": payload });
        headers.insert(
            "x-identity-auth-chain-0",
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
        headers
    }

    #[test]
    fn accepts_valid_signer_payload() {
        let headers = headers_with_signer("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        assert_eq!(
            auth_address_optional(&headers),
            Some("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_string())
        );
    }

    #[test]
    fn rejects_non_hex_signer_payload() {
        let headers = headers_with_signer("0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ");
        assert_eq!(auth_address_optional(&headers), None);
    }
}
