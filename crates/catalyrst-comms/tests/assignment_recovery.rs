#![cfg(feature = "nats")]

use async_trait::async_trait;
use catalyrst_comms::{
    cluster_subscriber::{ClusterGateway, ClusterSubscriber, GatewayError},
    config::ClusterConfig,
    livekit::Removal,
    nats::{BrokerBus, NatsBus},
    peer_state::ClusterPeerState,
};
use catalyrst_pulse::{
    cluster::nats::{NatsClusterFeed, NatsFeedOptions},
    cluster::{ClusterOptions, ClusterTracker},
    decentraland::{
        common::Vector3, kernel::comms::v3::IslandChangedMessage, pulse::PeerClusterSnapshot,
    },
    interest::SPATIAL_GRID_CELL_SIZE,
    realm_grids::RealmSpatialGrids,
    snapshot::{IdentityBoard, PeerSnapshot, SnapshotBoard},
};
use futures::StreamExt;
use prost::Message;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[path = "support/nats.rs"]
mod support;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct World {
    grids: RealmSpatialGrids,
    board: SnapshotBoard,
    identity: IdentityBoard,
    tracker: ClusterTracker,
}

impl World {
    fn new(feed: Arc<NatsClusterFeed>) -> Self {
        let mut world = Self {
            grids: RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, 8),
            board: SnapshotBoard::new(8, 4),
            identity: IdentityBoard::new(8),
            tracker: ClusterTracker::new(ClusterOptions::default(), 8, feed),
        };
        for peer in 0..3 {
            let position = Vector3 {
                x: 1.0,
                y: 0.0,
                z: 1.0,
            };
            world.board.set_active(peer);
            world.board.publish(
                peer,
                PeerSnapshot {
                    global_position: position,
                    realm: Some("fixture".into()),
                    ..Default::default()
                },
            );
            let (wallet, session) = if peer == 0 {
                (WALLET.into(), SESSION.into())
            } else {
                (format!("0x{peer:040x}"), format!("0x{:040x}", peer + 100))
            };
            world.identity.set_with_session(peer, wallet, session);
            world.grids.set(peer, "fixture", position);
        }
        world
    }

    fn pass(&mut self) {
        self.tracker
            .run_pass(&self.grids, &self.board, &self.identity);
    }
}

#[derive(Default)]
struct Gateway {
    minted: AtomicUsize,
    removals: AtomicUsize,
    mint_entered: tokio::sync::Notify,
    mint_pause: Mutex<Option<Arc<tokio::sync::Notify>>>,
}

#[async_trait]
impl ClusterGateway for Gateway {
    async fn is_denied(&self, _: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }
    async fn mint_connection_string(
        &self,
        _: &str,
        _: Option<&str>,
        room: &str,
        _: u64,
        _: Option<u64>,
    ) -> Result<String, GatewayError> {
        self.minted.fetch_add(1, Ordering::SeqCst);
        self.mint_entered.notify_one();
        let pause = self.mint_pause.lock().unwrap().clone();
        if let Some(pause) = pause {
            pause.notified().await;
        }
        Ok(format!("livekit:wss://fixture?access_token=fixture-{room}"))
    }
    async fn remove_participant(
        &self,
        _: &str,
        _: &str,
        _: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        self.removals.fetch_add(1, Ordering::SeqCst);
        Ok(Removal::Absent)
    }
    async fn holds_participant(&self, _: &str, _: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }
}

async fn start(url: String, gateway: Arc<Gateway>) -> (Arc<ClusterSubscriber>, Arc<BrokerBus>) {
    let bus = Arc::new(BrokerBus::new(Some(url), "comms-source-recovery"));
    let subscriber = ClusterSubscriber::new(
        bus.clone(),
        gateway,
        Arc::new(ClusterPeerState::default()),
        ClusterConfig {
            enabled: true,
            ..Default::default()
        },
    );
    subscriber.start();
    ready(&bus).await;
    (subscriber, bus)
}

