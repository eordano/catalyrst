#![cfg(feature = "nats")]

use async_trait::async_trait;
use catalyrst_comms::{
    cluster_gateway::{island_occupant, mint_island_connection_string},
    cluster_subscriber::{ClusterGateway, ClusterSubscriber, GatewayError, IslandOccupant},
    config::ClusterConfig,
    livekit::{join_grants, AccessToken, Removal, RoomServiceClient},
    nats::{BrokerBus, InProcessBus, NatsBus, RequestError},
    peer_state::ClusterPeerState,
};
use catalyrst_pulse::{
    cluster::nats::{NatsClusterFeed, NatsFeedOptions},
    cluster::{ClusterOptions, ClusterTracker},
    decentraland::{
        common::Vector3,
        kernel::comms::v3::IslandChangedMessage,
        pulse::{PeerClusterChange, PeerClusterSnapshot},
    },
    interest::SPATIAL_GRID_CELL_SIZE,
    realm_grids::RealmSpatialGrids,
    snapshot::{IdentityBoard, PeerSnapshot, SnapshotBoard},
};
use futures::StreamExt;
use prost::Message as _;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[path = "support/livekit.rs"]
mod support;

#[path = "support/nats.rs"]
mod nats_support;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SESSION_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SESSION_C: &str = "0xcccccccccccccccccccccccccccccccccccccccc";
const ROOM: &str = "island-ownership-fixture";

struct Gateway {
    server: support::Server,
    http: reqwest::Client,
    minted: AtomicUsize,
    removed: AtomicUsize,
    inspected: AtomicUsize,
}

impl Gateway {
    fn service(&self) -> RoomServiceClient<'_> {
        RoomServiceClient::new(
            &self.http,
            &self.server.url,
            support::KEY,
            &self.server.secret,
        )
    }
    fn mint(&self, session: &str) -> String {
        self.mint_with_ttl(session, 60)
    }
    fn mint_with_ttl(&self, session: &str, ttl_seconds: u64) -> String {
        mint_island_connection_string(
            support::KEY,
            &self.server.secret,
            &self.server.ws_url(),
            WALLET,
            Some(session),
            ROOM,
            ttl_seconds,
            None,
        )
        .unwrap()
    }
}

#[async_trait]
impl ClusterGateway for Gateway {
    async fn is_denied(&self, _: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }
    async fn mint_connection_string(
        &self,
        wallet: &str,
        session: Option<&str>,
        room: &str,
        ttl: u64,
        boundary: Option<u64>,
    ) -> Result<String, GatewayError> {
        self.minted.fetch_add(1, Ordering::SeqCst);
        mint_island_connection_string(
            support::KEY,
            &self.server.secret,
            &self.server.ws_url(),
            wallet,
            session,
            room,
            ttl,
            boundary,
        )
    }
    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        boundary: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        self.removed.fetch_add(1, Ordering::SeqCst);
        self.service()
            .remove_participant_with_cutoff(room, identity, boundary)
            .await
            .map_err(|_| GatewayError::new("fixture removal"))
    }
    async fn holds_participant(&self, room: &str, wallet: &str) -> Result<bool, GatewayError> {
        self.service()
            .holds_participant(room, wallet)
            .await
            .map_err(|_| GatewayError::new("fixture occupancy"))
    }
    async fn island_occupant(
        &self,
        room: &str,
        wallet: &str,
    ) -> Result<IslandOccupant, GatewayError> {
        self.inspected.fetch_add(1, Ordering::SeqCst);
        island_occupant(&self.service(), room, wallet).await
    }
}

async fn fixture(binary: &str) -> Arc<Gateway> {
    Arc::new(Gateway {
        server: support::Server::start(binary).await,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        minted: AtomicUsize::new(0),
        removed: AtomicUsize::new(0),
        inspected: AtomicUsize::new(0),
    })
}

async fn wait_owner(gateway: &Gateway, session: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if gateway.island_occupant(ROOM, WALLET).await.unwrap()
                == IslandOccupant::Session(session.into())
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("participant store must expose immutable session owner");
}

