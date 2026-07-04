use std::collections::HashMap;
use std::net::SocketAddr;

use catalyrst_enet::{Event, Host, HostConfig, Packet};
use prost::Message as _;

use crate::decentraland::common::Vector3;
use crate::decentraland::pulse::{
    client_message, server_message, ClientMessage, HandshakeResponse, PlayerInitialState,
    ServerMessage,
};
use crate::handshake::{verify_handshake, VerifiedHandshake};
use crate::hardening::{
    BanList, CorruptedPacketLimiter, DisconnectReason, HandshakeAttemptPolicy,
    HandshakeReplayPolicy, PreAuthAdmission, DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP,
    DEFAULT_MAX_EMOTE_DURATION_MS, DEFAULT_MAX_EMOTE_ID_LENGTH, DEFAULT_MAX_HANDSHAKE_ATTEMPTS,
    DEFAULT_PRE_AUTH_BUDGET,
};
use crate::interest::{
    ParcelEncoder, ParcelEncoderOptions, SpatialAreaOfInterest, SpatialAreaOfInterestOptions,
    SpatialGrid,
};
use crate::simulation::{
    OutgoingMessage, PacketMode, PeerConnectionState, PeerSimulation, PeerState,
};
use crate::snapshot::{
    EmoteInput, IdentityBoard, PeerSnapshotPublisher, ProfileBoard, SnapshotBoard,
};

pub mod channel {

    pub const RELIABLE: u8 = 0;

    pub const UNRELIABLE_SEQUENCED: u8 = 1;

    pub const UNRELIABLE_UNSEQUENCED: u8 = 2;
}

pub const DEFAULT_SIMULATION_STEPS: [u32; 3] = [50, 100, 200];

pub const DEFAULT_RING_CAPACITY: usize = 10;
const DEFAULT_MAX_PEERS: usize = 4096;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Reply(ServerMessage),

    Authenticated {
        wallet: String,
        duplicate_of: Option<u32>,
        initial_state: Option<Box<PlayerInitialState>>,
    },

    Applied,

    Reject {
        reply: Option<ServerMessage>,
        reason: DisconnectReason,
    },

    Ignore,
}

fn handshake_response(success: bool, error: Option<String>) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::Handshake(HandshakeResponse {
            success,
            error,
        })),
    }
}

mod validate {
    use super::Vector3;
    use crate::decentraland::pulse::PlayerState;
    use crate::interest::ParcelEncoder;

    fn is_finite_vec(v: &Vector3) -> bool {
        v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
    }

    pub fn vec_opt_finite(v: &Option<Vector3>) -> bool {
        v.as_ref().map(is_finite_vec).unwrap_or(false)
    }

    pub fn player_state(state: &PlayerState, encoder: &ParcelEncoder) -> bool {
        encoder.is_valid_index(state.parcel_index)
            && vec_opt_finite(&state.position)
            && vec_opt_finite(&state.velocity)
            && state.rotation_y.is_finite()
            && state.movement_blend.is_finite()
            && state.slide_blend.is_finite()
            && state.head_yaw.map(f32::is_finite).unwrap_or(true)
            && state.head_pitch.map(f32::is_finite).unwrap_or(true)
            && state.point_at.as_ref().map(is_finite_vec).unwrap_or(true)
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

    pub pre_auth: PreAuthAdmission,

    pub max_emote_id_length: usize,

    pub max_emote_duration_ms: u32,

    pub corrupted_limiter: CorruptedPacketLimiter,
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
            pre_auth: PreAuthAdmission::new(
                DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP,
                DEFAULT_PRE_AUTH_BUDGET,
            ),
            max_emote_id_length: DEFAULT_MAX_EMOTE_ID_LENGTH,
            max_emote_duration_ms: DEFAULT_MAX_EMOTE_DURATION_MS,
            corrupted_limiter: CorruptedPacketLimiter::new(
                crate::hardening::DEFAULT_CORRUPT_MAX_PER_MINUTE,
                crate::hardening::DEFAULT_CORRUPT_BURST,
            ),
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

        match inner {
            client_message::Message::Handshake(req) => {
                self.handle_handshake(peer, req, now_ms, now)
            }
            other => {
                if !self.is_authenticated(peer) {
                    return Action::Ignore;
                }
                self.apply_gameplay(peer, now, other)
            }
        }
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

        if let Some(state) = self.peers.get_mut(&peer) {
            match self
                .attempt_policy
                .try_record_attempt(state.handshake_attempts)
            {
                Some(next) => state.handshake_attempts = next,
                None => {
                    state.connection_state = PeerConnectionState::PendingDisconnect;
                    return Action::Reject {
                        reply: None,
                        reason: DisconnectReason::AuthFailed,
                    };
                }
            }
        }

        let verified = match verify_handshake(&req, now_ms) {
            Ok(v) => v,
            Err(e) => return Action::Reply(handshake_response(false, Some(e.message()))),
        };
        let VerifiedHandshake {
            user_address,
            timestamp,
        } = verified;

        if self.ban_list.is_banned(&user_address) {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: Some(handshake_response(false, Some("banned".into()))),
                reason: DisconnectReason::Banned,
            };
        }

