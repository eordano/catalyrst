use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use bytes::Bytes;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::http::ApiError;
use crate::livekit::{SCENE_ROOM_PREFIX, WORLD_ROOM_PREFIX};
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

pub async fn livekit_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let authorization = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if authorization.is_empty() {
        return Err(ApiError::bad_request("Authorization header not found"));
    }

    verify_webhook_token(
        authorization,
        &body,
        &state.cfg.livekit_api_key,
        &state.cfg.livekit_api_secret,
    )?;

    let evt: WebhookEvent =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("Invalid webhook body"))?;

    let Some(participant) = evt.participant.as_ref().filter(|p| !p.identity.is_empty()) else {
        return Err(ApiError::bad_request("Participant identity not found"));
    };

    let Some(room) = evt.room.as_ref().filter(|r| !r.name.is_empty()) else {
        return Err(ApiError::bad_request("Room name not found"));
    };

    let is_valid_event = evt.event == "participant_joined" || evt.event == "participant_left";

    let Some(RoomWorld {
        world,
        is_scene_room,
    }) = world_from_room(&room.name)
    else {
        return Ok(Json(json!({ "message": "Skipping event" })));
    };

    if !is_valid_event {
        return Ok(Json(json!({ "message": "Skipping event" })));
    }

    let action = match evt.event.as_str() {
        "participant_joined" => {
            state
                .presence
                .peer_joined(&participant.identity, &world, &room.name, is_scene_room);
            Some("join")
        }
        "participant_left" => {
            state
                .presence
                .peer_left(&participant.identity, &room.name, is_scene_room);
            Some("leave")
        }
        _ => None,
    };

    if let Some(action) = action {
        if let Err(e) = state
            .worlds
            .record_access(&world, &participant.identity, action, &room.name)
            .await
        {
            tracing::warn!(error = %e, "failed to persist world access log row");
        }
    }

    Ok(Json(json!({ "ok": true })))
}

fn verify_webhook_token(
    token: &str,
    body: &[u8],
    api_key: &str,
    api_secret: &str,
) -> Result<(), ApiError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(ApiError::unauthorized("Invalid webhook token"));
    }

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected_sig = {
        let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes())
            .map_err(|_| ApiError::unauthorized("Invalid webhook token"))?;
        mac.update(signing_input.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    };
    if !constant_time_eq(expected_sig.as_bytes(), parts[2].as_bytes()) {
        return Err(ApiError::unauthorized("Invalid webhook token"));
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| ApiError::unauthorized("Invalid webhook token"))?;
    let claims: Value = serde_json::from_slice(&payload_bytes)
        .map_err(|_| ApiError::unauthorized("Invalid webhook token"))?;

    if claims.get("iss").and_then(|v| v.as_str()) != Some(api_key) {
        return Err(ApiError::unauthorized("Invalid webhook token issuer"));
    }

    let claimed_sha = claims
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::unauthorized("Invalid webhook token"))?;

    let actual_sha = B64_STANDARD.encode(Sha256::digest(body));
    if !constant_time_eq(actual_sha.as_bytes(), claimed_sha.as_bytes()) {
        return Err(ApiError::unauthorized("Webhook body hash mismatch"));
    }

    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(Debug, serde::Deserialize)]
pub struct WebhookEvent {
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub room: Option<RoomInfo>,
    #[serde(default)]
    pub participant: Option<ParticipantInfo>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RoomInfo {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ParticipantInfo {
    #[serde(default)]
    pub identity: String,
}

const DCL_ETH_SUFFIX: &str = ".dcl.eth";

pub struct RoomWorld {
    pub world: String,
    pub is_scene_room: bool,
}

fn world_from_room(room: &str) -> Option<RoomWorld> {
    if let Some(rest) = room.strip_prefix(SCENE_ROOM_PREFIX) {
        return Some(RoomWorld {
            world: extract_scene_world(rest),
            is_scene_room: true,
        });
    }
    if let Some(rest) = room.strip_prefix(WORLD_ROOM_PREFIX) {
        return Some(RoomWorld {
            world: rest.to_string(),
            is_scene_room: false,
        });
    }
    None
}

fn extract_scene_world(rest: &str) -> String {
    if let Some(idx) = rest.find(DCL_ETH_SUFFIX) {
        return rest[..idx + DCL_ETH_SUFFIX.len()].to_lowercase();
    }
    rest.rsplit_once('-')
        .map(|(w, _)| w)
        .unwrap_or(rest)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_from_room_classifies_world_and_scene_rooms() {
        let w = world_from_room("world-foo.dcl.eth").expect("world room");
        assert_eq!(w.world, "foo.dcl.eth");
        assert!(!w.is_scene_room);

        let s = world_from_room("scene-foo.dcl.eth-bafybeihash").expect("scene room");
        assert_eq!(s.world, "foo.dcl.eth");
        assert!(s.is_scene_room);

        let s2 = world_from_room("scene-my-cool-world.dcl.eth-sid").expect("scene room");
        assert_eq!(s2.world, "my-cool-world.dcl.eth");
        assert!(s2.is_scene_room);

        assert!(world_from_room("lobby").is_none());
    }
}
