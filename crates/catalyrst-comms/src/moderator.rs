use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch::verify_signed_fetch_meta_with_legacy_fallback;
use catalyrst_crypto::{reject_if_signer, SignerGate};

use crate::auth_chain::FIVE_MINUTES;
use crate::http::{service_unavailable, unauthorized, ApiError};
use crate::AppState;

const MAX_MODERATOR_NAME_LENGTH: usize = 100;
const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// The only metadata field this surface authorizes on, so the only one pinned
/// to its declared spelling when a legacy-payload request is accepted.
const AUTHORIZED_METADATA_KEYS: &[&str] = &["signer"];

/// Reads `signer` unfolded. The legacy payload lowercases the metadata before
/// signing, so `{"Signer":...}` carries a signature identical to the canonical
/// spelling; a gate that compared after folding -- or that read the field with a
/// plain `get("signer")` -- would see the re-cased key as absent and serve a
/// scene-signed chain as the moderator whose identity signed it.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical"))
}

pub enum ModeratorMode {
    Read,
    Write,
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub(crate) fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub(crate) fn require_service_token(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.gatekeeper_auth_token.as_deref() else {
        return Err(service_unavailable(
            "This route is unavailable: COMMS_GATEKEEPER_AUTH_TOKEN is not configured, so the platform service token cannot be verified",
        ));
    };
    let presented = bearer_token(headers)
        .map(|t| timing_safe_eq(&t, expected))
        .unwrap_or(false);
    if presented {
        Ok(())
    } else {
        Err(unauthorized("Authentication required"))
    }
}

fn sanitize_moderator_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MODERATOR_NAME_LENGTH {
        return None;
    }
    let ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.');
    if ok {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub async fn authorize_moderator(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    mode: ModeratorMode,
    moderator_query: Option<&str>,
) -> Result<String, ApiError> {
    if let Some(expected) = state.moderator_token.as_deref() {
        if let Some(token) = bearer_token(headers) {
            if timing_safe_eq(&token, expected) {
                return match mode {
                    ModeratorMode::Write => {
                        let raw = moderator_query.ok_or_else(|| {
                            ApiError::bad_request("Missing moderator query parameter")
                        })?;
                        sanitize_moderator_name(raw).ok_or_else(|| {
                            ApiError::bad_request(
                                "Invalid moderator query parameter. Must be alphanumeric (spaces, hyphens, underscores, and dots allowed) and at most 100 characters",
                            )
                        })
                    }
                    ModeratorMode::Read => Ok("moderator-token".to_string()),
                };
            }
        }
    }

    let signer = verify_signed_fetch_meta_with_legacy_fallback(
        headers,
        method,
        path,
        FIVE_MINUTES,
        AUTHORIZED_METADATA_KEYS,
        Some(scene_signer_gate()),
    )
    .await
    .map_err(|_| unauthorized("You are not authorized to access this resource"))?
    .0
    .as_str()
    .to_string();

    if state.moderator_addresses.iter().any(|a| a == &signer) {
        Ok(signer)
    } else {
        Err(unauthorized(
            "You are not authorized to access this resource",
        ))
    }
}

#[cfg(test)]
mod scene_gate_tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use catalyrst_crypto::signed_fetch::{
        build_legacy_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    };
    use catalyrst_crypto::{create_simple_auth_chain, Wallet};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";

    fn legacy_signed(metadata: &str) -> HeaderMap {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let payload = build_legacy_payload("get", "/moderators", &timestamp, metadata);
        let chain = create_simple_auth_chain(&wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
            headers.insert(
                HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                    .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            );
        }
        headers
    }

    async fn verify(metadata: &str) -> Result<String, ()> {
        verify_signed_fetch_meta_with_legacy_fallback(
            &legacy_signed(metadata),
            "get",
            "/moderators",
            FIVE_MINUTES,
            AUTHORIZED_METADATA_KEYS,
            Some(scene_signer_gate()),
        )
        .await
        .map(|(signer, _)| signer.as_str().to_string())
        .map_err(|_| ())
    }

    #[tokio::test]
    async fn a_scene_signed_chain_is_refused() {
        assert!(verify(r#"{"signer":"decentraland-kernel-scene"}"#)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_respelled_scene_signer_key_is_refused_not_served_as_the_user() {
        assert!(verify(r#"{"Signer":"decentraland-kernel-scene"}"#)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn an_ordinary_user_signed_chain_still_authorizes() {
        assert!(verify("{}").await.is_ok());
    }
}