        if !self.replay_policy.try_admit(now, &user_address, &timestamp) {
            if let Some(state) = self.peers.get_mut(&peer) {
                state.connection_state = PeerConnectionState::PendingDisconnect;
            }
            return Action::Reject {
                reply: None,
                reason: DisconnectReason::HandshakeReplayRejected,
            };
        }

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

        let duplicate_of = self
            .identity
            .peer_by_wallet(&user_address)
            .filter(|p| *p != peer);
        Action::Authenticated {
            wallet: user_address,
            duplicate_of,
            initial_state: req.initial_state.map(Box::new),
        }
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

    fn is_authenticated(&self, peer: u32) -> bool {
        self.peers
            .get(&peer)
            .map(|s| s.connection_state == PeerConnectionState::Authenticated)
            .unwrap_or(false)
    }

    fn apply_gameplay(&mut self, peer: u32, now: u32, msg: client_message::Message) -> Action {
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
                if t.realm.is_empty()
                    || !self.encoder.is_valid_index(t.parcel_index)
                    || !validate::vec_opt_finite(&t.position)
                {
                    return Action::Ignore;
                }
                let local = t.position.unwrap_or(Vector3::default());
                PeerSnapshotPublisher::publish_teleport(
                    &mut self.board,
                    &mut self.grid,
                    &self.encoder,
                    peer,
                    now,
                    t.parcel_index,
                    local,
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
            client_message::Message::Handshake(_) => Action::Ignore,
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
        host: &mut Host,
        peer: u32,
        action: Action,
        now: u32,
    ) -> anyhow::Result<()> {
        match action {
            Action::Reply(msg) => {
                self.send(host, peer, channel::RELIABLE, &msg).await?;
            }
            Action::Reject { reply, reason } => {
                if let Some(msg) = reply {
                    self.send(host, peer, channel::RELIABLE, &msg).await?;
                }
                self.pre_auth.release_on_disconnect(peer);
                host.disconnect(peer as u16, reason.code()).await?;
                tracing::info!(peer, ?reason, "peer rejected by hardening");
            }
            Action::Authenticated {
                wallet,
                duplicate_of,
                initial_state,
            } => {
                if let Some(dup) = duplicate_of {
                    self.pre_auth.release_on_disconnect(dup);
                    host.disconnect(dup as u16, DisconnectReason::DuplicateSession.code())
                        .await?;
                    self.begin_disconnect(host, dup).await?;
                }
                if let Some(s) = self.peers.get_mut(&peer) {
                    s.wallet_id = Some(wallet.clone());
                    s.connection_state = PeerConnectionState::Authenticated;
                }

                self.pre_auth.release_on_promotion(peer);
                self.identity.set(peer, wallet.clone());
                self.board.set_active(peer);

                if let Some(init) = initial_state {
                    self.seed_initial_state(peer, now, &init);
                }
                let ok = handshake_response(true, None);
                self.send(host, peer, channel::RELIABLE, &ok).await?;
                tracing::info!(peer, %wallet, "peer authenticated");
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

    async fn run_tick(&mut self, host: &mut Host, now: u32) -> anyhow::Result<()> {
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
        self.flush(host, outbox).await?;

        let expired = std::mem::take(&mut self.simulation.expired);
        for e in expired {
            tracing::info!(peer = e.peer, reason = ?e.reason, "reaping peer");
            if e.reason == crate::simulation::ExpiredReason::AuthTimeout {
                self.pre_auth.release_on_disconnect(e.peer);
                host.disconnect(e.peer as u16, DisconnectReason::AuthTimeout.code())
                    .await?;
            }
            self.cleanup_peer(host, e.peer).await?;
        }
        Ok(())
    }

    async fn flush(&mut self, host: &mut Host, outbox: Vec<OutgoingMessage>) -> anyhow::Result<()> {
        for out in outbox {
            let channel = match out.mode {
                PacketMode::Reliable => channel::RELIABLE,
                PacketMode::UnreliableSequenced => channel::UNRELIABLE_SEQUENCED,
                PacketMode::UnreliableUnsequenced => channel::UNRELIABLE_UNSEQUENCED,
            };
            self.send(host, out.target, channel, &out.message).await?;
        }
        Ok(())
    }

    pub async fn run(self, bind: SocketAddr) -> anyhow::Result<()> {
        self.run_with_tick(bind, 50).await
    }

    pub async fn run_with_tick(mut self, bind: SocketAddr, tick_ms: u64) -> anyhow::Result<()> {
        let mut host = Host::bind(HostConfig {
            bind,
            max_peers: 4096,
            channel_limit: 8,
        })
        .await?;
        tracing::info!(%bind, "catalyrst-pulse listening (enet)");
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
        let started = std::time::Instant::now();
        loop {
            tokio::select! {
                serviced = host.service() => {
                    match serviced? {
                        Some(Event::Connect { peer }) => {
                            let now = started.elapsed().as_millis() as u32;

                            let ip = host.peer_ip(peer).unwrap_or_default();
                            let admit = self.pre_auth.try_admit(peer as u32, &ip);
                            if let Some(reason) = crate::hardening::pre_auth_refusal_reason(admit) {
                                self.pre_auth.release_on_disconnect(peer as u32);
                                tracing::warn!(peer, %ip, ?reason, "pre-auth admission refused");
                                host.disconnect_now(peer, reason.code()).await?;
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
                            self.apply(&mut host, peer as u32, action, now).await?;
                        }
                        Some(Event::Disconnect { peer }) => {
                            self.on_disconnect(&mut host, peer as u32).await?;
                        }
                        None => {}
                    }
                }
                _ = ticker.tick() => {
                    let now = started.elapsed().as_millis() as u32;
                    self.run_tick(&mut host, now).await?;
                }
            }
        }
    }

    async fn begin_disconnect(&mut self, host: &mut Host, peer: u32) -> anyhow::Result<()> {
        self.cleanup_peer(host, peer).await
    }

    async fn on_disconnect(&mut self, host: &mut Host, peer: u32) -> anyhow::Result<()> {
        self.pre_auth.release_on_disconnect(peer);
        self.corrupted_limiter.release(peer);
        self.cleanup_peer(host, peer).await
    }

    async fn cleanup_peer(&mut self, host: &mut Host, peer: u32) -> anyhow::Result<()> {
        self.board.clear_active(peer);
        self.grid.remove(peer);
        self.identity.remove(peer);
        self.profiles.remove(peer);
        self.simulation.cleanup_observer_views(peer);
        self.peers.remove(&peer);

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
            self.send(host, target, channel::RELIABLE, &left).await?;
        }
        Ok(())
    }

    async fn send(
        &self,
        host: &mut Host,
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
        host.send(peer as u16, packet).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_packets_exhaust_budget_then_disconnect() {
        let mut srv = PulseServer::new();

        let bad: &[u8] = &[0x0A, 0xFF];

        for _ in 0..5 {
            assert_eq!(
                srv.dispatch(7, channel::RELIABLE, bad, 1000, 1000),
                Action::Ignore
            );
        }

        assert_eq!(
            srv.dispatch(7, channel::RELIABLE, bad, 1000, 1000),
            Action::Reject {
                reply: None,
                reason: DisconnectReason::PacketCorrupted
            }
        );
    }

    use crate::decentraland::pulse::{
        client_message, ClientMessage, PlayerInitialState, PlayerState, PlayerStateInput,
        ProfileVersionAnnouncement, ResyncRequest, TeleportRequest,
    };

    fn client_msg(inner: client_message::Message) -> Vec<u8> {
        ClientMessage {
            message: Some(inner),
        }
        .encode_to_vec()
    }

    fn valid_state(parcel: i32) -> PlayerState {
        PlayerState {
            parcel_index: parcel,
            position: Some(Vector3 {
                x: 8.0,
                y: 0.0,
                z: 8.0,
            }),
            velocity: Some(Vector3::default()),
            ..Default::default()
        }
    }

    fn authed(srv: &mut PulseServer, peer: u32, wallet: &str) {
        let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
        st.wallet_id = Some(wallet.into());
        srv.peers.insert(peer, st);
        srv.identity.set(peer, wallet.into());
        srv.board.set_active(peer);
    }

    #[test]
    fn gameplay_from_unauthenticated_peer_is_ignored() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(7, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(PlayerState::default()),
        }));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 0),
            Action::Ignore
        );
    }

