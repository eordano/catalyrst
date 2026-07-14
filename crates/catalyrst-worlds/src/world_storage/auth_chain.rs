use std::sync::OnceLock;

use axum::http::HeaderMap;
use catalyrst_crypto::metadata_gate::assert_legacy_metadata_keys;
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_crypto::verify::{verify_auth_chain, verify_auth_chain_async};
use catalyrst_crypto::{reject_if_signer, signed_fetch, AuthError, Eip1654Validator, SignerGate};
use catalyrst_types::EthAddress;
use serde::Deserialize;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, header_str, signed_fetch_path, AuthChain, AuthChainError, AuthLink,
    AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const ONE_MINUTE: i64 = 60;

/// The metadata fields this surface authorizes on or derives the scene context
/// from. Only these are pinned to their declared spelling on the legacy payload,
/// where folding leaves key casing outside the signature; adding a field that a
/// handler starts reading belongs here too.
pub const CANONICAL_METADATA_KEYS: &[&str] = &[
    "signer",
    "realmName",
    "realm.serverName",
    "sceneId",
    "parcel",
];

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

/// Fail closed, like the shared signed-fetch path: a timestamp that is not plain
/// integer milliseconds must be rejected, not silently skip the
/// replay/expiration window.
pub fn check_freshness(
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthChainError> {
    let signed_at = timestamp
        .parse::<i64>()
        .map_err(|_| AuthChainError::InvalidTimestamp(timestamp.to_string()))?
        / 1000;
    if (now - signed_at).abs() > expiration_secs {
        return Err(AuthChainError::Expired {
            signed_at,
            now,
            window_secs: expiration_secs,
        });
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

/// A scene-signed request must never reach this surface. The gate refuses a
/// `signer` that is not already canonical instead of folding it before
/// comparing: padding survives the signature (it was there when signed), so a
/// folded comparison would read ` decentraland-kernel-scene` as "not a scene"
/// and serve it as an ordinary user-signed request.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| {
        reject_if_signer(&[crate::auth_chain::KERNEL_SCENE_SIGNER])
            .expect("KERNEL_SCENE_SIGNER is canonical")
    })
}

