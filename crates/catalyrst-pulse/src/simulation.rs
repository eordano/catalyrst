//! Per-tick peer simulation + broadcasting — a port of `Peers/Simulation/PeerSimulation.cs`,
//! `Peers/PeerToPeerView.cs`, `Peers/Diff/PeerViewDiff.cs`, and the resync handling in
//! `Messaging/ResyncRequestHandler.cs`.
//!
//! Each tick, for every authenticated observer, the simulation:
//!   1. queries the area of interest for the visible subjects + their tiers,
//!   2. sends `PlayerJoined` (full state) the first time a subject becomes visible,
//!   3. afterwards sends `PlayerStateDelta` (or discrete teleport / emote / profile events),
//!   4. answers pending `ResyncRequest`s with a targeted delta from the client's baseline,
//!      falling back to a full `PlayerStateFull` when that baseline is gone,
//!   5. sweeps stale views, emitting `PlayerLeft` when a subject leaves interest.
//!
//! This is the layer that gives peers only in-interest state (full-on-join, deltas after)
//! and lets out-of-sync clients recover — the gap the audit flagged as missing.

use std::collections::HashMap;

use crate::decentraland::pulse::{
    server_message, EmoteStarted, EmoteStopReason, EmoteStopped, PlayerJoined, PlayerLeft,
    PlayerProfileVersionsAnnounced, PlayerState, PlayerStateDeltaTier0, PlayerStateFull,
    ServerMessage, TeleportPerformed,
};
use crate::interest::{
    InterestCollector, InterestEntry, PeerViewSimulationTier, SpatialAreaOfInterest,
};
use crate::snapshot::{EmoteState, IdentityBoard, PeerSnapshot, ProfileBoard, SnapshotBoard};

/// Reliability hint for an outgoing message, mirroring upstream `PacketMode`. The server
/// maps it onto the ENet channel + delivery flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketMode {
    Reliable,
    UnreliableSequenced,
    UnreliableUnsequenced,
}

/// One message the simulation wants delivered to a peer (`Messaging/MessagePipe.OutgoingMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingMessage {
    pub target: u32,
    pub message: ServerMessage,
    pub mode: PacketMode,
}

/// The connection lifecycle of a peer (`Peers/PeerConnectionState.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerConnectionState {
    None,
    PendingAuth,
    Authenticated,
    PendingDisconnect,
    Disconnecting,
}

/// Server-side peer record (`Peers/PeerState.cs`).
#[derive(Debug, Clone)]
pub struct PeerState {
    pub wallet_id: Option<String>,
    pub connection_state: PeerConnectionState,
    pub connection_time: u32,
    pub disconnection_time: u32,
    /// Per-peer handshake-attempt counter (`PeerTransportState.HandshakeAttempts`),
    /// throttled by [`crate::hardening::HandshakeAttemptPolicy`]. Scoped to the slot
    /// lifetime — a reconnect starts fresh.
    pub handshake_attempts: u8,
    /// Source IP of the connection (for [`crate::hardening::PreAuthAdmission`]).
    pub ip: Option<String>,
    /// Pending resync requests keyed by subject (only the latest known_seq is kept).
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
            resync_requests: None,
        }
    }

    /// Record a resync request for `subject` at `known_seq` (`ResyncRequestHandler.Handle`).
    pub fn request_resync(&mut self, subject: u32, known_seq: u32) {
        self.resync_requests
            .get_or_insert_with(HashMap::new)
            .insert(subject, known_seq);
    }
}

/// The observer's "knowledge" about one subject (`Peers/PeerToPeerView.cs`).
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

// ── Float diff comparison (Peers/Diff/DiffComparison.cs) ────────────────────────

const TOLERANCE: f32 = 0.001;

fn float_equals(a: f32, b: f32) -> bool {
    (a - b).abs() < TOLERANCE
}

fn opt_float_equals(a: Option<f32>, b: Option<f32>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => (a - b).abs() < TOLERANCE,
        _ => false,
    }
}

