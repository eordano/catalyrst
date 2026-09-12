use super::*;
use crate::interest::SCENE_LISTENER_EXAMINED;

#[test]
fn scene_listener_sees_subject_in_parcel() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        joined_for(&out, 0, "0xsubject"),
        "subject inside a parcel joins"
    );
}

#[test]
fn scene_listener_ignores_subject_outside_parcel_set() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_OUT, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        !joined_for(&out, 0, "0xsubject"),
        "parcel-exact, not cell-approximate"
    );
}

#[test]
fn scene_listener_ignores_cross_realm_subject() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-b");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        !joined_for(&out, 0, "0xsubject"),
        "cross-realm subject is invisible"
    );
}

#[test]
fn scene_listener_v1_gets_batch_v0_gets_legacy_delta() {
    for (features, expect_batch) in [(crate::server::FEATURE_DELTA_BATCH, true), (0, false)] {
        let mut w = World::new();
        w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
        w.peers.get_mut(&0).unwrap().features = features;
        w.connect(1, "0xsubject");
        w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

        let mut sim = PeerSimulation::new(&[50, 100, 200], false);
        let _ = tick(&mut sim, &mut w, 1);
        w.input(1, P_IN, v3(10.0, 8.0));
        let out = tick(&mut sim, &mut w, 2);

        let batch = out.iter().find(|m| {
            m.target == 0
                && matches!(
                    &m.message.message,
                    Some(server_message::Message::PlayerStateDeltaBatch(_))
                )
        });
        let delta = out.iter().find(|m| {
            m.target == 0
                && matches!(&m.message.message,
                    Some(server_message::Message::PlayerStateDelta(d)) if d.subject_id == 1)
        });
        if expect_batch {
            assert!(
                batch.is_some() && delta.is_none(),
                "v1 listener gets a bit-packed batch"
            );
            assert_eq!(batch.unwrap().mode, PacketMode::UnreliableSequenced);
        } else {
            assert!(
                delta.is_some() && batch.is_none(),
                "v0 listener gets a legacy delta"
            );
            assert_eq!(delta.unwrap().mode, PacketMode::UnreliableSequenced);
        }
    }
}

#[test]
fn scene_listener_receives_emote_started() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);

    let state = PlayerState {
        parcel_index: P_IN,
        position_x: spec::POSITION_X.encode(10.0),
        position_z: spec::POSITION_Z.encode(8.0),
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
            mask: None,
        }),
    );
    let out = tick(&mut sim, &mut w, 2);

    assert!(
        out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::EmoteStarted(e)) if e.subject_id == 1
                    && e.emote_id == "wave")),
        "listeners receive emote broadcasts like player observers do"
    );
}

#[test]
fn scene_listener_receives_emote_mask() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);

    let state = PlayerState {
        parcel_index: P_IN,
        position_x: spec::POSITION_X.encode(10.0),
        position_z: spec::POSITION_Z.encode(8.0),
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
            mask: Some(5),
        }),
    );
    let out = tick(&mut sim, &mut w, 2);

    assert!(
        out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::EmoteStarted(e)) if e.subject_id == 1
                    && e.mask == Some(5))),
        "the mask a scene needs for AvatarEmoteCommand reaches the listener too"
    );
}

#[test]
fn scene_listener_receives_emote_stopped() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);

    let state = PlayerState {
        parcel_index: P_IN,
        position_x: spec::POSITION_X.encode(10.0),
        position_z: spec::POSITION_Z.encode(8.0),
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
            mask: None,
        }),
    );
    let _ = tick(&mut sim, &mut w, 2);

    let active: EmoteState = w.board.try_read(1).unwrap().emote.clone().unwrap();
    let stop = PeerSnapshot {
        seq: w.board.last_seq(1) + 1,
        server_tick: 40,
        emote: Some(EmoteState {
            emote_id: None,
            start_seq: active.start_seq,
            start_tick: active.start_tick,
            duration_ms: None,
            mask: None,
            stop_reason: Some(EmoteStopReason::Cancelled),
        }),
        ..w.board.try_read(1).unwrap().clone()
    };
    w.board.publish(1, stop);
    let out = tick(&mut sim, &mut w, 3);

    assert!(
        out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::EmoteStopped(e)) if e.subject_id == 1
                    && e.reason == EmoteStopReason::Cancelled as i32)),
        "the stop that follows a tracked emote reaches the listener"
    );
}

