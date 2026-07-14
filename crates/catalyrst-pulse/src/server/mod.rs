use std::collections::HashMap;
use std::net::SocketAddr;

use prost::Message as _;

use crate::decentraland::pulse::{
    client_message, server_message, ClientMessage, HandshakeResponse, PlayerInitialState,
    SceneListenerHandshakeRequest, ServerMessage,
};
use crate::handshake::{verify_handshake_bytes, VerifiedHandshake};
use crate::hardening::{
    BanList, CorruptedPacketLimiter, DisconnectReason, GameplayRateLimiter, HandshakeAttemptPolicy,
    HandshakeReplayPolicy, PreAuthAdmission, DEFAULT_DISCRETE_BURST, DEFAULT_DISCRETE_RATE_PER_SEC,
    DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ, DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP,
    DEFAULT_MAX_EMOTE_DURATION_MS, DEFAULT_MAX_EMOTE_ID_LENGTH, DEFAULT_MAX_HANDSHAKE_ATTEMPTS,
    DEFAULT_MAX_REALM_LENGTH, DEFAULT_PRE_AUTH_BUDGET, DEFAULT_PRE_AUTH_BUDGET_WT,
    DEFAULT_SCENE_LISTENER_MAX_PARCELS,
};
use crate::interest::{
    ParcelEncoder, ParcelEncoderOptions, SceneListenerState, SpatialAreaOfInterest,
    SpatialAreaOfInterestOptions, SpatialGrid,
};
use crate::simulation::{
    OutgoingMessage, PacketMode, PeerConnectionState, PeerSimulation, PeerState,
};
use crate::snapshot::{
    EmoteInput, IdentityBoard, PeerSnapshotPublisher, ProfileBoard, SnapshotBoard,
};
use crate::transport::webtransport::{WtConfig, WtHost};
use crate::transport::{Event, Host, HostConfig, Packet, Transports};

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
pub const SERVER_FEATURES: u32 = FEATURE_DELTA_BATCH;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Reply(ServerMessage),

    Authenticated {
        wallet: String,
        duplicate_of: Option<u32>,
        initial_state: Option<Box<PlayerInitialState>>,
        features: u32,
    },

    AuthenticatedListener {
        wallet: String,
        duplicate_of: Option<u32>,
        listener: Box<SceneListenerState>,
        features: u32,
    },

    Applied,

    Reject {
        reply: Option<ServerMessage>,
        reason: DisconnectReason,
    },

    Ignore,
}

struct Admitted {
    wallet: String,
    duplicate_of: Option<u32>,
}

