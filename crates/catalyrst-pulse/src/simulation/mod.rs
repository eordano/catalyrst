use std::collections::HashMap;

use crate::batch::{BatchSubject, SeqEncoding};
use crate::decentraland::pulse::{
    server_message, EmoteStarted, EmoteStopReason, EmoteStopped, PlayerJoined, PlayerLeft,
    PlayerProfileVersionsAnnounced, PlayerState, PlayerStateDeltaBatch, PlayerStateDeltaTier0,
    PlayerStateFull, ServerMessage, TeleportPerformed,
};
use crate::interest::{
    InterestCollector, InterestEntry, PeerViewSimulationTier, SceneListenerState,
    SpatialAreaOfInterest,
};
use crate::snapshot::{EmoteState, IdentityBoard, PeerSnapshot, ProfileBoard, SnapshotBoard};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketMode {
    Reliable,
    UnreliableSequenced,
    UnreliableUnsequenced,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingMessage {
    pub target: u32,
    pub message: ServerMessage,
    pub mode: PacketMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerConnectionState {
    None,
    PendingAuth,
    Authenticated,
    PendingDisconnect,
    Disconnecting,
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub wallet_id: Option<String>,
    pub connection_state: PeerConnectionState,
    pub connection_time: u32,
    pub disconnection_time: u32,

    pub handshake_attempts: u8,

    pub ip: Option<String>,

    pub features: u32,

    pub scene_listener: Option<SceneListenerState>,

    pub resync_requests: Option<HashMap<u32, u32>>,
}

impl PeerState {
    pub fn new(connection_state: PeerConnectionState, now: u32) -> Self {
        Self {
            wallet_id: None,
            connection_state,
            connection_time: now,
            disconnection_time: 0,
            handshake_attempts: 0,
            ip: None,
            features: 0,
            scene_listener: None,
            resync_requests: None,
        }
    }

    pub fn is_listener(&self) -> bool {
        self.scene_listener.is_some()
    }

    pub fn request_resync(&mut self, subject: u32, known_seq: u32) {
        self.resync_requests
            .get_or_insert_with(HashMap::new)
            .insert(subject, known_seq);
    }
}

#[derive(Debug, Clone)]
pub struct PeerToPeerView {
    pub onto: u32,
    pub last_sent_snapshot: PeerSnapshot,
    pub last_seen_tick: u32,
    pub last_sent_profile_version: i32,
    pub last_sent_emote: Option<EmoteState>,
    pub last_sent_teleport_seq: Option<u32>,
    pub last_sent_seq: u32,
    pub last_sent_wallet_id: Option<String>,
}

pub fn create_delta_message(
    subject_id: u32,
    from: &PeerSnapshot,
    to: &PeerSnapshot,
    tier: PeerViewSimulationTier,
) -> PlayerStateDeltaTier0 {
    let mut delta = PlayerStateDeltaTier0 {
        subject_id,
        baseline_seq: from.seq,
        new_seq: to.seq,
        server_tick: to.server_tick,
        ..Default::default()
    };

    if tier == PeerViewSimulationTier::TIER_0 {
        if from.slide_blend != to.slide_blend {
            delta.slide_blend = Some(to.slide_blend);
        }
        if from.head_yaw != to.head_yaw {
            if let Some(v) = to.head_yaw {
                delta.head_yaw = Some(v);
            }
        }
        if from.head_pitch != to.head_pitch {
            if let Some(v) = to.head_pitch {
                delta.head_pitch = Some(v);
            }
        }
    }

    if from.animation_flags != to.animation_flags {
        delta.state_flags = Some(to.animation_flags as u32);
    }

    if from.glide_state != to.glide_state {
        delta.glide_state = Some(to.glide_state);
    }

    if from.parcel != to.parcel {
        delta.parcel_index = Some(to.parcel);
    }

    if from.position_x != to.position_x {
        delta.position_x = Some(to.position_x);
    }
    if from.position_y != to.position_y {
        delta.position_y = Some(to.position_y);
    }
    if from.position_z != to.position_z {
        delta.position_z = Some(to.position_z);
    }

    if from.rotation_y != to.rotation_y {
        delta.rotation_y = Some(to.rotation_y);
    }

    if from.jump_count != to.jump_count {
        delta.jump_count = Some(to.jump_count);
    }

    if let Some(v) = to.point_at_x {
        if from.point_at_x != Some(v) {
            delta.point_at_x = Some(v);
        }
    }
    if let Some(v) = to.point_at_y {
        if from.point_at_y != Some(v) {
            delta.point_at_y = Some(v);
        }
    }
    if let Some(v) = to.point_at_z {
        if from.point_at_z != Some(v) {
            delta.point_at_z = Some(v);
        }
    }

    if tier == PeerViewSimulationTier::TIER_0 || tier == PeerViewSimulationTier::TIER_1 {
        if from.velocity_x != to.velocity_x {
            delta.velocity_x = Some(to.velocity_x);
        }
        if from.velocity_y != to.velocity_y {
            delta.velocity_y = Some(to.velocity_y);
        }
        if from.velocity_z != to.velocity_z {
            delta.velocity_z = Some(to.velocity_z);
        }
        if from.movement_blend != to.movement_blend {
            delta.movement_blend = Some(to.movement_blend);
        }
    }

    delta
}

fn create_player_state(snapshot: &PeerSnapshot) -> PlayerState {
    PlayerState {
        parcel_index: snapshot.parcel,
        position_x: snapshot.position_x,
        position_y: snapshot.position_y,
        position_z: snapshot.position_z,
        velocity_x: snapshot.velocity_x,
        velocity_y: snapshot.velocity_y,
        velocity_z: snapshot.velocity_z,
        rotation_y: snapshot.rotation_y,
        movement_blend: snapshot.movement_blend,
        slide_blend: snapshot.slide_blend,
        state_flags: snapshot.animation_flags as u32,
        glide_state: snapshot.glide_state,
        jump_count: 0,
        head_yaw: snapshot.head_yaw,
        head_pitch: snapshot.head_pitch,
        point_at_x: snapshot.point_at_x,
        point_at_y: snapshot.point_at_y,
        point_at_z: snapshot.point_at_z,
    }
}

fn create_full_state(subject_id: u32, snapshot: &PeerSnapshot) -> PlayerStateFull {
    PlayerStateFull {
        subject_id,
        sequence: snapshot.seq,
        server_tick: snapshot.server_tick,
        state: Some(create_player_state(snapshot)),
    }
}

pub const SELF_MIRROR_WALLET_ID: &str = "self_mirror";
const SWEEP_INTERVAL: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiredPeer {
    pub peer: u32,
    pub reason: ExpiredReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiredReason {
    AuthTimeout,

    DisconnectCleanTimeout,
}

pub struct PeerSimulation {
    observer_views: HashMap<u32, HashMap<u32, PeerToPeerView>>,
    tier_divisors: Vec<u32>,
    self_mirror_enabled: bool,
    self_mirror_tier: PeerViewSimulationTier,
    resync_with_delta: bool,
    seq_encoding: SeqEncoding,
    pub base_tick_ms: u32,

    pending_auth_clean_timeout_ms: u32,

    disconnection_clean_timeout_ms: u32,

    pub outbox: Vec<OutgoingMessage>,

    pub expired: Vec<ExpiredPeer>,
    collector: InterestCollector,

    observer_features: u32,
    delta_batch_buffer: Vec<BatchSubject>,
}

pub const DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS: u32 = 30_000;
pub const DEFAULT_DISCONNECTION_CLEAN_TIMEOUT_MS: u32 = 5_000;

impl PeerSimulation {
    pub fn new(simulation_steps: &[u32], resync_with_delta: bool) -> Self {
        let base_tick_ms = simulation_steps[0];
        let tier_divisors = simulation_steps.iter().map(|s| s / base_tick_ms).collect();
        Self {
            observer_views: HashMap::new(),
            tier_divisors,
            self_mirror_enabled: false,
            self_mirror_tier: PeerViewSimulationTier::TIER_0,
            resync_with_delta,
            seq_encoding: SeqEncoding::Absolute,
            base_tick_ms,
            pending_auth_clean_timeout_ms: DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS,
            disconnection_clean_timeout_ms: DEFAULT_DISCONNECTION_CLEAN_TIMEOUT_MS,
            outbox: Vec::new(),
            expired: Vec::new(),
            collector: InterestCollector::default(),
            observer_features: 0,
            delta_batch_buffer: Vec::new(),
        }
    }

    pub fn set_seq_encoding(&mut self, mode: SeqEncoding) {
        self.seq_encoding = mode;
    }

    fn send(&mut self, target: u32, message: ServerMessage, mode: PacketMode) {
        self.outbox.push(OutgoingMessage {
            target,
            message,
            mode,
        });
    }

    pub fn remove_observer(&mut self, observer_id: u32) {
        self.observer_views.remove(&observer_id);
    }

    pub fn simulate_tick(
        &mut self,
        peers: &mut HashMap<u32, PeerState>,
        board: &SnapshotBoard,
        aoi: &SpatialAreaOfInterest,
        identity: &IdentityBoard,
        profiles: &ProfileBoard,
        tick_counter: u32,
        now_ms: u32,
    ) {
        self.expired.clear();

        let observer_ids: Vec<u32> = peers.keys().copied().collect();

        for observer_id in observer_ids {
            let peer = &peers[&observer_id];
            let connection_state = peer.connection_state;

            if connection_state == PeerConnectionState::PendingAuth {
                if now_ms.wrapping_sub(peer.connection_time) >= self.pending_auth_clean_timeout_ms {
                    self.expired.push(ExpiredPeer {
                        peer: observer_id,
                        reason: ExpiredReason::AuthTimeout,
                    });
                }
                continue;
            }

            if connection_state == PeerConnectionState::Disconnecting {
                if now_ms.wrapping_sub(peer.disconnection_time)
                    >= self.disconnection_clean_timeout_ms
                {
                    self.expired.push(ExpiredPeer {
                        peer: observer_id,
                        reason: ExpiredReason::DisconnectCleanTimeout,
                    });
                }
                continue;
            }

            if connection_state != PeerConnectionState::Authenticated {
                continue;
            }

            let listener = peers
                .get(&observer_id)
                .and_then(|s| s.scene_listener.clone());
            let observer_snapshot = if listener.is_some() {
                None
            } else {
                match board.try_read(observer_id).cloned() {
                    Some(s) => Some(s),
                    None => continue,
                }
            };

            self.observer_features = peers.get(&observer_id).map(|s| s.features).unwrap_or(0);
            self.delta_batch_buffer.clear();

            let mut resync = peers
                .get_mut(&observer_id)
                .and_then(|s| s.resync_requests.take());

            self.observer_views.entry(observer_id).or_default();

            self.collector.clear();

            let mut collector = std::mem::take(&mut self.collector);
            let positional_only = if let Some(listener) = &listener {
                collect_scene_listener_subjects(board, listener, observer_id, &mut collector);
                true
            } else {
                let snap = observer_snapshot.as_ref().unwrap();
                aoi.get_visible_subjects(
                    board,
                    observer_id,
                    snap.realm.as_deref(),
                    snap.global_position,
                    &mut collector,
                );
                if self.self_mirror_enabled {
                    collector.add(observer_id, self.self_mirror_tier);
                }
                false
            };

            self.process_visible_subjects(
                observer_id,
                board,
                identity,
                profiles,
                &collector,
                resync.as_mut(),
                tick_counter,
                positional_only,
            );

            if positional_only {
                crate::metrics::scene_listener_visible_subjects(collector.count());
            }
            self.collector = collector;

            self.flush_delta_batch(observer_id, now_ms);

            if let Some(state) = peers.get_mut(&observer_id) {
                state.resync_requests = None;
            }
            let _ = &mut resync;

            if tick_counter.is_multiple_of(SWEEP_INTERVAL) {
                self.sweep_stale_views(observer_id, tick_counter);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_visible_subjects(
        &mut self,
        observer_id: u32,
        board: &SnapshotBoard,
        identity: &IdentityBoard,
        profiles: &ProfileBoard,
        collector: &InterestCollector,
        mut resync: Option<&mut HashMap<u32, u32>>,
        tick_counter: u32,
        positional_only: bool,
    ) {
        for &entry in &collector.entries {
            let is_self_mirror = entry.subject == observer_id;
            if is_self_mirror && !self.self_mirror_enabled {
                continue;
            }

            let has_view = self
                .observer_views
                .get(&observer_id)
                .map(|m| m.contains_key(&entry.subject))
                .unwrap_or(false);
            let mut is_new = !has_view;

            if !is_new
                && self.detect_and_handle_aliasing(
                    observer_id,
                    entry.subject,
                    is_self_mirror,
                    identity,
                )
            {
                is_new = true;
            }

            if !is_new {
                if let Some(v) = self
                    .observer_views
                    .get_mut(&observer_id)
                    .and_then(|m| m.get_mut(&entry.subject))
                {
                    v.last_seen_tick = tick_counter;
                }
            }

            let has_resync = !is_new
                && resync
                    .as_deref()
                    .map(|r| r.contains_key(&entry.subject))
                    .unwrap_or(false);
            let tier_index = entry.tier.value() as usize;
            if !has_resync
                && tier_index < self.tier_divisors.len()
                && !tick_counter.is_multiple_of(self.tier_divisors[tier_index])
            {
                continue;
            }

            let Some(latest) = board.try_read(entry.subject).cloned() else {
                continue;
            };

            if is_new {
                let view = self.handle_new_subject(
                    observer_id,
                    entry.subject,
                    &latest,
                    is_self_mirror,
                    identity,
                    profiles,
                    resync.as_deref_mut(),
                    positional_only,
                );
                let mut view = view;
                view.last_seen_tick = tick_counter;
                self.observer_views
                    .entry(observer_id)
                    .or_default()
                    .insert(entry.subject, view);
                continue;
            }

            if !positional_only {
                self.try_announce_profile(observer_id, entry.subject, profiles);
            }

            let mut view = self.observer_views[&observer_id][&entry.subject].clone();
            let last_sent_state = self.process_existing_subject(
                observer_id,
                entry,
                &mut view,
                board,
                &latest,
                resync.as_deref_mut(),
                positional_only,
            );
            view.last_sent_snapshot = last_sent_state;
            view.last_seen_tick = tick_counter;
            self.observer_views
                .entry(observer_id)
                .or_default()
                .insert(entry.subject, view);
        }
    }

    fn detect_and_handle_aliasing(
        &mut self,
        observer_id: u32,
        subject_id: u32,
        is_self_mirror: bool,
        identity: &IdentityBoard,
    ) -> bool {
        let current_wallet: Option<&str> = if is_self_mirror {
            Some(SELF_MIRROR_WALLET_ID)
        } else {
            identity.wallet_by_peer(subject_id)
        };

        let last_sent = self
            .observer_views
            .get(&observer_id)
            .and_then(|m| m.get(&subject_id))
            .and_then(|v| v.last_sent_wallet_id.as_deref());

        let same = match (last_sent, current_wallet) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return false;
        }

        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::PlayerLeft(PlayerLeft {
                    subject_id,
                })),
            },
            PacketMode::Reliable,
        );
        if let Some(m) = self.observer_views.get_mut(&observer_id) {
            m.remove(&subject_id);
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_new_subject(
        &mut self,
        observer_id: u32,
        subject_id: u32,
        latest: &PeerSnapshot,
        is_self_mirror: bool,
        identity: &IdentityBoard,
        profiles: &ProfileBoard,
        resync: Option<&mut HashMap<u32, u32>>,
        positional_only: bool,
    ) -> PeerToPeerView {
        if let Some(r) = resync {
            r.remove(&subject_id);
        }

        let profile_version = profiles.get(subject_id);
        let user_id = if is_self_mirror {
            SELF_MIRROR_WALLET_ID.to_string()
        } else {
            identity
                .wallet_by_peer(subject_id)
                .unwrap_or("")
                .to_string()
        };

        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::PlayerJoined(PlayerJoined {
                    user_id: user_id.clone(),
                    profile_version,
                    state: Some(create_full_state(subject_id, latest)),
                    realm: latest.realm.as_deref().unwrap_or_default().to_string(),
                })),
            },
            PacketMode::Reliable,
        );

        let mut view = PeerToPeerView {
            onto: subject_id,
            last_sent_profile_version: profile_version,
            last_sent_teleport_seq: Some(latest.seq),
            last_sent_snapshot: latest.clone(),
            last_sent_wallet_id: Some(user_id),
            last_sent_emote: None,
            last_sent_seq: latest.seq,
            last_seen_tick: 0,
        };

        match latest.emote.clone().filter(|e| e.emote_id.is_some()) {
            Some(active) if !positional_only => {
                self.send_emote_started(observer_id, &mut view, subject_id, latest, &active);
                view.last_sent_emote = Some(active);
            }
            _ => {
                view.last_sent_seq = latest.seq;
            }
        }

        view
    }

    fn process_existing_subject(
        &mut self,
        observer_id: u32,
        entry: InterestEntry,
        view: &mut PeerToPeerView,
        board: &SnapshotBoard,
        latest: &PeerSnapshot,
        mut resync: Option<&mut HashMap<u32, u32>>,
        positional_only: bool,
    ) -> PeerSnapshot {
        let mut last_sent_state = view.last_sent_snapshot.clone();
        let mut discrete_event_sent = false;

        let scan =
            scan_intermediate_events(board, entry.subject, view.last_sent_snapshot.seq, latest);

        if let Some(tp) = &scan.last_teleport {
            if view.last_sent_teleport_seq.unwrap_or(0) < tp.last_teleport_seq {
                self.send_teleport(observer_id, view, entry.subject, tp);
                if let Some(r) = resync.as_deref_mut() {
                    r.remove(&entry.subject);
                }
                view.last_sent_teleport_seq = Some(tp.last_teleport_seq);
                last_sent_state = tp.clone();
                discrete_event_sent = true;
            }
        }

        let emote_start_is_effective = match (&scan.last_emote_start, &scan.last_emote_stop) {
            (Some(start), stop) => start.seq > stop.as_ref().map(|s| s.seq).unwrap_or(0),
            _ => false,
        };

        if emote_start_is_effective && !positional_only {
            let es = scan.last_emote_start.as_ref().unwrap();
            if let Some(emote) = es.emote.clone().filter(|e| e.emote_id.is_some()) {
                let dup = view
                    .last_sent_emote
                    .as_ref()
                    .map(|l| l.emote_id == emote.emote_id && l.start_seq == emote.start_seq)
                    .unwrap_or(false);
                if !dup {
                    self.send_emote_started(observer_id, view, entry.subject, es, &emote);
                    if let Some(r) = resync.as_deref_mut() {
                        r.remove(&entry.subject);
                    }
                    view.last_sent_emote = Some(emote);
                    if es.seq > last_sent_state.seq {
                        last_sent_state = es.clone();
                    }
                    discrete_event_sent = true;
                }
            }
        }

        if !emote_start_is_effective {
            self.try_sync_emote_stop(
                observer_id,
                entry.subject,
                view,
                &mut last_sent_state,
                scan.last_emote_stop.as_ref(),
            );
        }

        if !discrete_event_sent {
            last_sent_state = self.handle_resync_or_delta(
                observer_id,
                entry,
                view,
                last_sent_state,
                board,
                latest,
                resync,
            );
        }

        last_sent_state
    }

    fn try_sync_emote_stop(
        &mut self,
        observer_id: u32,
        subject_id: u32,
        view: &mut PeerToPeerView,
        last_sent_state: &mut PeerSnapshot,
        stop_snapshot: Option<&PeerSnapshot>,
    ) {
        if view
            .last_sent_emote
            .as_ref()
            .map(|e| e.emote_id.is_none())
            .unwrap_or(true)
        {
            return;
        }
        if let Some(stop) = stop_snapshot {
            if let Some(reason) = stop.emote.as_ref().and_then(|e| e.stop_reason) {
                self.send_emote_stopped(observer_id, view, subject_id, stop, reason);
                view.last_sent_emote = None;
                if stop.seq > last_sent_state.seq {
                    *last_sent_state = stop.clone();
                }
            }
        }
    }

    fn handle_resync_or_delta(
        &mut self,
        observer_id: u32,
        entry: InterestEntry,
        view: &mut PeerToPeerView,
        last_sent_state: PeerSnapshot,
        board: &SnapshotBoard,
        latest: &PeerSnapshot,
        resync: Option<&mut HashMap<u32, u32>>,
    ) -> PeerSnapshot {
        let last_known = resync.and_then(|r| r.remove(&entry.subject));

        let Some(last_known_seq) = last_known else {
            self.send_delta(
                observer_id,
                view,
                entry.subject,
                &last_sent_state,
                latest,
                entry.tier,
                PacketMode::UnreliableSequenced,
            );
            return latest.clone();
        };

        let known = if self.resync_with_delta {
            board.try_read_seq(entry.subject, last_known_seq).cloned()
        } else {
            None
        };

        match known {
            Some(known_snapshot) if known_snapshot.seq != latest.seq => {
                self.send_delta(
                    observer_id,
                    view,
                    entry.subject,
                    &known_snapshot,
                    latest,
                    entry.tier,
                    PacketMode::Reliable,
                );
            }
            _ => {
                view.last_sent_seq = latest.seq;
                self.send(
                    observer_id,
                    ServerMessage {
                        message: Some(server_message::Message::PlayerStateFull(create_full_state(
                            entry.subject,
                            latest,
                        ))),
                    },
                    PacketMode::Reliable,
                );
            }
        }

        latest.clone()
    }

    fn send_teleport(
        &mut self,
        observer_id: u32,
        view: &mut PeerToPeerView,
        subject_id: u32,
        snapshot: &PeerSnapshot,
    ) {
        view.last_sent_seq = snapshot.seq;
        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::Teleported(TeleportPerformed {
                    subject_id,
                    sequence: snapshot.seq,
                    server_tick: snapshot.server_tick,
                    state: Some(create_player_state(snapshot)),
                    realm: snapshot.realm.as_deref().unwrap_or_default().to_string(),
                })),
            },
            PacketMode::Reliable,
        );
    }

    fn send_emote_started(
        &mut self,
        observer_id: u32,
        view: &mut PeerToPeerView,
        subject_id: u32,
        snapshot: &PeerSnapshot,
        emote: &EmoteState,
    ) {
        view.last_sent_seq = snapshot.seq;
        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::EmoteStarted(EmoteStarted {
                    subject_id,
                    sequence: snapshot.seq,
                    server_tick: emote.start_tick,
                    emote_id: emote.emote_id.as_deref().unwrap_or_default().to_string(),
                    player_state: Some(create_player_state(snapshot)),
                    mask: None,
                })),
            },
            PacketMode::Reliable,
        );
    }

    fn send_emote_stopped(
        &mut self,
        observer_id: u32,
        view: &mut PeerToPeerView,
        subject_id: u32,
        snapshot: &PeerSnapshot,
        reason: EmoteStopReason,
    ) {
        view.last_sent_seq = snapshot.seq;
        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::EmoteStopped(EmoteStopped {
                    subject_id,
                    server_tick: snapshot.server_tick,
                    reason: reason as i32,
                    sequence: snapshot.seq,
                    player_state: Some(create_player_state(snapshot)),
                })),
            },
            PacketMode::Reliable,
        );
    }

    fn send_delta(
        &mut self,
        observer_id: u32,
        view: &mut PeerToPeerView,
        subject_id: u32,
        baseline: &PeerSnapshot,
        target: &PeerSnapshot,
        tier: PeerViewSimulationTier,
        mode: PacketMode,
    ) {
        if baseline.seq == target.seq {
            return;
        }
        let delta = create_delta_message(subject_id, baseline, target, tier);
        view.last_sent_seq = target.seq;

        if mode == PacketMode::UnreliableSequenced
            && (self.observer_features & crate::server::FEATURE_DELTA_BATCH) != 0
        {
            self.delta_batch_buffer.push(BatchSubject::from_delta(
                &delta,
                target.animation_flags as u32,
            ));
            return;
        }

        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::PlayerStateDelta(delta)),
            },
            mode,
        );
    }

    fn flush_delta_batch(&mut self, observer_id: u32, server_tick: u32) {
        if self.delta_batch_buffer.is_empty() {
            return;
        }
        let batches = crate::batch::encode_batches(
            server_tick,
            &self.delta_batch_buffer,
            crate::batch::MAX_BATCH_BYTES,
            self.seq_encoding,
        );
        for b in batches {
            self.send(
                observer_id,
                ServerMessage {
                    message: Some(server_message::Message::PlayerStateDeltaBatch(
                        PlayerStateDeltaBatch {
                            server_tick: b.server_tick,
                            subject_count: b.subject_count,
                            payload: b.payload,
                        },
                    )),
                },
                PacketMode::UnreliableSequenced,
            );
        }
        self.delta_batch_buffer.clear();
    }

    fn try_announce_profile(&mut self, observer_id: u32, subject_id: u32, profiles: &ProfileBoard) {
        let current = profiles.get(subject_id);
        let last = self
            .observer_views
            .get(&observer_id)
            .and_then(|m| m.get(&subject_id))
            .map(|v| v.last_sent_profile_version);
        if last != Some(current) {
            self.send(
                observer_id,
                ServerMessage {
                    message: Some(server_message::Message::PlayerProfileVersionAnnounced(
                        PlayerProfileVersionsAnnounced {
                            subject_id,
                            version: current,
                        },
                    )),
                },
                PacketMode::Reliable,
            );
            if let Some(v) = self
                .observer_views
                .get_mut(&observer_id)
                .and_then(|m| m.get_mut(&subject_id))
            {
                v.last_sent_profile_version = current;
            }
        }
    }

    pub fn cleanup_observer_views(&mut self, peer_id: u32) {
        self.observer_views.remove(&peer_id);
    }

    fn sweep_stale_views(&mut self, observer_id: u32, tick_counter: u32) {
        let stale: Vec<u32> = self
            .observer_views
            .get(&observer_id)
            .map(|views| {
                views
                    .iter()
                    .filter(|(_, v)| tick_counter.wrapping_sub(v.last_seen_tick) > SWEEP_INTERVAL)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();

        for id in stale {
            self.send(
                observer_id,
                ServerMessage {
                    message: Some(server_message::Message::PlayerLeft(PlayerLeft {
                        subject_id: id,
                    })),
                },
                PacketMode::Reliable,
            );
            if let Some(m) = self.observer_views.get_mut(&observer_id) {
                m.remove(&id);
            }
        }
    }
}

fn collect_scene_listener_subjects(
    board: &SnapshotBoard,
    listener: &SceneListenerState,
    observer_id: u32,
    collector: &mut InterestCollector,
) {
    for &subject in board.active_peers() {
        if subject == observer_id {
            continue;
        }
        let Some(s) = board.try_read(subject) else {
            continue;
        };
        if s.realm.as_deref() != Some(listener.realm.as_str()) {
            continue;
        }
        if !listener.parcels.contains(&s.parcel) {
            continue;
        }
        collector.add(subject, PeerViewSimulationTier::TIER_0);
    }
}

struct IntermediateScan {
    last_emote_start: Option<PeerSnapshot>,
    last_emote_stop: Option<PeerSnapshot>,
    last_teleport: Option<PeerSnapshot>,
}

fn scan_intermediate_events(
    board: &SnapshotBoard,
    subject_id: u32,
    from_seq: u32,
    latest: &PeerSnapshot,
) -> IntermediateScan {
    let mut last_emote_start = None;
    let mut last_emote_stop = None;
    let mut last_teleport = None;
    let mut earliest_carry: Option<PeerSnapshot> = None;

    let mut seq = from_seq.wrapping_add(1);
    while seq <= latest.seq {
        if let Some(snapshot) = board.try_read_seq(subject_id, seq) {
            if let Some(e) = &snapshot.emote {
                if e.emote_id.is_some() {
                    if snapshot.seq == e.start_seq {
                        last_emote_start = Some(snapshot.clone());
                        earliest_carry = None;
                    } else if earliest_carry.is_none() {
                        earliest_carry = Some(snapshot.clone());
                    }
                }
            }
            if snapshot
                .emote
                .as_ref()
                .map(|e| e.stop_reason.is_some())
                .unwrap_or(false)
            {
                last_emote_stop = Some(snapshot.clone());
            }
            if snapshot.is_teleport {
                last_teleport = Some(snapshot.clone());
            }
        }
        seq += 1;
    }

    if last_emote_start.is_none() {
        if let Some(carry) = earliest_carry {
            last_emote_start = Some(carry);
        } else if latest
            .emote
            .as_ref()
            .map(|e| e.emote_id.is_some() && e.stop_reason.is_none())
            .unwrap_or(false)
        {
            last_emote_start = Some(latest.clone());
        }
    }

    if last_teleport.is_none() && latest.last_teleport_seq > from_seq {
        last_teleport = Some(latest.clone());
    }

    IntermediateScan {
        last_emote_start,
        last_emote_stop,
        last_teleport,
    }
}

#[cfg(test)]
mod tests;