#[test]
fn scene_listener_mid_emote_join_gets_emote_started() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");
    let state = PlayerState {
        parcel_index: P_IN,
        position_x: spec::POSITION_X.encode(8.0),
        position_z: spec::POSITION_Z.encode(8.0),
        ..Default::default()
    };
    PeerSnapshotPublisher::publish_from_player_state(
        &mut w.board,
        &mut w.grid,
        &w.encoder,
        1,
        20,
        &state,
        Some(crate::snapshot::EmoteInput {
            emote_id: "wave".into(),
            duration_ms: None,
            start_tick: None,
            mask: None,
        }),
    );

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        joined_for(&out, 0, "0xsubject"),
        "mid-emote subject still joins"
    );
    assert!(
        out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::EmoteStarted(e)) if e.subject_id == 1)),
        "the companion EmoteStarted players get on a mid-emote join reaches the listener too"
    );
}

#[test]
fn scene_listener_suppresses_profile_announcement() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);
    w.profiles.set(1, 5);
    w.input(1, P_IN, v3(10.0, 8.0));
    let out = tick(&mut sim, &mut w, 2);
    assert!(
        !out.iter().any(|m| m.target == 0
            && matches!(
                &m.message.message,
                Some(server_message::Message::PlayerProfileVersionAnnounced(_))
            )),
        "profile version is never announced to a listener"
    );
}

#[test]
fn scene_listener_receives_teleport() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN, P_IN2]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);
    w.teleport(1, P_IN2, v3(8.0, 8.0), "realm-a");
    let out = tick(&mut sim, &mut w, 2);
    assert!(
        out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::Teleported(t)) if t.subject_id == 1)),
        "teleport within the parcel set is relayed"
    );
}

#[test]
fn scene_listener_subject_leaving_parcels_is_swept() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);
    w.input(1, P_OUT, v3(8.0, 8.0));

    let mut swept = false;
    for t in 2..=first_sweep_tick_after(1) {
        if left_for(&tick(&mut sim, &mut w, t), 0, 1) {
            swept = true;
            break;
        }
    }
    assert!(
        swept,
        "a subject that left the parcels is swept with PlayerLeft within the sweep bound"
    );
}

#[test]
fn scene_listener_aoi_reassigned_joins_subject_in_the_new_parcels() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN2, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        !out.iter().any(|m| m.target == 0),
        "the subject's parcel is outside the announced set"
    );

    w.reassign_listener(0, &[("realm-a", &[P_IN2])]);
    let out = tick(&mut sim, &mut w, 2);
    assert!(
        joined_for(&out, 0, "0xsubject"),
        "the new set is in force on the very next tick, without reconnect or re-auth"
    );
}

#[test]
fn scene_listener_aoi_reassigned_stops_sending_for_dropped_parcels() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(joined_for(&out, 0, "0xsubject"));

    w.reassign_listener(0, &[("realm-a", &[P_IN2])]);
    w.input(1, P_IN, v3(9.0, 8.0));
    let out = tick(&mut sim, &mut w, 2);
    assert!(
        !out.iter().any(|m| m.target == 0
            && matches!(&m.message.message,
                Some(server_message::Message::PlayerStateDelta(d)) if d.subject_id == 1)),
        "a subject in a dropped parcel simply stops being collected"
    );
}

#[test]
fn scene_listener_aoi_reassigned_dropped_subject_swept_with_player_left() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(joined_for(&out, 0, "0xsubject"));

    w.reassign_listener(0, &[("realm-a", &[P_IN2])]);

    let mut swept = false;
    for t in 2..=first_sweep_tick_after(1) {
        if left_for(&tick(&mut sim, &mut w, t), 0, 1) {
            swept = true;
            break;
        }
    }
    assert!(swept, "the dropped subject is swept with PlayerLeft");
}

#[test]
fn scene_listener_multiple_realms_filters_per_realm_not_just_per_parcel() {
    let mut w = World::new();
    w.connect_listener_multi(
        0,
        "0xlistener",
        &[("world-a", &[P_IN]), ("world-b", &[P_IN2])],
    );
    w.connect(1, "0xworld-a-subject");
    w.connect(2, "0xworld-b-subject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "world-a");
    w.teleport(2, P_IN, v3(9.0, 9.0), "world-b");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(joined_for(&out, 0, "0xworld-a-subject"));
    assert!(
        !joined_for(&out, 0, "0xworld-b-subject"),
        "only the subject in the realm that parcel was announced for may be collected"
    );
}

#[test]
fn scene_listener_multiple_realms_collects_from_every_announced_realm() {
    let mut w = World::new();
    w.connect_listener_multi(
        0,
        "0xlistener",
        &[("world-a", &[P_IN]), ("world-b", &[P_IN])],
    );
    w.connect(1, "0xworld-a-subject");
    w.connect(2, "0xworld-b-subject");
    w.connect(3, "0xworld-c-subject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "world-a");
    w.teleport(2, P_IN, v3(9.0, 9.0), "world-b");
    w.teleport(3, P_IN, v3(9.0, 9.0), "world-c");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(joined_for(&out, 0, "0xworld-a-subject"));
    assert!(
        joined_for(&out, 0, "0xworld-b-subject"),
        "the same parcel index announced for two realms observes both of them"
    );
    assert!(!joined_for(&out, 0, "0xworld-c-subject"));
    let joins = out
        .iter()
        .filter(|m| {
            m.target == 0
                && matches!(
                    &m.message.message,
                    Some(server_message::Message::PlayerJoined(_))
                )
        })
        .count();
    assert_eq!(joins, 2, "each subject is collected exactly once");
}

#[test]
fn scene_listener_aoi_reassigned_can_replace_the_realm_set() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "world-a", &[P_IN]);
    w.connect(1, "0xworld-b-subject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "world-b");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        !out.iter().any(|m| m.target == 0),
        "world-b is not announced yet"
    );

    w.reassign_listener(0, &[("world-b", &[P_IN])]);
    let out = tick(&mut sim, &mut w, 2);
    assert!(
        joined_for(&out, 0, "0xworld-b-subject"),
        "a cohosting server that loaded a scene from another world announces it in place"
    );
}

