use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use catalyrst_comms::assignment_fence::{AssignmentFence, TokenFence};
use catalyrst_comms::cluster_subscriber::{
    ClusterGateway, ClusterSubscriber, GatewayError, IslandOccupant,
};
use catalyrst_comms::config::ClusterConfig;
use catalyrst_comms::livekit::Removal;
use catalyrst_comms::nats::{InProcessBus, RequestError};
use catalyrst_comms::peer_state::ClusterPeerState;
use catalyrst_pulse::decentraland::pulse::PeerClusterSnapshot;
use prost::Message;
use tokio::sync::Notify;

use super::{row_state, seed_realm, Fixture, AUDIENCE, WALLET};

const SESSION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const STALE_SESSION: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(Default)]
struct RecordingGateway {
    fences: Mutex<Vec<TokenFence>>,
    occupant: Mutex<Option<IslandOccupant>>,
    mint_pause: Mutex<Option<Arc<Notify>>>,
    mint_entered: Notify,
}

#[async_trait]
impl ClusterGateway for RecordingGateway {
    async fn is_denied(&self, _: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }

    async fn mint_connection_string(
        &self,
        _: &str,
        _: Option<&str>,
        _: &str,
        _: u64,
        _: Option<u64>,
    ) -> Result<String, GatewayError> {
        panic!("an owned realm must never mint an unfenced token")
    }

    async fn mint_fenced_connection_string(
        &self,
        wallet: &str,
        room: &str,
        _: u64,
        fence: &TokenFence,
    ) -> Result<String, GatewayError> {
        assert_eq!(wallet, WALLET);
        self.fences.lock().unwrap().push(fence.clone());
        self.mint_entered.notify_one();
        let pause = self.mint_pause.lock().unwrap().clone();
        if let Some(pause) = pause {
            pause.notified().await;
        }
        Ok(format!("livekit:wss://example.invalid/{room}"))
    }

    async fn remove_participant(
        &self,
        _: &str,
        _: &str,
        _: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        panic!("recovery must not blindly remove an existing participant")
    }

    async fn holds_participant(&self, _: &str, _: &str) -> Result<bool, GatewayError> {
        Ok(true)
    }

    async fn island_occupant(&self, _: &str, _: &str) -> Result<IslandOccupant, GatewayError> {
        Ok(self
            .occupant
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| IslandOccupant::Session(SESSION.into())))
    }
}

struct RecoveryHarness {
    subscriber: Arc<ClusterSubscriber>,
    bus: Arc<InProcessBus>,
    gateway: Arc<RecordingGateway>,
    source: Arc<Mutex<PeerClusterSnapshot>>,
}

impl RecoveryHarness {
    async fn new(fixture: &Fixture, session: &str) -> Self {
        let source = Arc::new(Mutex::new(PeerClusterSnapshot {
            wallet: WALLET.into(),
            session: session.into(),
            cluster_id: "current".into(),
            realm: "realm-current".into(),
            source_incarnation: "pulse-current".into(),
            pass: 4,
        }));
        let replies = source.clone();
        let bus = Arc::new(InProcessBus::new());
        bus.respond_to_requests(Arc::new(move |subject, payload| {
            assert_eq!(subject, format!("peer.{WALLET}.cluster_lookup"));
            let snapshot = replies.lock().unwrap();
            if snapshot.session.as_bytes() != payload {
                return Err(RequestError::NoResponders);
            }
            Ok(snapshot.encode_to_vec())
        }));
        let gateway = Arc::new(RecordingGateway::default());
        let subscriber = ClusterSubscriber::new_with_assignment_fence(
            bus.clone(),
            gateway.clone(),
            Arc::new(ClusterPeerState::default()),
            ClusterConfig {
                enabled: true,
                nats_url: Some("nats://127.0.0.1:4222".into()),
                drain_timeout_ms: 0,
                ..ClusterConfig::default()
            },
            Some(Arc::new(fixture.fence().await)),
        );
        subscriber.start();
        Self {
            subscriber,
            bus,
            gateway,
            source,
        }
    }

    fn connect(&self, session: &str) {
        self.bus
            .inject(&format!("peer.{WALLET}.connect"), session.as_bytes());
    }

    async fn settle(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.subscriber.settle())
            .await
            .expect("recovery finishes within its test deadline");
        assert!(
            self.bus.published().is_empty(),
            "no legacy assignment publication"
        );
    }
}

