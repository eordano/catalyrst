use axum::http::HeaderMap;
use catalyrst_crypto::verify::{verify_auth_chain, verify_auth_chain_async};
use catalyrst_crypto::{signed_fetch, AuthError, Eip1654Validator};
use catalyrst_types::EthAddress;
use serde::Deserialize;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, header_str, signed_fetch_path, AuthChain, AuthChainError, AuthLink,
    AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const ONE_MINUTE: i64 = 60;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SceneAuthMetadata {
    #[serde(default)]
    pub realm: Option<RealmField>,
    #[serde(rename = "realmName", default)]
    pub realm_name: Option<String>,
    #[serde(default)]
    pub parcel: Option<String>,
    #[serde(rename = "sceneId", default)]
    pub scene_id: Option<String>,
    #[serde(default)]
    pub signer: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RealmField {
    #[serde(rename = "serverName", default)]
    pub server_name: Option<String>,
}

/// world-storage's wire mapping over the shared chain error: exact upstream
/// worlds-content-server status codes and message text (pinned by tests).
pub trait AuthChainErrorExt {
    fn status_code(&self) -> u16;
    fn raw_message(&self) -> String;
}

impl AuthChainErrorExt for AuthChainError {
    fn status_code(&self) -> u16 {
        match self {
            AuthChainError::MalformedChain { .. }
            | AuthChainError::InsufficientLinks
            | AuthChainError::InvalidTimestamp(_)
            | AuthChainError::ForbiddenSigner
            | AuthChainError::SceneSignerRejected => 400,
            AuthChainError::MissingTimestamp
            | AuthChainError::Expired { .. }
            | AuthChainError::AddressMismatch { .. }
            | AuthChainError::InvalidSignature(_) => 401,
            AuthChainError::EipNotImplemented | AuthChainError::CatalystUnavailable(_) => 503,
        }
    }

    fn raw_message(&self) -> String {
        match self {
            AuthChainError::MalformedChain { detail } => format!("Invalid chain format: {detail}"),
            AuthChainError::InsufficientLinks => "Invalid Auth Chain".to_string(),
            AuthChainError::MissingTimestamp => "Missing timestamp".to_string(),
            AuthChainError::InvalidTimestamp(value) => {
                format!("Invalid chain timestamp: {value}")
            }
            AuthChainError::Expired {
                signed_at,
                now,
                window_secs,
            } => format!(
                "Expired signature: signature timestamp: {signed_at}, timestamp expiration: {}, local timestamp: {now}",
                signed_at + window_secs
            ),
            AuthChainError::InvalidSignature(detail) => format!("Invalid signature: {detail}"),
            AuthChainError::EipNotImplemented => self.to_string(),
            AuthChainError::CatalystUnavailable(detail) => {
                format!("Error connecting to catalyst: {detail}")
            }
            AuthChainError::ForbiddenSigner | AuthChainError::SceneSignerRejected => {
                "Invalid metadata".to_string()
            }
            AuthChainError::AddressMismatch { .. } => self.to_string(),
        }
    }
}

pub fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    signed_fetch::extract_auth_chain(headers)
}

pub fn check_freshness(
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthChainError> {
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if now - signed_at > expiration_secs {
            return Err(AuthChainError::Expired {
                signed_at,
                now,
                window_secs: expiration_secs,
            });
        }
    }
    Ok(())
}

pub async fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
    eip1654_validator: Option<&dyn Eip1654Validator>,
) -> Result<EthAddress, AuthChainError> {
    check_freshness(timestamp, expiration_secs, now)?;

    let crypto_chain = signed_fetch::to_crypto_chain(chain);

    match eip1654_validator {
        Some(validator) => {
            verify_auth_chain_async(&crypto_chain, payload, Some(now * 1000), Some(validator))
                .await
                .map_err(map_auth_error)?;
        }
        None => {
            verify_auth_chain(&crypto_chain, payload, Some(now * 1000)).map_err(map_auth_error)?;
        }
    }
    Ok(chain.signer.clone())
}

