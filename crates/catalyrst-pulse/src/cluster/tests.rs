use std::sync::Arc;

use super::*;
use crate::cluster::feed::encode_cluster_change;
use crate::interest::SPATIAL_GRID_CELL_SIZE;
use crate::snapshot::PeerSnapshot;

const CAP: usize = 32;

/// Fraction of the pass interval the timing assert allows, deliberately looser than the tenth
/// `warn_on_overrun` alerts at. Parallel agents share this tree's `target/`, so a debug-build
/// wall-clock assert must only fail for the code's own reasons; the warn stays the tight signal
/// and upstream keeps the real measurement in `DCLPulseBenchmarks` rather than the test suite.
const TIMING_ASSERT_BUDGET_DIVISOR: u64 = 2;

struct World {
    grids: RealmSpatialGrids,
    board: SnapshotBoard,
    identity: IdentityBoard,
    feed: Arc<RecordingClusterFeed>,
    tracker: ClusterTracker,
}

fn v3(x: f32, z: f32) -> Vector3 {
    Vector3 { x, y: 0.0, z }
}

impl World {
    fn with(options: ClusterOptions) -> Self {
        let feed = Arc::new(RecordingClusterFeed::default());
        World {
            grids: RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, CAP),
            board: SnapshotBoard::new(CAP, 4),
            identity: IdentityBoard::new(CAP),
            feed: feed.clone(),
            tracker: ClusterTracker::new(options, CAP, feed),
        }
    }

    /// Debounce off unless a test is about the debounce: every other scenario asserts the
    /// partition, and a three-pass wait in each would only hide which pass changed it.
    fn new() -> Self {
        Self::with(ClusterOptions {
            dwell_passes: 1,
            ..Default::default()
        })
    }

    fn place(&mut self, peer: u32, wallet: &str, realm: &str, position: Vector3) {
        self.place_session(peer, wallet, wallet, realm, position, false);
    }

    fn place_session(
        &mut self,
        peer: u32,
        wallet: &str,
        session: &str,
        realm: &str,
        position: Vector3,
        is_teleport: bool,
    ) {
        self.board.set_active(peer);
        self.board.publish(
            peer,
            PeerSnapshot {
                global_position: position,
                realm: Some(realm.into()),
                is_teleport,
                ..Default::default()
            },
        );
        self.identity
            .set_with_session(peer, wallet.to_string(), session.to_string());
        self.grids.set(peer, realm, position);
    }

    fn remove(&mut self, peer: u32) {
        self.grids.remove(peer);
        self.identity.remove(peer);
        self.board.clear_active(peer);
    }

    fn pass(&mut self) {
        self.tracker
            .run_pass(&self.grids, &self.board, &self.identity);
    }

    fn cluster_of(&self, peer: u32) -> Option<String> {
        self.tracker.board().cluster_id(peer).map(str::to_string)
    }

    fn cluster_count(&self) -> usize {
        self.tracker.board().clusters.len()
    }
}

#[test]
fn peers_in_one_cell_form_one_cluster() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 2.0));
    w.place(2, "0xc", "r", v3(3.0, 3.0));
    w.pass();

    assert_eq!(w.cluster_count(), 1);
    assert_eq!(w.cluster_of(0), w.cluster_of(1));
    assert_eq!(w.cluster_of(1), w.cluster_of(2));
    assert_eq!(w.tracker.board().clusters[0].count, 3);
}

#[test]
fn diagonally_adjacent_cells_join_and_a_gap_cell_separates() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(cell * 0.5, cell * 0.5));
    w.place(1, "0xb", "r", v3(cell * 1.5, cell * 1.5));
    w.place(2, "0xc", "r", v3(cell * 3.5, cell * 3.5));
    w.pass();

    assert_eq!(
        w.cluster_count(),
        2,
        "8-neighbour adjacency joins diagonals"
    );
    assert_eq!(w.cluster_of(0), w.cluster_of(1));
    assert_ne!(w.cluster_of(0), w.cluster_of(2));
}

