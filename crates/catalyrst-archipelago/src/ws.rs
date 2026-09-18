use crate::control_v4::{MAX_FRAME_BYTES as V4_MAX_FRAME_BYTES, SUBPROTOCOL as V4_SUBPROTOCOL};
use crate::feed::{connect_subject, disconnect_subject, FeedCache};
use crate::proto::archipelago::{
    client_packet, server_packet, ChallengeResponseMessage, ClientPacket, IslandChangedMessage,
    KickedMessage, KickedReason, ServerPacket, WelcomeMessage,
};
use crate::registry::{
    AdmissionTicket, PeerLink, ReconciliationPeer, SocketEvent, SocketEvents, MAX_ASSIGNMENT_BYTES,
};
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

mod control_position;
mod io;
mod legacy_fence;
mod v4;

const LEGACY_SUBPROTOCOL: &str = "archipelago";
const AUTHORITY_OPERATION_TIMEOUT: Duration = Duration::from_secs(2);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ws", get(ws_upgrade))
        .merge(v4::routes())
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let ws = ws.protocols([V4_SUBPROTOCOL, LEGACY_SUBPROTOCOL]);
    let is_v4 = ws
        .selected_protocol()
        .is_some_and(|protocol| protocol == V4_SUBPROTOCOL);
    let max_frame_bytes = if is_v4 {
        V4_MAX_FRAME_BYTES
    } else {
        MAX_ASSIGNMENT_BYTES
    };
    let max_write_buffer_size = if is_v4 {
        V4_MAX_FRAME_BYTES
    } else {
        MAX_ASSIGNMENT_BYTES + 1024
    };
    ws.read_buffer_size(16 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(max_write_buffer_size)
        .max_message_size(max_frame_bytes)
        .max_frame_size(max_frame_bytes)
        .on_upgrade(move |socket| async move {
            if is_v4 {
                v4::handle_socket(socket, state).await;
            } else {
                handle_socket(socket, state).await;
            }
        })
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
        request_sequence: 0,
    };
    let mut buf = Vec::with_capacity(packet.encoded_len());
    packet.encode(&mut buf).expect("ServerPacket encodes");
    buf
}

async fn send_packet(
    socket: &mut WebSocket,
    message: server_packet::Message,
    owner: Option<&PeerLink>,
    feed: &FeedCache,
) -> bool {
    io::send(socket, Message::Binary(craft(message).into()), owner, feed).await
}

fn kicked_packet() -> server_packet::Message {
    server_packet::Message::Kicked(KickedMessage {
        reason: KickedReason::KrNewSession as i32,
    })
}