async fn replace_control_owner(fixture: &Fixture, session: &str, owner_epoch: i64) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "UPDATE archipelago_v4_assignments
         SET owner_session = $3,
             owner_epoch = $4,
             assignment_revision = assignment_revision + 1,
             fencing_token = fencing_token + 1,
             assignment_json = NULL,
             acknowledged_revision = 0,
             updated_at = clock_timestamp()
         WHERE deployment_audience = $1
           AND owner_address = $2
           AND lane_key = 'realm'
         RETURNING assignment_revision, fencing_token",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .bind(session)
    .bind(owner_epoch)
    .fetch_one(&fixture.pool)
    .await
    .expect("replace the authenticated control owner")
}

#[tokio::test]
async fn a_stationary_reconnect_refills_the_claim_once_despite_an_old_participant() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 7, 3, 9, None).await;
    let harness = RecoveryHarness::new(&fixture, SESSION).await;

    harness.connect(SESSION);
    harness.settle().await;

    let (revision, fence, document) = row_state(&fixture.pool).await;
    assert_eq!((revision, fence), (4, 9));
    let document = document.expect("reconnect fills the claimed authority row");
    assert_eq!(document["islandId"], "island-current");
    assert_eq!(
        document["connectionString"],
        "livekit:wss://example.invalid/island-current"
    );
    assert!(document["fromIslandId"].is_null());
    let token = harness.gateway.fences.lock().unwrap()[0].clone();
    assert_eq!(token.audience, AUDIENCE);
    assert_eq!(token.lane, "realm");
    assert_eq!(token.owner_session, SESSION);
    assert_eq!(
        (
            token.owner_epoch,
            token.assignment_revision,
            token.fencing_token
        ),
        (7, 4, 9)
    );

    harness.connect(SESSION);
    harness.settle().await;
    assert_eq!(row_state(&fixture.pool).await, (4, 9, Some(document)));
    assert_eq!(harness.gateway.fences.lock().unwrap().len(), 1);
    harness.subscriber.stop().await;
    drop(harness);
    fixture.finish().await;
}

#[tokio::test]
async fn a_stale_connect_cannot_refill_a_newer_owners_claim() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 7, 3, 9, None).await;
    let harness = RecoveryHarness::new(&fixture, STALE_SESSION).await;

    harness.connect(STALE_SESSION);
    harness.settle().await;

    assert_eq!(row_state(&fixture.pool).await, (3, 9, None));
    assert!(harness.gateway.fences.lock().unwrap().is_empty());
    harness.subscriber.stop().await;
    drop(harness);
    fixture.finish().await;
}

#[tokio::test]
async fn same_room_control_handoffs_refill_same_and_different_session_claims() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        1,
        1,
        1,
        Some(serde_json::json!({
            "islandId": "island-current",
            "connectionString": "livekit:wss://example.invalid/v4-old",
            "tokenExpiresAtUnix": 1
        })),
    )
    .await;
    let harness = RecoveryHarness::new(&fixture, SESSION).await;

    assert_eq!(replace_control_owner(&fixture, SESSION, 2).await, (2, 2));
    harness.connect(SESSION);
    harness.settle().await;
    let (revision, fence, same_session_assignment) = row_state(&fixture.pool).await;
    assert_eq!((revision, fence), (3, 2));
    assert_eq!(
        same_session_assignment.as_ref().unwrap()["islandId"],
        "island-current"
    );
    let same_session_token = harness.gateway.fences.lock().unwrap()[0].clone();
    assert_eq!(same_session_token.owner_session, SESSION);
    assert_eq!(
        (
            same_session_token.owner_epoch,
            same_session_token.assignment_revision,
            same_session_token.fencing_token
        ),
        (2, 3, 2)
    );

    assert_eq!(
        replace_control_owner(&fixture, STALE_SESSION, 3).await,
        (4, 3)
    );
    {
        let mut source = harness.source.lock().unwrap();
        source.session = STALE_SESSION.into();
        source.pass += 1;
    }
    *harness.gateway.occupant.lock().unwrap() = Some(IslandOccupant::Session(SESSION.into()));
    harness.connect(STALE_SESSION);
    harness.settle().await;
    let (revision, fence, new_session_assignment) = row_state(&fixture.pool).await;
    assert_eq!((revision, fence), (5, 3));
    assert_eq!(
        new_session_assignment.as_ref().unwrap()["islandId"],
        "island-current"
    );
    let tokens = harness.gateway.fences.lock().unwrap().clone();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[1].owner_session, STALE_SESSION);
    assert_eq!(
        (
            tokens[1].owner_epoch,
            tokens[1].assignment_revision,
            tokens[1].fencing_token
        ),
        (3, 5, 3)
    );

    harness.connect(SESSION);
    harness.settle().await;
    assert_eq!(
        row_state(&fixture.pool).await,
        (5, 3, new_session_assignment)
    );
    assert_eq!(harness.gateway.fences.lock().unwrap().len(), 2);
    harness.subscriber.stop().await;
    drop(harness);
    fixture.finish().await;
}

