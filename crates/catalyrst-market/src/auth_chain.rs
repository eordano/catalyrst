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

/// Mirrors @dcl/crypto-middleware >=5.1.0 (marketplace-server #388): the
/// signed-fetch `verify()` entrypoint rejects, with HTTP 400 and a message
/// prefixed `Invalid chain metadata: `, any request whose `x-identity-metadata`
/// JSON carries a `signer` or `intent` that is not canonical -- i.e. differs from
/// its own `trim().to_lowercase()` (mixed case or surrounding whitespace). This
/// fires before any route-specific validator. A request with no `signer`/`intent`
/// (or non-JSON metadata) is unaffected.
///
/// Why it matters: the signed-fetch client lowercases the payload before signing
/// but delivers the metadata header with its original casing, so a mixed-case
/// `signer` produces a signature byte-identical to the canonical spelling's -- a
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

/// The signer allow-list upstream installs on the routes a marketplace or
/// builder client is the only legitimate caller of (routes.ts:122,165).
pub const MARKETPLACE_AUTH_SIGNERS: &[&str] = &["dcl:marketplace", "dcl:builder"];

/// The intent upstream demands of POST /v1/trades (routes.ts:122).
pub const CREATE_TRADE_INTENT: &str = "dcl:create-trade";

/// Upstream `validateAuthMetadata(signers, intent)` (marketplace-server
/// src/controllers/utils.ts), the route-level policy behind POST /v1/trades and
/// GET /v1/activity.
///
/// Both comparisons are exact against canonical declarations and nothing is
/// folded first: @dcl/crypto-middleware 6.x hands the validator the metadata
/// exactly as it was signed, and the legacy payload lowercases the metadata
/// before signing, so a re-spelled `Dcl:Marketplace` or
/// `Decentraland-Kernel-Scene` carries a byte-identical signature. Comparing
/// after folding would authorize a request as something it is not; comparing
/// without folding refuses it, which is the whole point of upstream #393.
pub fn require_auth_metadata(
    headers: &HeaderMap,
    allowed_signers: &[&str],
    intent: Option<&str>,
) -> Result<(), ApiError> {
    let raw = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata =
        serde_json::from_str::<serde_json::Value>(raw).unwrap_or(serde_json::Value::Null);

    let signer = metadata.get("signer").and_then(serde_json::Value::as_str);
    if !signer.is_some_and(|declared| allowed_signers.contains(&declared)) {
        return Err(ApiError::bad_request("Invalid auth signer"));
    }

    if let Some(expected) = intent {
        let declared = metadata.get("intent").and_then(serde_json::Value::as_str);
        if declared != Some(expected) {
            return Err(ApiError::bad_request(
                "Invalid auth intent to perform this operation",
            ));
        }
    }

    Ok(())
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
        assert!(check_canonical_metadata(
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#
        )
        .is_ok());
        // The canonical kernel-scene spelling is not a canonicalization failure
        // (the scene-signer policy is a separate, route-level concern).
        assert!(check_canonical_metadata(r#"{"signer":"decentraland-kernel-scene"}"#).is_ok());
        assert!(check_canonical_metadata(r#"{"intent":"dcl:marketplace:remove-pick"}"#).is_ok());
        assert!(check_canonical_metadata("{}").is_ok());
        // Non-JSON metadata is not a canonicalization question here.
        assert!(check_canonical_metadata("not json").is_ok());
    }
}

#[cfg(test)]
mod auth_metadata_tests {
    use super::{
        require_auth_metadata, AUTH_METADATA_HEADER, CREATE_TRADE_INTENT, MARKETPLACE_AUTH_SIGNERS,
    };
    use axum::http::{HeaderMap, HeaderValue};

    fn headers(metadata: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        h
    }

    fn message(err: crate::http::response::ApiError) -> String {
        err.to_string()
    }

    #[test]
    fn accepts_every_allowed_signer_with_the_declared_intent() {
        for signer in MARKETPLACE_AUTH_SIGNERS {
            let metadata = format!(r#"{{"signer":"{signer}","intent":"{CREATE_TRADE_INTENT}"}}"#);
            assert!(require_auth_metadata(
                &headers(&metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT)
            )
            .is_ok());
        }
    }

    #[test]
    fn refuses_a_signer_outside_the_allow_list() {
        for metadata in [
            r#"{"signer":"decentraland-kernel-scene","intent":"dcl:create-trade"}"#,
            r#"{"signer":"Decentraland-Kernel-Scene","intent":"dcl:create-trade"}"#,
            r#"{"signer":"dcl:explorer","intent":"dcl:create-trade"}"#,
            r#"{"signer":"Dcl:Marketplace","intent":"dcl:create-trade"}"#,
            r#"{"signer":" dcl:marketplace","intent":"dcl:create-trade"}"#,
            r#"{"signer":42,"intent":"dcl:create-trade"}"#,
            r#"{"intent":"dcl:create-trade"}"#,
            "{}",
            "not json",
        ] {
            let err = require_auth_metadata(
                &headers(metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT),
            )
            .expect_err(metadata);
            assert_eq!(message(err), "Invalid auth signer", "{metadata}");
        }
    }

    #[test]
    fn refuses_a_wrong_absent_or_respelled_intent() {
        for metadata in [
            r#"{"signer":"dcl:marketplace"}"#,
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#,
            r#"{"signer":"dcl:marketplace","intent":"Dcl:Create-Trade"}"#,
            r#"{"signer":"dcl:marketplace","intent":"dcl:create-trade "}"#,
            r#"{"signer":"dcl:marketplace","intent":null}"#,
        ] {
            let err = require_auth_metadata(
                &headers(metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT),
            )
            .expect_err(metadata);
            assert_eq!(
                message(err),
                "Invalid auth intent to perform this operation",
                "{metadata}"
            );
        }
    }

    #[test]
    fn leaves_intent_alone_when_the_route_declares_none() {
        for metadata in [
            r#"{"signer":"dcl:marketplace"}"#,
            r#"{"signer":"dcl:builder","intent":"dcl:marketplace:add-pick"}"#,
        ] {
            assert!(
                require_auth_metadata(&headers(metadata), MARKETPLACE_AUTH_SIGNERS, None).is_ok(),
                "{metadata}"
            );
        }
    }

    #[test]
    fn a_missing_metadata_header_carries_no_signer() {
        let err = require_auth_metadata(&HeaderMap::new(), MARKETPLACE_AUTH_SIGNERS, None)
            .expect_err("no metadata header means no signer");
        assert_eq!(message(err), "Invalid auth signer");
    }
}
