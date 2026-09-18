use super::*;
use crate::control_extensions::{
    canonical_challenge, validate_offer, validate_selection, FEATURE_INHERIT_CONTEXT,
    KNOWN_FEATURES,
};
use crate::proto::archipelago as compatible;

pub(super) struct Wire {
    address: String,
    offer: compatible::ConnectionOffer,
    selection: compatible::ConnectionSelection,
    pub(super) challenge: String,
    last_sample: u64,
}

impl Wire {
    fn lane(&self, handle: u32) -> Result<String, V4Error> {
        self.selection
            .lanes
            .iter()
            .find(|lane| lane.handle == handle)
            .map(|lane| lane.name.clone())
            .ok_or(V4Error::InvalidLane)
    }

    fn handle(&self, name: &str) -> Option<u32> {
        self.selection
            .lanes
            .iter()
            .find(|lane| lane.name == name)
            .map(|lane| lane.handle)
    }

    pub(super) fn encode(&self, packet: &pb::ServerPacket) -> Option<Vec<u8>> {
        use compatible::server_packet::Message as Out;
        let message = match packet.message.as_ref()? {
            pb::server_packet::Message::Welcome(welcome) => {
                Out::Welcome(compatible::WelcomeMessage {
                    peer_id: welcome.peer_id.clone(),
                    context: (self.selection.selected_features & FEATURE_INHERIT_CONTEXT == 0)
                        .then(|| self.selection.clone()),
                })
            }
            pb::server_packet::Message::AssignmentSnapshot(snapshot) => {
                let assignment = snapshot.assignment.clone().unwrap_or_default();
                Out::IslandChanged(compatible::IslandChangedMessage {
                    island_id: assignment.island_id,
                    conn_str: assignment.connection_string,
                    from_island_id: assignment.from_island_id,
                    peers: assignment.peers.into_iter().collect(),
                    assignment_context: Some(compatible::AssignmentContext {
                        lane_handle: self.handle(&snapshot.lane)?,
                        assignment_revision: snapshot.assignment_revision,
                        realm_revision: snapshot.realm_revision,
                        fencing_token: snapshot.fencing_token,
                        lease_expires_at_ms: 0,
                        removed: snapshot.assignment.is_none(),
                    }),
                })
            }
            pb::server_packet::Message::Ack(ack) => Out::Ack(compatible::Ack {
                lane_handle: self.handle(&ack.lane)?,
                applied_revision: ack.applied_revision,
            }),
            pb::server_packet::Message::Error(error) => Out::Error(compatible::ControlError {
                code: error.code,
                retry: error.retry,
                public_detail: None,
            }),
            _ => return None,
        };
        Some(
            compatible::ServerPacket {
                message: Some(message),
                request_sequence: packet.request_sequence,
            }
            .encode_to_vec(),
        )
    }

    fn translate(
        &self,
        packet: compatible::ClientPacket,
        epoch: u64,
    ) -> Result<pb::ClientPacket, V4Error> {
        use compatible::client_packet::Message as In;
        let request_id = packet.request_sequence.to_string();
        let session_id = format!("0x{}", hex(&self.offer.session_id));
        let message = match packet.message.ok_or(V4Error::InvalidFrame)? {
            In::SignedChallenge(signed) => {
                if !signed.auth_chain_json.is_empty() || signed.typed_auth_chain.is_none() {
                    return Err(V4Error::AuthFailed);
                }
                pb::client_packet::Message::Authenticate(pb::Authenticate {
                    request_id,
                    address: self.address.clone(),
                    session_id,
                    connection_epoch: epoch,
                    auth_chain: signed.typed_auth_chain,
                    lanes: self.offer.lanes.clone(),
                    ..Default::default()
                })
            }
            In::Ack(ack) => pb::client_packet::Message::Ack(pb::Ack {
                operation_id: request_id.clone(),
                request_id,
                session_id,
                connection_epoch: epoch,
                lane: self.lane(ack.lane_handle)?,
                applied_revision: ack.applied_revision,
            }),
            In::RequestSnapshot(snapshot) => {
                pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                    operation_id: request_id.clone(),
                    request_id,
                    session_id,
                    connection_epoch: epoch,
                    lane: self.lane(snapshot.lane_handle)?,
                    from_revision: snapshot.from_revision,
                })
            }
            _ => return Err(V4Error::Protocol),
        };
        Ok(pb::ClientPacket {
            message: Some(message),
            request_sequence: packet.request_sequence,
        })
    }
}

