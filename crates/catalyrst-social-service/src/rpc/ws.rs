use crate::rpc::auth_chain::{
    frame_declares_refused_signer, handshake_path, verify_handshake, FIVE_MINUTES_SECS,
};
use crate::rpc::transport::AxumWsTransport;
use crate::rpc::AppState;
use anyhow::{anyhow, Result};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use catalyrst_crypto::Signer;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub async fn ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let path = handshake_path(&headers, "/");
    ws.max_message_size(state.cfg.ws_max_payload_bytes)
        .on_upgrade(move |socket| async move {
            if let Err(err) = handle_connection(state, path, socket).await {
                tracing::warn!(error = %err, "social-rpc ws connection ended");
            }
        })
}

async fn handle_connection(state: AppState, path: String, mut socket: WebSocket) -> Result<()> {
    if let Some(max) = state.cfg.ws_max_concurrent_connections {
        if state.ctx.live_connections() >= max {
            tracing::warn!(max, "rejecting ws connection: pool is full");
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 1013,
                    reason: "Server is at capacity; try again later".into(),
                })))
                .await;
            return Ok(());
        }
    }
    let address = match auth_handshake(&state, &path, &mut socket).await {
        Ok(addr) => addr,
        Err(err) => {
            tracing::info!(%err, "auth handshake failed");
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 3003,
                    reason: "Unauthorized".into(),
                })))
                .await;
            return Ok(());
        }
    };
    tracing::info!(%address, "social-rpc client authenticated");

    let Some(events) = state.rpc_events() else {
        return Err(anyhow!(
            "rpc events sender not initialised; call AppStateInner::init_rpc on boot"
        ));
    };
    let transport = Arc::new(AxumWsTransport::spawn(socket, address.as_str().to_string()));
    events
        .send_attach_transport(transport)
        .map_err(|e| anyhow!("attach transport: {e:?}"))?;
    Ok(())
}

async fn auth_handshake(state: &AppState, path: &str, socket: &mut WebSocket) -> Result<Signer> {
    let window = Duration::from_secs(state.cfg.auth_window_secs as u64);
    let first = timeout(window, socket.recv())
        .await
        .map_err(|_| anyhow!("auth timeout after {}s", state.cfg.auth_window_secs))?
        .ok_or_else(|| anyhow!("socket closed before auth"))?
        .map_err(|e| anyhow!("ws recv error: {e}"))?;

    let text: String = match first {
        Message::Text(s) => s.to_string(),
        Message::Binary(b) => String::from_utf8(b.to_vec())
            .map_err(|_| anyhow!("auth payload was binary but not utf-8 json"))?,
        _ => return Err(anyhow!("expected text/binary frame for auth")),
    };

    // ADR-44: a scene must not authenticate as a user on this surface, matching the HTTP routes
    // (upstream #440). The metadata gate answers before verification (upstream #492), so a refused
    // frame costs no crypto; either way `handle_connection` closes with the same 3003, so there is
    // no weaker check for it to fall back to.
    if frame_declares_refused_signer(&text) {
        return Err(anyhow!("handshake: requests from scenes are not allowed"));
    }

    let now_secs = Utc::now().timestamp();
    verify_handshake(&text, "get", path, FIVE_MINUTES_SECS, now_secs)
        .await
        .map_err(|e| anyhow!("handshake: {e}"))
}
