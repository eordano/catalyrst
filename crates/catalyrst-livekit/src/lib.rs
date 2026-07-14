//! Shared LiveKit token primitives: the HS256 JWT signing core, the video-grant
//! and access-token shapes, admin tokens, and webhook verification.
//!
//! Policy decisions (which grants a joiner gets, room naming) stay in the
//! service crates; this crate owns only the mechanics, and callers that build
//! their own claim structs serialize them themselves so the signed payload
//! bytes are exactly what they produced.

pub mod sfu_health;

pub use sfu_health::{probe_target, SfuHealth};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum LivekitError {
    #[error("hmac key error: {0}")]
    HmacKey(String),
    #[error("clock skew before unix epoch")]
    Clock,
    #[error("json encode: {0}")]
    Json(#[from] serde_json::Error),
}

pub const TRACK_SOURCE_MICROPHONE: &str = "MICROPHONE";

/// The upstream LiveKit dev-image placeholder credentials. A deployment whose
/// env still carries these has no working SFU auth -- tokens minted against
/// them parse locally but are rejected by any real LiveKit cluster -- so
/// credential resolution treats them exactly like an unset variable.
pub fn is_placeholder_cred(value: &str) -> bool {
    let v = value.trim();
    v.eq_ignore_ascii_case("devkey") || v.eq_ignore_ascii_case("devsecret")
}

/// Outcome of resolving `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` against the
/// `LIVEKIT_ALLOW_DEV_CREDS` opt-in. Unset, blank, and placeholder
/// (`devkey`/`devsecret`, any case) credentials all count as *unconfigured*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCreds {
    /// Real credentials: mint against them, `livekit_configured = true`.
    Configured { api_key: String, api_secret: String },
    /// No real credentials but the dev opt-in is set: run on
    /// `devkey`/`devsecret` with `livekit_configured = false`; the caller
    /// should warn that no real SFU will accept the minted tokens.
    DevFallback,
    /// No real credentials and no opt-in: the service must refuse to boot.
    Unconfigured,
}

pub const DEV_API_KEY: &str = "devkey";
pub const DEV_API_SECRET: &str = "devsecret";

/// Shared `livekit_configured` policy for the service configs: both values
/// must be present, non-blank, and not the dev placeholders before LiveKit
/// counts as configured.
pub fn resolve_creds(api_key: String, api_secret: String, allow_dev_creds: bool) -> ResolvedCreds {
    let real = |v: &str| !v.trim().is_empty() && !is_placeholder_cred(v);
    if real(&api_key) && real(&api_secret) {
        ResolvedCreds::Configured {
            api_key,
            api_secret,
        }
    } else if allow_dev_creds {
        ResolvedCreds::DevFallback
    } else {
        ResolvedCreds::Unconfigured
    }
}

/// Signs pre-serialized JSON header/payload bytes as an HS256 JWT.
pub fn sign_hs256(
    api_secret: &str,
    header_json: &[u8],
    payload_json: &[u8],
) -> Result<String, LivekitError> {
    let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes())
        .map_err(|e| LivekitError::HmacKey(e.to_string()))?;
    mac.update(signing_input.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);

    Ok(format!("{}.{}", signing_input, sig_b64))
}

