use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use bytes::Bytes;
use hmac::{Hmac, Mac};
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

    let evt: WebhookEvent = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("Invalid webhook body"))?;

    let Some(participant) = evt.participant.as_ref().filter(|p| !p.identity.is_empty()) else {
        return Err(ApiError::bad_request("Participant identity not found"));
    };

    let Some(room) = evt.room.as_ref().filter(|r| !r.name.is_empty()) else {
        return Err(ApiError::bad_request("Room name not found"));
    };

    let is_valid_event =
        evt.event == "participant_joined" || evt.event == "participant_left";

    if !is_valid_event || !room.name.ends_with(".dcl.eth") {
        return Ok(Json(json!({ "message": "Skipping event" })));
    }

    if let Some(world) = world_from_room(&room.name) {
        let action = match evt.event.as_str() {
            "participant_joined" => {
                state.presence.set_peer_world(&participant.identity, &world);
                Some("join")
            }
            "participant_left" => {
                state.presence.remove_peer(&participant.identity);
                Some("leave")
            }
            _ => None,
        };
        // Persist the access event for the bearer-gated GET /admin/access-log
        // view. Best-effort: a logging failure must not fail the webhook.
        if let Some(action) = action {
            if let Err(e) = state
                .worlds
                .record_access(&world, &participant.identity, action, &room.name)
                .await
            {
                tracing::warn!(error = %e, "failed to persist world access log row");
            }
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

fn world_from_room(room: &str) -> Option<String> {
    if let Some(rest) = room.strip_prefix(WORLD_ROOM_PREFIX) {
        return Some(rest.to_string());
    }
    if let Some(rest) = room.strip_prefix(SCENE_ROOM_PREFIX) {
        return rest.rsplit_once('-').map(|(w, _)| w.to_string());
    }
    None
}
