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

fn tick(sim: &mut PeerSimulation, w: &mut World, tick_counter: u32) -> Vec<OutgoingMessage> {
    sim.outbox.clear();

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

    w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
    w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);

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

            assert_eq!(full.server_tick, 10);
        }
        _ => unreachable!(),
    }
    assert_eq!(joined[0].mode, PacketMode::Reliable);

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

    w.teleport(1, 0, v3(8.0, 8.0), "realm-a");
    w.input(1, 5000, v3(8.0, 8.0));

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

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);

    let _ = tick(&mut sim, &mut w, 1);

    w.input(1, 0, v3(10.0, 8.0));

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
    w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], true);

    let _ = tick(&mut sim, &mut w, 1);

    w.input(1, 0, v3(10.0, 8.0));
    w.input(1, 0, v3(11.0, 8.0));
    let known_seq = 1;

    w.peers.get_mut(&0).unwrap().request_resync(1, known_seq);
    let out = tick(&mut sim, &mut w, 2);

    let delta = out.iter().find_map(|m| match &m.message.message {
        Some(server_message::Message::PlayerStateDelta(d)) if m.target == 0 => Some((d, m.mode)),
        _ => None,
    });
    let (delta, mode) = delta.expect("targeted resync delta expected");
    assert_eq!(
        delta.baseline_seq, known_seq,
        "delta is from the client's known baseline"
    );
    assert_eq!(delta.new_seq, w.board.last_seq(1));
    assert_eq!(mode, PacketMode::Reliable, "resync delta is reliable");

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
    let _ = tick(&mut sim, &mut w, 1);

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
    let sim = PeerSimulation::new(&[50, 100, 200], false);
    assert_eq!(sim.tier_divisors, vec![1, 2, 4]);
}

#[test]
fn delta_baseline_seq_detects_gap() {
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
    let _ = tick(&mut sim, &mut w, 1);

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
    let _ = tick(&mut sim, &mut w, 1);

    w.profiles.set(1, 5);
    w.input(1, 0, v3(10.0, 8.0));
    let out = tick(&mut sim, &mut w, 2);
    let ann = out.iter().find_map(|m| match &m.message.message {
        Some(server_message::Message::PlayerProfileVersionAnnounced(a)) if m.target == 0 => Some(a),
        _ => None,
    });
    let ann = ann.expect("profile announcement expected");
    assert_eq!(ann.subject_id, 1);
    assert_eq!(ann.version, 5);
}

#[test]
fn pending_auth_peer_times_out() {
    let mut w = World::new();

    w.peers
        .insert(9, PeerState::new(PeerConnectionState::PendingAuth, 0));

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);

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