#[tokio::test]
async fn a_missed_same_room_takeover_recovers_without_blind_removal_and_leaves_the_current_owner_alone(
) {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_LIVEKIT_SERVER_BIN") else {
        return;
    };
    let Some(nats_binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = nats_support::Broker::start(&nats_binary, None);
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut changes = client
        .subscribe(format!("peer.{WALLET}.cluster_change"))
        .await
        .unwrap();
    let mut deliveries = client
        .subscribe(format!("engine.peer.{WALLET}.island_changed.{SESSION_B}"))
        .await
        .unwrap();
    nats_support::round_trip(&client).await;
    let gateway = fixture(&binary).await;
    let feed = Arc::new(NatsClusterFeed::spawn(NatsFeedOptions {
        url: broker.url(),
        discovery_interval_ms: 0,
        ..Default::default()
    }));
    let mut tracker = ClusterTracker::new(ClusterOptions::default(), 1, feed);
    let mut grids = RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, 1);
    let mut board = SnapshotBoard::new(1, 4);
    let mut identity = IdentityBoard::new(1);
    let position = Vector3 {
        x: 1.0,
        y: 0.0,
        z: 1.0,
    };
    board.set_active(0);
    board.publish(
        0,
        PeerSnapshot {
            global_position: position,
            realm: Some("fixture".into()),
            ..Default::default()
        },
    );
    identity.set_with_session(0, WALLET.into(), SESSION_B.into());
    grids.set(0, "fixture", position);
    tracker.run_pass(&grids, &board, &identity);
    let room = format!("island-{}", tracker.board().cluster_id(0).unwrap());
    let passes = tokio::spawn(async move {
        loop {
            tracker.run_pass(&grids, &board, &identity);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    let old_adapter = mint_island_connection_string(
        support::KEY,
        &gateway.server.secret,
        &gateway.server.ws_url(),
        WALLET,
        Some(SESSION_A),
        &room,
        60,
        None,
    )
    .unwrap();
    let (mut old_socket, old) = support::join(&gateway.server, &old_adapter).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while gateway.island_occupant(&room, WALLET).await.unwrap()
            != IslandOccupant::Session(SESSION_A.into())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let bus = Arc::new(BrokerBus::new(Some(broker.url()), "comms-live-sfu"));
    let subscriber = ClusterSubscriber::new(
        bus.clone(),
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        ClusterConfig {
            enabled: true,
            ..Default::default()
        },
    );
    subscriber.start();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !bus.is_ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        subscriber.peers().mirrored(WALLET).is_none(),
        "the cluster event was lost before comms started"
    );
    client
        .publish(
            format!("peer.{WALLET}.connect"),
            SESSION_B.as_bytes().to_vec().into(),
        )
        .await
        .unwrap();
    let published = tokio::time::timeout(Duration::from_secs(5), deliveries.next())
        .await
        .unwrap()
        .unwrap();
    subscriber.settle().await;
    assert_eq!(gateway.removed.load(Ordering::SeqCst), 0);
    let assignment = IslandChangedMessage::decode(published.payload).unwrap();
    assert_eq!(assignment.island_id, room);
    let (_new_socket, new) = support::join(&gateway.server, &assignment.conn_str).await;
    assert_ne!(old.sid, new.sid);
    assert_eq!(
        support::leave_reason(&mut old_socket).await,
        2,
        "duplicate identity"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while gateway.island_occupant(&room, WALLET).await.unwrap()
            != IslandOccupant::Session(SESSION_B.into())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let inspected = gateway.inspected.load(Ordering::SeqCst);
    client
        .publish(
            format!("peer.{WALLET}.connect"),
            SESSION_B.as_bytes().to_vec().into(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while gateway.inspected.load(Ordering::SeqCst) == inspected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    subscriber.settle().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), deliveries.next())
            .await
            .is_err()
    );
    assert_eq!(gateway.minted.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.removed.load(Ordering::SeqCst), 0);
    subscriber.stop().await;
    passes.abort();
    assert!(passes.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn island_ownership_metadata_is_readable_but_not_client_writable() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_LIVEKIT_SERVER_BIN") else {
        return;
    };
    let gateway = fixture(&binary).await;
    let (mut socket, original) = support::join(&gateway.server, &gateway.mint(SESSION_A)).await;
    wait_owner(&gateway, SESSION_A).await;
    let response = support::metadata_update(&mut socket, "forged", 1).await;
    assert_eq!(response["reason"], "NOT_ALLOWED");
    assert_eq!(
        gateway
            .service()
            .get_participant(ROOM, WALLET)
            .await
            .unwrap()
            .unwrap()
            .metadata
            .as_deref(),
        Some(original.metadata.as_str())
    );
    let legacy = AccessToken::new(
        support::KEY,
        &gateway.server.secret,
        WALLET,
        join_grants("island-legacy"),
    )
    .with_metadata("before")
    .to_jwt()
    .unwrap();
    let (mut writable, _) = support::join(
        &gateway.server,
        &format!("livekit:{}?access_token={legacy}", gateway.server.ws_url()),
    )
    .await;
    let response = support::metadata_update(&mut writable, &original.metadata, 2).await;
    assert!(
        response.get("reason").is_none() || response["reason"] == "OK",
        "positive control accepts metadata"
    );
    assert_eq!(
        gateway
            .island_occupant("island-legacy", WALLET)
            .await
            .unwrap(),
        IslandOccupant::Unattributed,
        "a writable copy of authentic ownership metadata is not authority"
    );
}

#[tokio::test]
async fn self_hosted_takeover_waits_out_the_old_token_before_publishing_the_replacement() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_LIVEKIT_SERVER_BIN") else {
        return;
    };
    let gateway = fixture(&binary).await;
    let adapter = gateway.mint_with_ttl(SESSION_A, 1);
    let (mut socket, _) = support::join(&gateway.server, &adapter).await;
    wait_owner(&gateway, SESSION_A).await;
    let bus = Arc::new(InProcessBus::new());
    let source = Arc::new(Mutex::new(PeerClusterSnapshot {
        wallet: WALLET.into(),
        session: SESSION_B.into(),
        cluster_id: "ownership-fixture".into(),
        realm: "main".into(),
        pass: 1,
        source_incarnation: "livekit-expiry-fixture".into(),
    }));
    let replies = source.clone();
    bus.respond_to_requests(Arc::new(move |_, payload| {
        let snapshot = replies.lock().unwrap();
        if snapshot.session.as_bytes() != payload {
            return Err(RequestError::TimedOut);
        }
        Ok(snapshot.encode_to_vec())
    }));
    let subscriber = ClusterSubscriber::new(
        bus.clone(),
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        ClusterConfig {
            enabled: true,
            island_token_ttl_seconds: 1,
            self_hosted_token_quarantine: true,
            ..Default::default()
        },
    );
    subscriber.start();
    let change = PeerClusterChange {
        cluster_id: "ownership-fixture".into(),
        realm: "main".into(),
        session: SESSION_B.into(),
        displaced_session: SESSION_A.into(),
        displaced_cluster_id: "ownership-fixture".into(),
    };
    bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change.encode_to_vec(),
    );
    assert_eq!(
        support::leave_reason(&mut socket).await,
        4,
        "participant removed"
    );
    *source.lock().unwrap() = PeerClusterSnapshot {
        wallet: WALLET.into(),
        session: SESSION_C.into(),
        cluster_id: "ownership-fixture".into(),
        realm: "main".into(),
        pass: 2,
        source_incarnation: "livekit-expiry-fixture".into(),
    };
    let successor = PeerClusterChange {
        cluster_id: "ownership-fixture".into(),
        realm: "main".into(),
        session: SESSION_C.into(),
        displaced_session: SESSION_B.into(),
        displaced_cluster_id: "ownership-fixture".into(),
    };
    bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &successor.encode_to_vec(),
    );
    subscriber.settle().await;
    assert_eq!(gateway.removed.load(Ordering::SeqCst), 3);
    let assignments = bus
        .published()
        .into_iter()
        .filter(|(subject, _)| subject.starts_with("engine.peer."))
        .map(|(subject, payload)| {
            (
                subject,
                IslandChangedMessage::decode(payload.as_slice()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(assignments.len(), 1);
    assert!(assignments[0].0.ends_with(SESSION_C));
    let assignment = &assignments[0].1;
    let (_newcomer, _) = support::join(&gateway.server, &assignment.conn_str).await;
    wait_owner(&gateway, SESSION_C).await;
    assert!(
        support::try_join(&gateway.server, &adapter).await.is_err(),
        "an old token must be expired before its replacement is admitted"
    );
    wait_owner(&gateway, SESSION_C).await;
}
