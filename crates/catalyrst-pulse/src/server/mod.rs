use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use prost::Message as _;
use sha2::{Digest, Sha256};

use crate::decentraland::pulse::{
    client_message, server_message, ClientMessage, EmoteStopReason, HandshakeResponse,
    PlayerInitialState, PulseV4ErrorCode, PulseV4Hello, PulseV4Result, PulseV4RetryClass,
    PulseV4Role, SceneListenerAoi, SceneListenerHandshakeRequest, SceneListenerUpdate,
    ServerMessage,
};
use crate::handshake::{verify_handshake_bytes, VerifiedHandshake};
use crate::hardening::{
    BanList, CorruptedPacketLimiter, DisconnectReason, GameplayRateLimiter, HandshakeAttemptPolicy,
    HandshakeReplayPolicy, PreAuthAdmission, ReplayRejection, DEFAULT_DISCRETE_BURST,
    DEFAULT_DISCRETE_RATE_PER_SEC, DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ,
    DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP, DEFAULT_MAX_EMOTE_DURATION_MS,
    DEFAULT_MAX_EMOTE_ID_LENGTH, DEFAULT_MAX_HANDSHAKE_ATTEMPTS, DEFAULT_MAX_REALM_LENGTH,
    DEFAULT_PRE_AUTH_BUDGET, DEFAULT_PRE_AUTH_BUDGET_WT, DEFAULT_SCENE_LISTENER_MAX_PARCELS,
    SCENE_LISTENER_REALM_BUDGET_COST,
};
use crate::interest::{
    ParcelEncoder, ParcelEncoderOptions, SceneListenerCellMapper, SceneListenerState,
    SpatialAreaOfInterest, SpatialAreaOfInterestOptions, SPATIAL_GRID_CELL_SIZE,
};
use crate::realm_grids::RealmSpatialGrids;
use crate::simulation::{
    HandshakeProtocol, OutgoingMessage, PacketMode, PeerConnectionState, PeerSimulation, PeerState,
};
use crate::snapshot::{
    EmoteInput, IdentityBoard, PeerSnapshotPublisher, ProfileBoard, SnapshotBoard,
};
use crate::transport::webtransport::{WtConfig, WtHost};
use crate::transport::{Event, Host, HostConfig, Packet, Transports};
use crate::v4::{
    server_result, PulseV4Authority, V4AuthOutcome, CAPABILITY_DELTA_BATCH,
    CAPABILITY_DELTA_BATCH_BASELINE, CAPABILITY_DELTA_BATCH_DICTIONARY,
    CAPABILITY_DELTA_BATCH_SAMPLE_TICK,
};

mod application;
pub mod inspection;

pub mod channel {

    pub const RELIABLE: u8 = 0;

    pub const UNRELIABLE_SEQUENCED: u8 = 1;

    pub const UNRELIABLE_UNSEQUENCED: u8 = 2;
}

pub const DEFAULT_SIMULATION_STEPS: [u32; 3] = [50, 100, 200];

pub const DEFAULT_RING_CAPACITY: usize = 10;

pub const ENET_CAPACITY: usize = 4095;
pub const WT_CAPACITY: usize = 4096;
const DEFAULT_MAX_PEERS: usize = ENET_CAPACITY + WT_CAPACITY;

const _: () = assert!(ENET_CAPACITY + WT_CAPACITY <= u16::MAX as usize);
const _: () = assert!(DEFAULT_MAX_PEERS == ENET_CAPACITY + WT_CAPACITY);

pub const FEATURE_DELTA_BATCH: u32 = 1 << 0;
pub const FEATURE_DELTA_BATCH_BASELINE: u32 = 1 << 1;
pub const FEATURE_DELTA_BATCH_DICTIONARY: u32 = 1 << 2;
pub const FEATURE_APPLICATION_RELAY: u32 = 1 << 3;
pub const FEATURE_DELTA_BATCH_SAMPLE_TICK: u32 = 1 << 4;
pub const SERVER_FEATURES: u32 = FEATURE_DELTA_BATCH
    | FEATURE_DELTA_BATCH_BASELINE
    | FEATURE_DELTA_BATCH_DICTIONARY
    | FEATURE_DELTA_BATCH_SAMPLE_TICK;

fn grantable_features(features: u32) -> u32 {
    if features & (FEATURE_DELTA_BATCH_BASELINE | FEATURE_DELTA_BATCH_DICTIONARY) == 0 {
        features & !FEATURE_DELTA_BATCH_SAMPLE_TICK
    } else {
        features
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Reply(ServerMessage),

    Authenticated {
        wallet: String,
        session: String,
        duplicate_of: Option<u32>,
        initial_state: Option<Box<PlayerInitialState>>,
        features: u32,
    },

    AuthenticatedListener {
        wallet: String,
        session: String,
        duplicate_of: Option<u32>,
        listener: Arc<SceneListenerState>,
        features: u32,
    },

    AuthenticatedV4 {
        wallet: String,
        session: String,
        duplicate_of: Option<u32>,
        initial_state: Option<Box<PlayerInitialState>>,
        features: u32,
        result: PulseV4Result,
    },

    AuthenticatedListenerV4 {
        wallet: String,
        session: String,
        duplicate_of: Option<u32>,
        listener: Arc<SceneListenerState>,
        features: u32,
        result: PulseV4Result,
    },

    Application(client_message::Message),

    Applied,

    Reject {
        reply: Option<ServerMessage>,
        reason: DisconnectReason,
    },

    Ignore,
}

struct Admitted {
    wallet: String,
    session: String,
    duplicate_of: Option<u32>,
}

enum Admission {
    Ok(Admitted),
    Deny(Box<Action>),
}

fn scene_listener_max_parcels_from_env() -> usize {
    std::env::var("PULSE_SCENE_LISTENER_MAX_PARCELS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SCENE_LISTENER_MAX_PARCELS)
}

fn handshake_response(success: bool, error: Option<String>) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::Handshake(HandshakeResponse {
            success,
            error,
            protocol_features: SERVER_FEATURES,
            application_relay_nonce: Vec::new(),
        })),
    }
}

fn top_level_has_v4_arm(data: &[u8]) -> bool {
    top_level_has_arm(data, 20, 21)
}

fn top_level_has_arm(data: &[u8], first: u64, last: u64) -> bool {
    fn varint(data: &[u8], cursor: &mut usize) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *data.get(*cursor)?;
            *cursor += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    let mut cursor = 0;
    while cursor < data.len() {
        let Some(key) = varint(data, &mut cursor) else {
            return false;
        };
        let tag = key >> 3;
        if (first..=last).contains(&tag) {
            return true;
        }
        let skip = match key & 7 {
            0 => {
                if varint(data, &mut cursor).is_none() {
                    return false;
                }
                0
            }
            1 => 8,
            2 => match varint(data, &mut cursor).and_then(|len| usize::try_from(len).ok()) {
                Some(len) => len,
                None => return false,
            },
            5 => 4,
            _ => return false,
        };
        let Some(next) = cursor.checked_add(skip) else {
            return false;
        };
        if next > data.len() {
            return false;
        }
        cursor = next;
    }
    false
}

mod validate {
    use crate::decentraland::pulse::{PlayerState, TeleportRequest};
    use crate::interest::ParcelEncoder;