/// The gate is only meaningful over an object, and a non-object metadata header
/// carries no `signer` for it to read either way.
fn metadata_object(raw: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        _ => serde_json::Value::Object(serde_json::Map::new()),
    }
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
    let metadata_raw = header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();

    let metadata_value = metadata_object(&metadata_raw);
    if !scene_signer_gate().permits(&metadata_value) {
        return Err(AuthChainError::SceneSignerRejected);
    }

    let now = chrono::Utc::now().timestamp();
    let payload = build_payload_v6(method, path, &ts, &metadata_raw);
    let signer =
        match validate_signature(&chain, &payload, &ts, ONE_MINUTE, now, eip1654_validator).await {
            Ok(signer) => signer,
            Err(AuthChainError::InvalidSignature(_)) => {
                assert_legacy_metadata_keys(&metadata_value, CANONICAL_METADATA_KEYS)?;
                let legacy = build_legacy_payload(method, path, &ts, &metadata_raw);
                validate_signature(&chain, &legacy, &ts, ONE_MINUTE, now, eip1654_validator).await?
            }
            Err(err) => return Err(err),
        };

    let metadata: SceneAuthMetadata = serde_json::from_str(&metadata_raw).unwrap_or_default();

    Ok(VerifiedRequest { signer, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn freshness_accepts_future_timestamp_within_window() {
        assert!(check_freshness(&ts_ms(-60), ONE_MINUTE, NOW).is_ok());
    }

    #[test]
    fn freshness_rejects_future_timestamp_just_past_window() {
        let err = check_freshness(&ts_ms(-61), ONE_MINUTE, NOW).unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    /// The bound is symmetric, like the shared signed-fetch path: a signature
    /// dated far in the future must not stay replayable forever.
    #[test]
    fn freshness_rejects_far_future_timestamps() {
        let err = check_freshness(&ts_ms(-10_000), ONE_MINUTE, NOW).unwrap_err();
        assert!(matches!(err, AuthChainError::Expired { .. }));
    }

    /// The window used to be skipped whenever the timestamp did not parse, which
    /// minted a credential that never expired. Every shape that is not plain
    /// integer milliseconds is now refused outright.
    #[test]
    fn freshness_rejects_timestamps_that_are_not_plain_integer_milliseconds() {
        for raw in [
            "",
            "not-a-number",
            "1.7e12",
            "1700000000000.0",
            "Infinity",
            "NaN",
            " 1700000000000",
        ] {
            let err = check_freshness(raw, ONE_MINUTE, NOW).unwrap_err();
            assert!(
                matches!(&err, AuthChainError::InvalidTimestamp(value) if value == raw),
                "timestamp {raw:?} produced {err:?}"
            );
        }
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

    fn permits(metadata: serde_json::Value) -> bool {
        scene_signer_gate().permits(&metadata)
    }

    #[test]
    fn scene_signer_is_rejected() {
        assert!(!permits(json!({ "signer": "decentraland-kernel-scene" })));
    }

    /// A re-cased spelling used to be folded before comparing; it is now refused
    /// as non-canonical, one layer earlier and without any crypto.
    #[test]
    fn recased_scene_signer_is_rejected() {
        assert!(!permits(json!({ "signer": "Decentraland-Kernel-Scene" })));
        assert!(!permits(json!({ "signer": "DECENTRALAND-KERNEL-SCENE" })));
    }

    /// Padding is signature-bound, so no folded comparison can catch it: the
    /// value was padded when it was signed and verifies exactly as delivered.
    #[test]
    fn padded_scene_signer_is_rejected() {
        assert!(!permits(json!({ "signer": " decentraland-kernel-scene" })));
        assert!(!permits(json!({ "signer": "decentraland-kernel-scene " })));
        assert!(!permits(json!({ "signer": "\tdecentraland-kernel-scene" })));
    }

    #[test]
    fn non_canonical_signer_is_rejected_even_when_it_is_not_a_scene() {
        assert!(!permits(json!({ "signer": "0xAbC" })));
        assert!(!permits(json!({ "signer": 42 })));
    }

    #[test]
    fn canonical_non_scene_signer_passes() {
        assert!(permits(json!({ "signer": "dcl:authoritative-server" })));
        assert!(permits(json!({ "signer": "0xabc" })));
    }

    /// Absence is not a claim to be a scene, and every crypto-middleware version
    /// has allowed it.
    #[test]
    fn absent_signer_passes() {
        assert!(permits(json!({ "realmName": "some.dcl.eth" })));
        assert!(permits(json!({})));
    }

    #[test]
    fn non_object_metadata_gates_as_empty() {
        assert!(permits(metadata_object("\"decentraland-kernel-scene\"")));
        assert!(permits(metadata_object("not json")));
        assert!(permits(metadata_object("null")));
    }

    /// The 6.x payload binds the metadata bytes: only method and path fold, so a
    /// re-spelled metadata no longer shares the original signature.
    #[test]
    fn v6_payload_keeps_metadata_casing() {
        let metadata = r#"{"realmName":"Some.dcl.eth","sceneId":"bafkreiABC"}"#;
        assert_eq!(
            build_payload_v6("PUT", "/Values/K", "123", metadata),
            format!("put:/values/k:123:{metadata}")
        );
        assert_ne!(
            build_payload_v6("PUT", "/Values/K", "123", metadata),
            build_legacy_payload("PUT", "/Values/K", "123", metadata)
        );
    }

    /// The legacy payload folds the metadata, so key casing sits outside the
    /// signature. Every field the handlers read is pinned to its declared
    /// spelling before that payload is accepted.
    #[test]
    fn legacy_fallback_refuses_respelled_authorized_keys() {
        for respelled in [
            json!({ "Signer": "dcl:authoritative-server" }),
            json!({ "RealmName": "some.dcl.eth" }),
            json!({ "realm": { "ServerName": "some.dcl.eth" } }),
            json!({ "SceneId": "bafkrei" }),
            json!({ "Parcel": "0,0" }),
        ] {
            assert!(
                assert_legacy_metadata_keys(&respelled, CANONICAL_METADATA_KEYS).is_err(),
                "{respelled} must not be accepted on the legacy payload"
            );
        }
    }

    #[test]
    fn legacy_fallback_accepts_the_metadata_our_scene_runtime_signs() {
        let metadata = json!({
            "origin": "catalyrst-scene-state://",
            "signer": "dcl:authoritative-server",
            "isGuest": false,
            "realmName": "some.dcl.eth",
            "realm": { "serverName": "some.dcl.eth" },
            "sceneId": "bafkreiabc",
            "parcel": "0,0",
        });
        assert!(assert_legacy_metadata_keys(&metadata, CANONICAL_METADATA_KEYS).is_ok());
    }

    #[test]
    fn rpc_validation_failure_is_catalyst_unavailable_503() {
        let mapped = map_auth_error(AuthError::Eip1654ValidationFailed("RPC timeout".into()));
        assert!(matches!(mapped, AuthChainError::CatalystUnavailable(_)));
        assert_eq!(mapped.status_code(), 503);
    }
}
