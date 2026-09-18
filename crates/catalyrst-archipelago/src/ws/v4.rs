use super::{announce_v4_connect, io, Registration, AUTHORITY_OPERATION_TIMEOUT};
use crate::control_v4::{
    canonical_signing_payload, AssignmentPayload, LaneKey, PublicError, RetryClass,
    SigningTranscript, V4Error, V4Owner, MAX_FRAME_BYTES, MAX_LANES_PER_SOCKET,
    MAX_OPERATION_ID_BYTES, MAX_REQUEST_ID_BYTES, OWNER_RENEWAL_INTERVAL, PROTOCOL_VERSION,
    SUBPROTOCOL,
};
use crate::feed::disconnect_subject;
use crate::proto::archipelago_v4 as pb;
use crate::registry::ReconciliationPeer;
use crate::registry::{SocketEvent, SocketEvents};
use crate::session::{is_address, is_session_key, session_key_of};
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use catalyrst_crypto::hash_dynamic;
use catalyrst_types::{AuthChain, AuthLink, AuthLinkType, MAX_AUTH_CHAIN_LINKS};
use prost::Message as _;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;
use uuid::Uuid;

const MAX_RETAINED_REQUEST_SEQUENCES: usize = 256;
const MAX_RETAINED_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_AUTH_CHAIN_BYTES: usize = 3_072;
const RECOVERY_INTERVAL: Duration = Duration::from_secs(2);
const RELEASE_BUDGET: Duration = Duration::from_secs(2);

mod extensions;
pub(super) use extensions::handle_socket as handle_extensions;

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws/v4", get(ws_upgrade))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.protocols([SUBPROTOCOL])
        .read_buffer_size(16 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_FRAME_BYTES)
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

struct Connection {
    extensions: Option<extensions::Wire>,
    connection_id: String,
    connection_epoch: u64,
    challenge_id: String,
    challenge: String,
    capabilities: Vec<String>,
    registration: Option<Registration>,
    events: Option<SocketEvents>,
    lanes: Vec<LaneKey>,
    owner: Option<V4Owner>,
    auth_attempted: bool,
    request_high_water: u64,
    requests: HashMap<u64, RequestRecord>,
    request_order: VecDeque<u64>,
    pinned_auth_sequence: Option<u64>,
    retained_response_bytes: usize,
    last_sent: HashMap<String, u64>,
}

struct RequestRecord {
    fingerprint: [u8; 32],
    response: Option<Vec<u8>>,
}

enum RequestStart {
    Reserved(RequestContext),
    Replayed,
    Rejected,
}

struct RequestContext {
    request_sequence: u64,
    request_id: String,
    fingerprint: [u8; 32],
    session_id: Option<String>,
    connection_epoch: Option<u64>,
}

impl Connection {
    fn new(
        connection_id: String,
        connection_epoch: u64,
        challenge_id: String,
        challenge: String,
    ) -> Self {
        Self {
            extensions: None,
            connection_id,
            connection_epoch,
            challenge_id,
            challenge,
            capabilities: server_capabilities(),
            registration: None,
            events: None,
            lanes: Vec::new(),
            owner: None,
            auth_attempted: false,
            request_high_water: 0,
            requests: HashMap::new(),
            request_order: VecDeque::new(),
            pinned_auth_sequence: None,
            retained_response_bytes: 0,
            last_sent: HashMap::new(),
        }
    }

    fn storage_key(&self) -> String {
        format!("v4:{}", self.connection_id)
    }
}

pub(super) async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let handshake_timeout = Duration::from_millis(state.cfg.auth.handshake_timeout_ms);
    let conn = match tokio::time::timeout(handshake_timeout, new_connection(&state)).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => {
            let _ = send_boot_error(&state, &mut socket, e).await;
            return;
        }
        Err(_) => {
            let _ = send_boot_error(&state, &mut socket, V4Error::AuthorityUnavailable).await;
            return;
        }
    };
    let handshake_deadline = tokio::time::Instant::now() + handshake_timeout;
    let hello = pb::Hello {
        protocol_version: PROTOCOL_VERSION as u32,
        audience: state.cfg.server.control_v4_audience.clone(),
        replica_id: state.replica_id.clone(),
        authority_incarnation: state.control_v4.incarnation().to_string(),
        connection_id: conn.connection_id.clone(),
        connection_epoch: conn.connection_epoch,
        challenge_id: conn.challenge_id.clone(),
        challenge: conn.challenge.clone(),
        expires_in_ms: handshake_timeout.as_millis() as u64,
        capabilities: conn.capabilities.clone(),
        limits: Some(pb::Limits {
            max_frame_bytes: MAX_FRAME_BYTES as u64,
            max_lanes_per_socket: MAX_LANES_PER_SOCKET as u64,
            max_request_id_bytes: MAX_REQUEST_ID_BYTES as u64,
            max_requests_per_connection: u64::MAX,
            max_retained_response_bytes: MAX_RETAINED_RESPONSE_BYTES as u64,
            handshake_timeout_ms: state.cfg.auth.handshake_timeout_ms,
            max_retained_request_sequences: MAX_RETAINED_REQUEST_SEQUENCES as u64,
        }),
    };
    state
        .challenges
        .put_for_key(&conn.storage_key(), &conn.challenge_id);
    if !send_frame(
        &mut socket,
        &state,
        &conn,
        &server_packet(pb::server_packet::Message::Hello(hello)),
    )
    .await
    {
        cleanup(&state, conn).await;
        return;
    }
    run_socket(socket, state, conn, handshake_deadline).await;
}