pub(in crate::ws) async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    opening: compatible::ChallengeRequestMessage,
    deadline: tokio::time::Instant,
) {
    let timeout_ms = state.cfg.auth.handshake_timeout_ms.min(30_000);
    let deadline = deadline.min(tokio::time::Instant::now() + Duration::from_millis(timeout_ms));
    let result = tokio::time::timeout_at(deadline, prepare(&state, opening)).await;
    let conn = match result {
        Ok(Ok(conn)) => conn,
        result => {
            let error = match result {
                Ok(Err(error)) => error,
                _ => V4Error::AuthorityUnavailable,
            };
            let public = error.public();
            let packet = compatible::ServerPacket {
                message: Some(compatible::server_packet::Message::Error(
                    compatible::ControlError {
                        code: public.code as i32,
                        retry: retry_class_to_proto(public.retry),
                        public_detail: None,
                    },
                )),
                request_sequence: 0,
            };
            let _ = io::send(
                &mut socket,
                Message::Binary(packet.encode_to_vec().into()),
                None,
                &state.feed,
            )
            .await;
            return;
        }
    };
    let wire = conn
        .extensions
        .as_ref()
        .expect("prepared connection has extensions");
    let packet = compatible::ServerPacket {
        message: Some(compatible::server_packet::Message::ChallengeResponse(
            compatible::ChallengeResponseMessage {
                challenge_to_sign: wire.challenge.clone(),
                already_connected: state.registry.has_peer(&wire.address),
                selection: Some(wire.selection.clone()),
            },
        )),
        request_sequence: 0,
    };
    state
        .challenges
        .put_for_key(&conn.storage_key(), &conn.challenge_id);
    if !send_bytes(&mut socket, &state, &conn, packet.encode_to_vec()).await {
        cleanup(&state, conn).await;
        return;
    }
    run_socket(socket, state, conn, deadline).await;
}

async fn prepare(
    state: &AppState,
    opening: compatible::ChallengeRequestMessage,
) -> Result<Connection, V4Error> {
    let timeout_ms = state.cfg.auth.handshake_timeout_ms.min(30_000);
    let address = opening.address.to_ascii_lowercase();
    if !is_address(&address) {
        return Err(V4Error::Protocol);
    }
    let offer = opening.offer.ok_or(V4Error::Protocol)?;
    validate_offer(&offer, KNOWN_FEATURES).map_err(|_| V4Error::Protocol)?;
    if state.deny_list.is_denied(&address).await {
        return Err(V4Error::AuthFailed);
    }
    let mut conn = new_connection(state).await?;
    let mut nonce = [0u8; 32];
    rand::rng().fill_bytes(&mut nonce);
    conn.challenge_id = hex(&nonce);
    let selection = compatible::ConnectionSelection {
        version: 4,
        selected_features: offer.offered_features & KNOWN_FEATURES,
        context: Some(compatible::ConnectionContext {
            server_nonce: nonce.to_vec(),
            audience: state.cfg.server.control_v4_audience.clone(),
            issuer: state.replica_id.clone(),
            process_incarnation: Uuid::parse_str(&state.replica_id)
                .map_err(|_| V4Error::AuthorityUnavailable)?
                .as_bytes()
                .to_vec(),
            connection_id: Uuid::parse_str(&conn.connection_id)
                .map_err(|_| V4Error::AuthorityUnavailable)?
                .as_bytes()
                .to_vec(),
            connection_epoch: conn.connection_epoch,
            expires_at_ms: (chrono::Utc::now().timestamp_millis() as u64)
                .checked_add(timeout_ms)
                .ok_or(V4Error::Protocol)?,
            authority_incarnation: state.control_v4.incarnation().to_string(),
        }),
        limits: Some(compatible::ConnectionLimits {
            max_frame_bytes: MAX_FRAME_BYTES as u32,
            max_lanes: MAX_LANES_PER_SOCKET as u32,
            max_pending_requests: MAX_RETAINED_REQUEST_SEQUENCES as u32,
            max_retained_responses: MAX_RETAINED_REQUEST_SEQUENCES as u32,
            max_retained_response_bytes: MAX_RETAINED_RESPONSE_BYTES as u32,
            handshake_timeout_ms: timeout_ms as u32,
            max_public_detail_bytes: 256,
        }),
        lanes: offer
            .lanes
            .iter()
            .enumerate()
            .map(|(index, name)| compatible::LaneBinding {
                handle: if name == "realm" {
                    0
                } else {
                    index as u32 + u32::from(offer.lanes[0] != "realm")
                },
                name: name.clone(),
            })
            .collect(),
    };
    validate_selection(&offer, &selection, KNOWN_FEATURES).map_err(|_| V4Error::Protocol)?;
    let challenge =
        canonical_challenge(&address, &offer, &selection).map_err(|_| V4Error::Protocol)?;
    conn.extensions = Some(Wire {
        address,
        offer,
        selection,
        challenge,
        last_sample: 0,
    });
    Ok(conn)
}

