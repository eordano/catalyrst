use thiserror::Error;

use super::{room_admin_token, room_service_base, LivekitError};

/// How many rooms the ban kick scans at once.
pub(crate) const KICK_CONCURRENCY: usize = 8;

pub async fn list_room_participant_identities(
    client: &reqwest::Client,
    host: &str,
    api_key: &str,
    api_secret: &str,
    room: &str,
) -> Vec<String> {
    let token = match room_admin_token(api_key, api_secret, room) {
        Ok(t) => t,
        Err(_) => {
            tracing::warn!(
                reason = "token mint failed",
                "failed to mint room-admin token for participants list"
            );
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
        Err(_) => {
            tracing::debug!(room, "ListParticipants request failed");
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
    pub sid: String,
    pub identity: String,
    pub name: Option<String>,
    pub state: i64,
    pub metadata: Option<String>,
    pub is_publisher: bool,
    pub can_update_metadata: Option<bool>,
}

#[derive(Error)]
pub enum RoomServiceError {
    #[error("livekit not configured")]
    NotConfigured,
    #[error("livekit token mint failed")]
    Token(
        #[from]
        #[source]
        LivekitError,
    ),
    #[error("livekit request failed")]
    Request,
    #[error("livekit returned an invalid response")]
    InvalidResponse,
    #[error("livekit returned status {0}")]
    Status(u16),
    #[error("livekit room not found")]
    NotFound,
}

impl std::fmt::Debug for RoomServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let class = match self {
            Self::NotConfigured => "NotConfigured",
            Self::Token(_) => "Token",
            Self::Request => "Request",
            Self::InvalidResponse => "InvalidResponse",
            Self::Status(_) => "Status",
            Self::NotFound => "NotFound",
        };
        formatter
            .debug_tuple("RoomServiceError")
            .field(&class)
            .finish()
    }
}

/// What a removal found in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    Absent,
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
            .map_err(|_| RoomServiceError::Request)?;
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
            .map_err(|_| RoomServiceError::InvalidResponse)
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

    /// Removes a participant and requests a token cutoff. Only LiveKit Cloud enforces this
    /// revocation; self-hosted LiveKit accepts the field but old tokens remain usable.
    ///
    /// Unlike [`RoomServiceClient::remove_participant`], an absent participant is reported rather
    /// than folded into success: the takeover path counts "had already left" apart from "removed",
    /// and only the second means a live session was ended.
    ///
    /// The boundary goes on the wire in seconds, matching the `nbf` unit LiveKit compares it
    /// against, and as a string because that is proto3 JSON's canonical encoding for an int64.
    pub async fn remove_participant_with_cutoff(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, RoomServiceError> {
        let mut body = serde_json::json!({ "room": room, "identity": identity });
        if let Some(boundary) = revoke_tokens_minted_before {
            body["revokeTokenTs"] = serde_json::Value::String(boundary.to_string());
        }
        match self.call("RemoveParticipant", room, body).await {
            Ok(_) => Ok(Removal::Removed),
            Err(RoomServiceError::NotFound) => Ok(Removal::Absent),
            Err(e) => Err(e),
        }
    }

    /// Whether the identity is currently in the room, compared case-insensitively.
    ///
    /// A room or participant that is absent answers `false`; every other failure propagates. A
    /// caller that must fail closed on "could not tell" can only do so if the two are distinct.
    pub async fn holds_participant(
        &self,
        room: &str,
        identity: &str,
    ) -> Result<bool, RoomServiceError> {
        let participants = match self.list_participants(room).await {
            Ok(participants) => participants,
            Err(RoomServiceError::NotFound) => return Ok(false),
            Err(e) => return Err(e),
        };
        let target = identity.to_lowercase();
        Ok(participants
            .iter()
            .any(|p| p.identity.to_lowercase() == target))
    }

    pub async fn get_participant(
        &self,
        room: &str,
        identity: &str,
    ) -> Result<Option<ParticipantInfo>, RoomServiceError> {
        match self
            .call(
                "GetParticipant",
                room,
                serde_json::json!({ "room": room, "identity": identity }),
            )
            .await
        {
            Ok(v) => Ok(Some(parse_participant(&v))),
            Err(RoomServiceError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn remove_participant_from_all_rooms(
        &self,
        identity: &str,
    ) -> Result<(), RoomServiceError> {
        use futures::StreamExt;
        let rooms = self.list_rooms().await?;
        let lower = identity.to_lowercase();
        futures::stream::iter(rooms)
            .for_each_concurrent(KICK_CONCURRENCY, |room| {
                let lower = lower.clone();
                async move {
                    let participants = match self.list_participants(&room).await {
                        Ok(p) => p,
                        Err(RoomServiceError::NotFound) => return,
                        Err(e) => {
                            tracing::warn!(error = %e, room = %room, identity = %lower, "failed to list participants for room");
                            return;
                        }
                    };
                    let Some(p) = participants
                        .into_iter()
                        .find(|p| p.identity.to_lowercase() == lower)
                    else {
                        return;
                    };
                    if let Err(e) = self.remove_participant(&room, &p.identity).await {
                        tracing::warn!(error = %e, room = %room, identity = %p.identity, "failed to remove participant from room");
                    }
                }
            })
            .await;
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
        let metadata = self.merged_metadata(room, identity, patch).await;
        self.update_participant(room, identity, Some(&metadata), None)
            .await
    }

    /// One UpdateParticipant carrying both the merged metadata and the permission.
    pub async fn merge_participant_metadata_with_permission(
        &self,
        room: &str,
        identity: &str,
        patch: serde_json::Map<String, serde_json::Value>,
        permission: serde_json::Value,
    ) -> Result<(), RoomServiceError> {
        let metadata = self.merged_metadata(room, identity, patch).await;
        self.update_participant(room, identity, Some(&metadata), Some(permission))
            .await
    }

    async fn merged_metadata(
        &self,
        room: &str,
        identity: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> String {
        let existing = self
            .get_participant(room, identity)
            .await
            .ok()
            .flatten()
            .and_then(|p| p.metadata);
        let mut merged: serde_json::Map<String, serde_json::Value> = existing
            .as_deref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or_default();
        for (k, v) in patch {
            merged.insert(k, v);
        }
        serde_json::Value::Object(merged).to_string()
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
        sid: p
            .get("sid")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .into(),
        identity: p
            .get("identity")
            .and_then(|i| i.as_str())
            .unwrap_or_default()
            .to_string(),
        name: p.get("name").and_then(|n| n.as_str()).map(String::from),
        state: p.get("state").and_then(|s| s.as_i64()).unwrap_or(0),
        metadata: p.get("metadata").and_then(|m| m.as_str()).map(String::from),
        is_publisher: permission_publish || has_tracks,
        can_update_metadata: p
            .get("permission")
            .and_then(|p| p.as_object())
            .and_then(
                |p| match (p.get("canUpdateMetadata"), p.get("can_update_metadata")) {
                    (Some(a), Some(b)) if a != b => None,
                    (Some(value), _) | (_, Some(value)) => value.as_bool(),
                    (None, None) => Some(false),
                },
            ),
    }
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn metadata_permissions_accept_both_proto_json_spellings_and_fail_closed_on_conflicts() {
        for (permission, expected) in [
            (serde_json::json!({"can_update_metadata": true}), Some(true)),
            (serde_json::json!({"canUpdateMetadata": true}), Some(true)),
            (
                serde_json::json!({"can_update_metadata": false}),
                Some(false),
            ),
            (serde_json::json!({"canUpdateMetadata": false}), Some(false)),
            (serde_json::json!({}), Some(false)),
            (serde_json::Value::Null, None),
            (serde_json::json!({"can_update_metadata": "false"}), None),
            (
                serde_json::json!({"canUpdateMetadata": false, "can_update_metadata": true}),
                None,
            ),
            (
                serde_json::json!({"canUpdateMetadata": true, "can_update_metadata": false}),
                None,
            ),
        ] {
            let participant = parse_participant(&serde_json::json!({"permission": permission}));
            assert_eq!(participant.can_update_metadata, expected);
        }
        assert_eq!(
            parse_participant(&serde_json::json!({})).can_update_metadata,
            None
        );
    }
}
