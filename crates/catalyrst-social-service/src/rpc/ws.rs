use crate::rpc::auth_chain::{verify_handshake, FIVE_MINUTES_SECS};
use crate::rpc::transport::AxumWsTransport;
use crate::rpc::AppState;
use anyhow::{anyhow, Result};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub async fn ws_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.max_message_size(state.cfg.ws_max_payload_bytes)
        .on_upgrade(move |socket| async move {
            if let Err(err) = handle_connection(state, socket).await {
                tracing::warn!(error = %err, "social-rpc ws connection ended");
            }
        })
}

async fn handle_connection(state: AppState, mut socket: WebSocket) -> Result<()> {
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
    let address = match auth_handshake(&state, &mut socket).await {
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
    let transport = Arc::new(AxumWsTransport::spawn(socket, address.clone()));
    events
        .send_attach_transport(transport)
        .map_err(|e| anyhow!("attach transport: {e:?}"))?;
    Ok(())
}

async fn auth_handshake(state: &AppState, socket: &mut WebSocket) -> Result<String> {
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

    let now_secs = Utc::now().timestamp();
    let signer = verify_handshake(&text, "get", "/", FIVE_MINUTES_SECS, now_secs)
        .map_err(|e| anyhow!("handshake: {e}"))?;

    Ok(signer)
}