#[test]
fn a_chain_of_adjacent_cells_is_transitively_one_cluster() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::new();
    for i in 0..6u32 {
        w.place(i, &format!("0x{i}"), "r", v3(cell * (i as f32 + 0.5), 0.5));
    }
    w.pass();

    assert_eq!(w.cluster_count(), 1);
    let first = w.cluster_of(0);
    for i in 1..6u32 {
        assert_eq!(w.cluster_of(i), first, "peer {i} joined through the chain");
    }
}

#[test]
fn a_cluster_never_spans_realms_even_in_one_cell() {
    let mut w = World::new();
    w.place(0, "0xa", "realm-a", v3(1.0, 1.0));
    w.place(1, "0xb", "realm-b", v3(1.0, 1.0));
    w.pass();

    assert_eq!(w.cluster_count(), 2);
    assert_ne!(w.cluster_of(0), w.cluster_of(1));
    let realms: Vec<&str> = w
        .tracker
        .board()
        .clusters
        .iter()
        .map(|c| c.realm.as_str())
        .collect();
    assert!(realms.contains(&"realm-a") && realms.contains(&"realm-b"));
}

#[test]
fn a_stable_cluster_keeps_its_id_across_passes() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 2.0));
    w.pass();
    let id = w.cluster_of(0).unwrap();

    for _ in 0..5 {
        w.pass();
        assert_eq!(w.cluster_of(0), Some(id.clone()));
        assert_eq!(w.cluster_of(1), Some(id.clone()));
    }
}

#[test]
fn a_split_leaves_the_id_with_the_majority_and_mints_for_the_rest() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::new();
    for i in 0..4u32 {
        w.place(i, &format!("0x{i}"), "r", v3(1.0 + i as f32, 1.0));
    }
    w.place(4, "0x4", "r", v3(cell * 0.9, 1.0));
    w.pass();
    let original = w.cluster_of(0).unwrap();
    assert_eq!(w.cluster_count(), 1);

    w.place(4, "0x4", "r", v3(cell * 6.5, 1.0));
    w.pass();

    assert_eq!(w.cluster_count(), 2);
    assert_eq!(
        w.cluster_of(0),
        Some(original.clone()),
        "the majority keeps the ID"
    );
    assert_ne!(w.cluster_of(4), Some(original));
}

#[test]
fn a_merge_leaves_the_id_with_the_older_cluster_on_an_overlap_tie() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.pass();
    let older = w.cluster_of(0).unwrap();

    w.place(1, "0xb", "r", v3(cell * 8.5, 1.0));
    w.pass();
    let newer = w.cluster_of(1).unwrap();
    assert_ne!(older, newer);

    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.pass();

    assert_eq!(w.cluster_count(), 1);
    assert_eq!(
        w.cluster_of(0),
        Some(older),
        "equal overlap breaks to the older creation sequence"
    );
}

#[test]
fn a_new_id_is_minted_per_cluster_and_carries_the_configured_prefix() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::with(ClusterOptions {
        dwell_passes: 1,
        id_prefix: "pulse-".into(),
        ..Default::default()
    });
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(cell * 8.5, 1.0));
    w.pass();

    let mut ids: Vec<String> = vec![w.cluster_of(0).unwrap(), w.cluster_of(1).unwrap()];
    ids.sort();
    assert_eq!(ids, vec!["pulse-1".to_string(), "pulse-2".to_string()]);
}

#[test]
fn an_assignment_is_published_once_and_not_republished_while_it_holds() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.pass();
    assert_eq!(w.feed.changes().len(), 1);

    w.pass();
    w.pass();
    assert_eq!(
        w.feed.changes().len(),
        1,
        "an unchanged assignment is silent"
    );
}

#[test]
fn a_move_between_clusters_waits_out_the_dwell_before_publishing() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::with(ClusterOptions::default());
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.place(2, "0xc", "r", v3(cell * 8.5, 1.0));
    w.place(3, "0xd", "r", v3(cell * 8.6, 1.0));
    w.pass();
    assert_eq!(w.feed.changes().len(), 4, "first assignments bypass dwell");
    w.feed.clear();

    w.place(1, "0xb", "r", v3(cell * 8.7, 1.0));
    w.pass();
    assert!(w.feed.changes().is_empty(), "pass 1 of the dwell is silent");
    w.pass();
    assert!(w.feed.changes().is_empty(), "pass 2 of the dwell is silent");
    w.pass();
    assert_eq!(
        w.feed.changes().len(),
        1,
        "the third agreeing pass publishes"
    );
    assert_eq!(w.feed.changes()[0].wallet, "0xb");
}