    pub fn player_state(state: &PlayerState, encoder: &ParcelEncoder) -> bool {
        encoder.is_valid_index(state.parcel_index) && state.are_quantized_fields_in_range()
    }

    pub fn teleport(req: &TeleportRequest, encoder: &ParcelEncoder) -> bool {
        !req.realm.is_empty()
            && encoder.is_valid_index(req.parcel_index)
            && req.are_quantized_fields_in_range()
    }

    pub fn realm_length(realm: &str, max_len: usize) -> bool {
        max_len == 0 || realm.chars().count() <= max_len
    }

    pub fn emote_caps(
        emote_id: Option<&str>,
        duration_ms: Option<u32>,
        max_id_len: usize,
        max_duration_ms: u32,
    ) -> bool {
        if max_id_len > 0 {
            if let Some(id) = emote_id {
                if id.chars().count() > max_id_len {
                    return false;
                }
            }
        }
        if max_duration_ms > 0 {
            if let Some(d) = duration_ms {
                if d > max_duration_ms {
                    return false;
                }
            }
        }
        true
    }
}

pub struct PulseServer {
    pub peers: HashMap<u32, PeerState>,
    pub board: SnapshotBoard,
    pub grids: RealmSpatialGrids,
    pub encoder: ParcelEncoder,
    pub cell_mapper: SceneListenerCellMapper,
    pub aoi: SpatialAreaOfInterest,
    pub identity: IdentityBoard,
    pub profiles: ProfileBoard,
    pub simulation: PeerSimulation,

    pub replay_policy: HandshakeReplayPolicy,

    pub v4: PulseV4Authority,
    pub legacy_handshake: bool,

    pub application_relay: Option<crate::application_relay::ApplicationRelay>,

    pub ban_list: BanList,

    pub attempt_policy: HandshakeAttemptPolicy,

    pub pre_auth_enet: PreAuthAdmission,
    pub pre_auth_wt: PreAuthAdmission,

    pub max_emote_id_length: usize,

    pub max_emote_duration_ms: u32,

    pub corrupted_limiter: CorruptedPacketLimiter,

    pub gameplay_limiter: GameplayRateLimiter,

    pub max_realm_length: usize,

    pub max_scene_listener_parcels: usize,

    pub scene_listener_forbidden_drops: u64,

    /// `None` leaves every cluster path -- pass, board, publisher -- entirely out of the loop,
    /// which is what `PULSE_CLUSTERS_ENABLED=0` buys: not a tracker publishing nothing.
    pub clusters: Option<crate::cluster::ClusterTracker>,

    tick_counter: u32,
}

impl Default for PulseServer {
    fn default() -> Self {
        Self::new()
    }
}

impl PulseServer {
    pub fn new() -> Self {
        Self::with_config(
            DEFAULT_MAX_PEERS,
            DEFAULT_RING_CAPACITY,
            &DEFAULT_SIMULATION_STEPS,
            false,
        )
    }

    pub fn with_config(
        max_peers: usize,
        ring_capacity: usize,
        simulation_steps: &[u32],
        resync_with_delta: bool,
    ) -> Self {
        let grids = RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, max_peers);
        let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
        let cell_mapper = SceneListenerCellMapper::new(&grids, &encoder);
        Self {
            peers: HashMap::new(),
            board: SnapshotBoard::new(max_peers, ring_capacity),
            grids,
            encoder,
            cell_mapper,
            aoi: SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default()),
            identity: IdentityBoard::new(max_peers),
            profiles: ProfileBoard::new(max_peers),
            simulation: PeerSimulation::new(simulation_steps, resync_with_delta),