fn jwt_header() -> serde_json::Value {
    serde_json::json!({ "alg": "HS256", "typ": "JWT" })
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoGrants {
    #[serde(rename = "roomJoin")]
    pub room_join: bool,
    pub room: String,
    #[serde(rename = "canPublish")]
    pub can_publish: bool,
    #[serde(rename = "canSubscribe")]
    pub can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    pub can_publish_data: bool,
    #[serde(rename = "canUpdateOwnMetadata")]
    pub can_update_own_metadata: bool,

    /// `None` omits the claim entirely (worlds' grant shape); `Some(v)`
    /// serializes it (comms' grant shape).
    #[serde(rename = "roomList", skip_serializing_if = "Option::is_none")]
    pub room_list: Option<bool>,

    #[serde(rename = "canPublishSources", skip_serializing_if = "Option::is_none")]
    pub can_publish_sources: Option<Vec<String>>,
}

pub struct AccessToken {
    pub api_key: String,
    pub api_secret: String,
    pub identity: String,
    pub name: Option<String>,
    pub metadata: Option<String>,
    pub grants: VideoGrants,
    pub ttl: Duration,
}

impl AccessToken {
    pub fn new(
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        identity: impl Into<String>,
        grants: VideoGrants,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            identity: identity.into(),
            name: None,
            metadata: None,
            grants,
            ttl: Duration::from_secs(5 * 60),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn to_jwt(&self) -> Result<String, LivekitError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| LivekitError::Clock)?
            .as_secs();
        let exp = now + self.ttl.as_secs();

        let mut payload = serde_json::Map::new();
        payload.insert("exp".into(), serde_json::json!(exp));
        payload.insert("iss".into(), serde_json::json!(self.api_key));
        payload.insert("sub".into(), serde_json::json!(self.identity));
        payload.insert("nbf".into(), serde_json::json!(now));
        if let Some(n) = &self.name {
            payload.insert("name".into(), serde_json::json!(n));
        }
        if let Some(m) = &self.metadata {
            payload.insert("metadata".into(), serde_json::json!(m));
        }
        payload.insert("video".into(), serde_json::to_value(&self.grants)?);

        sign_hs256(
            &self.api_secret,
            &serde_json::to_vec(&jwt_header())?,
            &serde_json::to_vec(&serde_json::Value::Object(payload))?,
        )
    }
}

pub fn room_admin_token(
    api_key: &str,
    api_secret: &str,
    room: &str,
) -> Result<String, LivekitError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LivekitError::Clock)?
        .as_secs();
    let exp = now + 60;
    let payload = serde_json::json!({
        "exp": exp,
        "iss": api_key,
        "sub": api_key,
        "nbf": now,
        "video": { "roomList": true, "roomAdmin": true, "room": room },
    });
    sign_hs256(
        api_secret,
        &serde_json::to_vec(&jwt_header())?,
        &serde_json::to_vec(&payload)?,
    )
}

pub fn ingress_admin_token(api_key: &str, api_secret: &str) -> Result<String, LivekitError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LivekitError::Clock)?
        .as_secs();
    let exp = now + 60;
    let payload = serde_json::json!({
        "exp": exp,
        "iss": api_key,
        "sub": api_key,
        "nbf": now,
        "video": { "ingressAdmin": true, "roomList": true },
    });
    sign_hs256(
        api_secret,
        &serde_json::to_vec(&jwt_header())?,
        &serde_json::to_vec(&payload)?,
    )
}

pub fn build_adapter_url(host: &str, token: &str) -> String {
    let host = if host.starts_with("wss://") || host.starts_with("ws://") {
        host.to_string()
    } else {
        format!("wss://{}", host)
    };
    format!("livekit:{}?access_token={}", host, token)
}

fn json_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().map(|f| f as u64))
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn decode_body_digest(claim: &str) -> Option<Vec<u8>> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE};
    STANDARD
        .decode(claim)
        .or_else(|_| STANDARD_NO_PAD.decode(claim))
        .or_else(|_| URL_SAFE.decode(claim))
        .or_else(|_| URL_SAFE_NO_PAD.decode(claim))
        .ok()
        .filter(|b| b.len() == 32)
}

