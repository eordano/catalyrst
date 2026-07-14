use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::Signer;

use crate::http::response::ApiError;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES).await
}

/// Mirrors @dcl/crypto-middleware ≥5.1.0 (pulled in by events#1007's
/// decentraland-gatsby 8.4.8 bump): a signed-fetch request whose
/// `x-identity-metadata` JSON carries a `signer` or `intent` that is not already
/// its own `trim().to_lowercase()` is rejected with a 400 message prefixed
/// `Invalid chain metadata: `. Metadata without `signer`/`intent`, or non-JSON
/// metadata, is unaffected.
///
/// The gate must exist because the payload is lowercased before signing while the
/// header keeps its original casing: a mixed-case `Decentraland-Kernel-Scene`
/// signs byte-identically to the canonical spelling, so without this a scene could
/// present a scene-signed request as a directly user-signed action (e.g. silently
/// RSVPing a visiting player). Matches catalyrst-market's wording for cross-service
/// parity; kept crate-local like market's copy rather than shared through
/// catalyrst-crypto.
fn check_canonical_metadata(metadata: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return Ok(());
    };
    for key in ["signer", "intent"] {
        if let Some(raw) = value.get(key).and_then(serde_json::Value::as_str) {
            if raw != raw.trim().to_lowercase() {
                let echo: String = metadata.chars().take(64).collect();
                return Err(format!("Invalid chain metadata: {echo}"));
            }
        }
    }
    Ok(())
}

const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// Mirrors decentraland-gatsby's default `verifySigner` metadataValidator, wired
/// onto every `auth()`/`auth({optional:true})` route in upstream events. It
/// throws `RequestError('Invalid signer', 400)` when the `x-identity-metadata`
/// `signer` is the kernel-scene signer — a scene-originated request presenting
/// itself as a directly user-signed action. The canonical gate above already
/// forces `signer` to `trim().to_lowercase()`, so an exact match here catches
/// every spelling (the mixed-case escape 400s at the canonical gate first).
fn check_metadata_signer(metadata: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return Ok(());
    };
    if value.get("signer").and_then(serde_json::Value::as_str) == Some(SCENE_SIGNER) {
        return Err("Invalid signer".to_string());
    }
    Ok(())
}

/// The single choke point every mutating events route funnels through. The
/// canonical-metadata gate runs first (400 on a non-canonical `signer`/`intent`),
/// then the signed-fetch auth chain is verified (401 on any failure), so every
/// current and future authenticated handler inherits both. Read-only GET routes
/// stay unauthenticated and use [`try_extract_signer`] instead.
pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, ApiError> {
    let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    check_canonical_metadata(metadata).map_err(ApiError::bad_request)?;
    check_metadata_signer(metadata).map_err(ApiError::bad_request)?;
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES)
        .await
        .map_err(|_| ApiError::unauthorized("Unauthorized"))
}

/// Optional-auth counterpart of [`require_signer`], mirroring gatsby's
/// `auth({optional:true})`: a request with no auth-chain headers resolves to
/// `None`, but one that DOES present headers still runs the canonical and
/// verifySigner metadata gates, so a scene-signed or non-canonical read is
/// rejected 400 rather than silently served as anonymous. A merely unverifiable
/// signature still degrades to anonymous, matching crypto-middleware's optional
/// verify.
pub async fn optional_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Option<Signer>, ApiError> {
    let first_link = format!("{AUTH_CHAIN_HEADER_PREFIX}0");
    if signed_fetch::header_str(headers, &first_link).is_some() {
        let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
        check_canonical_metadata(metadata).map_err(ApiError::bad_request)?;
        check_metadata_signer(metadata).map_err(ApiError::bad_request)?;
    }
    Ok(try_extract_signer(headers, method, path).await)
}

#[cfg(test)]
mod canonical_metadata_tests {
    use super::*;

    fn status(e: ApiError) -> u16 {
        match e {
            ApiError::Common(catalyrst_types::ApiError::Http { status, .. }) => status,
            _ => 0,
        }
    }

    #[test]
    fn rejects_non_canonical_signer_and_intent() {
        for meta in [
            // The exploit this closes: a mixed-case kernel-scene signer.
            r#"{"origin":"https://play.decentraland.org","signer":"Decentraland-Kernel-Scene"}"#,
            r#"{"signer":" dcl:marketplace"}"#,
            r#"{"intent":"Dcl:Intent"}"#,
            r#"{"intent":"dcl:intent "}"#,
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
        assert!(check_canonical_metadata(r#"{"signer":"decentraland-kernel-scene"}"#).is_ok());
        assert!(check_canonical_metadata(r#"{"intent":"dcl:intent"}"#).is_ok());
        assert!(check_canonical_metadata("{}").is_ok());
        assert!(check_canonical_metadata("not json").is_ok());
    }

    /// The gate fires before signature verification, so a valid-looking request
    /// carrying non-canonical metadata is a 400, not a 401.
    #[tokio::test]
    async fn require_signer_rejects_non_canonical_metadata_with_400() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            r#"{"signer":"Decentraland-Kernel-Scene"}"#.parse().unwrap(),
        );
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("non-canonical metadata must be rejected");
        assert_eq!(status(err), 400);
    }

    #[tokio::test]
    async fn require_signer_missing_auth_is_401_not_400() {
        let headers = HeaderMap::new();
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("missing auth must be rejected");
        assert_eq!(status(err), 401);
    }

    #[test]
    fn rejects_canonical_scene_signer() {
        let err = check_metadata_signer(r#"{"signer":"decentraland-kernel-scene"}"#)
            .expect_err("kernel-scene signer must be rejected");
        assert_eq!(err, "Invalid signer");
        assert!(check_metadata_signer(r#"{"signer":"0xabc"}"#).is_ok());
        assert!(check_metadata_signer("{}").is_ok());
        assert!(check_metadata_signer("not json").is_ok());
    }

    /// The canonical spelling clears the casing gate but is still a scene
    /// impersonation — require_signer 400s it with gatsby's "Invalid signer".
    #[tokio::test]
    async fn require_signer_rejects_canonical_scene_signer_with_400() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            r#"{"signer":"decentraland-kernel-scene"}"#.parse().unwrap(),
        );
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("canonical scene signer must be rejected");
        assert_eq!(status(err), 400);
    }
}
