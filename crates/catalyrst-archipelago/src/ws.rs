use crate::feed::{connect_subject, disconnect_subject};
use crate::proto::archipelago::{
    client_packet, server_packet, ChallengeResponseMessage, ClientPacket, IslandChangedMessage,
    KickedMessage, KickedReason, ServerPacket, WelcomeMessage,
};
use crate::registry::{PeerLink, SocketEvent};
use crate::session::{is_address, is_session_key, session_key_of};
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use catalyrst_types::AuthChain;
use prost::Message as _;
use rand::RngExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_upgrade))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.protocols(["archipelago"])
        .on_upgrade(move |socket| handle_socket(socket, state))
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    HandshakeStart,
    ChallengeSent,
    Completed,
}

fn craft(message: server_packet::Message) -> Vec<u8> {
    let packet = ServerPacket {
        message: Some(message),
    };
    let mut buf = Vec::with_capacity(packet.encoded_len());
    packet.encode(&mut buf).expect("ServerPacket encodes");
    buf
}

async fn send_packet(socket: &mut WebSocket, message: server_packet::Message) -> bool {
    socket
        .send(Message::Binary(craft(message).into()))
        .await
        .is_ok()
}

fn kicked_packet() -> server_packet::Message {
    server_packet::Message::Kicked(KickedMessage {
        reason: KickedReason::KrNewSession as i32,
    })
}

fn heartbeat_position(hb: &crate::proto::archipelago::Heartbeat) -> Option<[f32; 3]> {
    hb.position.as_ref().map(|p| [p.x, p.y, p.z])
}

/// The same room delivered to the same socket inside the window is the client's own assignment
/// arriving again through the re-announce path, and it already holds a token for that room.
fn is_repeat(
    last: Option<&(String, Instant)>,
    island_id: &str,
    now: Instant,
    dedup_ms: u64,
) -> bool {
    match last {
        Some((seen, at)) => {
            dedup_ms > 0
                && seen == island_id
                && now.duration_since(*at).as_millis() < dedup_ms as u128
        }
        None => false,
    }
}

/// Never resolves before the handshake completes, which keeps a socket with no registration out of
/// the select without a second copy of the loop.
async fn next_event(rx: Option<&mut UnboundedReceiver<SocketEvent>>) -> Option<SocketEvent> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

struct Registration {
    address: String,
    session: String,
    link: PeerLink,
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut stage = Stage::HandshakeStart;
    let mut challenge_to_sign = String::new();
    let mut claimed: Option<String> = None;
    let mut registration: Option<Registration> = None;
    let mut events: Option<UnboundedReceiver<SocketEvent>> = None;
    let mut last_island: Option<(String, Instant)> = None;
    let dedup_ms = state.cfg.nats.island_changed_dedup_ms;
    let handshake_timeout = Duration::from_millis(state.cfg.auth.handshake_timeout_ms);
    let mut handshake_deadline = tokio::time::Instant::now() + handshake_timeout;