fn map_auth_error(err: AuthError) -> AuthChainError {
    match err {
        AuthError::MalformedChain(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::MissingSignature { .. } => AuthChainError::MalformedChain {
            detail: err.to_string(),
        },
        AuthError::RecoveryFailed(d) => AuthChainError::InvalidSignature(d),
        AuthError::SignerMismatch { .. } | AuthError::FinalAuthorityMismatch { .. } => {
            AuthChainError::InvalidSignature(err.to_string())
        }
        AuthError::EphemeralExpired {
            expiration_ms,
            now_ms,
        } => AuthChainError::Expired {
            signed_at: expiration_ms / 1000,
            now: now_ms / 1000,
            window_secs: 0,
        },
        AuthError::InvalidEphemeralPayload(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::Eip1654NotImplemented => AuthChainError::EipNotImplemented,
        AuthError::Eip1654Rejected { .. } => AuthChainError::InvalidSignature(err.to_string()),
        AuthError::Eip1654ValidationFailed(d) => AuthChainError::CatalystUnavailable(d),
    }
}

pub struct VerifiedRequest {
    pub signer: EthAddress,
    pub metadata: SceneAuthMetadata,
}

pub async fn verify_request(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    eip1654_validator: Option<&dyn Eip1654Validator>,
) -> Result<VerifiedRequest, AuthChainError> {
    let path = signed_fetch_path(headers, path);
    let path = path.as_ref();
    let chain = extract_auth_chain(headers)?;
    let ts = header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    if !ts.is_empty() && ts.parse::<f64>().is_err() {
        return Err(AuthChainError::InvalidTimestamp(ts));
    }
    let metadata_raw = header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();

    let payload = build_payload(method, path, &ts, &metadata_raw);
    let now = chrono::Utc::now().timestamp();
    let signer =
        validate_signature(&chain, &payload, &ts, ONE_MINUTE, now, eip1654_validator).await?;

    let metadata: SceneAuthMetadata = serde_json::from_str(&metadata_raw).unwrap_or_default();

    if metadata.signer.as_deref() == Some("decentraland-kernel-scene") {
        return Err(AuthChainError::SceneSignerRejected);
    }

    Ok(VerifiedRequest { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn ts_ms(secs_ago: i64) -> String {
        ((NOW - secs_ago) * 1000).to_string()
    }

    #[test]
    fn freshness_uses_one_minute_window() {
        assert_eq!(ONE_MINUTE, 60);
    }

    #[test]
    fn freshness_accepts_signature_within_window() {
        assert!(check_freshness(&ts_ms(59), ONE_MINUTE, NOW).is_ok());
    }

    #[test]
    fn freshness_accepts_exactly_at_window_boundary() {
        assert!(check_freshness(&ts_ms(60), ONE_MINUTE, NOW).is_ok());
    }

    #[test]
    fn freshness_rejects_just_past_window() {
        let err = check_freshness(&ts_ms(61), ONE_MINUTE, NOW).unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    #[test]
    fn freshness_rejects_five_minute_old_signature() {
        let err = check_freshness(&ts_ms(4 * 60), ONE_MINUTE, NOW).unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    #[test]
    fn freshness_does_not_reject_future_timestamps() {
        assert!(check_freshness(&ts_ms(-10_000), ONE_MINUTE, NOW).is_ok());
    }

    #[test]
    fn freshness_skips_check_for_non_numeric_timestamp() {
        assert!(check_freshness("not-a-number", ONE_MINUTE, NOW).is_ok());
    }

    #[test]
    fn status_code_maps_to_upstream_request_error_codes() {
        assert_eq!(
            AuthChainError::MalformedChain {
                detail: "bad json".into()
            }
            .status_code(),
            400
        );
        assert_eq!(AuthChainError::InsufficientLinks.status_code(), 400);
        assert_eq!(
            AuthChainError::InvalidTimestamp("abc".into()).status_code(),
            400
        );
        assert_eq!(AuthChainError::SceneSignerRejected.status_code(), 400);

        assert_eq!(AuthChainError::MissingTimestamp.status_code(), 401);
        assert_eq!(
            AuthChainError::Expired {
                signed_at: 0,
                now: 100,
                window_secs: 60
            }
            .status_code(),
            401
        );
        assert_eq!(
            AuthChainError::InvalidSignature("nope".into()).status_code(),
            401
        );

        assert_eq!(AuthChainError::EipNotImplemented.status_code(), 503);
        assert_eq!(
            AuthChainError::CatalystUnavailable("rpc down".into()).status_code(),
            503
        );
    }

    #[test]
    fn raw_message_mirrors_upstream_error_text() {
        assert_eq!(
            AuthChainError::MalformedChain {
                detail: "unexpected token".into()
            }
            .raw_message(),
            "Invalid chain format: unexpected token"
        );
        assert_eq!(
            AuthChainError::InsufficientLinks.raw_message(),
            "Invalid Auth Chain"
        );
        assert_eq!(
            AuthChainError::InvalidTimestamp("xyz".into()).raw_message(),
            "Invalid chain timestamp: xyz"
        );
        assert_eq!(
            AuthChainError::SceneSignerRejected.raw_message(),
            "Invalid metadata"
        );
        assert!(AuthChainError::InvalidSignature("recovery failed".into())
            .raw_message()
            .starts_with("Invalid signature: "));
    }

    #[test]
    fn rpc_validation_failure_is_catalyst_unavailable_503() {
        let mapped = map_auth_error(AuthError::Eip1654ValidationFailed("RPC timeout".into()));
        assert!(matches!(mapped, AuthChainError::CatalystUnavailable(_)));
        assert_eq!(mapped.status_code(), 503);
    }
}
