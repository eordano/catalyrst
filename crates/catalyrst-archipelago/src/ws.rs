use crate::cluster::ClusterEvent;
use crate::proto::archipelago::{
    client_packet, server_packet, ChallengeResponseMessage, ClientPacket, IslandChangedMessage,
    ServerPacket, WelcomeMessage,
};
use crate::proto::Position;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use catalyrst_types::AuthChain;
use prost::Message as _;
use rand::Rng;
use std::collections::HashMap;

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_upgrade))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
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

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.cluster.subscribe();
    let mut stage = Stage::HandshakeStart;
    let mut challenge_to_sign = String::new();
    let mut address: Option<String> = None;
    let mut conn_gen: Option<u64> = None;
    let mut initial_island_sent = false;
    // Per-socket idempotence: the reconnect catch-up push and the broadcast from
    // a kicked recluster can otherwise deliver the SAME island twice back-to-back
    // (observed tripping a despawn race in bevy-explorer's manage_islands).
    // Upstream only ever notifies on actual changes; member updates flow via the
    // LiveKit room, so deduping by island id is protocol-faithful.
    let mut last_island_sent: Option<String> = None;

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
                                if req.address.is_empty() {
                                    break;
                                }
                                let addr = req.address.to_ascii_lowercase();
                                let already_connected = false;

                                challenge_to_sign = format!(
                                    "dcl-{}",
                                    rand::thread_rng().gen::<u64>().to_string()
                                );

                                state.challenges.put(&addr, &challenge_to_sign);
                                address = Some(addr);
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
                            }
                            Stage::ChallengeSent => {
                                let Some(client_packet::Message::SignedChallenge(signed)) = packet.message else {
                                    break;
                                };
                                let chain: AuthChain = match serde_json::from_str(&signed.auth_chain_json) {
                                    Ok(c) => c,
                                    Err(_) => break,
                                };
                                let claimed = address.clone().unwrap_or_default();
                                match state
                                    .challenges
                                    .redeem_and_verify(&claimed, &challenge_to_sign, &chain)
                                {
                                    Ok(()) => {
                                        let signer = chain
                                            .first()
                                            .map(|l| l.payload.to_ascii_lowercase())
                                            .unwrap_or(claimed);
                                        address = Some(signer.clone());
                                        conn_gen = Some(state.cluster.register_conn(&signer));
                                        tracing::info!(addr = %signer, "archipelago ws handshake complete (welcome)");
                                        if !send_packet(
                                            &mut socket,
                                            server_packet::Message::Welcome(WelcomeMessage {
                                                peer_id: signer,
                                            }),
                                        )
                                        .await
                                        {
                                            break;
                                        }
                                        stage = Stage::Completed;
                                    }
                                    Err(e) => {
                                        tracing::warn!(addr = %claimed, err = %e, "archipelago signed-challenge rejected");
                                        break;
                                    }
                                }
                            }
                            Stage::Completed => {
                                let Some(client_packet::Message::Heartbeat(hb)) = packet.message else {
                                    continue;
                                };
                                let Some(addr) = address.clone() else { continue };
                                let pos = hb.position.unwrap_or(Position { x: 0.0, y: 0.0, z: 0.0 });
                                let position = [pos.x, pos.y, pos.z];
                                let parcel = crate::cluster::to_parcel(pos.x, pos.z);
                                let realm = hb.desired_room.unwrap_or_else(|| "catalyrst".into());
                                state.cluster.upsert_peer(addr.clone(), position, parcel, realm);

                                // Reconnect catch-up: IslandChanged only broadcasts on
                                // assignment CHANGES, so a peer that reconnects while its
                                // old assignment is still current would never learn its
                                // island on this socket and the client times out. Push the
                                // current assignment once, directly.
                                if !initial_island_sent {
                                    initial_island_sent = true;
                                    if state.cluster.island_of(&addr).is_none() {
                                        // Fresh peer: don't make it wait for the periodic
                                        // recluster tick — assign now; the IslandChanged
                                        // broadcast delivers it to this socket's rx arm.
                                        state.cluster.kick_recluster();
                                    }
                                    if let Some((island_id, peers)) = state.cluster.island_of(&addr) {
                                        if last_island_sent.as_deref() != Some(island_id.as_str()) {
                                            let conn_str = if state.cluster.livekit().is_armed() {
                                                let g = state.cluster.livekit().mint(&addr, &island_id);
                                                match &g.token {
                                                    Some(tok) => format!("livekit:{}?access_token={}", g.url, tok),
                                                    None => format!("livekit:{}", g.url),
                                                }
                                            } else {
                                                format!("livekit:{}", state.livekit.ws_url())
                                            };
                                            let lookup = state.cluster.peers_by_address();
                                            let mut peer_map: HashMap<String, Position> = HashMap::new();
                                            for p in &peers {
                                                let pos = lookup
                                                    .get(p)
                                                    .map(|ps| Position { x: ps.position[0], y: ps.position[1], z: ps.position[2] })
                                                    .unwrap_or(Position { x: 0.0, y: 0.0, z: 0.0 });
                                                peer_map.insert(p.clone(), pos);
                                            }
                                            last_island_sent = Some(island_id.clone());
                                            if !send_packet(
                                                &mut socket,
                                                server_packet::Message::IslandChanged(IslandChangedMessage {
                                                    island_id,
                                                    conn_str,
                                                    from_island_id: None,
                                                    peers: peer_map,
                                                }),
                                            )
                                            .await
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Message::Ping(p) => { let _ = socket.send(Message::Pong(p)).await; }
                    Message::Close(_) => break,

                    _ => {}
                }
            }
            evt = rx.recv() => {
                let Ok(evt) = evt else { continue };
                let Some(addr) = address.as_deref() else { continue };
                match evt {
                    ClusterEvent::IslandChanged { address: ev_addr, island_id, from_island_id, peers, livekit } if ev_addr == addr => {
                        if last_island_sent.as_deref() == Some(island_id.as_str()) {
                            continue; // already delivered (e.g. by the reconnect catch-up push)
                        }
                        last_island_sent = Some(island_id.clone());
                        let conn_str = match livekit {
                            Some(ref g) => match &g.token {
                                Some(tok) => format!("livekit:{}?access_token={}", g.url, tok),
                                None => format!("livekit:{}", g.url),
                            },
                            None => format!("livekit:{}", state.livekit.ws_url()),
                        };

                        // Upstream archipelago-workers (core/src/adapters/publisher.ts
                        // `onChangeToIsland`) populates `peers` from *every* member of the
                        // target island, keyed by peer id with that peer's last position.
                        // Mirror that: include all island members, falling back to the
                        // origin position when a member has no recorded heartbeat yet.
                        let lookup = state.cluster.peers_by_address();
                        let mut peer_map: HashMap<String, Position> = HashMap::new();
                        for p in &peers {
                            let pos = lookup
                                .get(p)
                                .map(|ps| Position { x: ps.position[0], y: ps.position[1], z: ps.position[2] })
                                .unwrap_or(Position { x: 0.0, y: 0.0, z: 0.0 });
                            peer_map.insert(p.clone(), pos);
                        }
                        if !send_packet(
                            &mut socket,
                            server_packet::Message::IslandChanged(IslandChangedMessage {
                                island_id,
                                conn_str,
                                from_island_id,
                                peers: peer_map,
                            }),
                        )
                        .await
                        {
                            break;
                        }
                    }
                    // NOTE: upstream archipelago-workers never sends LeftIsland/JoinIsland
                    // over /ws. Peer departures are conveyed purely via LiveKit room
                    // membership (the explorer learns a peer left when its LiveKit track
                    // disappears). The PeerLeft cluster event therefore drives no ws frame
                    // here — emitting LeftIslandMessage diverged from the real protocol and
                    // the Unity client ignores that message type entirely.
                    _ => {}
                }
            }
        }
    }

    // Only remove the registration THIS socket made. An unconditional
    // remove_peer here let a stale socket's late close delete the peer a newer
    // reconnect had just registered, cascading into repeated 10s client
    // timeouts on world->genesis realm changes.
    if let (Some(addr), Some(gen)) = (address, conn_gen) {
        tracing::info!(addr = %addr, gen, "archipelago ws closed");
        state.cluster.remove_peer_if_conn(&addr, gen);
    }
}
