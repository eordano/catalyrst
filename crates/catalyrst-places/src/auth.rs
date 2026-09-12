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

pub fn auth_chain_claimed_address(headers: &HeaderMap) -> Option<String> {
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

pub async fn auth_address_optional(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Option<String> {
    crate::auth_chain::optional_signer(headers, method, path)
        .await
        .map(|s| s.as_str().to_lowercase())
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

pub const HIGHLIGHTED_RANKING_IS_EDITORIAL: &str =
    "The ranking of a highlighted entity is editorial and can only be changed with the admin token";

pub const EXCLUDED_RANKING_IS_EDITORIAL: &str =
    "This entity is excluded from the automated ranking and its ranking can only be changed with \
     the admin token";

pub fn is_admin_token(headers: &HeaderMap, admin: Option<&str>) -> bool {
    secret_matches(ADMIN_TOKEN_ENV, admin, bearer_token(headers).as_deref()).is_ok()
}

pub fn require_admin_token_for_curated_ranking(
    headers: &HeaderMap,
    admin: Option<&str>,
    highlighted: bool,
    exclude_from_ranking: bool,
) -> Result<(), crate::http::errors::ApiError> {
    if !highlighted && !exclude_from_ranking {
        return Ok(());
    }
    if is_admin_token(headers, admin) {
        return Ok(());
    }
    Err(crate::http::errors::ApiError::forbidden(if highlighted {
        HIGHLIGHTED_RANKING_IS_EDITORIAL
    } else {
        EXCLUDED_RANKING_IS_EDITORIAL
    }))
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
            auth_chain_claimed_address(&headers),
            Some("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_string())
        );
    }

    #[test]
    fn rejects_non_hex_signer_payload() {
        let headers = headers_with_signer("0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ");
        assert_eq!(auth_chain_claimed_address(&headers), None);
    }

    #[tokio::test]
    async fn a_claimed_address_without_a_valid_signature_reads_as_anonymous() {
        let headers = headers_with_signer("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        assert_eq!(
            auth_address_optional(&headers, "get", "/v1/destinations").await,
            None
        );
        assert_eq!(
            auth_address_optional(&HeaderMap::new(), "get", "/v1/destinations").await,
            None
        );
    }

    #[test]
    fn lowercase_bearer_scheme_still_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&headers, Some("secret")).is_ok());
        assert!(require_bearer_token(&headers, Some("secret")).is_ok());
    }

    #[test]
    fn bearer_token_rejects_missing_scheme_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "secret".parse().unwrap());
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn bearer_token_trims_surrounding_whitespace_a_second_divergence_beyond_lowercase() {
        let mut trailing = HeaderMap::new();
        trailing.insert("authorization", "Bearer secret  ".parse().unwrap());
        assert_eq!(bearer_token(&trailing).as_deref(), Some("secret"));

        let mut leading_lower = HeaderMap::new();
        leading_lower.insert("authorization", "  bearer secret".parse().unwrap());
        assert_eq!(bearer_token(&leading_lower).as_deref(), Some("secret"));

        let mut empty_token = HeaderMap::new();
        empty_token.insert("authorization", "Bearer ".parse().unwrap());
        assert_eq!(bearer_token(&empty_token), None);
    }

    #[test]
    fn require_admin_bearer_fails_closed_when_secret_unset_even_with_lowercase_scheme() {
        let mut lower = HeaderMap::new();
        lower.insert("authorization", "bearer whatever".parse().unwrap());
        assert!(require_admin_bearer(&lower, None).is_err());
        assert!(require_admin_bearer(&HeaderMap::new(), None).is_err());
    }

    #[test]
    fn require_admin_bearer_rejects_wrong_and_missing_prefix_and_accepts_both_schemes() {
        let mut wrong = HeaderMap::new();
        wrong.insert("authorization", "Bearer nope".parse().unwrap());
        assert!(require_admin_bearer(&wrong, Some("secret")).is_err());

        let mut no_prefix = HeaderMap::new();
        no_prefix.insert("authorization", "secret".parse().unwrap());
        assert!(require_admin_bearer(&no_prefix, Some("secret")).is_err());

        let mut upper = HeaderMap::new();
        upper.insert("authorization", "Bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&upper, Some("secret")).is_ok());

        let mut lower = HeaderMap::new();
        lower.insert("authorization", "bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&lower, Some("secret")).is_ok());
    }
}
