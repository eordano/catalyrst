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
use rand::RngExt;
use std::collections::HashMap;

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

fn conn_str(grant: Option<&crate::livekit::LivekitGrant>, fallback_ws_url: &str) -> String {
    match grant {
        Some(g) => match &g.token {
            Some(tok) => format!("livekit:{}?access_token={}", g.url, tok),
            None => format!("livekit:{}", g.url),
        },
        None => format!("livekit:{}", fallback_ws_url),
    }
}

fn heartbeat_position(hb: &crate::proto::archipelago::Heartbeat) -> Option<[f32; 3]> {
    hb.position.as_ref().map(|p| [p.x, p.y, p.z])
}

fn position_map(state: &AppState, peers: &[String]) -> HashMap<String, Position> {
    let lookup = state.cluster.peers_by_address();
    peers
        .iter()
        .map(|p| {
            let pos = lookup
                .get(p)
                .map(|ps| Position {
                    x: ps.position[0],
                    y: ps.position[1],
                    z: ps.position[2],
                })
                .unwrap_or(Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                });
            (p.clone(), pos)
        })
        .collect()
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.cluster.subscribe();
    let mut stage = Stage::HandshakeStart;
    let mut challenge_to_sign = String::new();
    let mut address: Option<String> = None;
    let mut conn_gen: Option<u64> = None;
    let mut initial_island_sent = false;

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
                                challenge_to_sign = format!("dcl-{}", rand::rng().random::<u64>());

                                state.challenges.put(&addr, &challenge_to_sign);
                                address = Some(addr);
                                if !send_packet(
                                    &mut socket,
                                    server_packet::Message::ChallengeResponse(ChallengeResponseMessage {
                                        challenge_to_sign: challenge_to_sign.clone(),
                                        already_connected: false,
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
                                        if state.deny_list.is_denied(&signer).await {
                                            tracing::warn!(addr = %signer, "archipelago ws rejected: deny-listed wallet (post-auth)");
                                            break;
                                        }
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
                                let Some(position) = heartbeat_position(&hb) else { continue };
                                let parcel = crate::cluster::to_parcel(position[0], position[2]);
                                let realm = hb.desired_room.unwrap_or_else(|| "catalyrst".into());
                                state.cluster.upsert_peer(addr.clone(), position, parcel, realm);

                                if !initial_island_sent {
                                    initial_island_sent = true;
                                    if state.cluster.island_of(&addr).is_none() {
                                        state.cluster.kick_recluster().await;
                                    }
                                    if let Some((island_id, peers)) = state.cluster.island_of(&addr) {
                                        if last_island_sent.as_deref() != Some(island_id.as_str()) {
                                            if state.ban_checker.is_banned(&addr).await {
                                                tracing::info!(addr = %addr, island = %island_id, "peer banned; evicting from engine, no livekit token minted");
                                                state.cluster.remove_peer(&addr);
                                            } else {
                                                let grant = state
                                                    .cluster
                                                    .livekit()
                                                    .is_armed()
                                                    .then(|| state.cluster.livekit().mint(&addr, &island_id));
                                                let conn_str = conn_str(grant.as_ref(), state.livekit.ws_url());
                                                let peer_map = position_map(&state, &peers);
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
                            continue;
                        }
                        last_island_sent = Some(island_id.clone());
                        let conn_str = conn_str(livekit.as_ref(), state.livekit.ws_url());
                        let peer_map = position_map(&state, &peers);
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

                    _ => {}
                }
            }
        }
    }

    if let (Some(addr), Some(gen)) = (address, conn_gen) {
        tracing::info!(addr = %addr, gen, "archipelago ws closed");
        state.cluster.remove_peer_if_conn(&addr, gen);
    }
}

#[cfg(test)]
mod tests {
    use super::heartbeat_position;
    use crate::proto::archipelago::Heartbeat;
    use crate::proto::Position;

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
}
