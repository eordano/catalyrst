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
    #[serde(rename = "roomList")]
    pub room_list: bool,

    #[serde(rename = "canPublishSources", skip_serializing_if = "Option::is_none")]
    pub can_publish_sources: Option<Vec<String>>,
}

impl VideoGrants {
    pub fn join(room: impl Into<String>) -> Self {
        Self {
            room_join: true,
            room: room.into(),
            can_publish: true,
            can_subscribe: true,
            can_publish_data: true,
            can_update_own_metadata: true,
            room_list: false,
            can_publish_sources: None,
        }
    }
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

        let header = serde_json::json!({ "alg": "HS256", "typ": "JWT" });
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

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
        let payload_b64 =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&serde_json::Value::Object(payload))?);
        let signing_input = format!("{}.{}", header_b64, payload_b64);

        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .map_err(|e| LivekitError::HmacKey(e.to_string()))?;
        mac.update(signing_input.as_bytes());
        let sig = mac.finalize().into_bytes();
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig);

        Ok(format!("{}.{}", signing_input, sig_b64))
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
    let header = serde_json::json!({ "alg": "HS256", "typ": "JWT" });
    let payload = serde_json::json!({
        "exp": exp,
        "iss": api_key,
        "sub": api_key,
        "nbf": now,
        "video": { "roomList": true, "roomAdmin": true, "room": room },
    });
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?);
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes())
        .map_err(|e| LivekitError::HmacKey(e.to_string()))?;
    mac.update(signing_input.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
    Ok(format!("{}.{}", signing_input, sig_b64))
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