#[test]
fn scene_listener_resync_served_reliably() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xsubject");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let _ = tick(&mut sim, &mut w, 1);
    w.input(1, P_IN, v3(10.0, 8.0));
    w.peers.get_mut(&0).unwrap().request_resync(1, 0);
    let out = tick(&mut sim, &mut w, 2);

    let served = out
        .iter()
        .find(|m| {
            m.target == 0
                && matches!(&m.message.message,
                    Some(server_message::Message::PlayerStateFull(f)) if f.subject_id == 1)
        })
        .expect("resync served with PlayerStateFull");
    assert_eq!(
        served.mode,
        PacketMode::Reliable,
        "resync response is reliable"
    );
}

#[test]
fn scene_listener_is_invisible_to_players() {
    let mut w = World::new();
    w.connect_listener(0, "0xlistener", "realm-a", &[P_IN]);
    w.connect(1, "0xplayer");
    w.teleport(1, P_IN, v3(8.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], false);
    let out = tick(&mut sim, &mut w, 1);
    assert!(
        !out.iter().any(|m| m.target == 1),
        "a player receives nothing about the listener (never a subject)"
    );
    assert!(
        joined_for(&out, 0, "0xplayer"),
        "the listener does see the player"
    );
}

fn seed_teleport(
    board: &mut SnapshotBoard,
    grid: &mut SpatialGrid,
    encoder: &ParcelEncoder,
    id: u32,
    parcel: i32,
    realm: &str,
) {
    board.set_active(id);
    PeerSnapshotPublisher::publish_teleport(
        board,
        grid,
        encoder,
        id,
        0,
        parcel,
        spec::POSITION_X.encode(1.0),
        spec::POSITION_Y.encode(0.0),
        spec::POSITION_Z.encode(1.0),
        realm.into(),
    );
}

#[test]
fn scene_listener_iterations_independent_of_total_peers() {
    fn examined_for(n: u32) -> usize {
        let mut board = SnapshotBoard::new((n + 1) as usize, 16);
        let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
        let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
        let watched: Vec<i32> = (0..3).map(|x| encoder.encode(x, 0)).collect();
        let mut id = 0u32;
        for &parcel in &watched {
            for _ in 0..3 {
                seed_teleport(&mut board, &mut grid, &encoder, id, parcel, "r");
                id += 1;
            }
        }
        while id < n {
            let parcel = encoder.encode(20 + (id % 100) as i32, 20 + (id % 7) as i32);
            seed_teleport(&mut board, &mut grid, &encoder, id, parcel, "r");
            id += 1;
        }

        let listener = listener_state(&[("r", &watched)]);
        SCENE_LISTENER_EXAMINED.with(|c| c.set(0));
        let mut c = InterestCollector::default();
        listener.get_visible_subjects(&board, &grid, u32::MAX, &mut c);
        assert_eq!(c.count(), 9, "all 9 watched-parcel occupants collected");
        SCENE_LISTENER_EXAMINED.with(|c| c.get())
    }

    let e500 = examined_for(500);
    let e2000 = examined_for(2000);
    assert_eq!(
        e500, e2000,
        "examined must be independent of total peers ({e500} vs {e2000})"
    );
    assert!(
        e500 <= 3 * 10,
        "only ~watched-parcel occupants examined, got {e500}"
    );
}