/// Build a tiered delta between two snapshots (`Peers/Diff/PeerViewDiff.CreateMessage`).
/// Quantized float fields are set via the `messages` accessors so they encode bit-identically.
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

    // TIER_0: animation details + head IK.
    if tier == PeerViewSimulationTier::TIER_0 {
        if !float_equals(from.slide_blend, to.slide_blend) {
            delta.set_slide_blend_f(to.slide_blend);
        }
        if !opt_float_equals(from.head_yaw, to.head_yaw) {
            if let Some(v) = to.head_yaw {
                delta.set_head_yaw_f(v);
            }
        }
        if !opt_float_equals(from.head_pitch, to.head_pitch) {
            if let Some(v) = to.head_pitch {
                delta.set_head_pitch_f(v);
            }
        }
    }

    if from.animation_flags != to.animation_flags {
        delta.state_flags = Some(to.animation_flags as u32);
    }

    // Glide state matters at every tier (visible from afar).
    if from.glide_state != to.glide_state {
        delta.glide_state = Some(to.glide_state);
    }

    if from.parcel != to.parcel {
        delta.parcel_index = Some(to.parcel);
    }

    if !float_equals(from.local_position.x, to.local_position.x) {
        delta.set_position_x_f(to.local_position.x);
    }
    if !float_equals(from.local_position.y, to.local_position.y) {
        delta.set_position_y_f(to.local_position.y);
    }
    if !float_equals(from.local_position.z, to.local_position.z) {
        delta.set_position_z_f(to.local_position.z);
    }

    if !float_equals(from.rotation_y, to.rotation_y) {
        delta.set_rotation_y_f(to.rotation_y);
    }

    if from.jump_count != to.jump_count {
        delta.jump_count = Some(to.jump_count);
    }

    // Point-at: only when present on the target.
    if let Some(to_pa) = to.point_at {
        let from_pa = from.point_at;
        if from_pa.is_none() || !float_equals(from_pa.unwrap().x, to_pa.x) {
            delta.set_point_at_x_f(to_pa.x);
        }
        if from_pa.is_none() || !float_equals(from_pa.unwrap().y, to_pa.y) {
            delta.set_point_at_y_f(to_pa.y);
        }
        if from_pa.is_none() || !float_equals(from_pa.unwrap().z, to_pa.z) {
            delta.set_point_at_z_f(to_pa.z);
        }
    }

    // TIER_0 + TIER_1: velocity + movement blend (omitted at TIER_2, "spatial flags only").
    if tier == PeerViewSimulationTier::TIER_0 || tier == PeerViewSimulationTier::TIER_1 {
        if !float_equals(from.velocity.x, to.velocity.x) {
            delta.set_velocity_x_f(to.velocity.x);
        }
        if !float_equals(from.velocity.y, to.velocity.y) {
            delta.set_velocity_y_f(to.velocity.y);
        }
        if !float_equals(from.velocity.z, to.velocity.z) {
            delta.set_velocity_z_f(to.velocity.z);
        }
        if !float_equals(from.movement_blend, to.movement_blend) {
            delta.set_movement_blend_f(to.movement_blend);
        }
    }

    delta
}

/// Build a full [`PlayerState`] from a snapshot (`PeerSimulation.CreatePlayerState`).
///
/// `jump_count` is deliberately left at its proto3 default (0): upstream
/// `CreatePlayerState` does not populate it, so we don't either. (Per-tick jump
/// changes still ride the delta path via `create_delta_message`.) Head IK and
/// point-at stay `Option` so an absent value is omitted on the wire, matching
/// upstream's `if (snapshot.HeadYaw.HasValue)` guards.
fn create_player_state(snapshot: &PeerSnapshot) -> PlayerState {
    PlayerState {
        parcel_index: snapshot.parcel,
        position: Some(snapshot.local_position),
        velocity: Some(snapshot.velocity),
        rotation_y: snapshot.rotation_y,
        movement_blend: snapshot.movement_blend,
        slide_blend: snapshot.slide_blend,
        state_flags: snapshot.animation_flags as u32,
        glide_state: snapshot.glide_state,
        jump_count: 0,
        head_yaw: snapshot.head_yaw,
        head_pitch: snapshot.head_pitch,
        point_at: snapshot.point_at,
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

/// The `self_mirror` wallet id (`PeerSimulation.SELF_MIRROR_WALLET_ID`).
pub const SELF_MIRROR_WALLET_ID: &str = "self_mirror";
const SWEEP_INTERVAL: u32 = 100;

/// A peer the simulation tick decided must leave, with the reason (`peersToBeRemoved`
/// and the `transport.Disconnect(..)` call in `PeerSimulation.SimulateTick`). The server
/// drains these after the tick: it disconnects the transport and wipes the boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiredPeer {
    pub peer: u32,
    pub reason: ExpiredReason,
}

/// Why a peer was reaped by the per-tick connection-lifecycle sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiredReason {
    /// PENDING_AUTH peer that never authenticated within the budget — transport
    /// disconnect with `AUTH_TIMEOUT`.
    AuthTimeout,
    /// DISCONNECTING peer whose grace period elapsed — board cleanup only.
    DisconnectCleanTimeout,
}