enum Admission {
    Ok(Admitted),
    Deny(Action),
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
        })),
    }
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
    pub grid: SpatialGrid,
    pub encoder: ParcelEncoder,
    pub aoi: SpatialAreaOfInterest,
    pub identity: IdentityBoard,
    pub profiles: ProfileBoard,
    pub simulation: PeerSimulation,

    pub replay_policy: HandshakeReplayPolicy,

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
        Self {
            peers: HashMap::new(),
            board: SnapshotBoard::new(max_peers, ring_capacity),
            grid: SpatialGrid::new(16.0),
            encoder: ParcelEncoder::new(ParcelEncoderOptions::default()),
            aoi: SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default()),
            identity: IdentityBoard::new(max_peers),
            profiles: ProfileBoard::new(max_peers),
            simulation: PeerSimulation::new(simulation_steps, resync_with_delta),

            replay_policy: HandshakeReplayPolicy::new(
                true,
                crate::simulation::DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS,
                max_peers,
            ),
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
        let _ = channel;
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

        if self.is_scene_listener(peer) && !matches!(inner, client_message::Message::Resync(_)) {
            self.scene_listener_forbidden_drops =
                self.scene_listener_forbidden_drops.wrapping_add(1);
            crate::metrics::scene_listener_forbidden_dropped();
            return Action::Ignore;
        }

        match inner {
            client_message::Message::Handshake(req) => {
                self.handle_handshake(peer, req, now_ms, now)
            }
            client_message::Message::SceneListenerHandshake(req) => {
                self.handle_scene_listener_handshake(peer, req, now_ms, now)
            }
            other => {
                if !self.is_authenticated(peer) {
                    return Action::Ignore;
                }
                self.apply_gameplay(peer, now, other)
            }
        }
    }

    fn is_scene_listener(&self, peer: u32) -> bool {
        self.peers
            .get(&peer)
            .map(|s| s.connection_state == PeerConnectionState::Authenticated && s.is_listener())
            .unwrap_or(false)
    }

    fn handle_handshake(
        &mut self,
        peer: u32,
        req: crate::decentraland::pulse::HandshakeRequest,
        now_ms: i64,
        now: u32,
    ) -> Action {
        if self.is_authenticated(peer) {
            return Action::Ignore;
        }

        let admitted = match self.verify_and_admit(peer, &req.auth_chain, now, now_ms) {
            Admission::Ok(a) => a,
            Admission::Deny(action) => return action,
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
            duplicate_of: admitted.duplicate_of,
            initial_state: req.initial_state.map(Box::new),
            features: req.protocol_features & SERVER_FEATURES,
        }
    }

    fn verify_and_admit(
        &mut self,
        peer: u32,
        auth_chain: &[u8],
        now: u32,
        now_ms: i64,
    ) -> Admission {
        if !self.peers.contains_key(&peer) {
            return Admission::Deny(Action::Ignore);
        }

        if let Some(state) = self.peers.get_mut(&peer) {
            match self
                .attempt_policy
                .try_record_attempt(state.handshake_attempts)
            {
                Some(next) => state.handshake_attempts = next,
                None => {
                    state.connection_state = PeerConnectionState::PendingDisconnect;
                    return Admission::Deny(Action::Reject {
                        reply: None,
                        reason: DisconnectReason::AuthFailed,
                    });
                }
            }
        }

        let verified = match verify_handshake_bytes(auth_chain, now_ms) {
            Ok(v) => v,
            Err(e) => {
                return Admission::Deny(Action::Reply(handshake_response(false, Some(e.message()))))
            }
        };
        let VerifiedHandshake {
            user_address,
            timestamp,
        } = verified;

        if self.ban_list.is_banned(&user_address) {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Admission::Deny(Action::Reject {
                reply: Some(handshake_response(false, Some("banned".into()))),
                reason: DisconnectReason::Banned,
            });
        }

        if !self.replay_policy.try_admit(now, &user_address, &timestamp) {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Admission::Deny(Action::Reject {
                reply: None,
                reason: DisconnectReason::HandshakeReplayRejected,
            });
        }

        let duplicate_of = self
            .identity
            .peer_by_wallet(&user_address)
            .filter(|p| *p != peer);
        Admission::Ok(Admitted {
            wallet: user_address,
            duplicate_of,
        })
    }

    fn handle_scene_listener_handshake(
        &mut self,
        peer: u32,
        req: SceneListenerHandshakeRequest,
        now_ms: i64,
        now: u32,
    ) -> Action {
        if self.peers.get(&peer).map(|s| s.connection_state)
            != Some(PeerConnectionState::PendingAuth)
        {
            return Action::Ignore;
        }

        let admitted = match self.verify_and_admit(peer, &req.auth_chain, now, now_ms) {
            Admission::Ok(a) => a,
            Admission::Deny(action) => return action,
        };

        let Some(listener) = self.build_listener(&req) else {
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
            duplicate_of: admitted.duplicate_of,
            listener: Box::new(listener),
            features: req.protocol_features & SERVER_FEATURES,
        }
    }

    fn build_listener(&self, req: &SceneListenerHandshakeRequest) -> Option<SceneListenerState> {
        if req.realm.is_empty() || req.realm.chars().count() > self.max_realm_length {
            return None;
        }
        if req.parcel_rects.is_empty() {
            return None;
        }

        let mut nominal: i64 = 0;
        for r in &req.parcel_rects {
            if r.min_x > r.max_x || r.min_z > r.max_z {
                return None;
            }
            if !self.encoder.is_valid_coordinate(r.min_x, r.min_z)
                || !self.encoder.is_valid_coordinate(r.max_x, r.max_z)
            {
                return None;
            }
            nominal += (r.max_x - r.min_x + 1) as i64 * (r.max_z - r.min_z + 1) as i64;
            if nominal > self.max_scene_listener_parcels as i64 {
                return None;
            }
        }

        let mut parcels = std::collections::HashSet::new();
        for r in &req.parcel_rects {
            for z in r.min_z..=r.max_z {
                for x in r.min_x..=r.max_x {
                    parcels.insert(self.encoder.encode(x, z));
                }
            }
        }

        Some(SceneListenerState {
            realm: req.realm.clone(),
            parcels,
        })
    }

    fn validate_handshake_initial_state(&self, init: &PlayerInitialState) -> bool {
        let state_ok = init
            .state
            .as_ref()
            .map(|s| validate::player_state(s, &self.encoder))
            .unwrap_or(false);
        state_ok
            && validate::emote_caps(
                init.emote_id.as_deref(),
                init.emote_duration_ms,
                self.max_emote_id_length,
                self.max_emote_duration_ms,
            )
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
            | client_message::Message::SceneListenerHandshake(_) => true,
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
                    &mut self.grid,
                    &self.encoder,
                    peer,
                    now,
                    &state,
                    None,
                );
                Action::Applied
            }
            client_message::Message::Teleport(t) => {
                if !validate::teleport(&t, &self.encoder) {
                    return Action::Ignore;
                }
                PeerSnapshotPublisher::publish_teleport(
                    &mut self.board,
                    &mut self.grid,
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
                    &mut self.grid,
                    &self.encoder,
                    peer,
                    now,
                    &state,
                    Some(EmoteInput {
                        emote_id: e.emote_id,
                        duration_ms: e.duration_ms,
                        start_tick: None,
                    }),
                );
                Action::Applied
            }
            client_message::Message::EmoteStop(_) => {
                self.publish_emote_stop(peer, now);
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
            | client_message::Message::SceneListenerHandshake(_) => Action::Ignore,
        }
    }

    fn publish_emote_stop(&mut self, peer: u32, now: u32) {
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
                stop_reason: Some(crate::decentraland::pulse::EmoteStopReason::Cancelled),
            }),
            ..current
        };
        self.board.publish(peer, stop);
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
                self.identity.set(peer, wallet.clone());
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
                let parcel_count = listener.parcels.len();
                if let Some(s) = self.peers.get_mut(&peer) {
                    s.wallet_id = Some(wallet.clone());
                    s.connection_state = PeerConnectionState::Authenticated;
                    s.features = features;
                    s.scene_listener = Some(*listener);
                }

                self.pre_auth_for(peer).release_on_promotion(peer);
                self.identity.set(peer, wallet.clone());
                crate::metrics::scene_listener_connected_inc();
                crate::metrics::scene_listener_parcels(parcel_count);

                let ok = handshake_response(true, None);
                self.send(transports, peer, channel::RELIABLE, &ok).await?;
                tracing::info!(peer, %wallet, parcel_count, "scene listener authenticated");
            }
            Action::Applied | Action::Ignore => {}
        }
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
                }
            });
        PeerSnapshotPublisher::publish_from_player_state(
            &mut self.board,
            &mut self.grid,
            &self.encoder,
            peer,
            now,
            state,
            emote,
        );
    }

    async fn run_tick(&mut self, transports: &mut Transports, now: u32) -> anyhow::Result<()> {
        self.tick_counter = self.tick_counter.wrapping_add(1);
        self.simulation.outbox.clear();
        self.simulation.simulate_tick(
            &mut self.peers,
            &self.board,
            &self.aoi,
            &self.identity,
            &self.profiles,
            self.tick_counter,
            now,
        );
        let outbox = std::mem::take(&mut self.simulation.outbox);
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
                let (host, events) = WtHost::start(cfg)?;
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

    async fn run_on(mut self, mut transports: Transports, tick_ms: u64) -> anyhow::Result<()> {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
        let started = std::time::Instant::now();
        loop {
            tokio::select! {
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
                _ = ticker.tick() => {
                    let now = started.elapsed().as_millis() as u32;
                    self.run_tick(&mut transports, now).await?;
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

    async fn cleanup_peer(&mut self, transports: &mut Transports, peer: u32) -> anyhow::Result<()> {
        let was_listener = self
            .peers
            .get(&peer)
            .map(|s| s.connection_state == PeerConnectionState::Authenticated && s.is_listener())
            .unwrap_or(false);

        self.board.clear_active(peer);
        self.grid.remove(peer);
        self.identity.remove(peer);
        self.profiles.remove(peer);
        self.simulation.cleanup_observer_views(peer);
        self.gameplay_limiter.release(peer);
        self.peers.remove(&peer);

        if was_listener {
            crate::metrics::scene_listener_connected_dec();
            return Ok(());
        }

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