#[tokio::test]
async fn a_move_during_token_mint_rolls_back_then_recovers_the_current_room() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 7, 3, 9, None).await;
    let harness = RecoveryHarness::new(&fixture, SESSION).await;
    let release = Arc::new(Notify::new());
    *harness.gateway.mint_pause.lock().unwrap() = Some(release.clone());

    harness.connect(SESSION);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        harness.gateway.mint_entered.notified(),
    )
    .await
    .expect("mint reached while the real authority row is locked");
    let competing_authority = fixture.fence().await;
    let mut competing =
        tokio::spawn(async move { competing_authority.lock_realm(WALLET, SESSION).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut competing)
            .await
            .is_err(),
        "recovery holds the authority row through token minting"
    );
    {
        let mut source = harness.source.lock().unwrap();
        source.cluster_id = "moved".into();
        source.pass += 1;
    }
    release.notify_one();
    harness.settle().await;
    let competing_lease = competing
        .await
        .unwrap()
        .expect("rollback releases the row lock");
    assert_eq!(competing_lease.token_fence().assignment_revision, 4);
    drop(competing_lease);
    assert_eq!(row_state(&fixture.pool).await, (3, 9, None));

    *harness.gateway.mint_pause.lock().unwrap() = None;
    harness.connect(SESSION);
    harness.settle().await;
    let (revision, fence, document) = row_state(&fixture.pool).await;
    assert_eq!((revision, fence), (4, 9));
    assert_eq!(document.unwrap()["islandId"], "island-moved");
    assert_eq!(harness.gateway.fences.lock().unwrap().len(), 2);
    harness.subscriber.stop().await;
    drop(harness);
    fixture.finish().await;
}

#[tokio::test]
async fn same_room_recovery_renews_expired_credentials_without_periodic_churn() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for (expiry, occupant, should_renew) in [
        (Some(now + 3600), IslandOccupant::Absent, false),
        (Some(1), IslandOccupant::Absent, true),
        (None, IslandOccupant::Absent, true),
        (Some(1), IslandOccupant::Session(SESSION.into()), false),
        (Some(1), IslandOccupant::Unattributed, false),
        (Some(1), IslandOccupant::Session(STALE_SESSION.into()), true),
    ] {
        let Some(fixture) = Fixture::new().await else {
            return;
        };
        let mut document = serde_json::json!({
            "islandId": "island-current",
            "connectionString": "livekit:wss://example.invalid/previous"
        });
        if let Some(expiry) = expiry {
            document["tokenExpiresAtUnix"] = expiry.into();
        }
        seed_realm(
            &fixture.pool,
            WALLET,
            SESSION,
            7,
            3,
            9,
            Some(document.clone()),
        )
        .await;
        let harness = RecoveryHarness::new(&fixture, SESSION).await;
        *harness.gateway.occupant.lock().unwrap() = Some(occupant.clone());

        harness.connect(SESSION);
        harness.settle().await;
        let (revision, fence, stored) = row_state(&fixture.pool).await;
        assert_eq!(fence, 9);
        assert_eq!(
            revision,
            if should_renew { 4 } else { 3 },
            "{expiry:?} {occupant:?}"
        );
        assert_eq!(
            harness.gateway.fences.lock().unwrap().len(),
            usize::from(should_renew)
        );
        if should_renew {
            let stored = stored.expect("renewed assignment");
            assert!(stored["tokenExpiresAtUnix"].as_u64().unwrap() > now);
            assert_eq!(stored["fromIslandId"], "island-current");
            harness.connect(SESSION);
            harness.settle().await;
            assert_eq!(row_state(&fixture.pool).await, (4, 9, Some(stored)));
            assert_eq!(
                harness.gateway.fences.lock().unwrap().len(),
                1,
                "fresh renewal does not churn at the next announcement"
            );
        } else {
            assert_eq!(stored, Some(document));
        }
        harness.subscriber.stop().await;
        drop(harness);
        fixture.finish().await;
    }
}
