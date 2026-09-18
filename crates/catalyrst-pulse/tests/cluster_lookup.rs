#![cfg(feature = "nats")]

use catalyrst_pulse::{
    cluster::nats::{NatsClusterFeed, NatsFeedOptions},
    cluster::{ClusterOptions, ClusterTracker},
    decentraland::{common::Vector3, pulse::PeerClusterSnapshot},
    interest::SPATIAL_GRID_CELL_SIZE,
    realm_grids::RealmSpatialGrids,
    snapshot::{IdentityBoard, PeerSnapshot, SnapshotBoard},
};
use catalyrst_testgate::nats::Broker;
use futures_util::StreamExt;
use prost::Message;
use std::{sync::Arc, time::Duration};

const CAP: usize = 8;

fn address(number: usize) -> String {
    format!("0x{number:040x}")
}

struct World {
    grids: RealmSpatialGrids,
    board: SnapshotBoard,
    identity: IdentityBoard,
    tracker: ClusterTracker,
    feed: Arc<NatsClusterFeed>,
}

impl World {
    fn new(url: String) -> Self {
        let feed = Arc::new(NatsClusterFeed::spawn(NatsFeedOptions {
            url,
            capacity: 1,
            discovery_interval_ms: 0,
            ..Default::default()
        }));
        Self {
            grids: RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, CAP),
            board: SnapshotBoard::new(CAP, 4),
            identity: IdentityBoard::new(CAP),
            tracker: ClusterTracker::new(ClusterOptions::default(), CAP, feed.clone()),
            feed,
        }
    }

    fn place(&mut self, peer: u32, wallet: &str, session: &str) {
        let position = Vector3 {
            x: 1.0,
            y: 0.0,
            z: 1.0,
        };
        self.board.set_active(peer);
        self.board.publish(
            peer,
            PeerSnapshot {
                global_position: position,
                realm: Some("test-realm".into()),
                ..Default::default()
            },
        );
        self.identity
            .set_with_session(peer, wallet.into(), session.into());
        self.grids.set(peer, "test-realm", position);
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
}

async fn lookup(
    client: &async_nats::Client,
    wallet: &str,
    session: &str,
) -> Option<PeerClusterSnapshot> {
    let response = tokio::time::timeout(
        Duration::from_millis(200),
        client.request(
            format!("peer.{wallet}.cluster_lookup"),
            session.as_bytes().to_vec().into(),
        ),
    )
    .await
    .ok()?
    .ok()?;
    Some(PeerClusterSnapshot::decode(response.payload).expect("valid snapshot wire reply"))
}

async fn current(
    client: &async_nats::Client,
    world: &mut World,
    wallet: &str,
    session: &str,
) -> PeerClusterSnapshot {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            world.pass();
            if let Some(snapshot) = lookup(client, wallet, session).await {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("current authoritative assignment must become queryable")
}

async fn round_trip(client: &async_nats::Client) {
    let inbox = client.new_inbox();
    let mut replies = client.subscribe(inbox.clone()).await.unwrap();
    client.publish(inbox, "ready".into()).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), replies.next())
            .await
            .unwrap()
            .unwrap()
            .payload
            .as_ref(),
        b"ready"
    );
}

#[tokio::test]
async fn current_lookup_survives_lost_edges_and_broker_restart_without_reemitting_changes() {
    let Some(binary) = catalyrst_testgate::require_env("PULSE_NATS_SERVER_BIN") else {
        return;
    };
    let broker = Broker::start(&binary, None);
    let port = broker.port;
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut changes = client.subscribe("peer.*.cluster_change").await.unwrap();
    round_trip(&client).await;
    let mut world = World::new(broker.url());
    let mut other_source = World::new(broker.url());
    other_source.pass();
    for peer in 0..3 {
        world.place(
            peer,
            &address(peer as usize + 1),
            &address(peer as usize + 101),
        );
    }
    world.pass();
    assert_eq!(
        world.feed.pending_changes(),
        1,
        "the two older edges were evicted before the publisher could run"
    );
    let first = current(&client, &mut world, &address(1), &address(101)).await;
    assert_eq!(first.wallet, address(1));
    assert_eq!(first.session, address(101));
    assert_eq!(first.realm, "test-realm");
    assert!(!first.source_incarnation.is_empty());
    assert_eq!(
        first.cluster_id,
        world.tracker.board().cluster_id(0).unwrap()
    );
    while matches!(
        tokio::time::timeout(Duration::from_millis(50), changes.next()).await,
        Ok(Some(_))
    ) {}

    for peer in 0..3 {
        let snapshot = current(
            &client,
            &mut world,
            &address(peer + 1),
            &address(peer + 101),
        )
        .await;
        assert_eq!(snapshot.source_incarnation, first.source_incarnation);
        assert!(snapshot.pass >= first.pass);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(150), changes.next())
            .await
            .is_err(),
        "read-only recovery must not remint via cluster_change"
    );

    let mut reflected = client.subscribe("test.reflection").await.unwrap();
    round_trip(&client).await;
    client
        .publish_with_reply(
            format!("peer.{}.cluster_lookup", address(1)),
            "test.reflection",
            address(101).into(),
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(150), reflected.next())
            .await
            .is_err(),
        "a lookup must not reflect into an arbitrary feed subject"
    );

    drop(broker);
    world.pass();
    let broker = Broker::start(&binary, Some(port));
    let reader = async_nats::connect(broker.url()).await.unwrap();
    let recovered = current(&reader, &mut world, &address(1), &address(101)).await;
    assert_eq!(
        recovered.source_incarnation, first.source_incarnation,
        "broker reconnect is not a source restart"
    );
    assert_eq!(recovered.cluster_id, first.cluster_id);

    world.remove(0);
    world.place(0, &address(1), &address(201));
    world.pass();
    assert!(lookup(&reader, &address(1), &address(101)).await.is_none());
    assert_eq!(
        current(&reader, &mut world, &address(1), &address(201))
            .await
            .session,
        address(201)
    );
    world.remove(0);
    world.pass();
    assert!(
        lookup(&reader, &address(1), &address(201)).await.is_none(),
        "retained takeover history is not live membership"
    );

    drop(world);
    let mut replacement = World::new(broker.url());
    replacement.place(0, &address(1), &address(201));
    let restarted = current(&reader, &mut replacement, &address(1), &address(201)).await;
    assert_ne!(
        restarted.source_incarnation, first.source_incarnation,
        "a new feed owner cannot reuse the old ordering scope"
    );
}

#[tokio::test]
async fn a_stopped_tracker_cannot_serve_an_indefinitely_fresh_assignment() {
    let Some(binary) = catalyrst_testgate::require_env("PULSE_NATS_SERVER_BIN") else {
        return;
    };
    let broker = Broker::start(&binary, None);
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut world = World::new(broker.url());
    world.place(0, &address(1), &address(101));
    let before = current(&client, &mut world, &address(1), &address(101)).await;
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert!(lookup(&client, &address(1), &address(101)).await.is_none());
    let after = current(&client, &mut world, &address(1), &address(101)).await;
    assert!(after.pass > before.pass);
    assert_eq!(after.source_incarnation, before.source_incarnation);
}