/// Per-observer simulation state + per-tick broadcasting (`Peers/Simulation/PeerSimulation.cs`).
///
/// catalyrst runs a single simulation (no worker shards), so this owns all observer views.
pub struct PeerSimulation {
    /// observer -> (subject -> view).
    observer_views: HashMap<u32, HashMap<u32, PeerToPeerView>>,
    tier_divisors: Vec<u32>,
    self_mirror_enabled: bool,
    self_mirror_tier: PeerViewSimulationTier,
    resync_with_delta: bool,
    pub base_tick_ms: u32,
    /// PENDING_AUTH peers older than this (monotonic ms) are reaped with AUTH_TIMEOUT
    /// (`PeerSimulation.pendingAuthCleanTimeoutMs`, upstream default 30000).
    pending_auth_clean_timeout_ms: u32,
    /// DISCONNECTING peers idle this long (monotonic ms) are cleaned up
    /// (`PeerSimulation.disconnectionCleanTimeoutMs`, upstream default 5000).
    disconnection_clean_timeout_ms: u32,
    /// Outgoing message sink for the current tick (drained by the server).
    pub outbox: Vec<OutgoingMessage>,
    /// Peers the last tick decided must leave (drained by the server, like
    /// upstream `peersToBeRemoved` + the `transport.Disconnect` calls).
    pub expired: Vec<ExpiredPeer>,
    collector: InterestCollector,
}