#[test]
fn scene_listener_full_budget_tick_cost_is_bounded_by_cover() {
    let mut board = SnapshotBoard::new(64, 16);
    let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
    let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
    let mapper = SceneListenerCellMapper::new(&grid, &encoder);

    let inside: Vec<i32> = [(0, 0), (7, 7), (31, 32), (63, 63), (63, 0), (0, 63)]
        .iter()
        .map(|&(x, z)| encoder.encode(x, z))
        .collect();
    let margin = encoder.encode(66, 66);
    let far: Vec<i32> = (0..40).map(|i| encoder.encode(100 + i, 100)).collect();
    for (id, &parcel) in inside
        .iter()
        .chain(std::iter::once(&margin))
        .chain(&far)
        .enumerate()
    {
        seed_teleport(&mut board, &mut grid, &encoder, id as u32, parcel, "r");
    }

    let mut full_keys = std::collections::HashSet::new();
    mapper.add_covering_cells(&mut full_keys, 0, 0, 63, 63);
    let mut full_parcels = std::collections::HashSet::new();
    for z in 0..64 {
        for x in 0..64 {
            full_parcels.insert(encoder.encode(x, z));
        }
    }
    let full = SceneListenerState::new([("r".to_string(), full_parcels)].into(), full_keys);
    assert_eq!(full.parcel_count(), 4096);
    assert_eq!(full.cell_count(), 121);

    let one = listener_state(&[("r", &[encoder.encode(0, 0)])]);

    let examined = |listener: &SceneListenerState| {
        SCENE_LISTENER_EXAMINED.with(|c| c.set(0));
        let mut c = InterestCollector::default();
        listener.get_visible_subjects(&board, &grid, u32::MAX, &mut c);
        let mut got: Vec<u32> = c.entries.iter().map(|e| e.subject).collect();
        got.sort();
        (SCENE_LISTENER_EXAMINED.with(|c| c.get()), got)
    };

    let (full_examined, full_got) = examined(&full);
    assert_eq!(
        full_got,
        (0..inside.len() as u32).collect::<Vec<_>>(),
        "every subject inside the rect collected once, the margin and far ones filtered out"
    );
    assert_eq!(
        full_examined,
        inside.len() + 1,
        "examined = occupants of the covering cells (the six inside plus the one in the margin)"
    );

    let (one_examined, one_got) = examined(&one);
    assert_eq!(one_got, vec![0]);
    assert_eq!(one_examined, 1, "a lone parcel's cell holds one subject");

    let ticks = 2000;
    SCENE_LISTENER_EXAMINED.with(|c| c.set(0));
    for _ in 0..ticks {
        let mut c = InterestCollector::default();
        full.get_visible_subjects(&board, &grid, u32::MAX, &mut c);
    }
    let examined_over_ticks = SCENE_LISTENER_EXAMINED.with(|c| c.get());
    assert_eq!(
        examined_over_ticks,
        ticks * full_examined,
        "every tick examines exactly the cover's occupants; nothing accumulates across ticks"
    );
    assert!(
        examined_over_ticks < ticks * full.parcel_count() / 100,
        "the tick cost tracks the cover ({} cells, {} occupants), not the {} announced parcels",
        full.cell_count(),
        full_examined,
        full.parcel_count()
    );
}

#[test]
fn scene_listener_index_matches_linear() {
    let n = 400u32;
    let mut board = SnapshotBoard::new((n + 1) as usize, 16);
    let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
    let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
    let mut seed: u64 = 0xdead_beef_0000_0001;
    let mut rng = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as u32
    };
    for id in 0..n {
        let parcel = (rng() % 50) as i32;
        let realm = if rng() % 2 == 0 { "realm-a" } else { "realm-b" };
        seed_teleport(&mut board, &mut grid, &encoder, id, parcel, realm);
    }
    let announcements: [&[(&str, &[i32])]; 3] = [
        &[("realm-a", &[0i32, 5, 9, 33])],
        &[("realm-b", &[1i32, 2, 7, 40, 49])],
        &[
            ("realm-a", &[0i32, 5, 9, 33]),
            ("realm-b", &[1i32, 5, 7, 40, 49]),
        ],
    ];
    for aoi in announcements {
        let listener = listener_state(aoi);
        let mut idx_c = InterestCollector::default();
        listener.get_visible_subjects(&board, &grid, u32::MAX, &mut idx_c);
        let mut lin_c = InterestCollector::default();
        listener.get_visible_subjects_linear(&board, u32::MAX, &mut lin_c);
        let mut a: Vec<u32> = idx_c.entries.iter().map(|e| e.subject).collect();
        let mut b: Vec<u32> = lin_c.entries.iter().map(|e| e.subject).collect();
        a.sort();
        b.sort();
        assert!(!a.is_empty(), "the seeded board must exercise the cover");
        assert_eq!(a, b, "cell-cover and linear outputs must be set-equal");
        let mut deduped = a.clone();
        deduped.dedup();
        assert_eq!(
            a, deduped,
            "a parcel announced for two realms never yields a subject twice"
        );
    }
}
