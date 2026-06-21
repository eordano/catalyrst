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

/// LiveKit track sources that may be published. Mirrors upstream
/// `TrackSource.MICROPHONE`. The string is the proto3-JSON name LiveKit expects
/// in a `canPublishSources` grant.
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
    /// Restricts which track sources the participant can publish. `None` => no
    /// restriction (publish anything). Upstream `generateCredentials` sets this
    /// to `[MICROPHONE]` for voice-chat participants so they can only publish
    /// audio. Omitted from the JWT when `None`.
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

pub const SCENE_ROOM_PREFIX: &str = "scene-";
pub const WORLD_ROOM_PREFIX: &str = "world-";
pub const PRIVATE_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-private-";
pub const COMMUNITY_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-community";

pub fn private_voice_chat_room_name(room_id: &str) -> String {
    format!("{}{}", PRIVATE_VOICE_CHAT_ROOM_PREFIX, room_id)
}

/// True if the room name is a private (1:1) voice-chat room. Mirrors the
/// `roomName.startsWith('voice-chat-private-')` branch in upstream
/// `handleParticipantJoined`/`handleParticipantLeft`.
pub fn is_private_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(PRIVATE_VOICE_CHAT_ROOM_PREFIX)
}

/// True if the room name is a community voice-chat room. Mirrors the
/// `roomName.startsWith('voice-chat-community-')` branch upstream.
pub fn is_community_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
}

pub fn community_voice_chat_room_name(community_id: &str) -> String {
    format!("{}-{}", COMMUNITY_VOICE_CHAT_ROOM_PREFIX, community_id)
}

/// Extracts the community id from a community voice-chat room name. Mirrors
/// upstream `getCommunityIdFromRoomName`: strips the `voice-chat-community-`
/// prefix.
pub fn community_id_from_room_name(room_name: &str) -> String {
    room_name
        .strip_prefix(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
        .unwrap_or(room_name)
        .to_string()
}

pub fn scene_room_name(realm: &str, scene_id: &str) -> String {
    format!("{}{}:{}", SCENE_ROOM_PREFIX, realm, scene_id)
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
    let trimmed = host
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    format!("https://{}", trimmed)
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

    /// LiveKit `UpdateParticipant` Twirp call. Optionally sets a new metadata
    /// string and/or a new permission set. Mirrors upstream comms-gatekeeper
    /// `updateParticipant(room, identity, metadata?, permission?)`.
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

    /// Read-modify-write a participant's metadata: fetch current metadata,
    /// shallow-merge `patch` over it, and write it back. Matches upstream
    /// `updateParticipantMetadata` (LiveKit has no atomic metadata update).
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
        assert_eq!(scene_room_name("main", "abc"), "scene-main:abc");
        assert_eq!(world_scene_room_name("foo.eth", "xyz"), "world-foo.eth-xyz");
        assert_eq!(world_room_name("foo.eth"), "world-foo.eth");
    }

    #[test]
    fn community_room_name_round_trips() {
        let name = community_voice_chat_room_name("abc-123");
        assert_eq!(name, "voice-chat-community-abc-123");
        assert!(is_community_voice_chat_room(&name));
        // Mirrors upstream getCommunityIdFromRoomName: strips the single
        // `voice-chat-community-` prefix (a `-` inside the id is preserved).
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
    fn room_service_base_strips_ws_scheme() {
        assert_eq!(
            room_service_base("wss://livekit.example.com"),
            "https://livekit.example.com"
        );
        assert_eq!(
            room_service_base("livekit.example.com/"),
            "https://livekit.example.com"
        );
    }
}