async fn run_socket(
    mut socket: WebSocket,
    state: AppState,
    mut conn: Connection,
    mut handshake_deadline: tokio::time::Instant,
) {
    let handshake_timeout = Duration::from_millis(state.cfg.auth.handshake_timeout_ms);
    let mut recovery = tokio::time::interval(RECOVERY_INTERVAL);
    recovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut renewal = tokio::time::interval(OWNER_RENEWAL_INTERVAL);
    renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        if conn
            .registration
            .as_ref()
            .is_some_and(|current| current.link.is_closed())
        {
            break;
        }
        if conn.registration.is_none() && tokio::time::Instant::now() >= handshake_deadline {
            break;
        }
        tokio::select! {
            incoming = socket.recv() => {
                let Some(msg) = incoming else { break };
                let msg = match msg { Ok(m) => m, Err(_) => break };
                match msg {
                    Message::Text(_) => {
                        let _ = send_error(&state, &mut socket, &conn, 0, None, None, None, V4Error::InvalidFrame).await;
                    }
                    Message::Binary(bytes) => {
                        let handled = if conn.registration.is_none() {
                            matches!(
                                tokio::time::timeout_at(
                                    handshake_deadline,
                                    handle_binary(&state, &mut socket, &mut conn, bytes.as_ref()),
                                )
                                .await,
                                Ok(true)
                            )
                        } else {
                            handle_binary(&state, &mut socket, &mut conn, bytes.as_ref()).await
                        };
                        if !handled {
                            break;
                        }
                        if conn.registration.is_some() {
                            handshake_deadline = tokio::time::Instant::now() + handshake_timeout;
                        }
                    }
                    Message::Ping(p) => {
                        if !send_message(&mut socket, &state, &conn, Message::Pong(p)).await {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(handshake_deadline), if conn.registration.is_none() => {
                break;
            }
            event = next_event(conn.events.as_mut()) => {
                let Some(event) = event else { break };
                match event {
                    SocketEvent::IslandChanged(_) => {}
                    SocketEvent::Kicked => {
                        let _ = send_error(&state, &mut socket, &conn, 0, None, conn.owner.as_ref().map(|o| o.session.as_str()), Some(conn.connection_epoch), V4Error::AuthorityConflict).await;
                        break;
                    }
                }
            }
            _ = recovery.tick(), if conn.owner.is_some() => {
                if !matches!(tokio::time::timeout(
                    AUTHORITY_OPERATION_TIMEOUT, recover_snapshots(&state, &mut socket, &mut conn),
                ).await, Ok(true)) {
                    break;
                }
            }
            _ = renewal.tick(), if conn.owner.is_some() => {
                if let Some(owner) = conn.owner.as_ref() {
                    if tokio::time::timeout(
                        AUTHORITY_OPERATION_TIMEOUT, state.control_v4.renew(owner),
                    ).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    drop(socket);
    cleanup(&state, conn).await;
}

async fn handle_binary(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    bytes: &[u8],
) -> bool {
    if conn.extensions.is_some() {
        return extensions::handle_binary(state, socket, conn, bytes).await;
    }
    let packet = match pb::ClientPacket::decode(bytes) {
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
            .await;
        }
    };
    if packet.message.is_none() {
        return send_error(
            state,
            socket,
            conn,
            packet.request_sequence,
            None,
            None,
            None,
            V4Error::InvalidFrame,
        )
        .await;
    }
    let canonical = packet.encode_to_vec();
    handle_packet(state, socket, conn, packet, &canonical).await
}

async fn handle_packet(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    packet: pb::ClientPacket,
    fingerprint_bytes: &[u8],
) -> bool {
    let request = match begin_request(state, socket, conn, &packet, fingerprint_bytes).await {
        RequestStart::Reserved(request) => request,
        RequestStart::Replayed => return true,
        RequestStart::Rejected => return true,
    };
    match packet.message {
        Some(pb::client_packet::Message::Authenticate(frame)) => {
            authenticate(state, socket, conn, request, frame).await
        }
        Some(pb::client_packet::Message::Heartbeat(frame)) => {
            heartbeat(state, socket, conn, request, frame).await
        }
        Some(pb::client_packet::Message::Ack(frame)) => {
            ack(state, socket, conn, request, frame).await
        }
        Some(pb::client_packet::Message::RequestSnapshot(frame)) => {
            snapshot(state, socket, conn, request, frame).await
        }
        None => send_error_for_request(state, socket, conn, &request, V4Error::InvalidFrame).await,
    }
}

async fn authenticate(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: RequestContext,
    frame: pb::Authenticate,
) -> bool {
    if conn.registration.is_some() {
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    }
    if !begin_auth_attempt(conn) {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed).await;
    }
    if validate_request_id(&frame.request_id).is_err()
        || frame.connection_epoch != conn.connection_epoch
        || !is_address(&frame.address.to_ascii_lowercase())
        || !is_session_key(&frame.session_id.to_ascii_lowercase())
    {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    }
    let address = frame.address.to_ascii_lowercase();
    let session_id = frame.session_id.to_ascii_lowercase();
    if state.deny_list.is_denied(&address).await {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed).await;
    }
    let lanes = match parse_lanes(&frame.lanes) {
        Ok(lanes) => lanes,
        Err(e) => {
            discard_auth_challenge(state.challenges.as_ref(), conn);
            return send_error_for_request(state, socket, conn, &request, e).await;
        }
    };
    if unsupported_required_capability(&frame.capabilities, &conn.capabilities) {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    }
    let auth_chain: AuthChain = match frame.auth_chain.as_ref().map(auth_chain_from_proto) {
        Some(Ok(chain)) => chain,
        _ => {
            discard_auth_challenge(state.challenges.as_ref(), conn);
            return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed)
                .await;
        }
    };
    let transcript = conn
        .extensions
        .as_ref()
        .map(|wire| wire.challenge.clone())
        .unwrap_or_else(|| {
            canonical_signing_payload(SigningTranscript {
                audience: &state.cfg.server.control_v4_audience,
                replica_id: &state.replica_id,
                authority_incarnation: state.control_v4.incarnation(),
                connection_id: &conn.connection_id,
                connection_epoch: frame.connection_epoch,
                request_id: &frame.request_id,
                expires_in_ms: state.cfg.auth.handshake_timeout_ms,
                challenge_id: &conn.challenge_id,
                challenge: &conn.challenge,
                address: &address,
                session_id: &session_id,
                lanes: &lanes,
                capabilities: &frame.capabilities,
            })
        });
    if state
        .challenges
        .redeem_and_verify_payload(
            &conn.storage_key(),
            &address,
            &conn.challenge_id,
            &transcript,
            &auth_chain,
        )
        .is_err()
    {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed).await;
    }
    let signer = auth_chain
        .first()
        .map(|l| l.payload.to_ascii_lowercase())
        .unwrap_or_else(|| address.clone());
    if signer != address
        || state.deny_list.is_denied(&signer).await
        || state.ban_checker.is_banned(&signer).await
    {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed).await;
    }
    let chain_session = session_key_of(&auth_chain, &signer);
    if chain_session != session_id {
        discard_auth_challenge(state.challenges.as_ref(), conn);
        return send_error_for_request(state, socket, conn, &request, V4Error::AuthFailed).await;
    }
    let owner = V4Owner {
        address: signer.clone(),
        session: session_id.clone(),
        epoch: conn.connection_epoch,
    };
    let claimed = match state.control_v4.claim_lanes(&owner, &lanes).await {
        Ok(claimed) => claimed,
        Err(e) => {
            discard_auth_challenge(state.challenges.as_ref(), conn);
            return send_error_for_request(state, socket, conn, &request, e).await;
        }
    };
    let _resume_requested = claimed
        .iter()
        .any(|lane| frame.resume.contains_key(&lane.lane));
    let realm_owner = lanes.iter().any(|lane| lane.as_str() == "realm");
    let (link, rx) = state.registry.on_v4_peer_connected_with_realm_epoch(
        &signer,
        &session_id,
        &conn.connection_id,
        realm_owner,
        conn.connection_epoch,
    );
    let announcement = realm_owner.then(|| ReconciliationPeer {
        address: signer.clone(),
        session: session_id.clone(),
        id: link.id,
        connection_id: Some(conn.connection_id.clone()),
    });
    conn.registration = Some(Registration {
        fence: None,
        address: signer.clone(),
        session: session_id.clone(),
        link,
        announces_feed: realm_owner,
    });
    conn.events = Some(rx);
    conn.lanes = lanes;
    conn.owner = Some(owner.clone());
    if let Some(announcement) = announcement {
        announce_v4_connect(state, announcement);
    }
    conn.last_sent = claimed
        .iter()
        .map(|lane| (lane.lane.clone(), lane.assignment_revision))
        .collect();
    let welcome_realm_assignment = conn
        .extensions
        .is_none()
        .then(|| welcome_realm_assignment(&claimed))
        .flatten();
    let (welcome_lanes, initial_snapshots) = if conn.extensions.is_some() {
        (Vec::new(), claimed)
    } else {
        (
            claimed.into_iter().map(assignment_state_to_proto).collect(),
            Vec::new(),
        )
    };
    let response = server_packet(pb::server_packet::Message::Welcome(pb::Welcome {
        request_id: frame.request_id.clone(),
        session_id,
        connection_epoch: frame.connection_epoch,
        peer_id: signer,
        lanes: welcome_lanes,
    }));
    if !send_for_request(state, socket, conn, &request, &response).await {
        return false;
    }
    if let Some(assigned) = welcome_realm_assignment {
        if let Some(registration) = conn.registration.as_ref() {
            record_assignment_delivery(&registration.link, true, assigned);
        }
    }
    for snapshot in initial_snapshots {
        let realm_assignment = snapshot.lane == "realm";
        let assigned = snapshot.assignment.is_some();
        let frame = server_packet(pb::server_packet::Message::AssignmentSnapshot(
            frame_from_state(None, &owner, snapshot),
        ));
        if !send_frame(socket, state, conn, &frame).await {
            return false;
        }
        if realm_assignment {
            if let Some(registration) = conn.registration.as_ref() {
                record_assignment_delivery(&registration.link, true, assigned);
            }
        }
        state.feed.on_socket_assignment_written();
    }
    true
}

async fn heartbeat(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: RequestContext,
    frame: pb::Heartbeat,
) -> bool {
    let Some(owner) = conn.owner.as_ref() else {
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    };
    let compact = compact_heartbeat(&frame);
    if !compact
        && !valid_current_request(
            owner,
            &frame.request_id,
            &frame.operation_id,
            &frame.session_id,
            frame.connection_epoch,
        )
    {
        return send_error_for_request(state, socket, conn, &request, V4Error::StaleEpoch).await;
    }
    if frame
        .desired_room
        .as_deref()
        .is_some_and(|realm| !catalyrst_types::control_position::valid_realm(realm))
    {
        return send_error_for_request(state, socket, conn, &request, V4Error::InvalidFrame).await;
    }
    if let Some(position) = frame.position {
        let position = [position.x, position.y, position.z];
        if position.iter().any(|component| !component.is_finite()) {
            return send_error_for_request(state, socket, conn, &request, V4Error::InvalidFrame)
                .await;
        }
        match owns_realm(state, owner).await {
            Ok(true) => {
                let parcel = crate::peers::to_parcel(position[0], position[2]);
                let realm = frame.desired_room.unwrap_or_else(|| "catalyrst".into());
                super::control_position::publish(state, owner, &realm, Some(position));
                state
                    .peers
                    .upsert_peer(owner.address.clone(), position, parcel, realm);
            }
            Ok(false) => {}
            Err(error) => {
                return send_error_for_request(state, socket, conn, &request, error).await
            }
        }
    }
    send_for_request(
        state,
        socket,
        conn,
        &request,
        &server_packet(pb::server_packet::Message::Ack(if compact {
            pb::Ack::default()
        } else {
            pb::Ack {
                request_id: frame.request_id.clone(),
                session_id: owner.session.clone(),
                connection_epoch: owner.epoch,
                lane: "realm".into(),
                applied_revision: 0,
                operation_id: String::new(),
            }
        })),
    )
    .await
}

pub(super) async fn owns_realm(state: &AppState, owner: &V4Owner) -> Result<bool, V4Error> {
    let realm = LaneKey::parse("realm").expect("realm lane is valid");
    match tokio::time::timeout(
        AUTHORITY_OPERATION_TIMEOUT,
        state.control_v4.snapshot(owner, &realm),
    )
    .await
    .unwrap_or(Err(V4Error::AuthorityUnavailable))
    {
        Ok(_) => Ok(true),
        Err(V4Error::AuthorityConflict) => Ok(false),
        Err(error) => Err(error),
    }
}

async fn ack(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: RequestContext,
    frame: pb::Ack,
) -> bool {
    let Some(owner) = conn.owner.as_ref() else {
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    };
    if !valid_current_request(
        owner,
        &frame.request_id,
        &frame.operation_id,
        &frame.session_id,
        frame.connection_epoch,
    ) {
        return send_error_for_request(state, socket, conn, &request, V4Error::StaleEpoch).await;
    }
    let lane = match LaneKey::parse(&frame.lane) {
        Ok(lane) => lane,
        Err(e) => return send_error_for_request(state, socket, conn, &request, e).await,
    };
    match state
        .control_v4
        .ack(owner, &lane, frame.applied_revision)
        .await
    {
        Ok(_) => {
            let response = server_packet(pb::server_packet::Message::Ack(pb::Ack {
                request_id: frame.request_id.clone(),
                operation_id: String::new(),
                session_id: owner.session.clone(),
                connection_epoch: owner.epoch,
                lane: lane.as_str().to_string(),
                applied_revision: frame.applied_revision,
            }));
            send_for_request(state, socket, conn, &request, &response).await
        }
        Err(e) => send_error_for_request(state, socket, conn, &request, e).await,
    }
}

async fn snapshot(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: RequestContext,
    frame: pb::RequestSnapshot,
) -> bool {
    let Some(owner) = conn.owner.as_ref() else {
        return send_error_for_request(state, socket, conn, &request, V4Error::Protocol).await;
    };
    if !valid_current_request(
        owner,
        &frame.request_id,
        &frame.operation_id,
        &frame.session_id,
        frame.connection_epoch,
    ) {
        return send_error_for_request(state, socket, conn, &request, V4Error::StaleEpoch).await;
    }
    let lane = match LaneKey::parse(&frame.lane) {
        Ok(lane) => lane,
        Err(e) => return send_error_for_request(state, socket, conn, &request, e).await,
    };
    match state.control_v4.snapshot(owner, &lane).await {
        Ok(snapshot_state) => {
            if frame
                .from_revision
                .is_some_and(|from| from > snapshot_state.assignment_revision)
            {
                return send_error_for_request(
                    state,
                    socket,
                    conn,
                    &request,
                    V4Error::GapRequiresSnapshot,
                )
                .await;
            }
            let realm_assignment = snapshot_state.lane == "realm";
            let assigned = snapshot_state.assignment.is_some();
            conn.last_sent.insert(
                snapshot_state.lane.clone(),
                snapshot_state.assignment_revision,
            );
            let response = server_packet(pb::server_packet::Message::AssignmentSnapshot(
                frame_from_state(Some(frame.request_id.clone()), owner, snapshot_state),
            ));
            let sent = send_for_request(state, socket, conn, &request, &response).await;
            if sent && realm_assignment {
                if let Some(registration) = conn.registration.as_ref() {
                    record_assignment_delivery(&registration.link, true, assigned);
                }
            }
            sent
        }
        Err(e) => send_error_for_request(state, socket, conn, &request, e).await,
    }
}

fn frame_from_state(
    request_id: Option<String>,
    owner: &V4Owner,
    state: crate::control_v4::AssignmentState,
) -> pb::AssignmentSnapshot {
    pb::AssignmentSnapshot {
        request_id,
        session_id: owner.session.clone(),
        connection_epoch: owner.epoch,
        lane: state.lane,
        authority_incarnation: state.authority_incarnation,
        assignment_revision: state.assignment_revision,
        realm_revision: state.realm_revision,
        owner_session: state.owner_session,
        owner_epoch: state.owner_epoch,
        fencing_token: state.fencing_token,
        assignment: state.assignment.and_then(assignment_to_proto),
    }
}

async fn recover_snapshots(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
) -> bool {
    let Some(owner) = conn.owner.clone() else {
        return true;
    };
    let lanes = conn.lanes.clone();
    let mut failure = None;
    for lane in lanes {
        let snapshot = match state.control_v4.snapshot(&owner, &lane).await {
            Ok(snapshot) => snapshot,
            Err(V4Error::AuthorityConflict) => {
                conn.lanes.retain(|held| held != &lane);
                failure = Some(V4Error::AuthorityConflict);
                continue;
            }
            Err(e) => {
                failure.get_or_insert(e);
                continue;
            }
        };
        let previous = conn.last_sent.get(&snapshot.lane).copied().unwrap_or(0);
        if snapshot.assignment_revision <= previous {
            continue;
        }
        let realm_assignment = snapshot.lane == "realm";
        let assigned = snapshot.assignment.is_some();
        conn.last_sent
            .insert(snapshot.lane.clone(), snapshot.assignment_revision);
        let frame = server_packet(pb::server_packet::Message::AssignmentSnapshot(
            frame_from_state(None, &owner, snapshot),
        ));
        if !send_frame(socket, state, conn, &frame).await {
            return false;
        }
        if realm_assignment {
            if let Some(registration) = conn.registration.as_ref() {
                record_assignment_delivery(&registration.link, true, assigned);
            }
        }
        state.feed.on_socket_assignment_written();
    }
    let Some(error) = failure else {
        return true;
    };
    let unavailable = matches!(error, V4Error::AuthorityUnavailable);
    let delivered = send_error(
        state,
        socket,
        conn,
        0,
        None,
        Some(&owner.session),
        Some(owner.epoch),
        error,
    )
    .await;
    delivered && !unavailable && !conn.lanes.is_empty()
}

fn record_assignment_delivery(link: &crate::registry::PeerLink, realm: bool, assigned: bool) {
    if !realm {
        return;
    }
    if assigned {
        link.assignment_sent();
    } else {
        link.assignment_cleared();
    }
}

fn welcome_realm_assignment(assignments: &[crate::control_v4::AssignmentState]) -> Option<bool> {
    assignments
        .iter()
        .find(|assignment| assignment.lane == "realm")
        .map(|assignment| assignment.assignment.is_some())
}

fn unsupported_required_capability(requested: &[String], supported: &[String]) -> bool {
    requested.iter().any(|capability| {
        capability
            .strip_prefix("required:")
            .is_some_and(|required| !supported.iter().any(|held| held == required))
    })
}

async fn begin_request(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    packet: &pb::ClientPacket,
    canonical: &[u8],
) -> RequestStart {
    let request_sequence = packet.request_sequence;
    let compact_owner = match packet.message.as_ref() {
        Some(pb::client_packet::Message::Heartbeat(frame)) if compact_heartbeat(frame) => {
            conn.owner.as_ref()
        }
        _ => None,
    };
    let (request_id, session_id, connection_epoch) = match client_request_metadata(packet) {
        Some(("", "", 0)) if compact_owner.is_some() => {
            let owner = compact_owner.unwrap();
            (
                String::new(),
                Some(owner.session.clone()),
                Some(owner.epoch),
            )
        }
        Some((request_id, session_id, connection_epoch))
            if validate_request_id(request_id).is_ok() =>
        {
            (
                request_id.to_string(),
                Some(session_id.to_string()),
                Some(connection_epoch),
            )
        }
        _ => {
            let _ = send_error(
                state,
                socket,
                conn,
                request_sequence,
                None,
                None,
                None,
                V4Error::Protocol,
            )
            .await;
            return RequestStart::Rejected;
        }
    };
    if request_sequence == 0 {
        let _ = send_error(
            state,
            socket,
            conn,
            request_sequence,
            Some(&request_id),
            session_id.as_deref(),
            connection_epoch,
            V4Error::Protocol,
        )
        .await;
        return RequestStart::Rejected;
    }
    let fingerprint = hash_dynamic(canonical);
    match conn.requests.get(&request_sequence) {
        Some(record) if record.fingerprint == fingerprint => {
            if let Some(response) = record.response.clone() {
                if send_bytes(socket, state, conn, response).await {
                    RequestStart::Replayed
                } else {
                    RequestStart::Rejected
                }
            } else {
                let _ = send_limit_after_reconnect(
                    state,
                    socket,
                    conn,
                    request_sequence,
                    Some(&request_id),
                    session_id.as_deref(),
                    connection_epoch,
                )
                .await;
                RequestStart::Rejected
            }
        }
        Some(_) => {
            let _ = send_error(
                state,
                socket,
                conn,
                request_sequence,
                Some(&request_id),
                session_id.as_deref(),
                connection_epoch,
                V4Error::Protocol,
            )
            .await;
            RequestStart::Rejected
        }
        None if request_sequence <= conn.request_high_water => {
            let _ = send_frame(
                socket,
                state,
                conn,
                &error_packet(
                    request_sequence,
                    Some(request_id),
                    session_id,
                    connection_epoch,
                    stale_request_error(),
                ),
            )
            .await;
            RequestStart::Rejected
        }
        None => {
            let pin = matches!(
                packet.message.as_ref(),
                Some(pb::client_packet::Message::Authenticate(_))
            ) && conn.pinned_auth_sequence.is_none();
            reserve_request_sequence(conn, request_sequence, fingerprint, pin);
            RequestStart::Reserved(RequestContext {
                request_sequence,
                request_id,
                fingerprint,
                session_id,
                connection_epoch,
            })
        }
    }
}

fn reserve_request_sequence(
    conn: &mut Connection,
    request_sequence: u64,
    fingerprint: [u8; 32],
    pin: bool,
) {
    debug_assert!(request_sequence > conn.request_high_water);
    if pin {
        debug_assert!(conn.pinned_auth_sequence.is_none());
        conn.pinned_auth_sequence = Some(request_sequence);
    } else {
        while conn.request_order.len() >= MAX_RETAINED_REQUEST_SEQUENCES {
            evict_oldest_request(conn);
        }
        conn.request_order.push_back(request_sequence);
    }
    conn.request_high_water = request_sequence;
    conn.requests.insert(
        request_sequence,
        RequestRecord {
            fingerprint,
            response: None,
        },
    );
}

fn evict_oldest_request(conn: &mut Connection) {
    let Some(sequence) = conn.request_order.pop_front() else {
        return;
    };
    if let Some(record) = conn.requests.remove(&sequence) {
        conn.retained_response_bytes = conn
            .retained_response_bytes
            .saturating_sub(record.response.as_ref().map_or(0, Vec::len));
    }
}

async fn send_for_request(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: &RequestContext,
    frame: &pb::ServerPacket,
) -> bool {
    let mut frame = frame.clone();
    frame.request_sequence = request.request_sequence;
    let Some(bytes) = encode_frame(conn, &frame) else {
        return false;
    };
    if !retain_response(conn, request, bytes.clone()) {
        return send_limit_after_reconnect_for_request(state, socket, conn, request).await;
    }
    send_bytes(socket, state, conn, bytes).await
}

fn retain_response(conn: &mut Connection, request: &RequestContext, bytes: Vec<u8>) -> bool {
    let Some(record) = conn.requests.get_mut(&request.request_sequence) else {
        return false;
    };
    if record.fingerprint != request.fingerprint {
        return false;
    }
    let old = record.response.as_ref().map_or(0, Vec::len);
    let next_total = conn
        .retained_response_bytes
        .saturating_sub(old)
        .saturating_add(bytes.len());
    if next_total > MAX_RETAINED_RESPONSE_BYTES {
        return false;
    }
    record.response = Some(bytes);
    conn.retained_response_bytes = next_total;
    true
}

async fn send_error_for_request(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: &RequestContext,
    error: V4Error,
) -> bool {
    let frame = error_packet(
        0,
        Some(request.request_id.clone()),
        request.session_id.clone(),
        request.connection_epoch,
        error.public(),
    );
    send_for_request(state, socket, conn, request, &frame).await
}

async fn send_limit_after_reconnect_for_request(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &mut Connection,
    request: &RequestContext,
) -> bool {
    let frame = error_packet(
        request.request_sequence,
        Some(request.request_id.clone()),
        request.session_id.clone(),
        request.connection_epoch,
        limit_after_reconnect_error(),
    );
    let Some(bytes) = encode_frame(conn, &frame) else {
        return false;
    };
    if let Some(record) = conn.requests.get_mut(&request.request_sequence) {
        if record.fingerprint != request.fingerprint {
            return false;
        }
        let old = record.response.as_ref().map_or(0, Vec::len);
        let next_total = conn
            .retained_response_bytes
            .saturating_sub(old)
            .saturating_add(bytes.len());
        if next_total <= MAX_RETAINED_RESPONSE_BYTES {
            record.response = Some(bytes.clone());
            conn.retained_response_bytes = next_total;
        }
    }
    send_bytes(socket, state, conn, bytes).await
}

async fn send_frame(
    socket: &mut WebSocket,
    state: &AppState,
    conn: &Connection,
    frame: &pb::ServerPacket,
) -> bool {
    let Some(bytes) = encode_frame(conn, frame) else {
        return false;
    };
    send_bytes(socket, state, conn, bytes).await
}

fn encode_frame(conn: &Connection, frame: &pb::ServerPacket) -> Option<Vec<u8>> {
    match &conn.extensions {
        Some(wire) => wire.encode(frame),
        None => Some(frame.encode_to_vec()),
    }
}

async fn send_bytes(
    socket: &mut WebSocket,
    state: &AppState,
    conn: &Connection,
    bytes: Vec<u8>,
) -> bool {
    if bytes.len() > MAX_FRAME_BYTES {
        return false;
    }
    send_message(socket, state, conn, Message::Binary(bytes.into())).await
}

async fn send_message(
    socket: &mut WebSocket,
    state: &AppState,
    conn: &Connection,
    message: Message,
) -> bool {
    io::send(
        socket,
        message,
        conn.registration.as_ref().map(|current| &current.link),
        &state.feed,
    )
    .await
}

async fn send_error(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &Connection,
    request_sequence: u64,
    request_id: Option<&str>,
    session_id: Option<&str>,
    connection_epoch: Option<u64>,
    error: V4Error,
) -> bool {
    send_frame(
        socket,
        state,
        conn,
        &error_packet(
            request_sequence,
            request_id.map(str::to_string),
            session_id.map(str::to_string),
            connection_epoch,
            error.public(),
        ),
    )
    .await
}

async fn send_limit_after_reconnect(
    state: &AppState,
    socket: &mut WebSocket,
    conn: &Connection,
    request_sequence: u64,
    request_id: Option<&str>,
    session_id: Option<&str>,
    connection_epoch: Option<u64>,
) -> bool {
    send_frame(
        socket,
        state,
        conn,
        &error_packet(
            request_sequence,
            request_id.map(str::to_string),
            session_id.map(str::to_string),
            connection_epoch,
            limit_after_reconnect_error(),
        ),
    )
    .await
}

fn limit_after_reconnect_error() -> PublicError {
    PublicError {
        code: 1008,
        name: "limit_exceeded",
        retry: RetryClass::AfterReconnect,
        detail: None,
    }
}

fn stale_request_error() -> PublicError {
    PublicError {
        code: 1010,
        name: "stale_request",
        retry: RetryClass::Never,
        detail: None,
    }
}

async fn send_boot_error(state: &AppState, socket: &mut WebSocket, error: V4Error) -> bool {
    let bytes = error_packet(0, None, None, None, error.public()).encode_to_vec();
    if bytes.len() > MAX_FRAME_BYTES {
        return false;
    }
    io::send(socket, Message::Binary(bytes.into()), None, &state.feed).await
}

async fn new_connection(state: &AppState) -> Result<Connection, V4Error> {
    let mut challenge = [0u8; 32];
    rand::rng().fill_bytes(&mut challenge);
    Ok(Connection::new(
        Uuid::new_v4().to_string(),
        state.control_v4.next_connection_epoch().await?,
        Uuid::new_v4().to_string(),
        hex(&challenge),
    ))
}

fn server_packet(message: pb::server_packet::Message) -> pb::ServerPacket {
    pb::ServerPacket {
        message: Some(message),
        request_sequence: 0,
    }
}

fn error_packet(
    request_sequence: u64,
    request_id: Option<String>,
    session_id: Option<String>,
    connection_epoch: Option<u64>,
    error: PublicError,
) -> pb::ServerPacket {
    pb::ServerPacket {
        message: Some(pb::server_packet::Message::Error(pb::Error {
            request_id,
            session_id,
            connection_epoch,
            code: error.code as i32,
            name: error.name.to_string(),
            retry: retry_class_to_proto(error.retry),
            detail: error.detail,
        })),
        request_sequence,
    }
}

fn retry_class_to_proto(retry: RetryClass) -> i32 {
    match retry {
        RetryClass::Never => pb::RetryClass::Never as i32,
        RetryClass::AfterReconnect => pb::RetryClass::AfterReconnect as i32,
        RetryClass::Later => pb::RetryClass::Later as i32,
    }
}

fn client_request_metadata(packet: &pb::ClientPacket) -> Option<(&str, &str, u64)> {
    match packet.message.as_ref()? {
        pb::client_packet::Message::Authenticate(frame) => Some((
            frame.request_id.as_str(),
            frame.session_id.as_str(),
            frame.connection_epoch,
        )),
        pb::client_packet::Message::Heartbeat(frame) => Some((
            frame.request_id.as_str(),
            frame.session_id.as_str(),
            frame.connection_epoch,
        )),
        pb::client_packet::Message::Ack(frame) => Some((
            frame.request_id.as_str(),
            frame.session_id.as_str(),
            frame.connection_epoch,
        )),
        pb::client_packet::Message::RequestSnapshot(frame) => Some((
            frame.request_id.as_str(),
            frame.session_id.as_str(),
            frame.connection_epoch,
        )),
    }
}

fn begin_auth_attempt(conn: &mut Connection) -> bool {
    if conn.auth_attempted {
        return false;
    }
    conn.auth_attempted = true;
    true
}

fn discard_auth_challenge(challenges: &crate::auth::ChallengeStore, conn: &Connection) {
    challenges.discard_for_key(&conn.storage_key(), &conn.challenge_id);
}

fn assignment_state_to_proto(state: crate::control_v4::AssignmentState) -> pb::AssignmentState {
    pb::AssignmentState {
        lane: state.lane,
        authority_incarnation: state.authority_incarnation,
        assignment_revision: state.assignment_revision,
        realm_revision: state.realm_revision,
        owner_session: state.owner_session,
        owner_epoch: state.owner_epoch,
        fencing_token: state.fencing_token,
        assignment: state.assignment.and_then(assignment_to_proto),
    }
}

fn assignment_to_proto(assignment: AssignmentPayload) -> Option<pb::Assignment> {
    let mut peers = std::collections::BTreeMap::new();
    for (key, value) in assignment.peers {
        let position = match position_from_value(value) {
            Some(position) => position,
            None => continue,
        };
        peers.insert(key, position);
    }
    Some(pb::Assignment {
        island_id: assignment.island_id,
        connection_string: assignment.connection_string,
        from_island_id: assignment.from_island_id,
        peers,
    })
}

pub(super) fn position_from_value(value: serde_json::Value) -> Option<crate::proto::Position> {
    if let Some(array) = value.as_array() {
        if array.len() == 3 {
            return Some(crate::proto::Position {
                x: array.first()?.as_f64()? as f32,
                y: array.get(1)?.as_f64()? as f32,
                z: array.get(2)?.as_f64()? as f32,
            });
        }
    }
    Some(crate::proto::Position {
        x: value.get("x")?.as_f64()? as f32,
        y: value.get("y")?.as_f64()? as f32,
        z: value.get("z")?.as_f64()? as f32,
    })
}

fn auth_chain_from_proto(
    chain: &crate::proto::decentraland::common::AuthChain,
) -> Result<AuthChain, ()> {
    if chain.links.len() > MAX_AUTH_CHAIN_LINKS || chain.encoded_len() > MAX_AUTH_CHAIN_BYTES {
        return Err(());
    }
    chain.links.iter().map(auth_link_from_proto).collect()
}

fn auth_link_from_proto(
    link: &crate::proto::decentraland::common::AuthLink,
) -> Result<AuthLink, ()> {
    Ok(AuthLink {
        link_type: auth_link_type_from_proto(link.r#type)?,
        payload: link.payload.clone(),
        signature: link.signature.clone(),
    })
}

fn auth_link_type_from_proto(value: i32) -> Result<AuthLinkType, ()> {
    match crate::proto::decentraland::common::AuthLinkType::try_from(value).map_err(|_| ())? {
        crate::proto::decentraland::common::AuthLinkType::Signer => Ok(AuthLinkType::SIGNER),
        crate::proto::decentraland::common::AuthLinkType::EcdsaEphemeral => {
            Ok(AuthLinkType::EcdsaEphemeral)
        }
        crate::proto::decentraland::common::AuthLinkType::EcdsaSignedEntity => {
            Ok(AuthLinkType::EcdsaSignedEntity)
        }
        crate::proto::decentraland::common::AuthLinkType::EcdsaEip1654Ephemeral => {
            Ok(AuthLinkType::EcdsaEip1654Ephemeral)
        }
        crate::proto::decentraland::common::AuthLinkType::EcdsaEip1654SignedEntity => {
            Ok(AuthLinkType::EcdsaEip1654SignedEntity)
        }
        crate::proto::decentraland::common::AuthLinkType::Unknown => Err(()),
    }
}

fn parse_lanes(lanes: &[String]) -> Result<Vec<LaneKey>, V4Error> {
    if lanes.is_empty() || lanes.len() > MAX_LANES_PER_SOCKET {
        return Err(V4Error::LimitExceeded);
    }
    let mut seen = HashSet::new();
    let mut parsed = Vec::with_capacity(lanes.len());
    for lane in lanes {
        let lane = LaneKey::parse(lane)?;
        if seen.insert(lane.as_str().to_string()) {
            parsed.push(lane);
        }
    }
    Ok(parsed)
}

fn validate_request_id(id: &str) -> Result<(), V4Error> {
    if id.is_empty() || id.len() > MAX_REQUEST_ID_BYTES || id.bytes().any(|b| b.is_ascii_control())
    {
        return Err(V4Error::LimitExceeded);
    }
    Ok(())
}

fn validate_operation_id(id: &str) -> Result<(), V4Error> {
    if id.is_empty()
        || id.len() > MAX_OPERATION_ID_BYTES
        || id.bytes().any(|b| b.is_ascii_control())
    {
        return Err(V4Error::LimitExceeded);
    }
    Ok(())
}

fn valid_current_request(
    owner: &V4Owner,
    request_id: &str,
    operation_id: &str,
    session_id: &str,
    epoch: u64,
) -> bool {
    validate_request_id(request_id).is_ok()
        && validate_operation_id(operation_id).is_ok()
        && owner.session == session_id.to_ascii_lowercase()
        && owner.epoch == epoch
}

fn compact_heartbeat(frame: &pb::Heartbeat) -> bool {
    frame.request_id.is_empty()
        && frame.operation_id.is_empty()
        && frame.session_id.is_empty()
        && frame.connection_epoch == 0
}

async fn next_event(rx: Option<&mut SocketEvents>) -> Option<SocketEvent> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn cleanup(state: &AppState, conn: Connection) {
    state
        .challenges
        .discard_for_key(&conn.storage_key(), &conn.challenge_id);
    let released = if let Some(owner) = conn.owner.as_ref() {
        if conn.lanes.iter().any(|lane| lane.as_str() == "realm") {
            super::control_position::publish(state, owner, "", None);
        }
        matches!(
            tokio::time::timeout(RELEASE_BUDGET, state.control_v4.release(owner)).await,
            Ok(Ok(()))
        )
    } else {
        false
    };
    if let Some(current) = conn.registration {
        current.link.close();
        state
            .registry
            .on_v4_peer_disconnected(&current.address, current.link.id);
        if !state.registry.has_any_peer(&current.address) {
            state.peers.remove_peer(&current.address);
        }
        if current.announces_feed && released {
            state.publisher.publish(
                disconnect_subject(&current.address),
                current.session.as_bytes().to_vec(),
            );
        }
    }
}

fn server_capabilities() -> Vec<String> {
    [
        "auth.server_challenge",
        "request.correlation",
        "request.monotonic_sequence",
        "lane.realm",
        "lane.scene",
        "lane.voice",
        "lane.world",
        "assignment.snapshot",
        "assignment.ack",
        "resume.revision",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthError, ChallengeStore};
    use crate::config::AuthConfig;
    use crate::registry::PeersRegistry;

    #[test]
    fn scene_snapshots_never_change_realm_reconciliation_eligibility() {
        let registry = PeersRegistry::new();
        let (link, _events) = registry.on_v4_peer_connected("0xa", "0xs", "connection");

        record_assignment_delivery(&link, false, true);
        assert!(link.needs_assignment());
        record_assignment_delivery(&link, true, true);
        assert!(!link.needs_assignment());
        record_assignment_delivery(&link, false, false);
        assert!(!link.needs_assignment());
        record_assignment_delivery(&link, true, false);
        assert!(link.needs_assignment());
    }

    #[test]
    fn prototype_welcome_only_records_a_realm_assignment() {
        let state = |lane: &str, assigned: bool| crate::control_v4::AssignmentState {
            lane: lane.into(),
            authority_incarnation: "authority".into(),
            assignment_revision: 1,
            realm_revision: 1,
            owner_session: "0xs".into(),
            owner_epoch: 1,
            fencing_token: 1,
            assignment: assigned.then(|| AssignmentPayload {
                island_id: "island".into(),
                connection_string: "adapter".into(),
                from_island_id: None,
                peers: Default::default(),
            }),
        };
        assert_eq!(welcome_realm_assignment(&[state("scene:one", true)]), None);
        assert_eq!(
            welcome_realm_assignment(&[state("scene:one", true), state("realm", false)]),
            Some(false)
        );
        assert_eq!(
            welcome_realm_assignment(&[state("realm", true)]),
            Some(true)
        );
    }

    #[test]
    fn compact_heartbeat_requires_all_connection_context_to_be_implicit() {
        let base = pb::Heartbeat::default();
        assert!(compact_heartbeat(&base));
        for frame in [
            pb::Heartbeat {
                request_id: "request".into(),
                ..base.clone()
            },
            pb::Heartbeat {
                operation_id: "operation".into(),
                ..base.clone()
            },
            pb::Heartbeat {
                session_id: "session".into(),
                ..base.clone()
            },
            pb::Heartbeat {
                connection_epoch: 1,
                ..base
            },
        ] {
            assert!(!compact_heartbeat(&frame));
        }
    }

    #[test]
    fn heartbeat_wire_overhead_is_only_the_sequence() {
        let position = crate::proto::decentraland::common::Position {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let legacy = crate::proto::archipelago::ClientPacket {
            message: Some(
                crate::proto::archipelago::client_packet::Message::Heartbeat(
                    crate::proto::archipelago::Heartbeat {
                        position: Some(position),
                        desired_room: None,
                        sample_sequence: None,
                    },
                ),
            ),
            request_sequence: 0,
        };
        let compact = pb::ClientPacket {
            message: Some(pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                position: Some(position),
                ..Default::default()
            })),
            request_sequence: 1,
        };
        let explicit = pb::ClientPacket {
            message: Some(pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                request_id: "a".repeat(32),
                operation_id: "b".repeat(32),
                session_id: format!("0x{}", "c".repeat(40)),
                connection_epoch: 1,
                position: Some(position),
                desired_room: None,
            })),
            request_sequence: 1,
        };
        assert_eq!(legacy.encoded_len(), 19);
        assert_eq!(compact.encoded_len(), 21);
        assert_eq!(explicit.encoded_len(), 136);
        let acknowledgement = pb::ServerPacket {
            message: Some(pb::server_packet::Message::Ack(pb::Ack::default())),
            request_sequence: 1,
        };
        assert_eq!(acknowledgement.encoded_len(), 4);
    }

    #[test]
    fn decoded_authenticate_rejection_consumes_connection_challenge() {
        let challenges = ChallengeStore::new(AuthConfig::default());
        let mut conn = Connection::new(
            "conn-test".to_string(),
            1,
            "challenge-test".to_string(),
            "payload-test".to_string(),
        );
        challenges.put_for_key(&conn.storage_key(), &conn.challenge_id);

        assert!(begin_auth_attempt(&mut conn));
        discard_auth_challenge(challenges.as_ref(), &conn);
        assert!(
            !begin_auth_attempt(&mut conn),
            "a second decoded Authenticate must not get another challenge attempt"
        );

        let chain = vec![AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: "0x0000000000000000000000000000000000000001".to_string(),
            signature: None,
        }];
        let err = challenges
            .redeem_and_verify_payload(
                &conn.storage_key(),
                "0x0000000000000000000000000000000000000001",
                &conn.challenge_id,
                &conn.challenge,
                &chain,
            )
            .expect_err("rejected typed Authenticate must consume the pending challenge");
        assert!(matches!(err, AuthError::UnknownChallenge));
    }

    #[test]
    fn request_retention_keeps_auth_and_bounds_recent_sequences() {
        let mut conn = Connection::new(
            "conn-test".to_string(),
            1,
            "challenge-test".to_string(),
            "payload-test".to_string(),
        );
        reserve_request_sequence(&mut conn, 1, [1; 32], true);
        let auth = RequestContext {
            request_sequence: 1,
            request_id: "auth".to_string(),
            fingerprint: [1; 32],
            session_id: Some("session".to_string()),
            connection_epoch: Some(1),
        };
        assert!(retain_response(&mut conn, &auth, vec![1; 64]));

        for sequence in 2_u64..=1_002 {
            let fingerprint = hash_dynamic(&sequence.to_be_bytes());
            reserve_request_sequence(&mut conn, sequence, fingerprint, false);
            let request = RequestContext {
                request_sequence: sequence,
                request_id: format!("request-{sequence}"),
                fingerprint,
                session_id: Some("session".to_string()),
                connection_epoch: Some(1),
            };
            assert!(retain_response(&mut conn, &request, vec![2; 64]));
        }

        assert_eq!(conn.request_high_water, 1_002);
        assert_eq!(conn.request_order.len(), MAX_RETAINED_REQUEST_SEQUENCES);
        assert_eq!(conn.requests.len(), MAX_RETAINED_REQUEST_SEQUENCES + 1);
        assert!(conn.requests.contains_key(&1));
        assert!(!conn.requests.contains_key(&2));
        assert!(conn.requests.contains_key(&1_002));
        assert_eq!(
            conn.retained_response_bytes,
            (MAX_RETAINED_REQUEST_SEQUENCES + 1) * 64
        );
        assert!(conn.retained_response_bytes <= MAX_RETAINED_RESPONSE_BYTES);
    }
}
