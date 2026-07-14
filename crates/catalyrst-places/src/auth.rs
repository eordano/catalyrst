use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use catalyrst_authenticated_principal::{
    establish_platform_service_identity_by_comparing_presented_shared_secret,
    AuthorityNotEstablished,
};

use crate::http::errors::ApiError;
use crate::AppState;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";

const ADMIN_TOKEN_ENV: &str = "PLACES_ADMIN_AUTH_TOKEN";
const DATA_TEAM_TOKEN_ENV: &str = "DATA_TEAM_AUTH_TOKEN";

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

pub async fn auth_address_verified(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, crate::http::errors::ApiError> {
    crate::auth_chain::require_signer(headers, method, path)
        .await
        .map_err(|e| {
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

fn secret_matches(
    env_var: &'static str,
    expected: Option<&str>,
    presented: Option<&str>,
) -> Result<(), AuthorityNotEstablished> {
    establish_platform_service_identity_by_comparing_presented_shared_secret(
        env_var, expected, presented,
    )
    .map(|_| ())
}

pub fn require_bearer_token(
    headers: &HeaderMap,
    expected: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    secret_matches(ADMIN_TOKEN_ENV, expected, bearer_token(headers).as_deref())
        .map_err(|_| crate::http::errors::ApiError::unauthorized("Invalid authentication"))
}

pub fn require_ranking_token(
    headers: &HeaderMap,
    data_team: Option<&str>,
    admin: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    let presented = bearer_token(headers);
    let presented = presented.as_deref();
    if secret_matches(DATA_TEAM_TOKEN_ENV, data_team, presented).is_ok()
        || secret_matches(ADMIN_TOKEN_ENV, admin, presented).is_ok()
    {
        Ok(())
    } else {
        Err(crate::http::errors::ApiError::unauthorized(
            "Invalid authentication",
        ))
    }
}

pub fn require_admin_bearer(
    headers: &HeaderMap,
    expected: Option<&str>,
) -> Result<(), crate::http::errors::ApiError> {
    match secret_matches(ADMIN_TOKEN_ENV, expected, bearer_token(headers).as_deref()) {
        Ok(()) => Ok(()),
        Err(AuthorityNotEstablished::CredentialNotConfigured { .. }) => Err(
            crate::http::errors::ApiError::forbidden("Admin token not configured"),
        ),
        Err(_) => Err(crate::http::errors::ApiError::forbidden(
            "Invalid admin credentials",
        )),
    }
}

pub struct RequireAdmin(());

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        require_admin_bearer(&parts.headers, state.admin_auth_token.as_deref())?;
        Ok(RequireAdmin(()))
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

    #[test]
    fn lowercase_bearer_scheme_still_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&headers, Some("secret")).is_ok());
        assert!(require_bearer_token(&headers, Some("secret")).is_ok());
    }
}