/// Upstream `PeerSimulation` ctor defaults for the two lifecycle timeouts.
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
            base_tick_ms,
            pending_auth_clean_timeout_ms: DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS,
            disconnection_clean_timeout_ms: DEFAULT_DISCONNECTION_CLEAN_TIMEOUT_MS,
            outbox: Vec::new(),
            expired: Vec::new(),
            collector: InterestCollector::default(),
        }
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

    /// One simulation tick for all authenticated observers in `peers`.
    ///
    /// `now_ms` is the monotonic server clock (same scale as `PeerState::connection_time`
    /// / `disconnection_time`). Before fanning state out, each peer runs the connection
    /// lifecycle sweep (`PeerSimulation.SimulateTick`'s pre-loop): PENDING_AUTH peers past
    /// the auth budget are queued for an AUTH_TIMEOUT disconnect; DISCONNECTING peers past
    /// the grace window are queued for cleanup. Both land in `self.expired`, which the
    /// server drains after the tick (the `peersToBeRemoved` set + `transport.Disconnect`).
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

        // Iterate over a stable order of observer ids so a borrow of `peers` doesn't conflict
        // with the mutable resync-clearing we do per observer.
        let observer_ids: Vec<u32> = peers.keys().copied().collect();

        for observer_id in observer_ids {
            let peer = &peers[&observer_id];
            let connection_state = peer.connection_state;

            // PENDING_AUTH timeout (upstream: transport.Disconnect(.., AUTH_TIMEOUT)).
            if connection_state == PeerConnectionState::PendingAuth {
                if now_ms.wrapping_sub(peer.connection_time) >= self.pending_auth_clean_timeout_ms {
                    self.expired.push(ExpiredPeer {
                        peer: observer_id,
                        reason: ExpiredReason::AuthTimeout,
                    });
                }
                continue;
            }

            // DISCONNECTING grace timeout (upstream: CleanupDisconnectedPeer).
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

            let Some(observer_snapshot) = board.try_read(observer_id).cloned() else {
                continue;
            };

            // Drain resync requests for this observer up front (we replay them per subject).
            let mut resync = peers
                .get_mut(&observer_id)
                .and_then(|s| s.resync_requests.take());

            self.observer_views.entry(observer_id).or_default();

            self.collector.clear();
            // Move collector out to avoid double mutable borrow of self.
            let mut collector = std::mem::take(&mut self.collector);
            aoi.get_visible_subjects(
                board,
                observer_id,
                observer_snapshot.realm.as_deref(),
                observer_snapshot.global_position,
                &mut collector,
            );
            if self.self_mirror_enabled {
                collector.add(observer_id, self.self_mirror_tier);
            }

            self.process_visible_subjects(
                observer_id,
                board,
                identity,
                profiles,
                &collector,
                resync.as_mut(),
                tick_counter,
            );

            self.collector = collector;

            // Resync entries are cleared at end-of-tick (consumed or dropped).
            if let Some(state) = peers.get_mut(&observer_id) {
                state.resync_requests = None;
            }
            let _ = &mut resync;

            if tick_counter.is_multiple_of(SWEEP_INTERVAL) {
                self.sweep_stale_views(observer_id, tick_counter);
            }
        }
    }

    fn process_visible_subjects(
        &mut self,
        observer_id: u32,
        board: &SnapshotBoard,
        identity: &IdentityBoard,
        profiles: &ProfileBoard,
        collector: &InterestCollector,
        mut resync: Option<&mut HashMap<u32, u32>>,
        tick_counter: u32,
    ) {
        let entries: Vec<InterestEntry> = collector.entries.clone();
        for entry in entries {
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

            // Stamp last_seen_tick before the tier gate so a multi-tick tier doesn't trigger
            // false re-entry detection on intervening ticks.
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
                );
                let mut view = view;
                view.last_seen_tick = tick_counter;
                self.observer_views
                    .entry(observer_id)
                    .or_default()
                    .insert(entry.subject, view);
                continue;
            }

            self.try_announce_profile(observer_id, entry.subject, profiles);

            let mut view = self.observer_views[&observer_id][&entry.subject].clone();
            let last_sent_state = self.process_existing_subject(
                observer_id,
                entry,
                &mut view,
                board,
                &latest,
                resync.as_deref_mut(),
            );
            view.last_sent_snapshot = last_sent_state;
            view.last_seen_tick = tick_counter;
            self.observer_views
                .entry(observer_id)
                .or_default()
                .insert(entry.subject, view);
        }
    }

    /// Defense-in-depth against `PeerIndex` aliasing: if the view was seeded for a different
    /// wallet than now occupies the slot, emit `PlayerLeft` and re-enter the new path.
    fn detect_and_handle_aliasing(
        &mut self,
        observer_id: u32,
        subject_id: u32,
        is_self_mirror: bool,
        identity: &IdentityBoard,
    ) -> bool {
        let current_wallet = if is_self_mirror {
            Some(SELF_MIRROR_WALLET_ID.to_string())
        } else {
            identity.wallet_by_peer(subject_id).map(|s| s.to_string())
        };

        let last_sent = self
            .observer_views
            .get(&observer_id)
            .and_then(|m| m.get(&subject_id))
            .and_then(|v| v.last_sent_wallet_id.clone());

        let same = match (&last_sent, &current_wallet) {
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

    /// First-time visibility: send `PlayerJoined` with full state; announce an in-flight emote.
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

        // If already emoting at first visibility, broadcast the ongoing emote.
        if let Some(active) = latest.emote.clone().filter(|e| e.emote_id.is_some()) {
            self.send_emote_started(observer_id, &mut view, subject_id, latest, &active);
            view.last_sent_emote = Some(active);
        } else {
            view.last_sent_seq = latest.seq;
        }

        view
    }

    /// Process an already-known subject: scan intermediates for discrete events, sync emote
    /// stop, then resync or delta. Returns the snapshot that becomes the new baseline.
    fn process_existing_subject(
        &mut self,
        observer_id: u32,
        entry: InterestEntry,
        view: &mut PeerToPeerView,
        board: &SnapshotBoard,
        latest: &PeerSnapshot,
        mut resync: Option<&mut HashMap<u32, u32>>,
    ) -> PeerSnapshot {
        let mut last_sent_state = view.last_sent_snapshot.clone();
        let mut discrete_event_sent = false;

        let scan =
            scan_intermediate_events(board, entry.subject, view.last_sent_snapshot.seq, latest);

        // Teleport (spatial snap first), deduped on last_teleport_seq.
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

        // Emote start only if still active (not stopped in the same batch).
        let emote_start_is_effective = match (&scan.last_emote_start, &scan.last_emote_stop) {
            (Some(start), stop) => start.seq > stop.as_ref().map(|s| s.seq).unwrap_or(0),
            _ => false,
        };

        if emote_start_is_effective {
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

        // Emote stop (skip when start is still effective).
        if !emote_start_is_effective {
            self.try_sync_emote_stop(
                observer_id,
                entry.subject,
                view,
                &mut last_sent_state,
                scan.last_emote_stop.as_ref(),
            );
        }

        // Resync or delta (skip if a discrete event already carried full state).
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

        // Try a targeted delta from the client's baseline; fall back to full state.
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
                    emote_id: emote.emote_id.clone().unwrap_or_default(),
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
        self.send(
            observer_id,
            ServerMessage {
                message: Some(server_message::Message::PlayerStateDelta(delta)),
            },
            mode,
        );
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

    /// Remove the disconnected peer's observer views (`PeerSimulation.RemoveObserver` /
    /// `CleanupDisconnectedPeer` observer-view part).
    pub fn cleanup_observer_views(&mut self, peer_id: u32) {
        self.observer_views.remove(&peer_id);
    }

    /// Periodic sweep — emit `PlayerLeft` for and drop views not seen in recent ticks.
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

/// Result of `ScanIntermediateEvents`: the last teleport / emote-start / emote-stop in the
/// observer's unseen seq range, with the emote-from-eviction fallback applied.
struct IntermediateScan {
    last_emote_start: Option<PeerSnapshot>,
    last_emote_stop: Option<PeerSnapshot>,
    last_teleport: Option<PeerSnapshot>,
}

/// Collect the last teleport, emote start, and emote stop within `from_seq+1..=latest.seq`,
/// applying the same ring-wrap eviction fallbacks as `PeerSimulation.ScanIntermediateEvents`.
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

    // Teleport eviction fallback.
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
mod tests {
    use super::*;
    use crate::decentraland::common::Vector3;
    use crate::interest::{
        ParcelEncoder, ParcelEncoderOptions, SpatialAreaOfInterest, SpatialAreaOfInterestOptions,
        SpatialGrid,
    };
    use crate::snapshot::{EmoteState, PeerSnapshotPublisher};

    fn v3(x: f32, z: f32) -> Vector3 {
        Vector3 { x, y: 0.0, z }
    }

    struct World {
        board: SnapshotBoard,
        grid: SpatialGrid,
        encoder: ParcelEncoder,
        aoi: SpatialAreaOfInterest,
        identity: IdentityBoard,
        profiles: ProfileBoard,
        peers: HashMap<u32, PeerState>,
    }

    impl World {
        fn new() -> Self {
            World {
                board: SnapshotBoard::new(16, 16),
                grid: SpatialGrid::new(16.0),
                encoder: ParcelEncoder::new(ParcelEncoderOptions::default()),
                aoi: SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default()),
                identity: IdentityBoard::new(16),
                profiles: ProfileBoard::new(16),
                peers: HashMap::new(),
            }
        }

        fn connect(&mut self, id: u32, wallet: &str) {
            self.board.set_active(id);
            self.identity.set(id, wallet.into());
            let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
            st.wallet_id = Some(wallet.into());
            self.peers.insert(id, st);
        }

        /// Teleport the peer (seeds realm + position) so it becomes visible.
        fn teleport(&mut self, id: u32, parcel: i32, local: Vector3, realm: &str) {
            PeerSnapshotPublisher::publish_teleport(
                &mut self.board,
                &mut self.grid,
                &self.encoder,
                id,
                10,
                parcel,
                local,
                realm.into(),
            );
        }

        fn input(&mut self, id: u32, parcel: i32, local: Vector3) {
            let state = PlayerState {
                parcel_index: parcel,
                position: Some(local),
                ..Default::default()
            };
            PeerSnapshotPublisher::publish_from_player_state(
                &mut self.board,
                &mut self.grid,
                &self.encoder,
                id,
                20,
                &state,
                None,
            );
        }
    }

    /// Drive one tick and return the outbox.
    fn tick(sim: &mut PeerSimulation, w: &mut World, tick_counter: u32) -> Vec<OutgoingMessage> {
        sim.outbox.clear();
        // now_ms=0: tests connect peers at connection_time 0, so no lifecycle sweep fires.
        sim.simulate_tick(
            &mut w.peers,
            &w.board,
            &w.aoi,
            &w.identity,
            &w.profiles,
            tick_counter,
            0,
        );
        std::mem::take(&mut sim.outbox)
    }

    #[test]
    fn full_state_on_join_then_delta_after() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        // Both teleport into the same realm, close together.
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);

        // Tick 1: subject 1 is new to observer 0 -> PlayerJoined (full state).
        let out = tick(&mut sim, &mut w, 1);
        let joined: Vec<_> = out
            .iter()
            .filter(|m| m.target == 0)
            .filter(|m| {
                matches!(
                    &m.message.message,
                    Some(server_message::Message::PlayerJoined(_))
                )
            })
            .collect();
        assert_eq!(
            joined.len(),
            1,
            "first visibility sends exactly one PlayerJoined"
        );
        match &joined[0].message.message {
            Some(server_message::Message::PlayerJoined(pj)) => {
                assert_eq!(pj.user_id, "0xsubject");
                let full = pj.state.as_ref().unwrap();
                assert_eq!(full.subject_id, 1);
                // server_tick + sequence come from the real snapshot, NOT hardcoded 0.
                assert_eq!(full.server_tick, 10);
            }
            _ => unreachable!(),
        }
        assert_eq!(joined[0].mode, PacketMode::Reliable);

        // Subject moves; tick 2 -> a delta (not a full state, not a join).
        w.input(1, 0, v3(10.0, 8.0));
        let out = tick(&mut sim, &mut w, 2);
        let to_obs: Vec<_> = out.iter().filter(|m| m.target == 0).collect();
        assert!(
            to_obs.iter().any(|m| matches!(&m.message.message, Some(server_message::Message::PlayerStateDelta(d)) if d.subject_id == 1)),
            "subsequent updates are deltas, got {to_obs:?}"
        );
        assert!(
            !to_obs.iter().any(|m| matches!(
                &m.message.message,
                Some(server_message::Message::PlayerJoined(_))
            )),
            "no second PlayerJoined"
        );
        // The delta carries real per-peer sequence numbers, not 0.
        let delta = to_obs
            .iter()
            .find_map(|m| match &m.message.message {
                Some(server_message::Message::PlayerStateDelta(d)) => Some(d),
                _ => None,
            })
            .unwrap();
        assert_eq!(delta.new_seq, w.board.last_seq(1));
        assert!(delta.new_seq > delta.baseline_seq);
    }

    #[test]
    fn out_of_interest_subject_is_invisible() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xfaraway");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        // place subject far beyond max radius (parcel far away).
        w.teleport(1, 0, v3(8.0, 8.0), "realm-a");
        w.input(1, 5000, v3(8.0, 8.0)); // huge parcel index -> far global pos

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);
        let out = tick(&mut sim, &mut w, 1);
        assert!(
            !out.iter().any(|m| m.target == 0
                && matches!(
                    &m.message.message,
                    Some(server_message::Message::PlayerJoined(_))
                )),
            "a subject outside the interest radius never produces a join"
        );
    }

    #[test]
    fn resync_request_recovers_with_full_state() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

        // resync_with_delta = false -> always falls back to STATE_FULL.
        let mut sim = PeerSimulation::new(&[50, 100, 200], false);

        // Establish the view (join).
        let _ = tick(&mut sim, &mut w, 1);
        // Move the subject so latest seq advances.
        w.input(1, 0, v3(10.0, 8.0));

        // Client requests a resync for subject 1 from an old/unknown seq.
        w.peers.get_mut(&0).unwrap().request_resync(1, 0);
        let out = tick(&mut sim, &mut w, 2);

        let full: Vec<_> = out
            .iter()
            .filter(|m| m.target == 0)
            .filter_map(|m| match &m.message.message {
                Some(server_message::Message::PlayerStateFull(f)) => Some(f),
                _ => None,
            })
            .collect();
        assert_eq!(
            full.len(),
            1,
            "resync fallback delivers exactly one STATE_FULL"
        );
        assert_eq!(full[0].subject_id, 1);
        assert_eq!(full[0].sequence, w.board.last_seq(1));
        // STATE_FULL must be reliable so the recovering client actually gets it.
        let msg = out
            .iter()
            .find(|m| {
                matches!(
                    &m.message.message,
                    Some(server_message::Message::PlayerStateFull(_))
                )
            })
            .unwrap();
        assert_eq!(msg.mode, PacketMode::Reliable);
    }

    #[test]
    fn resync_request_recovers_with_targeted_delta_when_baseline_known() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a"); // seq 0 for subject 1

        let mut sim = PeerSimulation::new(&[50, 100, 200], true); // resync_with_delta

        let _ = tick(&mut sim, &mut w, 1);
        // advance subject a couple of seqs, all still in-ring.
        w.input(1, 0, v3(10.0, 8.0)); // seq 1
        w.input(1, 0, v3(11.0, 8.0)); // seq 2
        let known_seq = 1; // client has up to seq 1

        w.peers.get_mut(&0).unwrap().request_resync(1, known_seq);
        let out = tick(&mut sim, &mut w, 2);

        // Targeted delta over the reliable channel (baseline known in ring).
        let delta = out.iter().find_map(|m| match &m.message.message {
            Some(server_message::Message::PlayerStateDelta(d)) if m.target == 0 => {
                Some((d, m.mode))
            }
            _ => None,
        });
        let (delta, mode) = delta.expect("targeted resync delta expected");
        assert_eq!(
            delta.baseline_seq, known_seq,
            "delta is from the client's known baseline"
        );
        assert_eq!(delta.new_seq, w.board.last_seq(1));
        assert_eq!(mode, PacketMode::Reliable, "resync delta is reliable");
        // No STATE_FULL fallback this time.
        assert!(
            !out.iter().any(|m| matches!(
                &m.message.message,
                Some(server_message::Message::PlayerStateFull(_))
            )),
            "known baseline avoids the STATE_FULL fallback"
        );
    }

    #[test]
    fn teleport_is_broadcast_to_observer() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);
        let _ = tick(&mut sim, &mut w, 1); // join

        // Subject teleports again within the same realm.
        w.teleport(1, 0, v3(12.0, 8.0), "realm-a");
        let out = tick(&mut sim, &mut w, 2);

        let tp = out.iter().find_map(|m| match &m.message.message {
            Some(server_message::Message::Teleported(t)) if m.target == 0 => Some(t),
            _ => None,
        });
        let tp = tp.expect("teleport broadcast expected");
        assert_eq!(tp.subject_id, 1);
        assert_eq!(tp.sequence, w.board.last_seq(1));
    }

    #[test]
    fn tier_divisors_gate_far_subjects() {
        // TIER_1 (every 2nd tick), TIER_2 (every 4th tick).
        let sim = PeerSimulation::new(&[50, 100, 200], false);
        assert_eq!(sim.tier_divisors, vec![1, 2, 4]);
    }

    #[test]
    fn delta_baseline_seq_detects_gap() {
        // The delta exposes baseline_seq + new_seq so the client can detect loss.
        let from = PeerSnapshot {
            seq: 3,
            ..Default::default()
        };
        let to = PeerSnapshot {
            seq: 7,
            local_position: v3(1.0, 0.0),
            ..Default::default()
        };
        let d = create_delta_message(1, &from, &to, PeerViewSimulationTier::TIER_0);
        assert_eq!(d.baseline_seq, 3);
        assert_eq!(d.new_seq, 7);
        assert!(d.position_x.is_some(), "changed field is present");
    }

    #[test]
    fn tier2_omits_velocity_and_blend() {
        let from = PeerSnapshot {
            seq: 0,
            ..Default::default()
        };
        let to = PeerSnapshot {
            seq: 1,
            velocity: v3(5.0, 0.0),
            movement_blend: 2.0,
            local_position: v3(1.0, 0.0),
            ..Default::default()
        };
        let d2 = create_delta_message(1, &from, &to, PeerViewSimulationTier::TIER_2);
        assert!(d2.velocity_x.is_none(), "TIER_2 omits velocity");
        assert!(d2.movement_blend.is_none(), "TIER_2 omits movement blend");
        assert!(
            d2.position_x.is_some(),
            "TIER_2 still carries spatial position"
        );

        let d0 = create_delta_message(1, &from, &to, PeerViewSimulationTier::TIER_0);
        assert!(d0.velocity_x.is_some(), "TIER_0 includes velocity");
        assert!(d0.movement_blend.is_some());
    }

    #[test]
    fn emote_start_then_stop_broadcast() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);
        let _ = tick(&mut sim, &mut w, 1); // join

        // Subject starts an emote.
        let state = PlayerState {
            parcel_index: 0,
            position: Some(v3(9.0, 8.0)),
            ..Default::default()
        };
        PeerSnapshotPublisher::publish_from_player_state(
            &mut w.board,
            &mut w.grid,
            &w.encoder,
            1,
            30,
            &state,
            Some(crate::snapshot::EmoteInput {
                emote_id: "wave".into(),
                duration_ms: None,
                start_tick: None,
            }),
        );
        let out = tick(&mut sim, &mut w, 2);
        let started = out.iter().find_map(|m| match &m.message.message {
            Some(server_message::Message::EmoteStarted(e)) if m.target == 0 => Some(e),
            _ => None,
        });
        assert_eq!(started.expect("emote started broadcast").emote_id, "wave");

        // Subject stops the emote (publish a stop marker like EmoteStopHandler).
        let active: EmoteState = w.board.try_read(1).unwrap().emote.clone().unwrap();
        let stop = PeerSnapshot {
            seq: w.board.last_seq(1) + 1,
            server_tick: 40,
            emote: Some(EmoteState {
                emote_id: None,
                start_seq: active.start_seq,
                start_tick: active.start_tick,
                duration_ms: None,
                stop_reason: Some(EmoteStopReason::Cancelled),
            }),
            ..w.board.try_read(1).unwrap().clone()
        };
        w.board.publish(1, stop);
        let out = tick(&mut sim, &mut w, 3);
        let stopped = out.iter().find_map(|m| match &m.message.message {
            Some(server_message::Message::EmoteStopped(e)) if m.target == 0 => Some(e),
            _ => None,
        });
        assert_eq!(
            stopped.expect("emote stopped broadcast").reason,
            EmoteStopReason::Cancelled as i32
        );
    }

    #[test]
    fn profile_version_change_is_announced() {
        let mut w = World::new();
        w.connect(0, "0xobserver");
        w.connect(1, "0xsubject");
        w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
        w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);
        let _ = tick(&mut sim, &mut w, 1); // join at profile v0

        w.profiles.set(1, 5);
        w.input(1, 0, v3(10.0, 8.0)); // any change keeps it visible
        let out = tick(&mut sim, &mut w, 2);
        let ann = out.iter().find_map(|m| match &m.message.message {
            Some(server_message::Message::PlayerProfileVersionAnnounced(a)) if m.target == 0 => {
                Some(a)
            }
            _ => None,
        });
        let ann = ann.expect("profile announcement expected");
        assert_eq!(ann.subject_id, 1);
        assert_eq!(ann.version, 5);
    }

    #[test]
    fn pending_auth_peer_times_out() {
        let mut w = World::new();
        // Pending-auth peer connected at t=0, never authenticated.
        w.peers
            .insert(9, PeerState::new(PeerConnectionState::PendingAuth, 0));

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);

        // Just under the budget: no expiry.
        sim.simulate_tick(
            &mut w.peers,
            &w.board,
            &w.aoi,
            &w.identity,
            &w.profiles,
            1,
            DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS - 1,
        );
        assert!(
            sim.expired.is_empty(),
            "not expired before the auth budget elapses"
        );

        // At/after the budget: AUTH_TIMEOUT.
        sim.simulate_tick(
            &mut w.peers,
            &w.board,
            &w.aoi,
            &w.identity,
            &w.profiles,
            2,
            DEFAULT_PENDING_AUTH_CLEAN_TIMEOUT_MS,
        );
        assert_eq!(
            sim.expired,
            vec![ExpiredPeer {
                peer: 9,
                reason: ExpiredReason::AuthTimeout
            }]
        );
    }

    #[test]
    fn disconnecting_peer_cleaned_after_grace() {
        let mut w = World::new();
        let mut st = PeerState::new(PeerConnectionState::Disconnecting, 0);
        st.disconnection_time = 1_000;
        w.peers.insert(4, st);

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);

        // Within grace: still around.
        sim.simulate_tick(
            &mut w.peers,
            &w.board,
            &w.aoi,
            &w.identity,
            &w.profiles,
            1,
            1_000 + DEFAULT_DISCONNECTION_CLEAN_TIMEOUT_MS - 1,
        );
        assert!(sim.expired.is_empty());

        // Grace elapsed: cleanup queued.
        sim.simulate_tick(
            &mut w.peers,
            &w.board,
            &w.aoi,
            &w.identity,
            &w.profiles,
            2,
            1_000 + DEFAULT_DISCONNECTION_CLEAN_TIMEOUT_MS,
        );
        assert_eq!(
            sim.expired,
            vec![ExpiredPeer {
                peer: 4,
                reason: ExpiredReason::DisconnectCleanTimeout
            }]
        );
    }
}
