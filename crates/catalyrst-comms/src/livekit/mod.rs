use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
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

pub const WORLD_ROOM_PREFIX: &str = "world-";
pub const PRIVATE_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-private-";
pub const COMMUNITY_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-community";

pub fn private_voice_chat_room_name(room_id: &str) -> String {
    format!("{}{}", PRIVATE_VOICE_CHAT_ROOM_PREFIX, room_id)
}

pub fn is_private_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(PRIVATE_VOICE_CHAT_ROOM_PREFIX)
}

pub fn is_community_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
}

pub fn community_voice_chat_room_name(community_id: &str) -> String {
    format!("{}-{}", COMMUNITY_VOICE_CHAT_ROOM_PREFIX, community_id)
}

pub fn community_id_from_room_name(room_name: &str) -> String {
    room_name
        .strip_prefix(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
        .unwrap_or(room_name)
        .to_string()
}

pub fn scene_room_name(scene_id: &str) -> String {
    format!("scene:{scene_id}")
}

pub fn world_scene_room_name(world: &str, scene_id: &str) -> String {
    format!("{}{}-{}", WORLD_ROOM_PREFIX, world, scene_id)
}

pub fn world_room_name(world: &str) -> String {
    format!("{}{}", WORLD_ROOM_PREFIX, world)
}

pub fn build_adapter_url(host: &str, token: &str) -> String {
    let host = if host.starts_with("wss://") || host.starts_with("ws://") {
        host.to_string()
    } else {
        format!("wss://{}", host)
    };
    format!("livekit:{}?access_token={}", host, token)
}

pub fn address_from_identity(identity: &str) -> Option<String> {
    let lower = identity.to_lowercase();
    let candidate: String = lower.chars().take(42).collect();
    if candidate.len() == 42
        && candidate.starts_with("0x")
        && candidate[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        Some(candidate)
    } else {
        None
    }
}

pub fn room_service_base(host: &str) -> String {
    let insecure = host.starts_with("ws://") || host.starts_with("http://");
    let trimmed = host
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let scheme = if insecure { "http" } else { "https" };
    format!("{scheme}://{trimmed}")
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

pub async fn list_room_participant_identities(
    client: &reqwest::Client,
    host: &str,
    api_key: &str,
    api_secret: &str,
    room: &str,
) -> Vec<String> {
    let token = match room_admin_token(api_key, api_secret, room) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "failed to mint room-admin token for participants list");
            return Vec::new();
        }
    };
    let url = format!(
        "{}/twirp/livekit.RoomService/ListParticipants",
        room_service_base(host)
    );
    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({ "room": room }))
        .send()
        .await;
    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::debug!(status = %r.status(), room, "ListParticipants non-success");
            return Vec::new();
        }
        Err(e) => {
            tracing::debug!(error = %e, room, "ListParticipants request failed");
            return Vec::new();
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    body.get("participants")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("identity").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    pub identity: String,
    pub name: Option<String>,
    pub state: i64,
    pub metadata: Option<String>,
    pub is_publisher: bool,
}

