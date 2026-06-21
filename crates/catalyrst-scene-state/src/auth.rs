//! WebSocket authentication for scene-state-server.
//!
//! Upstream (`src/controllers/handlers/ws-handler.ts`) authenticates a WS
//! connection by waiting for the first frame (`MessageType::Auth`), whose body
//! is the JSON of the signed-fetch `x-identity-*` headers, then calling
//! `verify(method, pathname, headers, { fetcher })` from
//! `@dcl/platform-crypto-middleware`. The signed payload is
//! `GET:/ws/<scene>:<timestamp>:<metadata>` (lowercased) — the standard DCL
//! signed-fetch contract.
//!
//! This module ports that verification using the workspace
//! [`catalyrst_crypto::verify::verify_auth_chain`] primitive. The header map
//! arrives as a JSON object in the Auth frame (not as real HTTP headers), so we
//! parse it into a [`HashMap`] and reconstruct the signed payload ourselves.
//!
//! Matches the signed-fetch logic already used by `catalyrst-comms`
//! (`src/auth_chain.rs`) but keyed off a JSON map instead of an `axum`
//! `HeaderMap`, since the headers come over the socket.

use std::collections::HashMap;

use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::AuthLink;
use thiserror::Error;

pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";

pub const MAX_AUTH_CHAIN_LINKS: usize = 10;
/// Upstream signed-fetch verifier default expiry window.
pub const FIVE_MINUTES: i64 = 5 * 60;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("auth frame is not valid JSON: {0}")]
    BadJson(String),
    #[error("auth chain malformed: {0}")]
    MalformedChain(String),
    #[error("auth chain has fewer than 2 links")]
    InsufficientLinks,
    #[error("signature expired (signed_at={signed_at}, now={now}, window={window_secs}s)")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },
    #[error("signature rejected: {0}")]
    InvalidSignature(String),
}

/// Outcome of a successful auth: the recovered signer address (lowercased).
#[derive(Debug, Clone)]
pub struct Authenticated {
    pub signer: String,
}

/// Verifies the signed-fetch headers carried in an `Auth` frame body.
///
/// * `frame_json` — the raw UTF-8 JSON body of the Auth frame.
/// * `method` — HTTP method the client signed (always `GET` upstream).
/// * `path` — request pathname the client signed, e.g. `/ws/my-world.dcl.eth`.
/// * `now` — current unix seconds (injectable for tests).
pub fn verify_auth_frame(
    frame_json: &[u8],
    method: &str,
    path: &str,
    now: i64,
) -> Result<Authenticated, AuthError> {
    let headers: HashMap<String, String> =
        serde_json::from_slice(frame_json).map_err(|e| AuthError::BadJson(e.to_string()))?;
    let headers: HashMap<String, String> = headers
        .into_iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect();

    let chain = extract_auth_chain(&headers)?;
    let signer = chain
        .first()
        .map(|l| l.payload.to_ascii_lowercase())
        .ok_or(AuthError::InsufficientLinks)?;

    let ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .cloned()
        .unwrap_or_else(|| "0".into());
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .cloned()
        .unwrap_or_else(|| "{}".into());

    let payload = build_payload(method, path, &ts, &metadata);
    validate_signature(&chain, &payload, &ts, FIVE_MINUTES, now)?;

    Ok(Authenticated { signer })
}

/// Reconstructs the signed-fetch payload string. Mirrors `catalyrst-comms`
/// `build_payload`: `method:path:timestamp:metadata`, lowercased.
pub fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{method}:{path}:{timestamp}:{metadata}").to_lowercase()
}

/// Behind a front-host proxy that strips the service prefix, the WS upgrade
/// request carries the original, un-stripped path in `x-original-path` (set by
/// nginx). Prefer it (query-stripped) over the locally-built route path so the
/// reconstructed signed-fetch payload matches what the client signed. Falls back
/// to `fallback` for direct/loopback upgrades (no header).
pub fn signed_fetch_path<'a>(
    headers: &axum::http::HeaderMap,
    fallback: &'a str,
) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn extract_auth_chain(headers: &HashMap<String, String>) -> Result<Vec<AuthLink>, AuthError> {
    let mut links: Vec<AuthLink> = Vec::new();
    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{AUTH_CHAIN_HEADER_PREFIX}{i}");
        let Some(raw) = headers.get(&name) else { break };
        let link: AuthLink =
            serde_json::from_str(raw).map_err(|e| AuthError::MalformedChain(e.to_string()))?;
        links.push(link);
    }
    let overflow = format!("{AUTH_CHAIN_HEADER_PREFIX}{MAX_AUTH_CHAIN_LINKS}");
    if headers.contains_key(&overflow) {
        return Err(AuthError::MalformedChain(format!(
            "exceeds max length of {MAX_AUTH_CHAIN_LINKS}"
        )));
    }
    if links.len() < 2 {
        return Err(AuthError::InsufficientLinks);
    }
    Ok(links)
}

fn validate_signature(
    chain: &[AuthLink],
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<(), AuthError> {
    // Freshness window: the timestamp comes from the authoritative
    // `x-identity-timestamp` header (`timestamp`), NOT the 3rd colon-delimited
    // payload field. Paths containing ':' (URNs) would shift that field and
    // silently skip the freshness check.
    if let Ok(signed_at_ms) = timestamp.parse::<i64>() {
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthError::Expired {
                signed_at,
                now,
                window_secs: expiration_secs,
            });
        }
    }
    verify_auth_chain(&chain.to_vec(), payload, Some(now * 1000))
        .map_err(|e| AuthError::InvalidSignature(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_json() {
        let err = verify_auth_frame(b"not json", "GET", "/ws/x", 0).unwrap_err();
        matches!(err, AuthError::BadJson(_));
    }

    #[test]
    fn rejects_chain_with_one_link() {
        let body = serde_json::json!({
            "x-identity-auth-chain-0":
                "{\"type\":\"SIGNER\",\"payload\":\"0xabc\",\"signature\":\"\"}"
        })
        .to_string();
        let err = verify_auth_frame(body.as_bytes(), "GET", "/ws/x", 0).unwrap_err();
        matches!(err, AuthError::InsufficientLinks);
    }

    #[test]
    fn payload_is_lowercased() {
        assert_eq!(
            build_payload("GET", "/ws/MyWorld.dcl.eth", "123", "{}"),
            "get:/ws/myworld.dcl.eth:123:{}"
        );
    }
}
