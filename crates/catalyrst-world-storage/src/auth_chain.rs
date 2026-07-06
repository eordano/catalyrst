use axum::http::HeaderMap;
use catalyrst_crypto::verify::{verify_auth_chain, verify_auth_chain_async};
use catalyrst_crypto::{AuthError, Eip1654Validator};
use catalyrst_types::{AuthLink as CryptoAuthLink, AuthLinkType as CryptoAuthLinkType, EthAddress};
use serde::Deserialize;
use thiserror::Error;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";

pub const MAX_AUTH_CHAIN_LINKS: usize = 10;

pub const ONE_MINUTE: i64 = 60;

#[derive(Debug, Clone)]
pub struct AuthLink {
    pub kind: CryptoAuthLinkType,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct AuthChain {
    pub links: Vec<AuthLink>,
    pub signer: EthAddress,
}

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

#[derive(Debug, Error)]
pub enum AuthChainError {
    #[error("Invalid Auth Chain")]
    MalformedChain { detail: String },
    #[error("Invalid Auth Chain")]
    InsufficientLinks,
    #[error("Missing timestamp")]
    MissingTimestamp,
    #[error("Invalid timestamp")]
    InvalidTimestamp(String),
    #[error("Expired signature")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },
    #[error("Invalid signature")]
    InvalidSignature(String),
    #[error("EIP-1654 not implemented")]
    EipNotImplemented,
    #[error("Error connecting to catalyst")]
    CatalystUnavailable(String),
    #[error("Requests from scenes are not allowed")]
    SceneSignerRejected,
}

impl AuthChainError {
    pub fn status_code(&self) -> u16 {
        match self {
            AuthChainError::MalformedChain { .. }
            | AuthChainError::InsufficientLinks
            | AuthChainError::InvalidTimestamp(_)
            | AuthChainError::SceneSignerRejected => 400,
            AuthChainError::MissingTimestamp
            | AuthChainError::Expired { .. }
            | AuthChainError::InvalidSignature(_) => 401,
            AuthChainError::EipNotImplemented | AuthChainError::CatalystUnavailable(_) => 503,
        }
    }

    pub fn raw_message(&self) -> String {
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
            AuthChainError::SceneSignerRejected => "Invalid metadata".to_string(),
        }
    }
}

pub fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{}:{}:{}:{}", method, path, timestamp, metadata).to_lowercase()
}

fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

pub fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    let mut links = Vec::new();

    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, i);
        let Some(raw) = header_str(headers, &name) else {
            break;
        };

        let link: CryptoAuthLink = serde_json::from_str(raw).map_err(|e| {
            let mut detail = e.to_string();
            if detail.len() > 64 {
                detail.truncate(64);
            }
            AuthChainError::MalformedChain { detail }
        })?;

        match link.link_type {
            CryptoAuthLinkType::SIGNER => {
                if i != 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("SIGNER link at non-zero index {}", i),
                    });
                }
            }
            _ => {
                if i == 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: "first link must be SIGNER".to_string(),
                    });
                }
                if link.signature.as_deref().unwrap_or("").is_empty() {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("missing signature on link {}", i),
                    });
                }
            }
        }

        links.push(AuthLink {
            kind: link.link_type,
            payload: link.payload,
            signature: link.signature.unwrap_or_default(),
        });
    }

    let overflow = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, MAX_AUTH_CHAIN_LINKS);
    if header_str(headers, &overflow).is_some() {
        return Err(AuthChainError::MalformedChain {
            detail: format!("exceeds max length of {}", MAX_AUTH_CHAIN_LINKS),
        });
    }
    if links.len() < 2 {
        return Err(AuthChainError::InsufficientLinks);
    }
    let signer = links[0].payload.to_lowercase();
    Ok(AuthChain { links, signer })
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

fn to_crypto_chain(chain: &AuthChain) -> Vec<CryptoAuthLink> {
    chain
        .links
        .iter()
        .map(|link| CryptoAuthLink {
            link_type: link.kind,
            payload: link.payload.clone(),
            signature: if link.signature.is_empty() {
                None
            } else {
                Some(link.signature.clone())
            },
        })
        .collect()
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

    let crypto_chain = to_crypto_chain(chain);

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
