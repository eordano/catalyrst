use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::http::{unauthorized, ApiError};
use crate::livekit::{address_from_identity, verify_webhook_signature};
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
    let event_kind = event.get("event").and_then(|v| v.as_str()).unwrap_or("unknown");
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
            sqlx::query(
                "INSERT INTO voice_chat_users (address, room_name, status, joined_at, status_updated_at) \
                 VALUES ($1, $2, 'connected', now(), now()) \
                 ON CONFLICT (address, room_name) \
                 DO UPDATE SET status = 'connected', status_updated_at = now()",
            )
            .bind(&addr)
            .bind(&room)
            .execute(&state.pool)
            .await?;
        }
        "participant_left" => {
            let (Some(room), Some(addr)) = (room_name(event), participant_address(event)) else {
                return Ok(());
            };
            sqlx::query(
                "UPDATE voice_chat_users SET status = 'disconnected', status_updated_at = now() \
                 WHERE address = $1 AND room_name = $2",
            )
            .bind(&addr)
            .bind(&room)
            .execute(&state.pool)
            .await?;
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

    Some(address_from_identity(identity).unwrap_or_else(|| {
        identity.to_lowercase().chars().take(42).collect::<String>()
    }))
}

fn ingress_id(event: &serde_json::Value) -> Option<String> {
    event
        .get("ingressInfo")
        .and_then(|i| i.get("ingressId"))
        .and_then(|v| v.as_str())
        .map(String::from)
}