#[test]
fn a_candidate_that_stops_repeating_restarts_the_dwell() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::with(ClusterOptions::default());
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.place(2, "0xc", "r", v3(cell * 8.5, 1.0));
    w.place(3, "0xd", "r", v3(cell * 8.6, 1.0));
    w.pass();
    w.feed.clear();

    w.place(1, "0xb", "r", v3(cell * 8.7, 1.0));
    w.pass();
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.pass();
    w.place(1, "0xb", "r", v3(cell * 8.7, 1.0));
    w.pass();
    w.pass();
    assert!(
        w.feed.changes().is_empty(),
        "the streak restarted, so three agreeing passes have not elapsed"
    );
    w.pass();
    assert_eq!(w.feed.changes().len(), 1);
}

#[test]
fn a_teleport_bypasses_the_dwell() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::with(ClusterOptions::default());
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.place(2, "0xc", "r", v3(cell * 8.5, 1.0));
    w.place(3, "0xd", "r", v3(cell * 8.6, 1.0));
    w.pass();
    w.feed.clear();

    w.place_session(1, "0xb", "0xb", "r", v3(cell * 8.7, 1.0), true);
    w.pass();
    assert_eq!(w.feed.changes().len(), 1, "a teleport publishes at once");
}

#[test]
fn a_realm_change_bypasses_the_dwell_and_is_published_even_at_the_same_cluster_id() {
    let mut w = World::with(ClusterOptions::default());
    w.place(0, "0xa", "realm-a", v3(1.0, 1.0));
    w.pass();
    let first = w.feed.changes();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].realm, "realm-a");
    w.feed.clear();

    w.place(0, "0xa", "realm-b", v3(1.0, 1.0));
    w.pass();
    let second = w.feed.changes();
    assert_eq!(second.len(), 1, "a realm change publishes at once");
    assert_eq!(second[0].realm, "realm-b");
}

#[test]
fn a_second_session_on_one_wallet_names_what_it_displaced() {
    let mut w = World::new();
    w.place_session(0, "0xA", "0xdevice-one", "r", v3(1.0, 1.0), false);
    w.pass();
    let first = w.feed.changes();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].session.session, "0xdevice-one");
    assert_eq!(first[0].session.displaced_session, None);
    let displaced_cluster = first[0].cluster_id.clone();
    w.feed.clear();

    w.remove(0);
    w.place_session(1, "0xA", "0xdevice-two", "r", v3(1.0, 1.0), false);
    w.pass();

    let second = w.feed.changes();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].session.session, "0xdevice-two");
    assert_eq!(
        second[0].session.displaced_session.as_deref(),
        Some("0xdevice-one")
    );
    assert_eq!(
        second[0].session.displaced_cluster_id.as_deref(),
        Some(displaced_cluster.as_str())
    );
    assert!(second[0].session.is_takeover());
}

#[test]
fn the_same_session_returning_displaces_nothing() {
    let mut w = World::new();
    w.place_session(0, "0xA", "0xdevice-one", "r", v3(1.0, 1.0), false);
    w.pass();
    w.feed.clear();

    w.remove(0);
    w.place_session(1, "0xA", "0xdevice-one", "r", v3(1.0, 1.0), false);
    w.pass();

    let changes = w.feed.changes();
    assert_eq!(changes.len(), 1);
    assert!(
        !changes[0].session.is_takeover(),
        "one device reconnecting is not a takeover"
    );
}

#[test]
fn a_retained_assignment_expires_after_the_retention_window() {
    let mut w = World::with(ClusterOptions {
        dwell_passes: 1,
        session_retention_passes: 2,
        ..Default::default()
    });
    w.place_session(0, "0xA", "0xdevice-one", "r", v3(1.0, 1.0), false);
    w.pass();
    w.feed.clear();

    w.remove(0);
    for _ in 0..4 {
        w.pass();
    }

    w.place_session(1, "0xA", "0xdevice-two", "r", v3(1.0, 1.0), false);
    w.pass();
    let changes = w.feed.changes();
    assert_eq!(changes.len(), 1);
    assert!(
        !changes[0].session.is_takeover(),
        "past the window there is no participant left to name"
    );
}