            replay_policy: HandshakeReplayPolicy::new(
                true,
                crate::handshake::MAX_TIMESTAMP_SKEW_MS,
                max_peers,
            ),
            v4: PulseV4Authority::disabled(),
            legacy_handshake: true,
            application_relay: None,
            ban_list: BanList::new(),
            attempt_policy: HandshakeAttemptPolicy::new(DEFAULT_MAX_HANDSHAKE_ATTEMPTS),
            pre_auth_enet: PreAuthAdmission::new(
                DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP,
                DEFAULT_PRE_AUTH_BUDGET,
            ),
            pre_auth_wt: PreAuthAdmission::new(
                DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP,
                DEFAULT_PRE_AUTH_BUDGET_WT,
            ),
            max_emote_id_length: DEFAULT_MAX_EMOTE_ID_LENGTH,
            max_emote_duration_ms: DEFAULT_MAX_EMOTE_DURATION_MS,
            corrupted_limiter: CorruptedPacketLimiter::new(
                crate::hardening::DEFAULT_CORRUPT_MAX_PER_MINUTE,
                crate::hardening::DEFAULT_CORRUPT_BURST,
            ),
            gameplay_limiter: GameplayRateLimiter::new(
                DEFAULT_INPUT_MAX_HZ,
                DEFAULT_INPUT_BURST,
                DEFAULT_DISCRETE_RATE_PER_SEC,
                DEFAULT_DISCRETE_BURST,
            ),
            max_realm_length: DEFAULT_MAX_REALM_LENGTH,
            max_scene_listener_parcels: scene_listener_max_parcels_from_env(),
            scene_listener_forbidden_drops: 0,
            clusters: None,
            tick_counter: 0,
        }
    }

    pub fn dispatch(
        &mut self,
        peer: u32,
        channel: u8,
        data: &[u8],
        now_ms: i64,
        now: u32,
    ) -> Action {
        if data.len() > 4096 && top_level_has_arm(data, 12, 14) {
            return Action::Reject {
                reply: None,
                reason: DisconnectReason::PacketCorrupted,
            };
        }
        self.v4
            .set_application_relay(self.application_relay.is_some());
        if self.v4.frame_exceeded(data.len()) && top_level_has_v4_arm(data) {
            return Action::Reject {
                reply: None,
                reason: DisconnectReason::InvalidHandshakeField,
            };
        }
        let Ok(msg) = ClientMessage::decode(data) else {
            if self
                .corrupted_limiter
                .register_and_check_exhausted(peer, now)
            {
                return Action::Reject {
                    reply: None,
                    reason: DisconnectReason::PacketCorrupted,
                };
            }
            return Action::Ignore;
        };
        let Some(inner) = msg.message else {
            return Action::Ignore;
        };

        let protocol = match &inner {
            client_message::Message::Handshake(_)
            | client_message::Message::SceneListenerHandshake(_) => Some(HandshakeProtocol::Legacy),
            client_message::Message::V4Hello(_) | client_message::Message::V4Auth(_) => {
                Some(HandshakeProtocol::V4)
            }
            _ => None,
        };
        if protocol == Some(HandshakeProtocol::Legacy) && !self.legacy_handshake {
            crate::metrics::handshake("legacy", "refused");
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: Some(handshake_response(
                    false,
                    Some("Pulse v4 is required".into()),
                )),
                reason: DisconnectReason::AuthFailed,
            };
        }
        if let Some(protocol) = protocol {
            let Some(state) = self.peers.get_mut(&peer) else {
                return Action::Ignore;
            };
            if state
                .handshake_protocol
                .is_some_and(|selected| selected != protocol)
            {
                state.connection_state = PeerConnectionState::PendingDisconnect;
                return Action::Reject {
                    reply: None,
                    reason: DisconnectReason::InvalidHandshakeField,
                };
            }
            state.handshake_protocol = Some(protocol);
        }

        let is_handshake = matches!(
            inner,
            client_message::Message::Handshake(_)
                | client_message::Message::SceneListenerHandshake(_)
        );
        let fingerprint = is_handshake.then(|| <[u8; 32]>::from(Sha256::digest(data)));
        if is_handshake
            && self.is_authenticated(peer)
            && self.peers[&peer].handshake_fingerprint == fingerprint
        {
            return Action::Reply(handshake_response(true, None));
        }

        if matches!(
            inner,
            client_message::Message::ApplicationJoin(_)
                | client_message::Message::ApplicationLeave(_)
                | client_message::Message::ApplicationSend(_)
        ) {
            if !self.is_authenticated(peer)
                || self.peers[&peer].features & FEATURE_APPLICATION_RELAY == 0
            {
                return Action::Ignore;
            }
            let reliable = !matches!(&inner, client_message::Message::ApplicationSend(send) if send.unreliable);
            if reliable && channel != channel::RELIABLE {
                return Action::Ignore;
            }
            return Action::Application(inner);
        }

        if self.is_scene_listener(peer)
            && !matches!(
                inner,
                client_message::Message::Resync(_)
                    | client_message::Message::SceneListenerUpdate(_)
                    | client_message::Message::V4Hello(_)
                    | client_message::Message::V4Auth(_)
            )
        {
            self.scene_listener_forbidden_drops =
                self.scene_listener_forbidden_drops.wrapping_add(1);
            crate::metrics::scene_listener_forbidden_dropped();
            return Action::Ignore;
        }

        let action = match inner {
            client_message::Message::Handshake(req) => self.handle_handshake(peer, req, now_ms),
            client_message::Message::SceneListenerHandshake(req) => {
                self.handle_scene_listener_handshake(peer, req, now_ms)
            }
            client_message::Message::SceneListenerUpdate(req) => {
                self.handle_scene_listener_update(peer, req, now)
            }
            client_message::Message::V4Hello(hello) => self.handle_v4_hello(peer, hello, now_ms),
            client_message::Message::V4Auth(auth) => self.handle_v4_auth(peer, auth, now_ms),
            other => {
                if !self.is_authenticated(peer) {
                    return Action::Ignore;
                }
                self.apply_gameplay(peer, now, other)
            }
        };
        if matches!(
            action,
            Action::Authenticated { .. }
                | Action::AuthenticatedListener { .. }
                | Action::AuthenticatedV4 { .. }
                | Action::AuthenticatedListenerV4 { .. }
        ) {
            self.peers
                .get_mut(&peer)
                .expect("admitted peer exists")
                .handshake_fingerprint = fingerprint;
        }
        action
    }

    fn is_scene_listener(&self, peer: u32) -> bool {
        self.peers
            .get(&peer)
            .map(|s| s.connection_state == PeerConnectionState::Authenticated && s.is_listener())
            .unwrap_or(false)
    }

    fn handle_v4_hello(&mut self, peer: u32, hello: PulseV4Hello, now_ms: i64) -> Action {
        crate::metrics::handshake("v4", "hello");
        if self.peers.get(&peer).map(|state| state.connection_state)
            != Some(PeerConnectionState::PendingAuth)
        {
            return Action::Ignore;
        }
        let valid_intent = match PulseV4Role::try_from(hello.role) {
            Ok(PulseV4Role::Player) => hello
                .initial_state
                .as_ref()
                .is_none_or(|initial| self.validate_handshake_initial_state(initial)),
            Ok(PulseV4Role::Listener) => self.build_listener(&hello.listener_aoi).is_some(),
            Err(_) => false,
        };
        if !valid_intent {
            return Action::Reply(
                self.v4
                    .reject_hello(&hello, "invalid role-specific handshake fields"),
            );
        }
        let reply = self.v4.hello(peer, hello, now_ms);
        match reply.message.as_ref() {
            Some(server_message::Message::V4Challenge(_)) => {
                crate::metrics::handshake("v4", "challenge")
            }
            Some(server_message::Message::V4Result(result)) => {
                let outcome = PulseV4ErrorCode::try_from(result.error_code)
                    .map(|code| code.as_str_name())
                    .unwrap_or("PULSE_V4_ERROR_UNKNOWN");
                crate::metrics::handshake("v4", outcome);
            }
            _ => {}
        }
        Action::Reply(reply)
    }

    fn handle_v4_auth(
        &mut self,
        peer: u32,
        auth: crate::decentraland::pulse::PulseV4Auth,
        now_ms: i64,
    ) -> Action {
        crate::metrics::handshake("v4", "auth");
        let state = self.peers.get(&peer).map(|state| state.connection_state);
        if state != Some(PeerConnectionState::PendingAuth)
            && state != Some(PeerConnectionState::Authenticated)
        {
            return Action::Ignore;
        }
        if state == Some(PeerConnectionState::PendingAuth) {
            let Some(peer_state) = self.peers.get_mut(&peer) else {
                return Action::Ignore;
            };
            match self
                .attempt_policy
                .try_record_attempt(peer_state.handshake_attempts)
            {
                Some(next) => peer_state.handshake_attempts = next,
                None => {
                    peer_state.connection_state = PeerConnectionState::PendingDisconnect;
                    return Action::Reject {
                        reply: None,
                        reason: DisconnectReason::AuthFailed,
                    };
                }
            }
        }

        let verified = match self.v4.authenticate(peer, auth, now_ms) {
            V4AuthOutcome::Reply(result) => {
                let outcome = PulseV4ErrorCode::try_from(result.error_code)
                    .map(|code| code.as_str_name())
                    .unwrap_or("PULSE_V4_ERROR_UNKNOWN");
                crate::metrics::handshake("v4", outcome);
                return Action::Reply(server_result(result));
            }
            V4AuthOutcome::Verified(verified) => verified,
        };
        if state == Some(PeerConnectionState::Authenticated) {
            let result = self.v4.rejection_result(
                &verified,
                PulseV4ErrorCode::PulseV4ErrorAlreadyUsed,
                PulseV4RetryClass::PulseV4RetryNone,
                "connection is already authenticated",
            );
            self.v4.commit_verified(peer, &verified, result.clone());
            return Action::Reply(server_result(result));
        }
        if self.ban_list.is_banned(&verified.verified.user_address) {
            let result = self.v4.rejection_result(
                &verified,
                PulseV4ErrorCode::PulseV4ErrorBanned,
                PulseV4RetryClass::PulseV4RetryNone,
                "identity is not admitted",
            );
            self.v4.commit_verified(peer, &verified, result.clone());
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: Some(server_result(result)),
                reason: DisconnectReason::Banned,
            };
        }

        let duplicate_of = self
            .identity
            .peer_by_wallet(&verified.verified.user_address)
            .filter(|candidate| *candidate != peer);
        if duplicate_of
            .is_some_and(|holder| self.v4.holds_this_session_at_or_after(holder, &verified))
        {
            let result = self.v4.rejection_result(
                &verified,
                PulseV4ErrorCode::PulseV4ErrorExpired,
                PulseV4RetryClass::PulseV4RetryNone,
                "a newer connection of this session is admitted",
            );
            crate::metrics::handshake("v4", "superseded");
            self.v4.commit_verified(peer, &verified, result.clone());
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: Some(server_result(result)),
                reason: DisconnectReason::DuplicateSession,
            };
        }
        let features = grantable_features(verified.negotiated_capabilities.iter().fold(
            0,
            |mask, capability| {
                mask | match capability.as_str() {
                    CAPABILITY_DELTA_BATCH => FEATURE_DELTA_BATCH,
                    CAPABILITY_DELTA_BATCH_BASELINE => FEATURE_DELTA_BATCH_BASELINE,
                    CAPABILITY_DELTA_BATCH_DICTIONARY => FEATURE_DELTA_BATCH_DICTIONARY,
                    CAPABILITY_DELTA_BATCH_SAMPLE_TICK => FEATURE_DELTA_BATCH_SAMPLE_TICK,
                    crate::application_relay::CAPABILITY => FEATURE_APPLICATION_RELAY,
                    _ => 0,
                }
            },
        ));
        if features & FEATURE_APPLICATION_RELAY != 0 {
            let Ok(nonce) = verified.relay_nonce.clone().try_into() else {
                return Action::Ignore;
            };
            if let Some(relay) = self.application_relay.as_mut() {
                relay.authenticated(
                    peer,
                    nonce,
                    verified.verified.user_address.clone(),
                    verified.verified.session.clone(),
                );
            }
        }
        let result = self.v4.success_result(&verified);
        crate::metrics::handshake("v4", "admitted");
        self.v4.commit_verified(peer, &verified, result.clone());
        self.v4.admit(peer, &verified);
        match PulseV4Role::try_from(verified.hello.role) {
            Ok(PulseV4Role::Player) => Action::AuthenticatedV4 {
                wallet: verified.verified.user_address,
                session: verified.verified.session,
                duplicate_of,
                initial_state: verified.hello.initial_state.map(Box::new),
                features,
                result,
            },
            Ok(PulseV4Role::Listener) => {
                let Some(listener) = self.build_listener(&verified.hello.listener_aoi) else {
                    return Action::Ignore;
                };
                Action::AuthenticatedListenerV4 {
                    wallet: verified.verified.user_address,
                    session: verified.verified.session,
                    duplicate_of,
                    listener: Arc::new(listener),
                    features,
                    result,
                }
            }
            Err(_) => Action::Ignore,
        }
    }

    fn handle_handshake(
        &mut self,
        peer: u32,
        req: crate::decentraland::pulse::HandshakeRequest,
        now_ms: i64,
    ) -> Action {
        crate::metrics::handshake("legacy", "player_attempt");
        if self.is_authenticated(peer) {
            return Action::Ignore;
        }

        let admitted = match self.verify_and_admit(peer, &req.auth_chain, now_ms) {
            Admission::Ok(a) => a,
            Admission::Deny(action) => return *action,
        };

        if let Some(init) = req.initial_state.as_ref() {
            if !self.validate_handshake_initial_state(init) {
                if let Some(state) = self.peers.get_mut(&peer) {
                    state.connection_state = PeerConnectionState::PendingDisconnect;
                }
                return Action::Reject {
                    reply: Some(handshake_response(
                        false,
                        Some("Invalid initial state".into()),
                    )),
                    reason: DisconnectReason::InvalidHandshakeField,
                };
            }
        }

        Action::Authenticated {
            wallet: admitted.wallet,
            session: admitted.session,
            duplicate_of: admitted.duplicate_of,
            initial_state: req.initial_state.map(Box::new),
            features: grantable_features(req.protocol_features & SERVER_FEATURES),
        }
    }

    fn verify_and_admit(&mut self, peer: u32, auth_chain: &[u8], now_ms: i64) -> Admission {
        if self.peers.get(&peer).map(|state| state.connection_state)
            != Some(PeerConnectionState::PendingAuth)
        {
            return Admission::Deny(Box::new(Action::Ignore));
        }

        if let Some(state) = self.peers.get_mut(&peer) {
            match self
                .attempt_policy
                .try_record_attempt(state.handshake_attempts)
            {
                Some(next) => state.handshake_attempts = next,
                None => {
                    state.connection_state = PeerConnectionState::PendingDisconnect;
                    return Admission::Deny(Box::new(Action::Reject {
                        reply: None,
                        reason: DisconnectReason::AuthFailed,
                    }));
                }
            }
        }

        let verified = match verify_handshake_bytes(auth_chain, now_ms) {
            Ok(v) => v,
            Err(e) => {
                return Admission::Deny(Box::new(Action::Reply(handshake_response(
                    false,
                    Some(e.message()),
                ))))
            }
        };
        let VerifiedHandshake {
            user_address,
            session,
            timestamp,
        } = verified;

        if self.ban_list.is_banned(&user_address) {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Admission::Deny(Box::new(Action::Reject {
                reply: Some(handshake_response(false, Some("banned".into()))),
                reason: DisconnectReason::Banned,
            }));
        }

        match self
            .replay_policy
            .try_admit(now_ms, &user_address, &timestamp)
        {
            Ok(()) => {}
            Err(ReplayRejection::Forgotten) => {
                return Admission::Deny(Box::new(Action::Reply(handshake_response(
                    false,
                    Some(
                        "timestamp is older than this server accepts; sign a new handshake".into(),
                    ),
                ))))
            }
            Err(ReplayRejection::FutureDated) => {
                return Admission::Deny(Box::new(Action::Reply(handshake_response(
                    false,
                    Some(
                        "timestamp is ahead of this server; synchronize the clock and sign a new handshake"
                            .into(),
                    ),
                ))))
            }
            Err(rejection) => {
                if let Some(state) = self.peers.get_mut(&peer) {
                    state.connection_state = PeerConnectionState::PendingDisconnect;
                }
                return Admission::Deny(Box::new(Action::Reject {
                    reply: None,
                    reason: match rejection {
                        ReplayRejection::Capacity | ReplayRejection::Storage => {
                            DisconnectReason::ServerFull
                        }
                        _ => DisconnectReason::HandshakeReplayRejected,
                    },
                }));
            }
        }

        let duplicate_of = self
            .identity
            .peer_by_wallet(&user_address)
            .filter(|p| *p != peer);
        Admission::Ok(Admitted {
            wallet: user_address,
            session,
            duplicate_of,
        })
    }

    fn handle_scene_listener_handshake(
        &mut self,
        peer: u32,
        req: SceneListenerHandshakeRequest,
        now_ms: i64,
    ) -> Action {
        crate::metrics::handshake("legacy", "listener_attempt");
        if self.peers.get(&peer).map(|s| s.connection_state)
            != Some(PeerConnectionState::PendingAuth)
        {
            return Action::Ignore;
        }

        let admitted = match self.verify_and_admit(peer, &req.auth_chain, now_ms) {
            Admission::Ok(a) => a,
            Admission::Deny(action) => return *action,
        };

        let Some(listener) = self.build_listener(&req.aoi) else {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: None,
                reason: DisconnectReason::InvalidHandshakeField,
            };
        };

        Action::AuthenticatedListener {
            wallet: admitted.wallet,
            session: admitted.session,
            duplicate_of: admitted.duplicate_of,
            listener: Arc::new(listener),
            features: grantable_features(req.protocol_features & SERVER_FEATURES),
        }
    }

    /// Reassigns a scene listener's AoI in place: the connection and identity stay, only the
    /// descriptor is swapped, and the simulation reads it afresh every tick so the new set is in
    /// force on the next one. Subjects the listener dropped stop being collected and age out
    /// through the ordinary stale-view sweep, exactly like a player walking out of range.
    fn handle_scene_listener_update(
        &mut self,
        peer: u32,
        req: SceneListenerUpdate,
        now: u32,
    ) -> Action {
        if !self.is_authenticated(peer) {
            return Action::Ignore;
        }
        if !self.is_scene_listener(peer) {
            self.scene_listener_forbidden_drops =
                self.scene_listener_forbidden_drops.wrapping_add(1);
            crate::metrics::scene_listener_forbidden_dropped();
            return Action::Ignore;
        }
        if !self.gameplay_limiter.try_accept_discrete(peer, now) {
            return Action::Ignore;
        }

        let Some(updated) = self.build_listener(&req.aoi) else {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: None,
                reason: DisconnectReason::InvalidSceneListenerField,
            };
        };

        let Some(state) = self.peers.get_mut(&peer) else {
            return Action::Ignore;
        };
        let (previous_parcel_count, previous_realm_count) = state
            .scene_listener
            .as_ref()
            .map(|l| (l.parcel_count(), l.realm_count()))
            .unwrap_or((0, 0));
        let parcel_count = updated.parcel_count();
        let realm_count = updated.realm_count();
        let cell_count = updated.cell_count();
        tracing::debug!(
            peer,
            realms = ?updated.parcels_by_realm.keys().collect::<Vec<_>>(),
            "scene listener now observes"
        );
        state.scene_listener = Some(Arc::new(updated));

        crate::metrics::scene_listener_parcels(parcel_count);
        tracing::info!(
            peer,
            parcel_count,
            realm_count,
            cell_count,
            previous_parcel_count,
            previous_realm_count,
            "scene listener reassigned its AoI"
        );
        Action::Applied
    }

    /// Shared gate for both scene-listener announcements. Every realm is checked and every rect
    /// bounds-checked and priced before that realm's rects are expanded, so a rejected
    /// announcement buys no expansion work on its way out; the budget spans the whole
    /// announcement, realms and parcels alike, so neither can be bought by adding more of the
    /// other. Overlapping rects are budgeted by sum, not union. The second pass also takes the
    /// covering grid cells off each rect, the only point that holds both a rect and its set.
    /// Parcel expansion stops once a realm's union already covers the world, but its cell cover
    /// never does: the cover is O(covered cells) rather than O(area), and running it for every
    /// rect is what keeps the cell set independent of rect order. No test can hold that ordering:
    /// a rect behind a world-covering union only ever contributes cells the union already holds,
    /// so moving `add_covering_cells` after the `continue` stays invisible in a single world.
    fn build_listener(&self, aoi: &[SceneListenerAoi]) -> Option<SceneListenerState> {
        if aoi.is_empty() {
            return None;
        }

        let max_budget = self.max_scene_listener_parcels as i64;
        let world_parcels = self.encoder.max_index_exclusive() as usize;
        let mut budget: i64 = 0;
        let mut parcels_by_realm: HashMap<String, std::collections::HashSet<i32>> =
            HashMap::with_capacity(aoi.len());
        let mut cell_keys = std::collections::HashSet::new();

        for realm_aoi in aoi {
            let realm = &realm_aoi.realm;
            if realm.is_empty() {
                return None;
            }
            if !validate::realm_length(realm, self.max_realm_length) {
                return None;
            }
            if parcels_by_realm.contains_key(realm) {
                return None;
            }
            if realm_aoi.parcel_rects.is_empty() {
                return None;
            }

            budget += SCENE_LISTENER_REALM_BUDGET_COST as i64;
            if budget > max_budget {
                return None;
            }

            let mut realm_area: i64 = 0;
            for r in &realm_aoi.parcel_rects {
                if r.min_x > r.max_x || r.min_z > r.max_z {
                    return None;
                }
                if !self.encoder.is_valid_coordinate(r.min_x, r.min_z)
                    || !self.encoder.is_valid_coordinate(r.max_x, r.max_z)
                {
                    return None;
                }
                let area = (r.max_x - r.min_x + 1) as i64 * (r.max_z - r.min_z + 1) as i64;
                realm_area += area;
                budget += area;
                if budget > max_budget {
                    return None;
                }
            }

            let mut parcels =
                std::collections::HashSet::with_capacity((realm_area as usize).min(world_parcels));
            for r in &realm_aoi.parcel_rects {
                self.cell_mapper.add_covering_cells(
                    &mut cell_keys,
                    r.min_x,
                    r.min_z,
                    r.max_x,
                    r.max_z,
                );
                if parcels.len() >= world_parcels {
                    continue;
                }
                for z in r.min_z..=r.max_z {
                    for x in r.min_x..=r.max_x {
                        parcels.insert(self.encoder.encode(x, z));
                    }
                }
            }
            parcels_by_realm.insert(realm.clone(), parcels);
        }

        Some(SceneListenerState::new(parcels_by_realm, cell_keys))
    }

    fn validate_handshake_initial_state(&self, init: &PlayerInitialState) -> bool {
        let state_ok = init
            .state
            .as_ref()
            .map(|s| validate::player_state(s, &self.encoder))
            .unwrap_or(false);
        state_ok
            && !init.realm.is_empty()
            && validate::emote_caps(
                init.emote_id.as_deref(),
                init.emote_duration_ms,
                self.max_emote_id_length,
                self.max_emote_duration_ms,
            )
            && validate::realm_length(&init.realm, self.max_realm_length)
    }

    fn pre_auth_for(&mut self, peer: u32) -> &mut PreAuthAdmission {
        if peer >= ENET_CAPACITY as u32 {
            &mut self.pre_auth_wt
        } else {
            &mut self.pre_auth_enet
        }
    }

    fn is_authenticated(&self, peer: u32) -> bool {
        self.peers
            .get(&peer)
            .map(|s| s.connection_state == PeerConnectionState::Authenticated)
            .unwrap_or(false)
    }

    fn apply_gameplay(&mut self, peer: u32, now: u32, msg: client_message::Message) -> Action {
        let accepted = match &msg {
            client_message::Message::Input(_) => self.gameplay_limiter.try_accept_input(peer, now),
            client_message::Message::Teleport(_)
            | client_message::Message::EmoteStart(_)
            | client_message::Message::EmoteStop(_)
            | client_message::Message::ProfileAnnouncement(_) => {
                self.gameplay_limiter.try_accept_discrete(peer, now)
            }
            client_message::Message::Resync(_)
            | client_message::Message::Handshake(_)
            | client_message::Message::SceneListenerHandshake(_)
            | client_message::Message::SceneListenerUpdate(_)
            | client_message::Message::V4Hello(_)
            | client_message::Message::V4Auth(_)
            | client_message::Message::ApplicationJoin(_)
            | client_message::Message::ApplicationLeave(_)
            | client_message::Message::ApplicationSend(_) => true,
        };
        if !accepted {
            return Action::Ignore;
        }
        match msg {
            client_message::Message::Input(input) => {
                let Some(state) = input.state else {
                    return Action::Ignore;
                };
                if !validate::player_state(&state, &self.encoder) {
                    return Action::Ignore;
                }
                PeerSnapshotPublisher::publish_from_player_state(
                    &mut self.board,
                    &mut self.grids,
                    &self.encoder,
                    peer,
                    now,
                    &state,
                    None,
                    None,
                );
                Action::Applied
            }
            client_message::Message::Teleport(t) => {
                if !validate::realm_length(&t.realm, self.max_realm_length) {
                    return Action::Reject {
                        reply: None,
                        reason: DisconnectReason::InvalidTeleportField,
                    };
                }
                if !validate::teleport(&t, &self.encoder) {
                    return Action::Ignore;
                }
                PeerSnapshotPublisher::publish_teleport(
                    &mut self.board,
                    &mut self.grids,
                    &self.encoder,
                    peer,
                    now,
                    t.parcel_index,
                    t.position_x,
                    t.position_y,
                    t.position_z,
                    t.realm,
                );
                Action::Applied
            }
            client_message::Message::EmoteStart(e) => {
                if !validate::emote_caps(
                    Some(&e.emote_id),
                    e.duration_ms,
                    self.max_emote_id_length,
                    self.max_emote_duration_ms,
                ) {
                    return Action::Reject {
                        reply: None,
                        reason: DisconnectReason::InvalidEmoteField,
                    };
                }
                let Some(state) = e.player_state else {
                    return Action::Ignore;
                };
                if !validate::player_state(&state, &self.encoder) {
                    return Action::Ignore;
                }
                PeerSnapshotPublisher::publish_from_player_state(
                    &mut self.board,
                    &mut self.grids,
                    &self.encoder,
                    peer,
                    now,
                    &state,
                    Some(EmoteInput {
                        emote_id: e.emote_id,
                        duration_ms: e.duration_ms,
                        start_tick: None,
                        mask: e.mask,
                    }),
                    None,
                );
                Action::Applied
            }
            client_message::Message::EmoteStop(_) => {
                self.publish_emote_stop(peer, now, EmoteStopReason::Cancelled);
                Action::Applied
            }
            client_message::Message::ProfileAnnouncement(p) => {
                if p.version >= 0 && p.version > self.profiles.get(peer) {
                    self.profiles.set(peer, p.version);
                }
                Action::Applied
            }
            client_message::Message::Resync(r) => {
                if let Some(state) = self.peers.get_mut(&peer) {
                    state.request_resync(r.subject_id, r.known_seq);
                }
                Action::Applied
            }
            client_message::Message::Handshake(_)
            | client_message::Message::SceneListenerHandshake(_)
            | client_message::Message::SceneListenerUpdate(_)
            | client_message::Message::V4Hello(_)
            | client_message::Message::V4Auth(_)
            | client_message::Message::ApplicationJoin(_)
            | client_message::Message::ApplicationLeave(_)
            | client_message::Message::ApplicationSend(_) => Action::Ignore,
        }
    }

    fn publish_emote_stop(&mut self, peer: u32, now: u32, reason: EmoteStopReason) {
        let Some(current) = self.board.try_read(peer).cloned() else {
            return;
        };
        if !current.is_emoting() {
            return;
        }
        let active = current.emote.clone().unwrap();
        let stop = crate::snapshot::PeerSnapshot {
            seq: self.board.last_seq(peer).wrapping_add(1),
            server_tick: now,
            emote: Some(crate::snapshot::EmoteState {
                emote_id: None,
                start_seq: active.start_seq,
                start_tick: active.start_tick,
                duration_ms: None,
                mask: None,
                stop_reason: Some(reason),
            }),
            ..current
        };
        self.board.publish(peer, stop);
    }

    fn complete_expired_emotes(&mut self, now: u32) {
        let expired: Vec<u32> = self
            .peers
            .iter()
            .filter(|(_, state)| state.connection_state == PeerConnectionState::Authenticated)
            .filter_map(|(peer, _)| {
                let emote = self.board.try_read(*peer)?.emote.as_ref()?;
                emote.emote_id.as_ref()?;
                let duration_ms = emote.duration_ms?;
                (now >= emote.start_tick && now - emote.start_tick >= duration_ms).then_some(*peer)
            })
            .collect();
        for peer in expired {
            self.publish_emote_stop(peer, now, EmoteStopReason::Completed);
        }
    }

    async fn apply(
        &mut self,
        transports: &mut Transports,
        peer: u32,
        action: Action,
        now: u32,
    ) -> anyhow::Result<()> {
        match action {
            Action::Reply(msg) => {
                self.send(transports, peer, channel::RELIABLE, &msg).await?;
            }
            Action::Reject { reply, reason } => {
                if let Some(msg) = reply {
                    self.send(transports, peer, channel::RELIABLE, &msg).await?;
                }
                self.pre_auth_for(peer).release_on_disconnect(peer);
                transports.disconnect(peer, reason.code()).await?;
                tracing::info!(peer, ?reason, "peer rejected by hardening");
            }
            Action::Authenticated {
                wallet,
                session,
                duplicate_of,
                initial_state,
                features,
            } => {
                if !self.peers.contains_key(&peer) {
                    return Ok(());
                }
                if let Some(dup) = duplicate_of {
                    self.pre_auth_for(dup).release_on_disconnect(dup);
                    transports
                        .disconnect(dup, DisconnectReason::DuplicateSession.code())
                        .await?;
                    self.begin_disconnect(transports, dup).await?;
                }
                if let Some(s) = self.peers.get_mut(&peer) {
                    s.wallet_id = Some(wallet.clone());
                    s.connection_state = PeerConnectionState::Authenticated;
                    s.features = features;
                }

                self.pre_auth_for(peer).release_on_promotion(peer);
                self.identity
                    .set_with_session(peer, wallet.clone(), session);
                self.board.set_active(peer);

                if let Some(init) = initial_state {
                    self.seed_initial_state(peer, now, &init);
                }
                let ok = handshake_response(true, None);
                self.send(transports, peer, channel::RELIABLE, &ok).await?;
                tracing::info!(peer, %wallet, "peer authenticated");
            }
            Action::AuthenticatedListener {
                wallet,
                session,
                duplicate_of,
                listener,
                features,
            } => {
                if !self.peers.contains_key(&peer) {
                    return Ok(());
                }
                if let Some(dup) = duplicate_of {
                    self.pre_auth_for(dup).release_on_disconnect(dup);
                    transports
                        .disconnect(dup, DisconnectReason::DuplicateSession.code())
                        .await?;
                    self.begin_disconnect(transports, dup).await?;
                }
                let parcel_count = listener.parcel_count();
                let realm_count = listener.realm_count();
                let cell_count = listener.cell_count();
                tracing::debug!(
                    peer,
                    realms = ?listener.parcels_by_realm.keys().collect::<Vec<_>>(),
                    "scene listener observes"
                );
                if let Some(s) = self.peers.get_mut(&peer) {
                    s.wallet_id = Some(wallet.clone());
                    s.connection_state = PeerConnectionState::Authenticated;
                    s.features = features;
                    s.scene_listener = Some(listener);
                }

                self.pre_auth_for(peer).release_on_promotion(peer);
                self.identity
                    .set_with_session(peer, wallet.clone(), session);
                crate::metrics::scene_listener_connected_inc();
                crate::metrics::scene_listener_parcels(parcel_count);

                let ok = handshake_response(true, None);
                self.send(transports, peer, channel::RELIABLE, &ok).await?;
                tracing::info!(peer, %wallet, parcel_count, realm_count, cell_count, "scene listener authenticated");
            }
            Action::AuthenticatedV4 {
                wallet,
                session,
                duplicate_of,
                initial_state,
                features,
                result,
            } => {
                if !self.peers.contains_key(&peer) {
                    return Ok(());
                }
                if let Some(dup) = duplicate_of {
                    self.pre_auth_for(dup).release_on_disconnect(dup);
                    transports
                        .disconnect(dup, DisconnectReason::DuplicateSession.code())
                        .await?;
                    self.begin_disconnect(transports, dup).await?;
                }
                if let Some(state) = self.peers.get_mut(&peer) {
                    state.wallet_id = Some(wallet.clone());
                    state.connection_state = PeerConnectionState::Authenticated;
                    state.features = features;
                }
                self.pre_auth_for(peer).release_on_promotion(peer);
                self.identity
                    .set_with_session(peer, wallet.clone(), session);
                self.board.set_active(peer);
                if let Some(initial) = initial_state {
                    self.seed_initial_state(peer, now, &initial);
                }
                self.send(transports, peer, channel::RELIABLE, &server_result(result))
                    .await?;
                tracing::info!(peer, %wallet, protocol_version = 4, "peer authenticated");
            }
            Action::AuthenticatedListenerV4 {
                wallet,
                session,
                duplicate_of,
                listener,
                features,
                result,
            } => {
                if !self.peers.contains_key(&peer) {
                    return Ok(());
                }
                if let Some(dup) = duplicate_of {
                    self.pre_auth_for(dup).release_on_disconnect(dup);
                    transports
                        .disconnect(dup, DisconnectReason::DuplicateSession.code())
                        .await?;
                    self.begin_disconnect(transports, dup).await?;
                }
                let parcel_count = listener.parcel_count();
                let realm_count = listener.realm_count();
                let cell_count = listener.cell_count();
                if let Some(state) = self.peers.get_mut(&peer) {
                    state.wallet_id = Some(wallet.clone());
                    state.connection_state = PeerConnectionState::Authenticated;
                    state.features = features;
                    state.scene_listener = Some(listener);
                }
                self.pre_auth_for(peer).release_on_promotion(peer);
                self.identity
                    .set_with_session(peer, wallet.clone(), session);
                crate::metrics::scene_listener_connected_inc();
                crate::metrics::scene_listener_parcels(parcel_count);
                self.send(transports, peer, channel::RELIABLE, &server_result(result))
                    .await?;
                tracing::info!(peer, %wallet, parcel_count, realm_count, cell_count, protocol_version = 4, "scene listener authenticated");
            }
            Action::Application(message) => {
                self.apply_application(transports, peer, message).await?;
            }
            Action::Applied | Action::Ignore => {}
        }
        transports.flush();
        Ok(())
    }

    fn seed_initial_state(&mut self, peer: u32, now: u32, init: &PlayerInitialState) {
        let Some(state) = init.state.as_ref() else {
            return;
        };
        let emote = init
            .emote_id
            .as_ref()
            .filter(|id| !id.is_empty())
            .map(|id| {
                let offset = init.emote_start_offset_ms.unwrap_or(0);
                let start_tick = now.saturating_sub(offset);
                EmoteInput {
                    emote_id: id.clone(),
                    duration_ms: init.emote_duration_ms,
                    start_tick: Some(start_tick),
                    mask: init.emote_mask,
                }
            });
        PeerSnapshotPublisher::publish_from_player_state(
            &mut self.board,
            &mut self.grids,
            &self.encoder,
            peer,
            now,
            state,
            emote,
            Some(&init.realm),
        );
    }

    fn simulate(&mut self, now: u32) -> Vec<OutgoingMessage> {
        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.simulation.outbox.clear();
        self.complete_expired_emotes(now);
        self.simulation.simulate_tick(
            &mut self.peers,
            &self.board,
            &self.grids,
            &self.aoi,
            &self.identity,
            &self.profiles,
            self.tick_counter,
            now,
        );
        std::mem::take(&mut self.simulation.outbox)
    }

    async fn run_tick(&mut self, transports: &mut Transports, now: u32) -> anyhow::Result<()> {
        self.v4.expire(chrono::Utc::now().timestamp_millis());
        let outbox = self.simulate(now);
        self.flush(transports, outbox).await?;

        let expired = std::mem::take(&mut self.simulation.expired);
        for e in expired {
            tracing::info!(peer = e.peer, reason = ?e.reason, "reaping peer");
            if e.reason == crate::simulation::ExpiredReason::AuthTimeout {
                self.pre_auth_for(e.peer).release_on_disconnect(e.peer);
                transports
                    .disconnect(e.peer, DisconnectReason::AuthTimeout.code())
                    .await?;
            }
            self.cleanup_peer(transports, e.peer).await?;
        }
        Ok(())
    }

    async fn flush(
        &mut self,
        transports: &mut Transports,
        outbox: Vec<OutgoingMessage>,
    ) -> anyhow::Result<()> {
        for out in outbox {
            let channel = match out.mode {
                PacketMode::Reliable => channel::RELIABLE,
                PacketMode::UnreliableSequenced => channel::UNRELIABLE_SEQUENCED,
                PacketMode::UnreliableUnsequenced => channel::UNRELIABLE_UNSEQUENCED,
            };
            self.send(transports, out.target, channel, &out.message)
                .await?;
        }
        transports.flush();
        Ok(())
    }

    pub async fn run(self, bind: SocketAddr) -> anyhow::Result<()> {
        self.run_with_tick(bind, 50).await
    }

    pub async fn run_with_tick(self, bind: SocketAddr, tick_ms: u64) -> anyhow::Result<()> {
        let enet = Host::bind(HostConfig {
            bind,
            max_peers: ENET_CAPACITY,
            channel_limit: 8,
        })
        .await?;
        tracing::info!(%bind, "catalyrst-pulse listening (enet)");
        let transports = Transports::enet_only(enet, ENET_CAPACITY as u32);
        self.run_on(transports, tick_ms).await
    }

    pub async fn run_with_webtransport(
        self,
        bind: SocketAddr,
        tick_ms: u64,
        wt: Option<WtConfig>,
    ) -> anyhow::Result<()> {
        let enet = Host::bind(HostConfig {
            bind,
            max_peers: ENET_CAPACITY,
            channel_limit: 8,
        })
        .await?;
        tracing::info!(%bind, "catalyrst-pulse listening (enet)");
        let transports = match wt {
            Some(cfg) => {
                let wt_bind = cfg.bind_addr;
                let (host, events) =
                    tokio::task::spawn_blocking(move || WtHost::start(cfg)).await??;
                tracing::info!(bind = %wt_bind, local = %host.local_addr(),
                    "catalyrst-pulse listening (webtransport)");
                Transports::with_webtransport(enet, ENET_CAPACITY as u32, host, events)
            }
            None => Transports::enet_only(enet, ENET_CAPACITY as u32),
        };
        self.run_on(transports, tick_ms).await
    }

    pub async fn serve(self, transports: Transports, tick_ms: u64) -> anyhow::Result<()> {
        self.run_on(transports, tick_ms).await
    }

    async fn run_on(self, transports: Transports, tick_ms: u64) -> anyhow::Result<()> {
        self.run_inspected(transports, tick_ms, None).await
    }

    pub async fn serve_with_inspection(
        self,
        transports: Transports,
        tick_ms: u64,
        requests: inspection::Requests,
    ) -> anyhow::Result<()> {
        self.run_inspected(transports, tick_ms, Some(requests))
            .await
    }

    async fn run_inspected(
        mut self,
        mut transports: Transports,
        tick_ms: u64,
        mut inspection: Option<inspection::Requests>,
    ) -> anyhow::Result<()> {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
        let mut cluster_ticker = tokio::time::interval(std::time::Duration::from_millis(
            self.clusters
                .as_ref()
                .map(|c| c.pass_interval_ms())
                .unwrap_or(crate::cluster::DEFAULT_PASS_INTERVAL_MS),
        ));
        cluster_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.replay_policy
            .forget_before(chrono::Utc::now().timestamp_millis());
        let started = std::time::Instant::now();
        loop {
            let relay_wakeup = self
                .application_relay
                .as_ref()
                .and_then(|relay| relay.next_renewal());
            tokio::select! {
                request = async {
                    match inspection.as_mut() {
                        Some(requests) => requests.0.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(request) = request {
                        let mut snapshot = inspection::snapshot(&self, &request.wallet);
                        if let Some(snapshot) = snapshot.as_mut() {
                            snapshot.inspected_at_tick = started.elapsed().as_millis() as u32;
                        }
                        let _ = request.reply.send(snapshot);
                    } else {
                        inspection = None;
                    }
                }
                serviced = transports.service() => {
                    match serviced? {
                        Some(Event::Connect { peer, ip }) => {
                            let now = started.elapsed().as_millis() as u32;

                            let ip = ip
                                .or_else(|| transports.peer_ip(peer as u32))
                                .unwrap_or_default();
                            let admit = self.pre_auth_for(peer as u32).try_admit(peer as u32, &ip);
                            if let Some(reason) = crate::hardening::pre_auth_refusal_reason(admit) {
                                self.pre_auth_for(peer as u32).release_on_disconnect(peer as u32);
                                tracing::warn!(peer, %ip, ?reason, "pre-auth admission refused");
                                transports.disconnect_now(peer as u32, reason.code()).await?;
                                continue;
                            }
                            let mut state = PeerState::new(PeerConnectionState::PendingAuth, now);
                            state.ip = Some(ip);
                            if let Err(error) = self.v4.connected(peer as u32) {
                                self.pre_auth_for(peer as u32).release_on_disconnect(peer as u32);
                                tracing::error!(peer, %error, "Pulse v4 connection binding failed");
                                transports
                                    .disconnect_now(peer as u32, DisconnectReason::AuthFailed.code())
                                    .await?;
                                continue;
                            }
                            self.peers.insert(peer as u32, state);
                            tracing::debug!(peer, "session connected (pending auth)");
                        }
                        Some(Event::Receive { peer, channel, packet }) => {
                            let now_ms = chrono::Utc::now().timestamp_millis();
                            let now = started.elapsed().as_millis() as u32;
                            let action = self.dispatch(peer as u32, channel, &packet.data, now_ms, now);
                            self.apply(&mut transports, peer as u32, action, now).await?;
                        }
                        Some(Event::Corrupt { peer }) => {
                            let now = started.elapsed().as_millis() as u32;
                            if self
                                .corrupted_limiter
                                .register_and_check_exhausted(peer as u32, now)
                            {
                                self.pre_auth_for(peer as u32).release_on_disconnect(peer as u32);
                                transports
                                    .disconnect(peer as u32, DisconnectReason::PacketCorrupted.code())
                                    .await?;
                                tracing::info!(peer, "webtransport peer disconnected (corrupt budget)");
                            }
                        }
                        Some(Event::Disconnect { peer }) => {
                            self.on_disconnect(&mut transports, peer as u32).await?;
                        }
                        None => {}
                    }
                }
                _ = async {
                    match relay_wakeup {
                        Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(relay) = self.application_relay.as_mut() { relay.schedule_renewals(std::time::Instant::now()); }
                }
                completed = async {
                    match self.application_relay.as_mut() {
                        Some(relay) if relay.has_jobs() => relay.completed().await,
                        _ => std::future::pending().await,
                    }
                } => {
                    self.deliver_application(&mut transports, completed.0, completed.1).await?;
                }
                _ = ticker.tick() => {
                    let instant = std::time::Instant::now();
                    let mut expired = self.application_relay.as_mut().map(|relay| relay.maintenance(instant)).unwrap_or_default();
                    expired.extend(self.silent_relay_scopes(&transports, instant));
                    self.deliver_application(&mut transports, expired, vec![]).await?;
                    let now = started.elapsed().as_millis() as u32;
                    self.run_tick(&mut transports, now).await?;
                }
                _ = cluster_ticker.tick(), if self.clusters.is_some() => {
                    self.run_cluster_pass();
                }
            }
        }
    }

    async fn begin_disconnect(
        &mut self,
        transports: &mut Transports,
        peer: u32,
    ) -> anyhow::Result<()> {
        self.cleanup_peer(transports, peer).await
    }

    async fn on_disconnect(
        &mut self,
        transports: &mut Transports,
        peer: u32,
    ) -> anyhow::Result<()> {
        self.pre_auth_for(peer).release_on_disconnect(peer);
        self.corrupted_limiter.release(peer);
        self.cleanup_peer(transports, peer).await
    }

    fn remove_peer_state(&mut self, peer: u32) -> bool {
        let Some(state) = self.peers.remove(&peer) else {
            return false;
        };
        self.pre_auth_for(peer).release_on_disconnect(peer);
        self.corrupted_limiter.release(peer);
        self.board.clear_active(peer);
        self.grids.remove(peer);
        self.identity.remove(peer);
        self.profiles.remove(peer);
        self.simulation.cleanup_observer_views(peer);
        self.gameplay_limiter.release(peer);
        self.v4.disconnected(peer);
        if state.is_listener() {
            crate::metrics::scene_listener_connected_dec();
            false
        } else {
            true
        }
    }

    async fn cleanup_peer(&mut self, transports: &mut Transports, peer: u32) -> anyhow::Result<()> {
        let relay_left = self
            .application_relay
            .as_mut()
            .map(|relay| relay.disconnect(peer))
            .unwrap_or_default();
        let notify = self.remove_peer_state(peer);
        self.deliver_application(transports, relay_left, vec![])
            .await?;
        if notify {
            self.notify_player_left(transports, peer).await?;
        }
        Ok(())
    }

    async fn notify_player_left(
        &self,
        transports: &mut Transports,
        peer: u32,
    ) -> anyhow::Result<()> {
        let left = ServerMessage {
            message: Some(server_message::Message::PlayerLeft(
                crate::decentraland::pulse::PlayerLeft { subject_id: peer },
            )),
        };
        let targets: Vec<u32> = self
            .peers
            .iter()
            .filter(|(_, s)| s.connection_state == PeerConnectionState::Authenticated)
            .map(|(id, _)| *id)
            .collect();
        for target in targets {
            self.send(transports, target, channel::RELIABLE, &left)
                .await?;
        }
        transports.flush();
        Ok(())
    }

    async fn send(
        &self,
        transports: &mut Transports,
        peer: u32,
        channel: u8,
        msg: &ServerMessage,
    ) -> anyhow::Result<()> {
        let bytes = msg.encode_to_vec();
        let packet = if channel == channel::RELIABLE {
            Packet::reliable(channel, bytes)
        } else if channel == channel::UNRELIABLE_UNSEQUENCED {
            Packet::unsequenced(channel, bytes)
        } else {
            Packet::unreliable(channel, bytes)
        };
        transports.send(peer, packet).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
