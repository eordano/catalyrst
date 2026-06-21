use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac, KeyInit};
use serde::Serialize;
use sha2::Sha256;
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
        }
    }
}

pub struct AccessToken {
    pub api_key: String,
    pub api_secret: String,
    pub identity: String,
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
            grants,
            ttl: Duration::from_secs(5 * 60),
        }
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

pub const WORLD_ROOM_PREFIX: &str = "world-";
pub const SCENE_ROOM_PREFIX: &str = "scene-";

pub fn world_room_name(world: &str) -> String {
    format!("{}{}", WORLD_ROOM_PREFIX, world.to_lowercase())
}

pub fn world_scene_room_name(world: &str, scene_id: &str) -> String {
    format!(
        "{}{}-{}",
        SCENE_ROOM_PREFIX,
        world.to_lowercase(),
        scene_id.to_lowercase()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_has_three_parts() {
        let tok = AccessToken::new("k", "s", "0xabc", VideoGrants::join("world-foo.eth"))
            .to_jwt()
            .unwrap();
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn room_names_match_upstream() {
        assert_eq!(world_room_name("Foo.eth"), "world-foo.eth");
        assert_eq!(world_scene_room_name("Foo.eth", "ABC"), "scene-foo.eth-abc");
    }

    #[test]
    fn adapter_prefixes_wss() {
        assert!(build_adapter_url("lk.example.com", "t")
            .starts_with("livekit:wss://lk.example.com?access_token=t"));
    }
}
