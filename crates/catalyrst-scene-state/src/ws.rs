//! WebSocket endpoint + connection lifecycle.
//!
//! Port of `src/controllers/handlers/ws-handler.ts` + the per-client wiring in
//! `src/adapters/scene.ts`.
//!
//! Lifecycle (matching upstream):
//! 1. `GET /ws/:scene` upgrades. The scene must already be loaded (else 404).
//! 2. The connection counter increments (`wsRegistry.onWsConnected`).
//! 3. The server waits up to `auth_timeout_secs` for the first frame. It must be
//!    an `Auth` frame whose body is the signed-fetch headers JSON; the server
//!    verifies it against `GET <pathname>` (see [`crate::auth`]). On failure or
//!    timeout the socket is closed.
//! 4. On success the client is registered, assigned an integer index + entity
//!    range, and immediately sent an `Init` frame with the range + CRDT
//!    snapshot.
//! 5. Steady state: inbound `Crdt` frames are handed to the runtime
//!    (`on_client_crdt`), whose outputs are fanned out to the *other* clients.
//!    Outbound frames queued by the runtime/peers are flushed to the socket.
//! 6. On close the client is removed and the counter decrements.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::auth::verify_auth_frame;
use crate::protocol::{decode_message, encode_init_message, encode_message, MessageType};
use crate::scene::Scene;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws/{scene}", get(ws_upgrade))
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Path(scene_name): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(scene) = state.scenes.get(&scene_name) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            format!("{scene_name} is not currently loaded in the server"),
        )
            .into_response();
    };
    let pathname = format!("/ws/{scene_name}");
    let timeout_secs = state.cfg.auth_timeout_secs;
    let outbound_cap = state.cfg.client_outbound_max.max(1);
    // Clamp the inbound WS frame/message size from axum's 64 MiB default to a
    // sane limit so one client can't make the server buffer a huge frame.
    let ws = ws
        .max_frame_size(state.cfg.ws_max_frame_bytes)
        .max_message_size(state.cfg.ws_max_frame_bytes);
    let mgr = Arc::clone(&state);
    ws.on_upgrade(move |socket| async move {
        mgr.scenes.on_ws_connected();
        handle_socket(socket, scene, pathname, timeout_secs, outbound_cap).await;
        mgr.scenes.on_ws_closed();
    })
    .into_response()
}

async fn handle_socket(
    socket: WebSocket,
    scene: Arc<Scene>,
    pathname: String,
    timeout_secs: u64,
    outbound_cap: usize,
) {
    let (mut sink, mut stream) = socket.split();

    // --- Phase 1: authenticate (first frame, bounded by timeout) ---
    let auth = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Binary(bytes) = msg {
                if let Some((MessageType::Auth, body)) = decode_message(&bytes) {
                    let now = chrono::Utc::now().timestamp();
                    return verify_auth_frame(body, "GET", &pathname, now).ok();
                }
            }
        }
        None
    })
    .await;

    let authed = match auth {
        Ok(Some(a)) => a,
        _ => {
            tracing::debug!(scene = %scene.name, "ws auth failed or timed out");
            let _ = sink.close().await;
            return;
        }
    };

    // --- Phase 2: register client + send Init ---
    // Bounded outbound queue: a slow-reading client backs up here; once full,
    // the runtime/peers drop frames (try_send) rather than buffering without
    // bound. (Reconnect-to-recover is the upstream story for missed state.)
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(outbound_cap);
    let (client, init) = scene.add_client(authed.signer.clone(), tx);
    let index = client.index;

    let init_frame = encode_init_message(
        &init.crdt_state,
        init.start,
        init.size,
        init.reserved_local_entities,
    );
    if sink.send(Message::Binary(init_frame.into())).await.is_err() {
        scene.remove_client(index);
        return;
    }
    tracing::info!(scene = %scene.name, index, address = %authed.signer, "ws authenticated");

    // --- Phase 3: steady-state relay loop ---
    loop {
        tokio::select! {
            // Outbound frames queued by the runtime / peers.
            queued = rx.recv() => {
                let Some(frame) = queued else { break };
                if sink.send(Message::Binary(frame.into())).await.is_err() {
                    break;
                }
            }
            // Inbound frames from this client.
            incoming = stream.next() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Binary(bytes) => {
                        if let Some((MessageType::Crdt, body)) = decode_message(&bytes) {
                            if body.is_empty() { continue; }
                            let outbound = scene.runtime.on_client_crdt(index, body);
                            for out in outbound {
                                let frame = encode_message(MessageType::Crdt, &out);
                                scene.broadcast(&frame, index);
                            }
                        }
                    }
                    Message::Ping(p) => { let _ = sink.send(Message::Pong(p)).await; }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    scene.remove_client(index);
    tracing::debug!(scene = %scene.name, index, "ws closed");
}