async fn ready(bus: &BrokerBus) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !bus.is_ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn source(client: &async_nats::Client) -> PeerClusterSnapshot {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok(Ok(reply)) = tokio::time::timeout(
                Duration::from_millis(200),
                client.request(
                    format!("peer.{WALLET}.cluster_lookup"),
                    SESSION.as_bytes().to_vec().into(),
                ),
            )
            .await
            {
                return PeerClusterSnapshot::decode(reply.payload).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

async fn assignment(
    client: &async_nats::Client,
    deliveries: &mut async_nats::Subscriber,
) -> IslandChangedMessage {
    client
        .publish(
            format!("peer.{WALLET}.connect"),
            SESSION.as_bytes().to_vec().into(),
        )
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(5), deliveries.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        message.subject.as_str(),
        format!("engine.peer.{WALLET}.island_changed.{SESSION}")
    );
    IslandChangedMessage::decode(message.payload).unwrap()
}

#[tokio::test]
async fn pulse_to_comms_recovers_after_event_loss_and_cold_mirror_across_broker_restart() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = support::Broker::start(&binary, None);
    let port = broker.port;
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut changes = client.subscribe("peer.*.cluster_change").await.unwrap();
    let mut deliveries = client
        .subscribe(format!("engine.peer.{WALLET}.island_changed.*"))
        .await
        .unwrap();
    support::round_trip(&client).await;
    let feed = Arc::new(NatsClusterFeed::spawn(NatsFeedOptions {
        url: broker.url(),
        capacity: 1,
        discovery_interval_ms: 0,
        ..Default::default()
    }));
    let mut world = World::new(feed.clone());
    world.pass();
    assert_eq!(
        feed.pending_changes(),
        1,
        "two edges are evicted before the publisher runs"
    );
    let world = Arc::new(Mutex::new(world));
    let current = world.clone();
    let passes = tokio::spawn(async move {
        loop {
            current.lock().unwrap().pass();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    let original = source(&client).await;
    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changes.next())
            .await
            .is_err()
    );
    let gateway = Arc::new(Gateway::default());
    let (subscriber, bus) = start(broker.url(), gateway.clone()).await;
    assert!(subscriber.peers().mirrored(WALLET).is_none());
    assert_eq!(
        assignment(&client, &mut deliveries).await.island_id,
        format!("island-{}", original.cluster_id)
    );
    subscriber.settle().await;
    assert_eq!(gateway.minted.load(Ordering::SeqCst), 1);
    subscriber.stop().await;
    drop(subscriber);
    drop(bus);

    let (subscriber, bus) = start(broker.url(), gateway.clone()).await;
    assert!(subscriber.peers().mirrored(WALLET).is_none());
    drop(broker);
    tokio::time::timeout(Duration::from_secs(5), async {
        while bus.is_ready() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let _broker = support::Broker::start(&binary, Some(port));
    ready(&bus).await;
    let after = source(&client).await;
    assert_eq!(after.source_incarnation, original.source_incarnation);
    support::round_trip(&client).await;
    assert_eq!(
        assignment(&client, &mut deliveries).await.island_id,
        format!("island-{}", after.cluster_id)
    );
    subscriber.settle().await;
    assert_eq!(gateway.minted.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.removals.load(Ordering::SeqCst), 0);
    assert!(
        subscriber.peers().mirrored(WALLET).is_none(),
        "no replacement cluster event was needed"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), changes.next())
            .await
            .is_err()
    );
    subscriber.stop().await;
    passes.abort();
    assert!(passes.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn a_real_source_owner_change_during_mint_is_rejected_even_without_the_change_subscription() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = support::Broker::start(&binary, None);
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut deliveries = client
        .subscribe(format!("engine.peer.{WALLET}.island_changed.*"))
        .await
        .unwrap();
    support::round_trip(&client).await;
    let feed = Arc::new(NatsClusterFeed::spawn(NatsFeedOptions {
        url: broker.url(),
        discovery_interval_ms: 0,
        ..Default::default()
    }));
    let world = Arc::new(Mutex::new(World::new(feed)));
    let current = world.clone();
    let passes = tokio::spawn(async move {
        loop {
            current.lock().unwrap().pass();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    source(&client).await;
    let bus = Arc::new(BrokerBus::new(Some(broker.url()), "comms-held-mint"));
    bus.connect();
    ready(&bus).await;
    let gateway = Arc::new(Gateway::default());
    let release = Arc::new(tokio::sync::Notify::new());
    *gateway.mint_pause.lock().unwrap() = Some(release.clone());
    let subscriber = ClusterSubscriber::new(
        bus,
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        ClusterConfig {
            enabled: true,
            ..Default::default()
        },
    );
    let recovering = {
        let subscriber = subscriber.clone();
        tokio::spawn(async move {
            subscriber.process_peer_connect(WALLET, SESSION).await;
        })
    };
    tokio::time::timeout(Duration::from_secs(5), gateway.mint_entered.notified())
        .await
        .unwrap();
    let replacement = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    {
        let mut world = world.lock().unwrap();
        world
            .identity
            .set_with_session(0, WALLET.into(), replacement.into());
        world.pass();
    }
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), recovering)
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), deliveries.next())
            .await
            .is_err()
    );
    assert!(subscriber.peers().assignment(WALLET).is_none());
    *gateway.mint_pause.lock().unwrap() = None;
    subscriber.process_peer_connect(WALLET, replacement).await;
    let message = tokio::time::timeout(Duration::from_secs(5), deliveries.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        message.subject.as_str(),
        format!("engine.peer.{WALLET}.island_changed.{replacement}")
    );
    assert_eq!(gateway.minted.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.removals.load(Ordering::SeqCst), 0);
    passes.abort();
    assert!(passes.await.unwrap_err().is_cancelled());
}