pub(super) async fn handle_binary(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    bytes: &[u8],
) -> bool {
    if bytes.len() > MAX_FRAME_BYTES {
        return false;
    }
    let packet = match compatible::ClientPacket::decode(bytes) {
        Ok(packet) => packet,
        Err(_) => {
            return send_error(
                state,
                socket,
                conn,
                0,
                None,
                None,
                None,
                V4Error::InvalidFrame,
            )
            .await
        }
    };
    let sequence = packet.request_sequence;
    if let Some(compatible::client_packet::Message::Heartbeat(heartbeat)) = packet.message.as_ref()
    {
        let Some(owner) = conn.owner.as_ref() else {
            return send_error(
                state,
                socket,
                conn,
                sequence,
                None,
                None,
                None,
                V4Error::Protocol,
            )
            .await;
        };
        if sequence != 0
            || heartbeat.sample_sequence == Some(0)
            || heartbeat
                .desired_room
                .as_ref()
                .is_some_and(|room| !catalyrst_types::control_position::valid_realm(room))
            || heartbeat
                .position
                .as_ref()
                .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
        {
            return send_error(
                state,
                socket,
                conn,
                sequence,
                None,
                None,
                None,
                V4Error::InvalidFrame,
            )
            .await;
        }
        let wire = conn.extensions.as_mut().expect("extension mode is pinned");
        if let Some(sample) = heartbeat.sample_sequence {
            if sample <= wire.last_sample {
                return true;
            }
            wire.last_sample = sample;
        }
        if let Some(p) = heartbeat.position.as_ref() {
            match owns_realm(state, owner).await {
                Ok(true) => {
                    let realm = heartbeat
                        .desired_room
                        .clone()
                        .unwrap_or_else(|| "catalyrst".into());
                    super::super::control_position::publish(
                        state,
                        owner,
                        &realm,
                        Some([p.x, p.y, p.z]),
                    );
                    state.peers.upsert_peer(
                        owner.address.clone(),
                        [p.x, p.y, p.z],
                        crate::peers::to_parcel(p.x, p.z),
                        realm,
                    );
                }
                Ok(false) => {}
                Err(error) => {
                    return send_error(state, socket, conn, sequence, None, None, None, error).await
                }
            }
        }
        return true;
    }
    let translated = conn
        .extensions
        .as_ref()
        .expect("extension mode is pinned")
        .translate(packet, conn.connection_epoch);
    match translated {
        Ok(packet) => handle_packet(state, socket, conn, packet, bytes).await,
        Err(error) => {
            if conn.owner.is_none() {
                discard_auth_challenge(state.challenges.as_ref(), conn);
            }
            let wire = conn.extensions.as_ref().expect("extension mode is pinned");
            let metadata = pb::ClientPacket {
                request_sequence: sequence,
                message: Some(pb::client_packet::Message::Ack(pb::Ack {
                    request_id: sequence.to_string(),
                    session_id: format!("0x{}", hex(&wire.offer.session_id)),
                    connection_epoch: conn.connection_epoch,
                    ..Default::default()
                })),
            };
            match begin_request(state, socket, conn, &metadata, bytes).await {
                RequestStart::Reserved(request) => {
                    send_error_for_request(state, socket, conn, &request, error).await
                }
                RequestStart::Replayed | RequestStart::Rejected => true,
            }
        }
    }
}