#[test]
fn a_recycled_peer_slot_starts_the_next_wallet_from_scratch() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.pass();
    w.feed.clear();

    w.remove(0);
    w.pass();
    w.place(0, "0xzzz", "r", v3(1.0, 1.0));
    w.pass();

    let changes = w.feed.changes();
    assert_eq!(changes.len(), 1, "the new wallet on the slot is published");
    assert_eq!(changes[0].wallet, "0xzzz");
}

#[test]
fn a_peer_that_is_no_longer_its_wallet_s_live_binding_is_excluded() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xa", "r", v3(2.0, 1.0));
    w.pass();

    assert_eq!(w.cluster_of(0), None, "the evicted binding is not a member");
    assert!(w.cluster_of(1).is_some());
    assert_eq!(w.tracker.board().peers.len(), 1);
}

#[test]
fn a_peer_with_no_realm_is_in_no_cluster() {
    let mut w = World::new();
    w.board.set_active(3);
    w.board.publish(
        3,
        PeerSnapshot {
            global_position: v3(1.0, 1.0),
            ..Default::default()
        },
    );
    w.identity.set(3, "0xa".into());
    w.pass();

    assert_eq!(w.cluster_count(), 0);
    assert_eq!(w.cluster_of(3), None);
    assert!(w.feed.changes().is_empty());
}

#[test]
fn the_heartbeat_count_is_active_peers_not_clustered_peers() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.board.set_active(3);
    w.board.publish(
        3,
        PeerSnapshot {
            global_position: v3(1.0, 1.0),
            ..Default::default()
        },
    );
    w.identity.set(3, "0xb".into());
    w.pass();

    assert_eq!(w.tracker.board().peers.len(), 1, "only one peer clusters");
    assert_eq!(
        w.feed.active_peers(),
        vec![2],
        "the realm-less peer is carried by this server and counts as load"
    );
}

#[test]
fn a_full_enet_house_clusters_well_inside_the_tick() {
    const PEERS: u32 = crate::server::ENET_CAPACITY as u32;
    const REALMS: u32 = 4;

    let feed = Arc::new(RecordingClusterFeed::default());
    let options = ClusterOptions {
        dwell_passes: 1,
        ..Default::default()
    };
    let cap = PEERS as usize;
    let mut grids = RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, cap);
    let mut board = SnapshotBoard::new(cap, 4);
    let mut identity = IdentityBoard::new(cap);
    let mut tracker = ClusterTracker::new(options.clone(), cap, feed.clone());

    let side = (PEERS as f32 / REALMS as f32).sqrt().ceil();
    for peer in 0..PEERS {
        let realm = format!("realm-{}", peer % REALMS);
        let index = (peer / REALMS) as f32;
        let position = v3(
            (index % side) * SPATIAL_GRID_CELL_SIZE * 0.5,
            (index / side).floor() * SPATIAL_GRID_CELL_SIZE * 0.5,
        );
        board.set_active(peer);
        board.publish(
            peer,
            PeerSnapshot {
                global_position: position,
                realm: Some(realm.as_str().into()),
                ..Default::default()
            },
        );
        identity.set_with_session(peer, format!("0x{peer}"), format!("0x{peer}"));
        grids.set(peer, &realm, position);
    }

    tracker.run_pass(&grids, &board, &identity);
    let started = std::time::Instant::now();
    tracker.run_pass(&grids, &board, &identity);
    let encoded = crate::cluster::feed::encode_topology(tracker.board());
    let elapsed = started.elapsed();

    assert_eq!(tracker.board().peers.len(), PEERS as usize);
    assert!(!encoded.is_empty());
    let budget =
        std::time::Duration::from_millis(options.pass_interval_ms / TIMING_ASSERT_BUDGET_DIVISOR);
    assert!(
        elapsed < budget,
        "a pass over {PEERS} peers took {elapsed:?}, past its {budget:?} share of the tick"
    );
}