    #[test]
    fn authenticated_input_publishes_snapshot_with_real_sequence() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        assert_eq!(srv.board.last_seq(7), crate::snapshot::NO_SEQ);
        let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(valid_state(5)),
        }));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 100),
            Action::Applied
        );

        assert_eq!(srv.board.last_seq(7), 0);
        let snap = srv.board.try_read(7).unwrap();
        assert_eq!(snap.seq, 0);
        assert_eq!(snap.server_tick, 100);
        assert_eq!(snap.parcel, 5);

        let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(valid_state(6)),
        }));
        srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 150);
        assert_eq!(srv.board.last_seq(7), 1);
    }

    #[test]
    fn teleport_seeds_realm_and_position() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 3, "0xabc");
        let bytes = client_msg(client_message::Message::Teleport(TeleportRequest {
            parcel_index: 0,
            position: Some(Vector3 {
                x: 8.0,
                y: 0.0,
                z: 8.0,
            }),
            realm: "realm-a".into(),
        }));
        assert_eq!(
            srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
            Action::Applied
        );
        let snap = srv.board.try_read(3).unwrap();
        assert_eq!(snap.realm.as_deref(), Some("realm-a"));
        assert!(snap.is_teleport);
        assert_eq!(snap.last_teleport_seq, snap.seq);
    }

    #[test]
    fn teleport_with_empty_realm_is_rejected() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 3, "0xabc");
        let bytes = client_msg(client_message::Message::Teleport(TeleportRequest {
            parcel_index: 0,
            position: Some(Vector3 {
                x: 8.0,
                y: 0.0,
                z: 8.0,
            }),
            realm: "".into(),
        }));
        assert_eq!(
            srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
            Action::Ignore
        );
        assert!(
            srv.board.try_read(3).is_none(),
            "rejected teleport publishes nothing"
        );
    }

    #[test]
    fn resync_request_is_recorded_on_peer_state() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 1, "0xobs");
        let bytes = client_msg(client_message::Message::Resync(ResyncRequest {
            subject_id: 9,
            known_seq: 42,
        }));
        assert_eq!(
            srv.dispatch(1, channel::RELIABLE, &bytes, 0, 0),
            Action::Applied
        );
        let reqs = srv.peers[&1].resync_requests.as_ref().unwrap();
        assert_eq!(reqs.get(&9), Some(&42));
    }

    #[test]
    fn profile_announcement_is_monotonic() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 2, "0xabc");
        let bytes = client_msg(client_message::Message::ProfileAnnouncement(
            ProfileVersionAnnouncement { version: 5 },
        ));
        srv.dispatch(2, channel::RELIABLE, &bytes, 0, 0);
        assert_eq!(srv.profiles.get(2), 5);

        let bytes = client_msg(client_message::Message::ProfileAnnouncement(
            ProfileVersionAnnouncement { version: 3 },
        ));
        srv.dispatch(2, channel::RELIABLE, &bytes, 0, 0);
        assert_eq!(srv.profiles.get(2), 5);
    }

    #[test]
    fn malformed_packet_is_ignored() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 1, "0xabc");
        assert_eq!(
            srv.dispatch(1, 0, &[0xFF, 0xFF, 0xFF], 0, 0),
            Action::Ignore
        );
    }

    #[test]
    fn bad_handshake_replies_with_failure() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let bytes = client_msg(client_message::Message::Handshake(
            crate::decentraland::pulse::HandshakeRequest {
                auth_chain: b"not json".to_vec(),
                profile_version: 0,
                initial_state: None,
            },
        ));
        match srv.dispatch(1, channel::RELIABLE, &bytes, 1000, 0) {
            Action::Reply(ServerMessage {
                message: Some(server_message::Message::Handshake(h)),
            }) => {
                assert!(!h.success);
                assert!(h.error.is_some());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    async fn signed_handshake_request() -> (Vec<u8>, String, i64) {
        use crate::handshake::build_signed_fetch_payload;
        use catalyrst_types::{AuthLink, AuthLinkType};
        use ethers_signers::{LocalWallet, Signer};

        let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
        let root_addr = format!("{:#x}", root.address());
        let ephemeral: LocalWallet =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let eph_addr = format!("{:#x}", ephemeral.address());

        let ts = "1700000000000";
        let now_ms: i64 = ts.parse().unwrap();
        let metadata = "{\"signer\":\"dcl:explorer\"}";
        let connect_payload = build_signed_fetch_payload("connect", "/", ts, metadata);

        let eph_payload = format!(
            "Decentraland Login\nEphemeral address: {eph_addr}\nExpiration: 2099-01-01T00:00:00.000Z"
        );
        let eph_sig = format!(
            "0x{}",
            root.sign_message(eph_payload.as_bytes()).await.unwrap()
        );
        let final_sig = format!(
            "0x{}",
            ephemeral
                .sign_message(connect_payload.as_bytes())
                .await
                .unwrap()
        );

        let chain = [
            AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: root_addr.clone(),
                signature: None,
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaEphemeral,
                payload: eph_payload,
                signature: Some(eph_sig),
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaSignedEntity,
                payload: connect_payload,
                signature: Some(final_sig),
            },
        ];

        let mut map = serde_json::Map::new();
        for (i, link) in chain.iter().enumerate() {
            map.insert(
                format!("x-identity-auth-chain-{i}"),
                serde_json::Value::String(serde_json::to_string(link).unwrap()),
            );
        }
        map.insert(
            "x-identity-timestamp".into(),
            serde_json::Value::String(ts.into()),
        );
        map.insert(
            "x-identity-metadata".into(),
            serde_json::Value::String(metadata.into()),
        );
        let bag = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();

        let bytes = client_msg(client_message::Message::Handshake(
            crate::decentraland::pulse::HandshakeRequest {
                auth_chain: bag.into_bytes(),
                profile_version: 0,
                initial_state: None,
            },
        ));
        (bytes, root_addr.to_lowercase(), now_ms)
    }

    fn with_initial_state(bytes: &[u8], init: Option<PlayerInitialState>) -> Vec<u8> {
        let mut msg = ClientMessage::decode(bytes).unwrap();
        if let Some(client_message::Message::Handshake(h)) = msg.message.as_mut() {
            h.initial_state = init;
        }
        msg.encode_to_vec()
    }

    #[tokio::test]
    async fn handshake_seeds_valid_initial_state() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, wallet, now_ms) = signed_handshake_request().await;

        let init = PlayerInitialState {
            state: Some(valid_state(7)),
            realm: "realm-a".into(),
            ..Default::default()
        };
        let bytes = with_initial_state(&base, Some(init));

        let action = srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0);
        match &action {
            Action::Authenticated {
                wallet: w,
                initial_state,
                ..
            } => {
                assert_eq!(*w, wallet);
                assert!(
                    initial_state.is_some(),
                    "validated initial state carried on the action"
                );
            }
            other => panic!("expected Authenticated, got {other:?}"),
        }

        srv.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::Authenticated;
        srv.identity.set(1, wallet.clone());
        srv.board.set_active(1);
        if let Action::Authenticated {
            initial_state: Some(init),
            ..
        } = action
        {
            srv.seed_initial_state(1, 500, &init);
        }
        let snap = srv.board.try_read(1).expect("seeded snapshot present");
        assert_eq!(snap.parcel, 7);
        assert_eq!(snap.server_tick, 500);
    }

    #[tokio::test]
    async fn handshake_with_malformed_initial_state_is_rejected() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, _wallet, now_ms) = signed_handshake_request().await;

        let mut bad = valid_state(7);
        bad.position = Some(Vector3 {
            x: f32::NAN,
            y: 0.0,
            z: 0.0,
        });
        let init = PlayerInitialState {
            state: Some(bad),
            realm: "realm-a".into(),
            ..Default::default()
        };
        let bytes = with_initial_state(&base, Some(init));

        match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
            Action::Reject { reply, reason } => {
                assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
                match reply.and_then(|m| m.message) {
                    Some(server_message::Message::Handshake(h)) => {
                        assert!(!h.success, "malformed initial state must not authenticate");
                        assert!(h.error.is_some());
                    }
                    other => panic!("expected handshake failure reply, got {other:?}"),
                }
            }
            other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
        }
        assert!(!srv.is_authenticated(1));
    }

    #[test]
    fn input_with_nan_float_is_rejected() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        let mut state = valid_state(3);
        state.rotation_y = f32::INFINITY;
        let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(state),
        }));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 100),
            Action::Ignore
        );
        assert!(
            srv.board.try_read(7).is_none(),
            "rejected input publishes nothing"
        );
    }

    #[test]
    fn input_without_position_is_rejected() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");

        let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(PlayerState {
                parcel_index: 3,
                ..Default::default()
            }),
        }));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 100),
            Action::Ignore
        );
    }

    #[tokio::test]
    async fn valid_handshake_authenticates_and_binds_wallet() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (bytes, wallet, now_ms) = signed_handshake_request().await;

        match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
            Action::Authenticated {
                wallet: w,
                duplicate_of,
                initial_state,
            } => {
                assert_eq!(w, wallet);
                assert_eq!(duplicate_of, None);
                assert!(
                    initial_state.is_none(),
                    "no initial state in this handshake"
                );
            }
            other => panic!("expected Authenticated, got {other:?}"),
        }

        srv.peers.get_mut(&1).unwrap().wallet_id = Some(wallet.clone());
        srv.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::Authenticated;
        srv.identity.set(1, wallet.clone());
        srv.board.set_active(1);
        assert!(srv.is_authenticated(1));
        assert_eq!(srv.identity.peer_by_wallet(&wallet), Some(1));
    }

    #[tokio::test]
    async fn tampered_handshake_replies_with_failure() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));

        let (bytes, _wallet, now_ms) = signed_handshake_request().await;
        let mut req = ClientMessage::decode(&bytes[..]).unwrap();
        if let Some(client_message::Message::Handshake(h)) = req.message.as_mut() {
            let mut bag: serde_json::Value = serde_json::from_slice(&h.auth_chain).unwrap();
            let link_json = bag["x-identity-auth-chain-2"].as_str().unwrap();
            let mut link: serde_json::Value = serde_json::from_str(link_json).unwrap();
            link["payload"] = serde_json::Value::String("connect:/:1700000000000:tampered".into());
            bag["x-identity-auth-chain-2"] =
                serde_json::Value::String(serde_json::to_string(&link).unwrap());
            h.auth_chain = serde_json::to_vec(&bag).unwrap();
        }
        let tampered = req.encode_to_vec();

        match srv.dispatch(2, channel::RELIABLE, &tampered, now_ms, 0) {
            Action::Reply(ServerMessage {
                message: Some(server_message::Message::Handshake(h)),
            }) => {
                assert!(!h.success, "tampered chain must not succeed");
                assert!(h.error.is_some());
            }
            other => panic!("expected failure Reply, got {other:?}"),
        }
        assert!(!srv.is_authenticated(2));
    }

    #[tokio::test]
    async fn banned_wallet_is_rejected_at_handshake() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (bytes, wallet, now_ms) = signed_handshake_request().await;

        srv.ban_list.replace([wallet.clone()]);

        match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
            Action::Reject { reply, reason } => {
                assert_eq!(reason, DisconnectReason::Banned);

                match reply.and_then(|m| m.message) {
                    Some(server_message::Message::Handshake(h)) => {
                        assert!(!h.success);
                        assert_eq!(h.error.as_deref(), Some("banned"));
                    }
                    other => panic!("expected banned handshake reply, got {other:?}"),
                }
            }
            other => panic!("expected Reject(Banned), got {other:?}"),
        }
        assert!(!srv.is_authenticated(1));

        assert_eq!(
            srv.peers[&1].connection_state,
            PeerConnectionState::PendingDisconnect
        );
    }

    #[tokio::test]
    async fn replayed_handshake_pair_is_rejected() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        srv.peers
            .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (bytes, wallet, now_ms) = signed_handshake_request().await;

        match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 100) {
            Action::Authenticated { wallet: w, .. } => assert_eq!(w, wallet),
            other => panic!("first handshake should authenticate, got {other:?}"),
        }

        match srv.dispatch(2, channel::RELIABLE, &bytes, now_ms, 200) {
            Action::Reject { reply, reason } => {
                assert_eq!(reason, DisconnectReason::HandshakeReplayRejected);
                assert!(
                    reply.is_none(),
                    "replay rejection has no reply body (PeerDefense)"
                );
            }
            other => panic!("expected Reject(HandshakeReplayRejected), got {other:?}"),
        }
        assert!(!srv.is_authenticated(2));
    }

    #[tokio::test]
    async fn handshake_attempts_are_throttled() {
        let mut srv = PulseServer::new();

        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let bad = client_msg(client_message::Message::Handshake(
            crate::decentraland::pulse::HandshakeRequest {
                auth_chain: b"not json".to_vec(),
                profile_version: 0,
                initial_state: None,
            },
        ));

        for _ in 0..2 {
            assert!(matches!(
                srv.dispatch(1, channel::RELIABLE, &bad, 1000, 0),
                Action::Reply(_)
            ));
        }
        assert_eq!(srv.peers[&1].handshake_attempts, 2);

        match srv.dispatch(1, channel::RELIABLE, &bad, 1000, 0) {
            Action::Reject { reply, reason } => {
                assert_eq!(reason, DisconnectReason::AuthFailed);
                assert!(reply.is_none());
            }
            other => panic!("expected Reject(AuthFailed) on throttle, got {other:?}"),
        }
        assert_eq!(
            srv.peers[&1].connection_state,
            PeerConnectionState::PendingDisconnect
        );
    }

    #[test]
    fn oversized_emote_id_is_rejected() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        let huge = "u".repeat(srv.max_emote_id_length + 1);
        let bytes = client_msg(client_message::Message::EmoteStart(
            crate::decentraland::pulse::EmoteStart {
                emote_id: huge,
                duration_ms: None,
                player_state: Some(valid_state(3)),
                mask: None,
            },
        ));
        match srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 0) {
            Action::Reject { reason, .. } => {
                assert_eq!(reason, DisconnectReason::InvalidEmoteField)
            }
            other => panic!("expected Reject(InvalidEmoteField), got {other:?}"),
        }
        assert!(
            srv.board.try_read(7).is_none(),
            "rejected emote publishes nothing"
        );
    }

    #[test]
    fn excessive_emote_duration_is_rejected() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        let bytes = client_msg(client_message::Message::EmoteStart(
            crate::decentraland::pulse::EmoteStart {
                emote_id: "wave".into(),
                duration_ms: Some(srv.max_emote_duration_ms + 1),
                player_state: Some(valid_state(3)),
                mask: None,
            },
        ));
        match srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 0) {
            Action::Reject { reason, .. } => {
                assert_eq!(reason, DisconnectReason::InvalidEmoteField)
            }
            other => panic!("expected Reject(InvalidEmoteField), got {other:?}"),
        }
    }

    #[test]
    fn emote_within_caps_is_applied() {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        let bytes = client_msg(client_message::Message::EmoteStart(
            crate::decentraland::pulse::EmoteStart {
                emote_id: "urn:decentraland:off-chain:base-emotes:wave".into(),
                duration_ms: Some(2000),
                player_state: Some(valid_state(3)),
                mask: None,
            },
        ));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 50),
            Action::Applied
        );
        assert!(srv.board.try_read(7).unwrap().is_emoting());
    }

    #[tokio::test]
    async fn handshake_initial_state_emote_cap_is_enforced() {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, _wallet, now_ms) = signed_handshake_request().await;

        let init = PlayerInitialState {
            state: Some(valid_state(7)),
            emote_id: Some("u".repeat(srv.max_emote_id_length + 1)),
            realm: "realm-a".into(),
            ..Default::default()
        };
        let bytes = with_initial_state(&base, Some(init));

        match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
            Action::Reject { reason, .. } => {
                assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
            }
            other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
        }
        assert!(!srv.is_authenticated(1));
    }
}
