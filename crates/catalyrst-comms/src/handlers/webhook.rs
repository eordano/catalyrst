use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::http::{unauthorized, ApiError};
use crate::livekit::{
    address_from_identity, is_community_voice_chat_room, is_private_voice_chat_room,
    verify_webhook_signature,
};
use crate::voice_logic::DisconnectReason;
use crate::AppState;

pub async fn livekit_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if let Some(key) = state.livekit_webhook_key.as_deref() {
        let sig = headers
            .get("authorization")
            .or_else(|| headers.get("x-livekit-signature"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_webhook_signature(key, &body, sig) {
            return Err(unauthorized("invalid livekit webhook signature"));
        }
    }

    let event: serde_json::Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| serde_json::json!({ "raw": String::from_utf8_lossy(&body) }));
    let event_kind = event
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    tracing::info!(event = event_kind, "livekit webhook received");

    if let Err(e) = dispatch(&state, event_kind, &event).await {
        tracing::warn!(error = %e, event = event_kind, "livekit webhook side-effect failed");
    }

    Ok((axum::http::StatusCode::OK, body).into_response())
}

async fn dispatch(
    state: &AppState,
    event_kind: &str,
    event: &serde_json::Value,
) -> Result<(), ApiError> {
    match event_kind {
        "participant_joined" => {
            let (Some(room), Some(addr)) = (room_name(event), participant_address(event)) else {
                return Ok(());
            };
            // Only voice-chat rooms drive the voice state machine. Private rooms
            // run the full active-room / join-and-evict logic; community rooms
            // mark the joining user connected (mirrors upstream
            // `handleCommunityParticipantJoined`).
            if is_private_voice_chat_room(&room) {
                crate::voice_logic::handle_private_participant_joined(state, &addr, &room).await?;
            } else if is_community_voice_chat_room(&room) {
                crate::voice_logic::handle_community_participant_joined(state, &addr, &room)
                    .await?;
            }
        }
        "participant_left" => {
            let (Some(room), Some(addr)) = (room_name(event), participant_address(event)) else {
                return Ok(());
            };
            let disconnect_reason = DisconnectReason::parse(
                event
                    .get("participant")
                    .and_then(|p| p.get("disconnectReason")),
            );
            if is_private_voice_chat_room(&room) {
                crate::voice_logic::handle_private_participant_left(
                    state,
                    &addr,
                    &room,
                    disconnect_reason,
                )
                .await?;
            } else if is_community_voice_chat_room(&room) {
                // Full community teardown: branch on disconnect reason, tear down
                // the room when the last active moderator leaves / the room is
                // deleted, and publish CommunityStreamingEnded. Faithful port of
                // `handleCommunityParticipantLeft`.
                crate::voice_logic::handle_community_participant_left(
                    state,
                    &addr,
                    &room,
                    disconnect_reason,
                )
                .await?;
            }
        }
        "ingress_started" => {
            let Some(ingress_id) = ingress_id(event) else {
                return Ok(());
            };
            sqlx::query(
                "UPDATE scene_stream_access \
                 SET streaming = TRUE, streaming_start_time = now() \
                 WHERE ingress_id = $1 AND streaming = FALSE",
            )
            .bind(&ingress_id)
            .execute(&state.pool)
            .await?;
        }
        "ingress_ended" => {
            let Some(ingress_id) = ingress_id(event) else {
                return Ok(());
            };
            sqlx::query(
                "UPDATE scene_stream_access \
                 SET streaming = FALSE \
                 WHERE ingress_id = $1",
            )
            .bind(&ingress_id)
            .execute(&state.pool)
            .await?;
        }
        _ => {}
    }
    Ok(())
}

fn room_name(event: &serde_json::Value) -> Option<String> {
    event
        .get("room")
        .and_then(|r| r.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from)
}

fn participant_address(event: &serde_json::Value) -> Option<String> {
    let identity = event
        .get("participant")
        .and_then(|p| p.get("identity"))
        .and_then(|i| i.as_str())?;

    Some(
        address_from_identity(identity)
            .unwrap_or_else(|| identity.to_lowercase().chars().take(42).collect::<String>()),
    )
}

fn ingress_id(event: &serde_json::Value) -> Option<String> {
    event
        .get("ingressInfo")
        .and_then(|i| i.get("ingressId"))
        .and_then(|v| v.as_str())
        .map(String::from)
}