#[derive(Debug, Error)]
pub enum RoomServiceError {
    #[error("livekit not configured")]
    NotConfigured,
    #[error("token mint failed: {0}")]
    Token(#[from] LivekitError),
    #[error("livekit request failed: {0}")]
    Request(String),
    #[error("livekit returned status {0}")]
    Status(u16),
    #[error("livekit room not found")]
    NotFound,
}

pub const BANNED_ADDRESSES_FIELD: &str = "bannedAddresses";
pub const SCENE_ADMINS_FIELD: &str = "sceneAdmins";

pub fn parse_room_metadata(
    metadata_str: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    match metadata_str.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
        Some(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    }
}

pub fn metadata_with_appended(
    mut metadata: serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut arr: Vec<serde_json::Value> = match metadata.get(field) {
        Some(serde_json::Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    if arr.iter().any(|v| v.as_str() == Some(value)) {
        return None;
    }
    arr.push(serde_json::Value::String(value.to_string()));
    metadata.insert(field.to_string(), serde_json::Value::Array(arr));
    Some(metadata)
}

pub fn metadata_with_removed(
    mut metadata: serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let arr: Vec<serde_json::Value> = match metadata.get(field) {
        Some(serde_json::Value::Array(a)) => a.clone(),
        _ => return None,
    };
    let filtered: Vec<serde_json::Value> = arr
        .iter()
        .filter(|v| v.as_str() != Some(value))
        .cloned()
        .collect();
    if filtered.len() == arr.len() {
        return None;
    }
    metadata.insert(field.to_string(), serde_json::Value::Array(filtered));
    Some(metadata)
}

fn room_metadata_write_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub struct RoomServiceClient<'a> {
    pub http: &'a reqwest::Client,
    pub host: String,
    pub api_key: String,
    pub api_secret: String,
}

impl<'a> RoomServiceClient<'a> {
    pub fn new(http: &'a reqwest::Client, host: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            http,
            host: host.to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!(
            "{}/twirp/livekit.RoomService/{}",
            room_service_base(&self.host),
            method
        )
    }

    fn admin_token(&self, room: &str) -> Result<String, RoomServiceError> {
        room_admin_token(&self.api_key, &self.api_secret, room).map_err(RoomServiceError::Token)
    }

    async fn call(
        &self,
        method: &str,
        room: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, RoomServiceError> {
        let token = self.admin_token(room)?;
        let resp = self
            .http
            .post(self.endpoint(method))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| RoomServiceError::Request(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Err(RoomServiceError::NotFound);
        }
        if !status.is_success() {
            let txt = resp.text().await.unwrap_or_default();
            if txt.contains("not_found") || txt.contains("does not exist") {
                return Err(RoomServiceError::NotFound);
            }
            return Err(RoomServiceError::Status(status.as_u16()));
        }
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| RoomServiceError::Request(e.to_string()))
    }

    pub async fn delete_room(&self, room: &str) -> Result<(), RoomServiceError> {
        match self
            .call("DeleteRoom", room, serde_json::json!({ "room": room }))
            .await
        {
            Ok(_) | Err(RoomServiceError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
    ) -> Result<(), RoomServiceError> {
        match self
            .call(
                "RemoveParticipant",
                room,
                serde_json::json!({ "room": room, "identity": identity }),
            )
            .await
        {
            Ok(_) | Err(RoomServiceError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn remove_participant_from_all_rooms(
        &self,
        identity: &str,
    ) -> Result<(), RoomServiceError> {
        let rooms = self.list_rooms().await?;
        let lower = identity.to_lowercase();
        for room in rooms {
            let participants = match self.list_participants(&room).await {
                Ok(p) => p,
                Err(RoomServiceError::NotFound) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, room = %room, identity, "failed to list participants for room");
                    continue;
                }
            };
            let Some(p) = participants
                .into_iter()
                .find(|p| p.identity.to_lowercase() == lower)
            else {
                continue;
            };
            if let Err(e) = self.remove_participant(&room, &p.identity).await {
                tracing::warn!(error = %e, room = %room, identity = %p.identity, "failed to remove participant from room");
            }
        }
        Ok(())
    }

    pub async fn list_participants(
        &self,
        room: &str,
    ) -> Result<Vec<ParticipantInfo>, RoomServiceError> {
        let body = self
            .call(
                "ListParticipants",
                room,
                serde_json::json!({ "room": room }),
            )
            .await?;
        let arr = body
            .get("participants")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr.iter().map(parse_participant).collect())
    }

    pub async fn update_participant(
        &self,
        room: &str,
        identity: &str,
        metadata: Option<&str>,
        permission: Option<serde_json::Value>,
    ) -> Result<(), RoomServiceError> {
        let mut body = serde_json::Map::new();
        body.insert("room".into(), serde_json::json!(room));
        body.insert("identity".into(), serde_json::json!(identity));
        if let Some(m) = metadata {
            body.insert("metadata".into(), serde_json::json!(m));
        }
        if let Some(p) = permission {
            body.insert("permission".into(), p);
        }
        match self
            .call("UpdateParticipant", room, serde_json::Value::Object(body))
            .await
        {
            Ok(_) | Err(RoomServiceError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn merge_participant_metadata(
        &self,
        room: &str,
        identity: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), RoomServiceError> {
        let existing = self
            .list_participants(room)
            .await
            .ok()
            .and_then(|parts| {
                let target = identity.to_lowercase();
                parts
                    .into_iter()
                    .find(|p| p.identity.to_lowercase() == target)
            })
            .and_then(|p| p.metadata);
        let mut merged: serde_json::Map<String, serde_json::Value> = existing
            .as_deref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or_default();
        for (k, v) in patch {
            merged.insert(k, v);
        }
        let metadata = serde_json::Value::Object(merged).to_string();
        self.update_participant(room, identity, Some(&metadata), None)
            .await
    }

    pub async fn list_rooms(&self) -> Result<Vec<String>, RoomServiceError> {
        let body = self.call("ListRooms", "", serde_json::json!({})).await?;
        Ok(body
            .get("rooms")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn get_room_info(
        &self,
        room: &str,
    ) -> Result<Option<serde_json::Value>, RoomServiceError> {
        let body = self
            .call("ListRooms", room, serde_json::json!({ "names": [room] }))
            .await?;
        Ok(body
            .get("rooms")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first().cloned()))
    }

    async fn write_room_metadata(
        &self,
        room: &str,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), RoomServiceError> {
        let metadata_str = serde_json::Value::Object(metadata).to_string();
        match self
            .call(
                "UpdateRoomMetadata",
                room,
                serde_json::json!({ "room": room, "metadata": metadata_str }),
            )
            .await
        {
            Ok(_) | Err(RoomServiceError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn update_room_metadata(
        &self,
        room: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), RoomServiceError> {
        let _guard = room_metadata_write_lock().lock().await;
        let Some(info) = self.get_room_info(room).await? else {
            return Ok(());
        };
        let mut metadata = parse_room_metadata(info.get("metadata").and_then(|m| m.as_str()));
        for (k, v) in patch {
            metadata.insert(k, v);
        }
        self.write_room_metadata(room, metadata).await
    }

    pub async fn append_to_room_metadata_array(
        &self,
        room: &str,
        field: &str,
        value: &str,
    ) -> Result<(), RoomServiceError> {
        let _guard = room_metadata_write_lock().lock().await;
        let Some(info) = self.get_room_info(room).await? else {
            return Ok(());
        };
        let metadata = parse_room_metadata(info.get("metadata").and_then(|m| m.as_str()));
        if let Some(updated) = metadata_with_appended(metadata, field, value) {
            self.write_room_metadata(room, updated).await?;
        }
        Ok(())
    }

    pub async fn remove_from_room_metadata_array(
        &self,
        room: &str,
        field: &str,
        value: &str,
    ) -> Result<(), RoomServiceError> {
        let _guard = room_metadata_write_lock().lock().await;
        let Some(info) = self.get_room_info(room).await? else {
            return Ok(());
        };
        let metadata = parse_room_metadata(info.get("metadata").and_then(|m| m.as_str()));
        if let Some(updated) = metadata_with_removed(metadata, field, value) {
            self.write_room_metadata(room, updated).await?;
        }
        Ok(())
    }
}

fn parse_participant(p: &serde_json::Value) -> ParticipantInfo {
    let permission_publish = p
        .get("permission")
        .and_then(|perm| perm.get("canPublish"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_tracks = p
        .get("tracks")
        .and_then(|t| t.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    ParticipantInfo {
        identity: p
            .get("identity")
            .and_then(|i| i.as_str())
            .unwrap_or_default()
            .to_string(),
        name: p.get("name").and_then(|n| n.as_str()).map(String::from),
        state: p.get("state").and_then(|s| s.as_i64()).unwrap_or(0),
        metadata: p.get("metadata").and_then(|m| m.as_str()).map(String::from),
        is_publisher: permission_publish || has_tracks,
    }
}

pub fn verify_webhook_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let parts: std::collections::HashMap<&str, &str> = header
        .split(',')
        .filter_map(|kv| {
            let mut it = kv.splitn(2, '=');
            Some((it.next()?.trim(), it.next()?.trim()))
        })
        .collect();
    let Some(sig_hex) = parts.get("v1").or_else(|| parts.get("sha256")) else {
        return false;
    };
    let Ok(provided) = hex::decode(sig_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&provided).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_has_three_dot_parts() {
        let tok = AccessToken::new("devkey", "devsecret", "0xabc", VideoGrants::join("room1"))
            .to_jwt()
            .unwrap();
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn adapter_url_prefixes_wss() {
        let url = build_adapter_url("livekit.example.com", "tok");
        assert!(url.starts_with("livekit:wss://livekit.example.com?access_token=tok"));
    }

    #[test]
    fn room_names_match_upstream() {
        assert_eq!(scene_room_name("abc"), "scene:abc");

        assert_eq!(world_scene_room_name("foo.eth", "xyz"), "world-foo.eth-xyz");
        assert_eq!(world_room_name("foo.eth"), "world-foo.eth");
    }

    #[test]
    fn community_room_name_round_trips() {
        let name = community_voice_chat_room_name("abc-123");
        assert_eq!(name, "voice-chat-community-abc-123");
        assert!(is_community_voice_chat_room(&name));

        assert_eq!(community_id_from_room_name(&name), "abc-123");
    }

    #[test]
    fn address_from_identity_truncates_and_validates() {
        let addr = "0x1234567890abcdef1234567890abcdef12345678";
        assert_eq!(address_from_identity(addr).as_deref(), Some(addr));

        assert_eq!(
            address_from_identity(&format!("{addr}:session")).as_deref(),
            Some(addr)
        );

        assert_eq!(address_from_identity("authoritative-server"), None);
    }

    #[test]
    fn room_service_base_maps_scheme_like_livekit_sdk() {
        assert_eq!(
            room_service_base("wss://livekit.example.com"),
            "https://livekit.example.com"
        );
        assert_eq!(
            room_service_base("livekit.example.com/"),
            "https://livekit.example.com"
        );
        assert_eq!(
            room_service_base("https://livekit.example.com"),
            "https://livekit.example.com"
        );

        assert_eq!(
            room_service_base("ws://127.0.0.1:7880"),
            "http://127.0.0.1:7880"
        );
        assert_eq!(
            room_service_base("http://127.0.0.1:7880"),
            "http://127.0.0.1:7880"
        );
    }

    fn decode_jwt_payload(jwt: &str) -> serde_json::Value {
        let payload_b64 = jwt.split('.').nth(1).expect("jwt payload segment");
        let bytes = URL_SAFE_NO_PAD.decode(payload_b64).expect("base64url");
        serde_json::from_slice(&bytes).expect("payload json")
    }

    #[test]
    fn private_voice_grants_match_upstream_generate_credentials() {
        let mut grants = VideoGrants::join("voice-chat-private-call1");
        grants.can_publish = true;
        grants.can_subscribe = true;
        grants.can_update_own_metadata = false;
        grants.can_publish_sources = Some(vec![TRACK_SOURCE_MICROPHONE.to_string()]);

        let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
            .to_jwt()
            .unwrap();
        let p = decode_jwt_payload(&jwt);
        assert_eq!(p["iss"], "devkey");
        assert_eq!(p["sub"], "0xabc");
        let v = &p["video"];
        assert_eq!(v["roomJoin"], true);
        assert_eq!(v["room"], "voice-chat-private-call1");
        assert_eq!(v["canPublish"], true);
        assert_eq!(v["canSubscribe"], true);
        assert_eq!(v["canPublishData"], true);
        assert_eq!(v["canUpdateOwnMetadata"], false);
        assert_eq!(v["canPublishSources"], serde_json::json!(["MICROPHONE"]));

        assert!(p["exp"].as_i64().unwrap() > p["nbf"].as_i64().unwrap());
    }

    #[test]
    fn community_speaker_grant_omits_publish_sources_restriction() {
        let mut grants = VideoGrants::join("voice-chat-community-c1");
        grants.can_publish = true;
        grants.can_update_own_metadata = false;
        let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
            .with_metadata(r#"{"role":"owner","isSpeaker":true,"muted":false}"#)
            .to_jwt()
            .unwrap();
        let p = decode_jwt_payload(&jwt);
        let v = &p["video"];
        assert_eq!(v["canPublish"], true);
        assert!(v.get("canPublishSources").is_none());
        assert_eq!(
            p["metadata"],
            r#"{"role":"owner","isSpeaker":true,"muted":false}"#
        );
    }

    #[test]
    fn community_listener_cannot_publish() {
        let mut grants = VideoGrants::join("voice-chat-community-c1");
        grants.can_publish = false;
        grants.can_subscribe = true;
        let jwt = AccessToken::new("devkey", "devsecret", "0xabc", grants)
            .to_jwt()
            .unwrap();
        let v = decode_jwt_payload(&jwt)["video"].clone();
        assert_eq!(v["canPublish"], false);
        assert_eq!(v["canSubscribe"], true);
    }

    #[test]
    fn room_admin_token_grants_room_admin_and_list() {
        let jwt = room_admin_token("devkey", "devsecret", "some-room").unwrap();
        let p = decode_jwt_payload(&jwt);
        assert_eq!(p["iss"], "devkey");
        assert_eq!(p["sub"], "devkey");
        let v = &p["video"];
        assert_eq!(v["roomAdmin"], true);
        assert_eq!(v["roomList"], true);
        assert_eq!(v["room"], "some-room");

        assert_eq!(p["exp"].as_i64().unwrap() - p["nbf"].as_i64().unwrap(), 60);
    }

    struct Captured {
        line: String,
        auth: String,
        body: serde_json::Value,
    }

    async fn capture_once(
        resp_status: &'static str,
        resp_body: &'static str,
    ) -> (String, tokio::sync::oneshot::Receiver<Captured>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let mut total = 0;

            loop {
                let n = sock.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
                let text = String::from_utf8_lossy(&buf[..total]);
                if let Some(hdr_end) = text.find("\r\n\r\n") {
                    let header_part = &text[..hdr_end];
                    let content_len = header_part
                        .lines()
                        .find_map(|l| {
                            let l = l.to_ascii_lowercase();
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if total >= hdr_end + 4 + content_len {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&buf[..total]).to_string();
            let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
            let line = head.lines().next().unwrap_or("").to_string();
            let auth = head
                .lines()
                .find_map(|l| {
                    let ll = l.to_ascii_lowercase();
                    ll.strip_prefix("authorization:").map(|_| {
                        l.split_once(':')
                            .map(|x| x.1)
                            .unwrap_or("")
                            .trim()
                            .to_string()
                    })
                })
                .unwrap_or_default();
            let body_json: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            let resp = format!(
                "HTTP/1.1 {resp_status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{resp_body}",
                resp_body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
            let _ = tx.send(Captured {
                line,
                auth,
                body: body_json,
            });
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn delete_room_posts_twirp_deleteroom_with_admin_token() {
        let (host, rx) = capture_once("200 OK", "{}").await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client.delete_room("voice-chat-private-c1").await.unwrap();
        let cap = rx.await.unwrap();
        assert_eq!(
            cap.line,
            "POST /twirp/livekit.RoomService/DeleteRoom HTTP/1.1"
        );
        assert_eq!(
            cap.body,
            serde_json::json!({ "room": "voice-chat-private-c1" })
        );

        let bearer = cap.auth.strip_prefix("Bearer ").expect("bearer prefix");
        let claims = decode_jwt_payload(bearer);
        assert_eq!(claims["video"]["roomAdmin"], true);
        assert_eq!(claims["video"]["room"], "voice-chat-private-c1");
    }

    #[tokio::test]
    async fn delete_room_treats_404_as_success() {
        let (host, _rx) = capture_once("404 Not Found", "not_found").await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");

        client.delete_room("gone").await.unwrap();
    }

    #[tokio::test]
    async fn remove_participant_posts_room_and_identity() {
        let (host, rx) = capture_once("200 OK", "{}").await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client
            .remove_participant("voice-chat-community-c1", "0xabc")
            .await
            .unwrap();
        let cap = rx.await.unwrap();
        assert_eq!(
            cap.line,
            "POST /twirp/livekit.RoomService/RemoveParticipant HTTP/1.1"
        );
        assert_eq!(
            cap.body,
            serde_json::json!({ "room": "voice-chat-community-c1", "identity": "0xabc" })
        );
    }

    #[tokio::test]
    async fn update_participant_sends_permission_block() {
        let (host, rx) = capture_once("200 OK", "{}").await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client
            .update_participant(
                "voice-chat-community-c1",
                "0xabc",
                None,
                Some(serde_json::json!({
                    "canPublish": true,
                    "canSubscribe": true,
                    "canPublishData": true,
                })),
            )
            .await
            .unwrap();
        let cap = rx.await.unwrap();
        assert_eq!(
            cap.line,
            "POST /twirp/livekit.RoomService/UpdateParticipant HTTP/1.1"
        );
        assert_eq!(cap.body["room"], "voice-chat-community-c1");
        assert_eq!(cap.body["identity"], "0xabc");
        assert_eq!(cap.body["permission"]["canPublish"], true);

        assert!(cap.body.get("metadata").is_none());
    }

    async fn read_one_request<S>(sock: &mut S) -> Option<(String, serde_json::Value)>
    where
        S: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;
        let mut acc: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = sock.read(&mut chunk).await.ok()?;
            if n == 0 {
                if acc.is_empty() {
                    return None;
                }
                break;
            }
            acc.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&acc);
            if let Some(hdr_end) = text.find("\r\n\r\n") {
                let content_len = text[..hdr_end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                if acc.len() >= hdr_end + 4 + content_len {
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&acc).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let line = head.lines().next().unwrap_or("").to_string();
        let body_json: serde_json::Value =
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        Some((line, body_json))
    }

    async fn capture_seq(
        responses: Vec<&'static str>,
    ) -> (String, tokio::sync::oneshot::Receiver<Vec<Captured>>) {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut captured = Vec::new();
            let (mut sock, _) = listener.accept().await.unwrap();
            for resp_body in responses {
                let (line, body_json) = loop {
                    match read_one_request(&mut sock).await {
                        Some(req) => break req,

                        None => {
                            let (s, _) = listener.accept().await.unwrap();
                            sock = s;
                        }
                    }
                };
                captured.push(Captured {
                    line,
                    auth: String::new(),
                    body: body_json,
                });
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{resp_body}",
                    resp_body.len()
                );
                sock.write_all(resp.as_bytes()).await.unwrap();
                sock.flush().await.unwrap();
            }
            let _ = tx.send(captured);
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn merge_metadata_read_modify_writes_merged_blob() {
        let (host, rx) = capture_seq(vec![
            r#"{"participants":[{"identity":"0xabc","metadata":"{\"role\":\"owner\",\"muted\":false}"}]}"#,
            "{}",
        ])
        .await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        let mut patch = serde_json::Map::new();
        patch.insert("muted".into(), serde_json::json!(true));
        client
            .merge_participant_metadata("voice-chat-community-c1", "0xabc", patch)
            .await
            .unwrap();
        let caps = rx.await.unwrap();
        assert_eq!(caps.len(), 2);
        assert!(caps[0].line.contains("ListParticipants"));
        assert!(caps[1].line.contains("UpdateParticipant"));

        let written: serde_json::Value =
            serde_json::from_str(caps[1].body["metadata"].as_str().unwrap()).unwrap();
        assert_eq!(written["role"], "owner");
        assert_eq!(written["muted"], true);
    }

    #[test]
    fn merge_metadata_math_shallow_merges_over_existing() {
        let existing = r#"{"role":"owner","muted":false,"isSpeaker":true}"#;
        let mut merged: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(existing).unwrap();
        let mut patch = serde_json::Map::new();
        patch.insert("muted".into(), serde_json::json!(true));
        for (k, v) in patch {
            merged.insert(k, v);
        }
        assert_eq!(merged["muted"], true);
        assert_eq!(merged["role"], "owner");
        assert_eq!(merged["isSpeaker"], true);
    }

    #[tokio::test]
    async fn list_rooms_parses_names() {
        let (host, _rx) =
            capture_once("200 OK", r#"{"rooms":[{"name":"r1"},{"name":"r2"}]}"#).await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        let rooms = client.list_rooms().await.unwrap();
        assert_eq!(rooms, vec!["r1".to_string(), "r2".to_string()]);
    }

    #[tokio::test]
    async fn server_500_maps_to_status_error() {
        let (host, _rx) = capture_once("500 Internal Server Error", "boom").await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        let err = client.list_rooms().await.unwrap_err();
        assert!(matches!(err, RoomServiceError::Status(500)));
    }

    #[test]
    fn parse_room_metadata_handles_absent_and_garbage() {
        assert!(parse_room_metadata(None).is_empty());
        assert!(parse_room_metadata(Some("not json")).is_empty());
        assert!(parse_room_metadata(Some("[1,2,3]")).is_empty());
        let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa"]}"#));
        assert_eq!(m["bannedAddresses"], serde_json::json!(["0xa"]));
    }

    #[test]
    fn metadata_append_dedups_and_creates_missing_array() {
        let out = metadata_with_appended(serde_json::Map::new(), SCENE_ADMINS_FIELD, "0xadmin")
            .expect("a missing field must be created");
        assert_eq!(out[SCENE_ADMINS_FIELD], serde_json::json!(["0xadmin"]));
        let existing = parse_room_metadata(Some(r#"{"sceneAdmins":["0xadmin"]}"#));
        assert!(metadata_with_appended(existing, SCENE_ADMINS_FIELD, "0xadmin").is_none());
    }

    #[test]
    fn metadata_remove_is_noop_when_absent_and_removes_when_present() {
        let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa"]}"#));
        assert!(metadata_with_removed(m, BANNED_ADDRESSES_FIELD, "0xzzz").is_none());
        let m = parse_room_metadata(Some(r#"{"bannedAddresses":["0xa","0xb"]}"#));
        let out = metadata_with_removed(m, BANNED_ADDRESSES_FIELD, "0xa").unwrap();
        assert_eq!(out[BANNED_ADDRESSES_FIELD], serde_json::json!(["0xb"]));
        assert!(
            metadata_with_removed(serde_json::Map::new(), BANNED_ADDRESSES_FIELD, "0xa").is_none()
        );
    }

    #[tokio::test]
    async fn append_to_room_metadata_array_reads_then_writes_back() {
        let (host, rx) = capture_seq(vec![
            r#"{"rooms":[{"name":"scene:abc","metadata":"{\"bannedAddresses\":[\"0xold\"]}"}]}"#,
            "{}",
        ])
        .await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client
            .append_to_room_metadata_array("scene:abc", BANNED_ADDRESSES_FIELD, "0xnew")
            .await
            .unwrap();
        let caps = rx.await.unwrap();
        assert_eq!(caps.len(), 2);
        assert!(caps[0].line.contains("ListRooms"));
        assert!(caps[1].line.contains("UpdateRoomMetadata"));
        assert_eq!(caps[1].body["room"], "scene:abc");
        let written: serde_json::Value =
            serde_json::from_str(caps[1].body["metadata"].as_str().unwrap()).unwrap();
        assert_eq!(
            written["bannedAddresses"],
            serde_json::json!(["0xold", "0xnew"])
        );
    }

    #[tokio::test]
    async fn append_is_noop_when_value_already_present() {
        let (host, rx) = capture_seq(vec![
            r#"{"rooms":[{"name":"scene:abc","metadata":"{\"bannedAddresses\":[\"0xhere\"]}"}]}"#,
        ])
        .await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client
            .append_to_room_metadata_array("scene:abc", BANNED_ADDRESSES_FIELD, "0xhere")
            .await
            .unwrap();
        let caps = rx.await.unwrap();
        assert_eq!(
            caps.len(),
            1,
            "an already-present value must not trigger a write"
        );
        assert!(caps[0].line.contains("ListRooms"));
    }

    #[tokio::test]
    async fn metadata_write_is_noop_for_missing_room() {
        let (host, rx) = capture_seq(vec![r#"{"rooms":[]}"#]).await;
        let http = reqwest::Client::new();
        let client = RoomServiceClient::new(&http, &host, "devkey", "devsecret");
        client
            .remove_from_room_metadata_array("scene:gone", BANNED_ADDRESSES_FIELD, "0xa")
            .await
            .unwrap();
        let caps = rx.await.unwrap();
        assert_eq!(caps.len(), 1);
        assert!(caps[0].line.contains("ListRooms"));
    }
}