pub fn verify_webhook_token(
    api_key: &str,
    api_secret: &str,
    body: &[u8],
    auth_header: &str,
) -> bool {
    let trimmed = auth_header.trim();
    let token = trimmed.strip_prefix("Bearer ").unwrap_or(trimmed).trim();

    let mut segments = token.split('.');
    let (Some(header_b64), Some(payload_b64), Some(sig_b64), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };

    let Ok(header_bytes) = URL_SAFE_NO_PAD.decode(header_b64) else {
        return false;
    };
    let Ok(header) = serde_json::from_slice::<serde_json::Value>(&header_bytes) else {
        return false;
    };
    if header.get("alg").and_then(|a| a.as_str()) != Some("HS256") {
        return false;
    }

    let Ok(provided_sig) = URL_SAFE_NO_PAD.decode(sig_b64) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(api_secret.as_bytes()) else {
        return false;
    };
    mac.update(header_b64.as_bytes());
    mac.update(b".");
    mac.update(payload_b64.as_bytes());
    if mac.verify_slice(&provided_sig).is_err() {
        return false;
    }

    let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload_b64) else {
        return false;
    };
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&payload_bytes) else {
        return false;
    };

    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return false;
    };
    let now = now.as_secs();
    const LEEWAY: u64 = 60;

    let Some(exp) = payload.get("exp").and_then(json_u64) else {
        return false;
    };
    if now > exp.saturating_add(LEEWAY) {
        return false;
    }
    if let Some(nbf) = payload.get("nbf").and_then(json_u64) {
        if nbf > now.saturating_add(LEEWAY) {
            return false;
        }
    }
    if payload.get("iss").and_then(|v| v.as_str()) != Some(api_key) {
        return false;
    }

    let Some(claim) = payload.get("sha256").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(expected) = decode_body_digest(claim) else {
        return false;
    };

    let actual = Sha256::digest(body);
    ct_eq(&expected, &actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_payload(jwt: &str) -> serde_json::Value {
        let payload_b64 = jwt.split('.').nth(1).unwrap();
        let bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn grants(room: &str) -> VideoGrants {
        VideoGrants {
            room_join: true,
            room: room.into(),
            can_publish: true,
            can_subscribe: true,
            can_publish_data: true,
            can_update_own_metadata: false,
            room_list: None,
            can_publish_sources: None,
        }
    }

    #[test]
    fn jwt_has_three_parts() {
        let tok = AccessToken::new("k", "s", "0xabc", grants("world-foo.eth"))
            .to_jwt()
            .unwrap();
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn omitted_room_list_stays_out_of_the_grant_claim() {
        let jwt = AccessToken::new("k", "s", "0xabc", grants("r"))
            .to_jwt()
            .unwrap();
        let payload = decode_payload(&jwt);
        assert!(payload["video"].get("roomList").is_none());
        assert!(payload["video"].get("canPublishSources").is_none());
    }

    #[test]
    fn present_room_list_serializes_in_the_grant_claim() {
        let mut g = grants("r");
        g.room_list = Some(false);
        let jwt = AccessToken::new("k", "s", "0xabc", g).to_jwt().unwrap();
        let payload = decode_payload(&jwt);
        assert_eq!(payload["video"]["roomList"], false);
    }

    #[test]
    fn placeholder_creds_are_detected_case_insensitively() {
        for v in ["devkey", "devsecret", "DEVKEY", "DevSecret", " devkey "] {
            assert!(is_placeholder_cred(v), "{v:?} must count as a placeholder");
        }
        for v in ["", "APIabc", "devkey2", "supersecret", "prod-devkey"] {
            assert!(!is_placeholder_cred(v), "{v:?} must not be a placeholder");
        }
    }

    #[test]
    fn real_creds_resolve_configured() {
        assert_eq!(
            resolve_creds("APIabc".into(), "supersecret".into(), false),
            ResolvedCreds::Configured {
                api_key: "APIabc".into(),
                api_secret: "supersecret".into(),
            }
        );
    }

    #[test]
    fn placeholder_creds_count_as_unset() {
        // The literal dev pair, any casing, and a placeholder on either side
        // alone must all fail to count as configured.
        for (k, s) in [
            ("devkey", "devsecret"),
            ("DEVKEY", "DEVSECRET"),
            ("devkey", "supersecret"),
            ("APIabc", "devsecret"),
            ("", ""),
            ("   ", "   "),
            ("APIabc", ""),
        ] {
            assert_eq!(
                resolve_creds(k.into(), s.into(), false),
                ResolvedCreds::Unconfigured,
                "({k:?}, {s:?}) without the opt-in must be Unconfigured"
            );
            assert_eq!(
                resolve_creds(k.into(), s.into(), true),
                ResolvedCreds::DevFallback,
                "({k:?}, {s:?}) with the opt-in must be DevFallback"
            );
        }
    }

    #[test]
    fn signature_matches_recomputed_hmac() {
        let tok = AccessToken::new("k", "secret", "0xabc", grants("r"))
            .to_jwt()
            .unwrap();
        let parts: Vec<&str> = tok.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
        mac.update(signing_input.as_bytes());
        let want = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        assert_eq!(parts[2], want);
    }
}