    loop {
        tokio::select! {
            biased;
            incoming = socket.recv() => {
                let Some(msg) = incoming else { break };
                let msg = match msg { Ok(m) => m, Err(_) => break };
                match msg {
                    Message::Binary(bytes) => {
                        let packet = match ClientPacket::decode(bytes.as_ref()) {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = socket
                                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                        code: 1007,
                                        reason: "Cannot decode ClientPacket".into(),
                                    })))
                                    .await;
                                break;
                            }
                        };
                        match stage {
                            Stage::HandshakeStart => {
                                let Some(client_packet::Message::ChallengeRequest(req)) = packet.message else {
                                    break;
                                };
                                let addr = req.address.to_ascii_lowercase();
                                if !is_address(&addr) {
                                    tracing::debug!("rejecting: challengeRequest carries no valid address");
                                    break;
                                }
                                if state.deny_list.is_denied(&addr).await {
                                    tracing::warn!(addr = %addr, "archipelago ws rejected: deny-listed wallet");
                                    break;
                                }
                                challenge_to_sign = format!("dcl-{}", rand::rng().random::<u64>());

                                state.challenges.put(&addr, &challenge_to_sign);
                                let already_connected = state.registry.has_peer(&addr);
                                claimed = Some(addr);
                                if !send_packet(
                                    &mut socket,
                                    server_packet::Message::ChallengeResponse(ChallengeResponseMessage {
                                        challenge_to_sign: challenge_to_sign.clone(),
                                        already_connected,
                                    }),
                                )
                                .await
                                {
                                    break;
                                }
                                stage = Stage::ChallengeSent;
                                handshake_deadline = tokio::time::Instant::now() + handshake_timeout;
                            }
                            Stage::ChallengeSent => {
                                let Some(client_packet::Message::SignedChallenge(signed)) = packet.message else {
                                    break;
                                };
                                let chain: AuthChain = match serde_json::from_str(&signed.auth_chain_json) {
                                    Ok(c) => c,
                                    Err(_) => break,
                                };
                                let addr = claimed.clone().unwrap_or_default();
                                match state
                                    .challenges
                                    .redeem_and_verify(&addr, &challenge_to_sign, &chain)
                                {
                                    Ok(()) => {
                                        let signer = chain
                                            .first()
                                            .map(|l| l.payload.to_ascii_lowercase())
                                            .unwrap_or(addr);
                                        if state.deny_list.is_denied(&signer).await {
                                            tracing::warn!(addr = %signer, "archipelago ws rejected: deny-listed wallet (post-auth)");
                                            break;
                                        }
                                        if state.ban_checker.is_banned(&signer).await {
                                            tracing::warn!(addr = %signer, "archipelago ws rejected: platform-banned wallet");
                                            let _ = send_packet(&mut socket, kicked_packet()).await;
                                            break;
                                        }
                                        let session = session_key_of(&chain, &signer);
                                        if !is_session_key(&session) {
                                            tracing::warn!(addr = %signer, "rejecting: auth chain yields no usable session key");
                                            break;
                                        }
                                        let (link, rx, previous) =
                                            state.registry.on_peer_connected(&signer, &session);
                                        if let Some(previous) = previous {
                                            if !previous.is_closed() {
                                                tracing::debug!(addr = %signer, "replacing this device's previous socket");
                                                previous.send(SocketEvent::Kicked);
                                            }
                                            previous.close();
                                        }
                                        events = Some(rx);
                                        registration = Some(Registration {
                                            address: signer.clone(),
                                            session: session.clone(),
                                            link,
                                        });
                                        tracing::info!(addr = %signer, session = %session, "archipelago ws handshake complete (welcome)");
                                        if !send_packet(
                                            &mut socket,
                                            server_packet::Message::Welcome(WelcomeMessage {
                                                peer_id: signer.clone(),
                                            }),
                                        )
                                        .await
                                        {
                                            break;
                                        }
                                        announce_connect(&state, &signer, &session);
                                        stage = Stage::Completed;
                                        handshake_deadline = tokio::time::Instant::now() + handshake_timeout;
                                    }
                                    Err(e) => {
                                        tracing::warn!(addr = %claimed.clone().unwrap_or_default(), err = %e, "archipelago signed-challenge rejected");
                                        break;
                                    }
                                }
                            }
                            Stage::Completed => {
                                let Some(client_packet::Message::Heartbeat(hb)) = packet.message else {
                                    continue;
                                };
                                let Some(current) = registration.as_ref() else { continue };
                                let Some(position) = heartbeat_position(&hb) else { continue };
                                let parcel = crate::peers::to_parcel(position[0], position[2]);
                                let realm = hb.desired_room.unwrap_or_else(|| "catalyrst".into());
                                state.peers.upsert_peer(current.address.clone(), position, parcel, realm);
                            }
                        }
                    }
                    Message::Ping(p) => { let _ = socket.send(Message::Pong(p)).await; }
                    Message::Close(_) => break,

                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(handshake_deadline), if stage != Stage::Completed => {
                tracing::debug!("closing socket: the handshake stalled past its timeout");
                break;
            }
            event = next_event(events.as_mut()) => {
                let Some(event) = event else { continue };
                match event {
                    SocketEvent::IslandChanged(payload) => {
                        let Ok(island_changed) = IslandChangedMessage::decode(payload.as_slice()) else {
                            continue;
                        };
                        let now = Instant::now();
                        if is_repeat(last_island.as_ref(), &island_changed.island_id, now, dedup_ms) {
                            state.feed.on_deduplicated();
                            continue;
                        }
                        last_island = Some((island_changed.island_id.clone(), now));
                        if !send_packet(
                            &mut socket,
                            server_packet::Message::IslandChanged(island_changed),
                        )
                        .await
                        {
                            break;
                        }
                    }
                    SocketEvent::Kicked => {
                        let addr = registration.as_ref().map(|r| r.address.clone()).unwrap_or_default();
                        tracing::info!(addr = %addr, "session superseded or kicked; closing socket");
                        let _ = send_packet(&mut socket, kicked_packet()).await;
                        break;
                    }
                }
            }
        }
    }

    if let Some(current) = registration {
        current.link.close();
        state
            .registry
            .on_peer_disconnected(&current.address, &current.session, current.link.id);
        if !state.registry.has_peer(&current.address) {
            state.peers.remove_peer(&current.address);
        }
        state
            .publisher
            .publish(disconnect_subject(&current.address), Vec::new());
        tracing::info!(addr = %current.address, session = %current.session, "archipelago ws closed");
    }
}

/// Tells the cluster feed's consumer that this session now holds a live socket. The feed is
/// edge-triggered and says nothing while a peer's cluster is unchanged, so a client that
/// reconnects standing still is only ever given a room because of this announcement.
fn announce_connect(state: &AppState, address: &str, session: &str) {
    if !state
        .publisher
        .publish(connect_subject(address), session.as_bytes().to_vec())
    {
        tracing::warn!(
            addr = %address,
            "connect announcement dropped: no broker link, so this session is given no room \
             until its cluster changes"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::archipelago::Heartbeat;
    use crate::proto::Position;
    use std::time::Duration;

    #[test]
    fn positionless_heartbeat_is_ignored() {
        let hb = Heartbeat {
            position: None,
            desired_room: Some("catalyrst".into()),
        };
        assert_eq!(heartbeat_position(&hb), None);
    }

    #[test]
    fn heartbeat_with_position_yields_xyz() {
        let hb = Heartbeat {
            position: Some(Position {
                x: 1.5,
                y: 2.0,
                z: -3.25,
            }),
            desired_room: None,
        };
        assert_eq!(heartbeat_position(&hb), Some([1.5, 2.0, -3.25]));
    }

    #[test]
    fn the_same_room_inside_the_window_is_a_repeat_and_a_different_one_is_not() {
        let now = Instant::now();
        let last = ("island-C1".to_string(), now);
        assert!(is_repeat(Some(&last), "island-C1", now, 10_000));
        assert!(!is_repeat(Some(&last), "island-C2", now, 10_000));
        assert!(!is_repeat(None, "island-C1", now, 10_000));
    }

    #[test]
    fn a_zero_window_repeats_nothing_and_an_elapsed_one_lets_the_room_through() {
        let now = Instant::now();
        let last = ("island-C1".to_string(), now);
        assert!(!is_repeat(Some(&last), "island-C1", now, 0));
        assert!(!is_repeat(
            Some(&last),
            "island-C1",
            now + Duration::from_millis(10),
            10
        ));
    }
}