#[test]
fn a_departed_peer_leaves_the_cluster_and_the_emptied_cluster_vanishes() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.pass();
    assert_eq!(w.tracker.board().clusters[0].count, 2);

    w.remove(1);
    w.pass();
    assert_eq!(w.tracker.board().clusters[0].count, 1);

    w.remove(0);
    w.pass();
    assert_eq!(w.cluster_count(), 0);
    assert!(w.tracker.board().peers.is_empty());
}

#[test]
fn the_topology_feed_carries_every_pass_and_its_members() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(1.0, 1.0));
    w.place(1, "0xb", "r", v3(2.0, 1.0));
    w.pass();
    w.pass();

    let topologies = w.feed.topologies();
    assert_eq!(topologies.len(), 2, "topology goes out every pass");
    assert_eq!(topologies[1].len(), 1);
    assert_eq!(topologies[1][0].1, 2);
}

#[test]
fn centroid_and_radius_describe_the_members() {
    let mut w = World::new();
    w.place(0, "0xa", "r", v3(0.0, 0.0));
    w.place(1, "0xb", "r", v3(4.0, 0.0));
    w.pass();

    let cluster = &w.tracker.board().clusters[0];
    assert!((cluster.centroid.x - 2.0).abs() < 1e-3);
    assert!((cluster.centroid.z - 0.0).abs() < 1e-3);
    assert!((cluster.radius - 2.0).abs() < 1e-3);
}

#[test]
fn the_encoded_change_carries_the_displaced_fields_as_empty_strings_when_absent() {
    let plain = encode_cluster_change("C1", "r", &ClusterSession::new("0xs".into()));
    let decoded = <crate::PeerClusterChange as prost::Message>::decode(plain.as_slice()).unwrap();
    assert_eq!(decoded.cluster_id, "C1");
    assert_eq!(decoded.realm, "r");
    assert_eq!(decoded.session, "0xs");
    assert_eq!(decoded.displaced_session, "");
    assert_eq!(decoded.displaced_cluster_id, "");

    let takeover = encode_cluster_change(
        "C2",
        "r",
        &ClusterSession {
            session: "0xnew".into(),
            displaced_session: Some("0xold".into()),
            displaced_cluster_id: Some("C1".into()),
        },
    );
    let decoded =
        <crate::PeerClusterChange as prost::Message>::decode(takeover.as_slice()).unwrap();
    assert_eq!(decoded.displaced_session, "0xold");
    assert_eq!(decoded.displaced_cluster_id, "C1");
}

#[test]
fn the_change_subject_is_the_lower_cased_wallet() {
    assert_eq!(
        feed::cluster_change_subject("0xAbCd"),
        "peer.0xabcd.cluster_change"
    );
}

#[test]
fn clusters_scale_past_the_pair_count() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let mut w = World::with(ClusterOptions {
        dwell_passes: 1,
        ..Default::default()
    });
    for i in 0..CAP as u32 {
        let x = (i % 4) as f32 * cell * 4.0 + 1.0;
        let z = (i / 4) as f32;
        w.place(i, &format!("0x{i}"), "r", v3(x, z));
    }
    w.pass();

    assert_eq!(w.cluster_count(), 4, "four well-separated columns");
    let total: usize = w.tracker.board().clusters.iter().map(|c| c.count).sum();
    assert_eq!(total, CAP);
}

#[test]
fn two_trackers_over_one_world_mint_the_same_ids() {
    let cell = SPATIAL_GRID_CELL_SIZE;
    let build = || {
        let mut w = World::new();
        for i in 0..12u32 {
            let x = (i % 3) as f32 * cell * 5.0 + 1.0;
            let z = (i / 3) as f32;
            w.place(i, &format!("0x{i}"), "r", v3(x, z));
        }
        w.place(20, "0xz", "other-realm", v3(1.0, 1.0));
        w.pass();
        (0..12u32)
            .map(|i| w.cluster_of(i).unwrap())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        build(),
        build(),
        "cell and realm iteration order is pinned, so minting is reproducible"
    );
}