fn heartbeat_position(hb: &crate::proto::archipelago::Heartbeat) -> Option<[f32; 3]> {
    hb.position
        .as_ref()
        .map(|p| [p.x, p.y, p.z])
        .filter(|position| position.iter().all(|value| value.is_finite()))
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
async fn next_event(rx: Option<&mut SocketEvents>) -> Option<SocketEvent> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

struct Registration {
    address: String,
    session: String,
    link: PeerLink,
    announces_feed: bool,
    fence: Option<legacy_fence::LegacyFence>,
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut stage = Stage::HandshakeStart;
    let mut challenge_to_sign = String::new();
    let mut claimed: Option<String> = None;
    let mut admission: Option<AdmissionTicket> = None;
    let mut authority_epoch = None;
    let mut registration: Option<Registration> = None;
    let mut events: Option<SocketEvents> = None;
    let mut last_island: Option<(String, Instant)> = None;
    let dedup_ms = state.cfg.nats.island_changed_dedup_ms;
    let handshake_timeout = Duration::from_millis(state.cfg.auth.handshake_timeout_ms);
    let mut handshake_deadline = tokio::time::Instant::now() + handshake_timeout;
    let mut ownership_poll = tokio::time::interval(legacy_fence::POLL_INTERVAL);
    ownership_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut renewal = tokio::time::interval(crate::control_v4::OWNER_RENEWAL_INTERVAL);
    renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        if registration
            .as_ref()
            .is_some_and(|current| current.link.is_closed())
        {
            if matches!(
                events.as_mut().and_then(|rx| rx.try_recv().ok()),
                Some(SocketEvent::Kicked)
            ) {
                let _ = send_packet(&mut socket, kicked_packet(), None, &state.feed).await;
            }
            break;
        }
        if stage != Stage::Completed && tokio::time::Instant::now() >= handshake_deadline {
            tracing::debug!("closing socket: the handshake stalled past its timeout");
            break;
        }
        tokio::select! {
            incoming = socket.recv() => {
                let Some(msg) = incoming else { break };
                let msg = match msg { Ok(m) => m, Err(_) => break };
                match msg {
                    Message::Binary(bytes) => {
                        let packet = match ClientPacket::decode(bytes.as_ref()) {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = io::send(&mut socket,
                                    Message::Close(Some(axum::extract::ws::CloseFrame {
                                        code: 1007,
                                        reason: "Cannot decode ClientPacket".into(),
                                    })), registration.as_ref().map(|current| &current.link), &state.feed)
                                    .await;
                                break;
                            }
                        };
                        match stage {
                            Stage::HandshakeStart => {
                                let Some(client_packet::Message::ChallengeRequest(req)) = packet.message else {
                                    break;
                                };
                                if req.offer.is_some() {
                                    if bytes.len() > V4_MAX_FRAME_BYTES || packet.request_sequence != 0 {
                                        break;
                                    }
                                    v4::handle_extensions(socket, state, req, handshake_deadline).await;
                                    return;
                                }
                                let addr = req.address.to_ascii_lowercase();
                                if !is_address(&addr) {
                                    tracing::debug!("rejecting: challengeRequest carries no valid address");
                                    break;
                                }
                                if state.deny_list.is_denied(&addr).await {
                                    tracing::warn!(addr = %addr, "archipelago ws rejected: deny-listed wallet");
                                    break;
                                }
                                authority_epoch = match tokio::time::timeout_at(
                                    handshake_deadline, legacy_fence::reserve(&state),
                                ).await {
                                    Ok(Ok(epoch)) => epoch,
                                    _ => break,
                                };
                                let nonce: [u8; 24] = rand::rng().random();
                                challenge_to_sign = format!("dcl-{}", nonce.iter().map(|b| format!("{b:02x}")).collect::<String>());

                                let Some(ticket) = state.registry.begin_admission(&addr) else {
                                    tracing::error!(addr = %addr, "archipelago admission sequence exhausted");
                                    break;
                                };
                                state
                                    .challenges
                                    .put_for_key(&addr, &challenge_to_sign);
                                let already_connected = state.registry.has_any_peer(&addr);
                                admission = Some(ticket);
                                claimed = Some(addr);
                                if !send_packet(
                                    &mut socket,
                                    server_packet::Message::ChallengeResponse(ChallengeResponseMessage {
                                        challenge_to_sign: challenge_to_sign.clone(),
                                        already_connected,
                                        selection: None,
                                    }),
                                    None,
                                    &state.feed,
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
                                if signed.typed_auth_chain.is_some() {
                                    break;
                                }
                                let chain: AuthChain = match serde_json::from_str(&signed.auth_chain_json) {
                                    Ok(c) => c,
                                    Err(_) => break,
                                };
                                let addr = claimed.clone().unwrap_or_default();
                                match state.challenges.redeem_and_verify_payload(
                                    &addr,
                                    &addr,
                                    &challenge_to_sign,
                                    &challenge_to_sign,
                                    &chain,
                                ) {
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
                                            let _ = send_packet(&mut socket, kicked_packet(), None, &state.feed).await;
                                            break;
                                        }
                                        let session = session_key_of(&chain, &signer);
                                        if !is_session_key(&session) {
                                            tracing::warn!(addr = %signer, "rejecting: auth chain yields no usable session key");
                                            break;
                                        }
                                        let admitted = if let Some(epoch) = authority_epoch {
                                            admission.take();
                                            match tokio::time::timeout_at(
                                                handshake_deadline,
                                                legacy_fence::LegacyFence::claim(&state, &signer, &session, epoch),
                                            ).await {
                                                Ok(Ok((fence, link, rx))) => Some((link, rx, None, Some(fence))),
                                                Ok(Err(crate::control_v4::V4Error::AuthorityConflict)) => None,
                                                _ => {
                                                    tracing::debug!("closing legacy handshake: assignment authority unavailable");
                                                    break;
                                                }
                                            }
                                        } else {
                                            admission.take().and_then(|ticket| ticket.commit(&session))
                                                .map(|(link, rx, previous)| (link, rx, previous, None))
                                        };
                                        let Some((link, rx, previous, fence)) = admitted else {
                                            tracing::info!(addr = %signer, session = %session, "rejecting superseded handshake admission");
                                            let _ = send_packet(&mut socket, kicked_packet(), None, &state.feed).await;
                                            break;
                                        };
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
                                            announces_feed: true,
                                            fence,
                                        });
                                        tracing::info!(addr = %signer, session = %session, "archipelago ws handshake complete (welcome)");
                                        if !send_packet(
                                            &mut socket,
                                            server_packet::Message::Welcome(WelcomeMessage {
                                                peer_id: signer.clone(),
                                                context: None,
                                            }),
                                            registration.as_ref().map(|current| &current.link),
                                            &state.feed,
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
                                        tracing::warn!(addr = %claimed.clone().unwrap_or_default(), reason = e.class(), "archipelago signed-challenge rejected");
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
                                if !catalyrst_types::control_position::valid_realm(&realm) { continue; }
                                if let Some(fence) = current.fence.as_ref() {
                                    match tokio::time::timeout(
                                        AUTHORITY_OPERATION_TIMEOUT,
                                        v4::owns_realm(&state, &fence.owner),
                                    ).await {
                                        Ok(Ok(true)) => {
                                            control_position::publish(&state, &fence.owner, &realm, Some(position));
                                        }
                                        Ok(Ok(false)) => {
                                            let _ = send_packet(&mut socket, kicked_packet(), None, &state.feed).await;
                                            break;
                                        }
                                        _ => break,
                                    }
                                }
                                state.peers.upsert_peer(current.address.clone(), position, parcel, realm);
                            }
                        }
                    }
                    Message::Ping(p) => {
                        if !io::send(&mut socket, Message::Pong(p), registration.as_ref().map(|current| &current.link), &state.feed).await {
                            break;
                        }
                    }
                    Message::Close(_) => break,

                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(handshake_deadline), if stage != Stage::Completed => {
                tracing::debug!("closing socket: the handshake stalled past its timeout");
                break;
            }
            event = next_event(events.as_mut()) => {
                let Some(event) = event else { break };
                match event {
                    SocketEvent::IslandChanged(payload) => {
                        if registration.as_ref().is_some_and(|current| current.fence.is_some()) {
                            continue;
                        }
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
                            registration.as_ref().map(|current| &current.link),
                            &state.feed,
                        )
                        .await
                        {
                            break;
                        }
                        if let Some(current) = registration.as_ref() {
                            current.link.assignment_sent();
                        }
                        state.feed.on_socket_assignment_written();
                    }
                    SocketEvent::Kicked => {
                        let addr = registration.as_ref().map(|r| r.address.clone()).unwrap_or_default();
                        tracing::info!(addr = %addr, "session superseded or kicked; closing socket");
                        let _ = send_packet(&mut socket, kicked_packet(), None, &state.feed).await;
                        break;
                    }
                }
            }
            _ = ownership_poll.tick(), if registration.as_ref().is_some_and(|current| current.fence.is_some()) => {
                let current = registration.as_mut().expect("registered fenced socket");
                if !matches!(tokio::time::timeout(
                    AUTHORITY_OPERATION_TIMEOUT,
                    current.fence.as_mut().unwrap().poll(&state, &mut socket, &current.link),
                ).await, Ok(true)) {
                    break;
                }
            }
            _ = renewal.tick(), if registration.as_ref().is_some_and(|current| current.fence.is_some()) => {
                let owner = &registration.as_ref().unwrap().fence.as_ref().unwrap().owner;
                if tokio::time::timeout(
                    AUTHORITY_OPERATION_TIMEOUT, state.control_v4.renew(owner),
                ).await.is_err() {
                    break;
                }
            }
        }
    }

    drop(socket);
    if let Some(address) = claimed.as_ref() {
        state
            .challenges
            .discard_for_key(address, &challenge_to_sign);
    }
    if let Some(current) = registration {
        current.link.close();
        let released = if let Some(fence) = current.fence {
            control_position::publish(&state, &fence.owner, "", None);
            state
                .registry
                .on_v4_peer_disconnected(&current.address, current.link.id);
            matches!(
                tokio::time::timeout(
                    AUTHORITY_OPERATION_TIMEOUT,
                    state.control_v4.release(&fence.owner),
                )
                .await,
                Ok(Ok(()))
            )
        } else {
            state.registry.on_peer_disconnected(
                &current.address,
                &current.session,
                current.link.id,
            );
            true
        };
        if !state.registry.has_any_peer(&current.address) {
            state.peers.remove_peer(&current.address);
        }
        if current.announces_feed && released {
            state
                .publisher
                .publish(disconnect_subject(&current.address), Vec::new());
        }
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
            "connect announcement not queued; broker reconciliation will retry this live session"
        );
    }
}

fn announce_v4_connect(state: &AppState, target: ReconciliationPeer) {
    if !state.publisher.publish_connect(target) {
        tracing::warn!("v4 connect announcement not queued; broker reconciliation will retry this live realm owner");
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
            sample_sequence: None,
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
            sample_sequence: None,
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
