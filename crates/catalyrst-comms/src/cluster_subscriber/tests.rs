use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::*;
use crate::nats::{InProcessBus, RequestError};

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SESSION_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SESSION_C: &str = "0xcccccccccccccccccccccccccccccccccccccccc";

#[test]
fn gateway_errors_discard_private_downstream_details() {
    const CANARY: &str = "cluster-gateway-private-material-canary";
    let error = GatewayError::new(CANARY);
    assert_eq!(error.to_string(), "cluster gateway operation failed");
    assert_eq!(format!("{error:?}"), "GatewayError");
    assert!(!error.to_string().contains(CANARY));
    assert!(!format!("{error:?}").contains(CANARY));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MintCall {
    wallet: String,
    session: Option<String>,
    room: String,
    ttl_seconds: u64,
    not_before_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoveCall {
    room: String,
    identity: String,
    revoke_before: Option<i64>,
}

#[derive(Default)]
struct FakeGateway {
    denied: Mutex<HashSet<String>>,
    deny_error: AtomicBool,
    mint_error: AtomicBool,
    minted: Mutex<Vec<MintCall>>,
    removals: Mutex<Vec<RemoveCall>>,
    remove_outcomes: Mutex<VecDeque<Result<Removal, String>>>,
    holds: Mutex<HashMap<String, bool>>,
    held_sessions: Mutex<HashMap<String, String>>,
    holds_error: AtomicBool,
    holds_calls: AtomicUsize,
    mint_parked: AtomicBool,
    mint_pause: Mutex<Option<Arc<Notify>>>,
    mint_entered: Notify,
    fences: Mutex<Vec<TokenFence>>,
}

impl FakeGateway {
    fn minted(&self) -> Vec<MintCall> {
        self.minted.lock().unwrap().clone()
    }

    fn removals(&self) -> Vec<RemoveCall> {
        self.removals.lock().unwrap().clone()
    }

    fn fences(&self) -> Vec<TokenFence> {
        self.fences.lock().unwrap().clone()
    }

    fn queue_removals(&self, outcomes: impl IntoIterator<Item = Result<Removal, String>>) {
        *self.remove_outcomes.lock().unwrap() = outcomes.into_iter().collect();
    }
}

#[async_trait]
impl ClusterGateway for FakeGateway {
    async fn is_denied(&self, wallet: &str) -> Result<bool, GatewayError> {
        if self.deny_error.load(Ordering::Relaxed) {
            return Err(GatewayError::new("ban store unreachable"));
        }
        Ok(self.denied.lock().unwrap().contains(wallet))
    }

    async fn mint_connection_string(
        &self,
        wallet: &str,
        session: Option<&str>,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError> {
        self.mint_entered.notify_one();
        let pause = self.mint_pause.lock().unwrap().clone();
        if let Some(pause) = pause {
            pause.notified().await;
        }
        if self.mint_parked.load(Ordering::Relaxed) {
            futures::future::pending::<()>().await;
        }
        if self.mint_error.load(Ordering::Relaxed) {
            return Err(GatewayError::new("livekit refused the mint"));
        }
        self.minted.lock().unwrap().push(MintCall {
            wallet: wallet.to_string(),
            session: session.map(str::to_string),
            room: room.to_string(),
            ttl_seconds,
            not_before_unix,
        });
        Ok(format!("livekit:wss://sfu?access_token=token-for-{room}"))
    }

    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        self.removals.lock().unwrap().push(RemoveCall {
            room: room.to_string(),
            identity: identity.to_string(),
            revoke_before: revoke_tokens_minted_before,
        });
        match self.remove_outcomes.lock().unwrap().pop_front() {
            Some(Ok(removal)) => Ok(removal),
            Some(Err(error)) => Err(GatewayError::new(error)),
            None => Ok(Removal::Removed),
        }
    }

    async fn mint_fenced_connection_string(
        &self,
        wallet: &str,
        room: &str,
        ttl_seconds: u64,
        fence: &TokenFence,
    ) -> Result<String, GatewayError> {
        self.fences.lock().unwrap().push(fence.clone());
        self.mint_connection_string(wallet, Some(&fence.owner_session), room, ttl_seconds, None)
            .await
    }

    async fn holds_participant(&self, room: &str, _identity: &str) -> Result<bool, GatewayError> {
        self.holds_calls.fetch_add(1, Ordering::Relaxed);
        if self.holds_error.load(Ordering::Relaxed) {
            return Err(GatewayError::new("livekit lookup failed"));
        }
        Ok(self
            .holds
            .lock()
            .unwrap()
            .get(room)
            .copied()
            .unwrap_or(false))
    }

    async fn island_occupant(
        &self,
        room: &str,
        wallet: &str,
    ) -> Result<IslandOccupant, GatewayError> {
        let held = self.holds_participant(room, wallet).await?;
        if let Some(session) = self.held_sessions.lock().unwrap().get(room) {
            return Ok(IslandOccupant::Session(session.clone()));
        }
        Ok(if held {
            IslandOccupant::Unattributed
        } else {
            IslandOccupant::Absent
        })
    }
}

struct Harness {
    bus: Arc<InProcessBus>,
    gateway: Arc<FakeGateway>,
    subscriber: Arc<ClusterSubscriber>,
    source: Arc<Mutex<HashMap<String, PeerClusterSnapshot>>>,
}

fn config() -> ClusterConfig {
    ClusterConfig {
        nats_url: Some("nats://127.0.0.1:4222".into()),
        enabled: true,
        queue_group: "catalyrst-comms-cluster".into(),
        takeover_retry_delay_ms: 0,
        self_hosted_token_quarantine: false,
        ..ClusterConfig::default()
    }
}

fn harness_with(config: ClusterConfig) -> Harness {
    harness_with_fence(config, None)
}

fn harness_with_fence(
    config: ClusterConfig,
    assignment_fence: Option<Arc<dyn AssignmentFence>>,
) -> Harness {
    let bus = Arc::new(InProcessBus::new());
    let gateway = Arc::new(FakeGateway::default());
    let source = Arc::new(Mutex::new(HashMap::<String, PeerClusterSnapshot>::new()));
    let replies = source.clone();
    bus.respond_to_requests(Arc::new(move |subject, payload| {
        let wallet = wallet_from_subject(subject).ok_or(RequestError::Invalid)?;
        let current = replies.lock().unwrap();
        let snapshot = current
            .get(&wallet)
            .filter(|s| s.session.as_bytes() == payload)
            .ok_or(RequestError::TimedOut)?;
        Ok(snapshot.encode_to_vec())
    }));
    let subscriber = ClusterSubscriber::new_with_assignment_fence(
        bus.clone(),
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        config,
        assignment_fence,
    );
    subscriber.start();
    Harness {
        bus,
        gateway,
        subscriber,
        source,
    }
}

fn harness() -> Harness {
    harness_with(config())
}

struct FakeAssignmentFence {
    owner_session: String,
    previous: Option<String>,
    previous_expires_at: Option<u64>,
    commits: Arc<Mutex<Vec<AssignmentDocument>>>,
    commit_error: bool,
}

struct FakeRealmLease {
    fence: TokenFence,
    previous: Option<String>,
    previous_expires_at: Option<u64>,
    commits: Arc<Mutex<Vec<AssignmentDocument>>>,
    commit_error: bool,
}

#[async_trait]
impl AssignmentFence for FakeAssignmentFence {
    async fn lock_realm(
        &self,
        _wallet: &str,
        session: &str,
    ) -> Result<
        Box<dyn crate::assignment_fence::RealmAssignmentLease>,
        crate::assignment_fence::FenceError,
    > {
        if session != self.owner_session {
            return Err(crate::assignment_fence::FenceError::Conflict);
        }
        Ok(Box::new(FakeRealmLease {
            fence: TokenFence {
                audience: "deployment-a".into(),
                lane: "realm".into(),
                owner_session: session.into(),
                owner_epoch: 7,
                assignment_revision: 12,
                fencing_token: 3,
            },
            previous: self.previous.clone(),
            previous_expires_at: self.previous_expires_at,
            commits: self.commits.clone(),
            commit_error: self.commit_error,
        }))
    }
}

#[async_trait]
impl crate::assignment_fence::RealmAssignmentLease for FakeRealmLease {
    fn token_fence(&self) -> TokenFence {
        self.fence.clone()
    }

    fn previous_island(&self) -> Option<String> {
        self.previous.clone()
    }

    fn previous_token_expires_at_unix(&self) -> Option<u64> {
        self.previous_expires_at
    }

    async fn commit(
        self: Box<Self>,
        assignment: AssignmentDocument,
    ) -> Result<(), crate::assignment_fence::FenceError> {
        if self.commit_error {
            return Err(crate::assignment_fence::FenceError::Conflict);
        }
        self.commits.lock().unwrap().push(assignment);
        Ok(())
    }
}

#[tokio::test]
async fn fenced_realm_assignment_uses_live_source_and_commits_without_legacy_publish() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    let change = change("new", SESSION_A);
    h.feed(WALLET, &change).await;

    assert!(h.island_changed().is_empty());
    assert_eq!(h.gateway.removals(), Vec::new());
    assert_eq!(
        h.gateway.fences(),
        vec![TokenFence {
            audience: "deployment-a".into(),
            lane: "realm".into(),
            owner_session: SESSION_A.into(),
            owner_epoch: 7,
            assignment_revision: 12,
            fencing_token: 3,
        }]
    );
    let committed = commits.lock().unwrap();
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].island_id, "island-new");
    assert_eq!(committed[0].from_island_id.as_deref(), Some("island-old"));
    assert!(fenced_token_is_fresh(committed[0].token_expires_at_unix));
}

#[tokio::test]
async fn fenced_takeover_fails_closed_without_removing_or_minting() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_B.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    let mut change = change("new", SESSION_B);
    change.displaced_session = SESSION_A.into();
    change.displaced_cluster_id = "old".into();
    h.feed(WALLET, &change).await;

    assert!(h.gateway.removals().is_empty());
    assert!(h.gateway.minted().is_empty());
    assert!(h.gateway.fences().is_empty());
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test]
async fn fenced_assignment_never_falls_back_on_source_or_owner_conflict() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.snapshot(WALLET, "different", SESSION_A);
    h.subscriber
        .process_cluster_change(WALLET, &change("new", SESSION_A))
        .await;
    h.snapshot(WALLET, "new", SESSION_B);
    h.subscriber
        .process_cluster_change(WALLET, &change("new", SESSION_B))
        .await;

    assert!(h.gateway.minted().is_empty());
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test]
async fn stale_lease_commit_exposes_neither_assignment_nor_legacy_frame() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: true,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.feed(WALLET, &change("new", SESSION_A)).await;

    assert_eq!(h.gateway.minted().len(), 1);
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test]
async fn a_fenced_connect_refills_a_cleared_claim_without_a_legacy_publish() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: None,
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.snapshot(WALLET, "current", SESSION_A);
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-current".into(), SESSION_A.into());

    h.connect(WALLET, SESSION_A).await;

    assert!(h.island_changed().is_empty());
    assert_eq!(h.gateway.fences().len(), 1);
    let committed = commits.lock().unwrap();
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].island_id, "island-current");
    assert_eq!(committed[0].from_island_id, None);
    assert!(fenced_token_is_fresh(committed[0].token_expires_at_unix));
}

#[tokio::test]
async fn a_fenced_connect_does_not_reissue_an_existing_current_assignment() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-current".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.snapshot(WALLET, "current", SESSION_A);
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-current".into(), SESSION_A.into());

    h.connect(WALLET, SESSION_A).await;

    assert!(h.gateway.minted().is_empty());
    assert!(h.gateway.fences().is_empty());
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test]
async fn a_fresh_same_room_token_suppresses_absent_or_different_session_recovery() {
    for occupant in [None, Some(SESSION_B)] {
        let commits = Arc::new(Mutex::new(Vec::new()));
        let authority = Arc::new(FakeAssignmentFence {
            owner_session: SESSION_A.into(),
            previous: Some("island-current".into()),
            previous_expires_at: Some(u64::MAX),
            commits: commits.clone(),
            commit_error: false,
        });
        let h = harness_with_fence(config(), Some(authority));
        h.snapshot(WALLET, "current", SESSION_A);
        if let Some(occupant) = occupant {
            h.gateway
                .held_sessions
                .lock()
                .unwrap()
                .insert("island-current".into(), occupant.into());
        }

        h.connect(WALLET, SESSION_A).await;

        assert!(h.gateway.minted().is_empty());
        assert!(commits.lock().unwrap().is_empty());
        assert!(h.island_changed().is_empty());
    }
}

#[tokio::test]
async fn an_expired_or_unstamped_same_room_token_is_renewed_without_the_exact_participant() {
    for previous_expires_at in [Some(1), None] {
        for occupant in [None, Some(SESSION_B)] {
            let commits = Arc::new(Mutex::new(Vec::new()));
            let authority = Arc::new(FakeAssignmentFence {
                owner_session: SESSION_A.into(),
                previous: Some("island-current".into()),
                previous_expires_at,
                commits: commits.clone(),
                commit_error: false,
            });
            let h = harness_with_fence(config(), Some(authority));
            h.snapshot(WALLET, "current", SESSION_A);
            if let Some(occupant) = occupant {
                h.gateway
                    .held_sessions
                    .lock()
                    .unwrap()
                    .insert("island-current".into(), occupant.into());
            }

            h.connect(WALLET, SESSION_A).await;

            assert_eq!(h.gateway.fences().len(), 1, "{previous_expires_at:?}");
            let committed = commits.lock().unwrap();
            assert_eq!(committed.len(), 1, "{previous_expires_at:?}");
            assert!(fenced_token_is_fresh(committed[0].token_expires_at_unix));
            assert!(h.island_changed().is_empty());
        }
    }
}

#[tokio::test]
async fn an_unknown_or_unreadable_occupant_blocks_even_expired_fenced_recovery() {
    for lookup_error in [false, true] {
        let commits = Arc::new(Mutex::new(Vec::new()));
        let authority = Arc::new(FakeAssignmentFence {
            owner_session: SESSION_A.into(),
            previous: Some("island-current".into()),
            previous_expires_at: Some(1),
            commits: commits.clone(),
            commit_error: false,
        });
        let h = harness_with_fence(config(), Some(authority));
        h.snapshot(WALLET, "current", SESSION_A);
        if lookup_error {
            h.gateway.holds_error.store(true, Ordering::Relaxed);
        } else {
            h.gateway
                .holds
                .lock()
                .unwrap()
                .insert("island-current".into(), true);
        }

        h.connect(WALLET, SESSION_A).await;

        assert!(h.gateway.minted().is_empty(), "{lookup_error}");
        assert!(commits.lock().unwrap().is_empty(), "{lookup_error}");
        assert!(h.island_changed().is_empty(), "{lookup_error}");
    }
}

#[tokio::test]
async fn a_source_change_during_fenced_recovery_discards_the_private_token() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: None,
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.snapshot(WALLET, "current", SESSION_A);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    h.gateway.mint_entered.notified().await;
    h.snapshot(WALLET, "moved", SESSION_A);
    release.notify_one();
    h.subscriber.settle().await;

    assert_eq!(h.gateway.minted().len(), 1);
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test]
async fn a_source_change_during_fenced_cluster_mint_cannot_be_committed() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.snapshot(WALLET, "current", SESSION_A);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("current", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    h.snapshot(WALLET, "moved", SESSION_A);
    release.notify_one();
    h.subscriber.settle().await;

    assert_eq!(h.gateway.minted().len(), 1);
    assert!(commits.lock().unwrap().is_empty());
    assert!(h.island_changed().is_empty());
}

/// An authority that holds no row for the wallet, which is every unmodified v3 client.
struct UnownedFence;

#[async_trait]
impl AssignmentFence for UnownedFence {
    async fn lock_realm(
        &self,
        _wallet: &str,
        _session: &str,
    ) -> Result<
        Box<dyn crate::assignment_fence::RealmAssignmentLease>,
        crate::assignment_fence::FenceError,
    > {
        Err(crate::assignment_fence::FenceError::Unowned)
    }
}

struct RejectedFence(crate::assignment_fence::FenceError);

#[async_trait]
impl AssignmentFence for RejectedFence {
    async fn lock_realm(
        &self,
        _wallet: &str,
        _session: &str,
    ) -> Result<
        Box<dyn crate::assignment_fence::RealmAssignmentLease>,
        crate::assignment_fence::FenceError,
    > {
        Err(self.0)
    }
}

#[tokio::test]
async fn a_wallet_no_v4_socket_claimed_is_assigned_on_the_legacy_path() {
    let h = harness_with_fence(config(), Some(Arc::new(UnownedFence)));
    h.feed(WALLET, &change("new", SESSION_A)).await;

    assert!(h.gateway.fences().is_empty());
    assert_eq!(h.gateway.minted().len(), 1);
    let published = h.island_changed();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}")
    );
    assert_eq!(published[0].1.island_id, "island-new");
}

#[tokio::test]
async fn an_unowned_connect_alone_uses_legacy_recovery() {
    let h = harness_with_fence(config(), Some(Arc::new(UnownedFence)));
    h.snapshot(WALLET, "current", SESSION_A);

    h.connect(WALLET, SESSION_A).await;

    assert!(h.gateway.fences().is_empty());
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed()[0].1.island_id, "island-current");
}

#[tokio::test]
async fn rejected_fenced_recovery_never_falls_back_to_a_legacy_token() {
    for error in [
        crate::assignment_fence::FenceError::Conflict,
        crate::assignment_fence::FenceError::Unavailable,
    ] {
        let h = harness_with_fence(config(), Some(Arc::new(RejectedFence(error))));
        h.snapshot(WALLET, "current", SESSION_A);

        h.connect(WALLET, SESSION_A).await;

        assert!(h.gateway.minted().is_empty(), "{error:?}");
        assert!(h.gateway.fences().is_empty(), "{error:?}");
        assert!(h.island_changed().is_empty(), "{error:?}");
    }
}

#[tokio::test]
async fn a_session_that_cannot_own_a_fenced_lane_never_asks_the_authority() {
    let commits = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(FakeAssignmentFence {
        owner_session: SESSION_A.into(),
        previous: Some("island-old".into()),
        previous_expires_at: None,
        commits: commits.clone(),
        commit_error: false,
    });
    let h = harness_with_fence(config(), Some(authority));
    h.feed(WALLET, &change("new", "")).await;

    assert!(h.gateway.fences().is_empty());
    assert!(commits.lock().unwrap().is_empty());
    let published = h.island_changed();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].0,
        format!("engine.peer.{WALLET}.island_changed")
    );
}

#[tokio::test]
async fn subscriber_caps_programmatic_token_ttl_to_its_quarantine_budget() {
    let h = harness_with(ClusterConfig {
        island_token_ttl_seconds: crate::config::MAX_ISLAND_TOKEN_TTL_SECONDS + 1,
        ..config()
    });
    assert_eq!(
        h.subscriber.config.island_token_ttl_seconds,
        crate::config::MAX_ISLAND_TOKEN_TTL_SECONDS
    );
}

fn change(cluster_id: &str, session: &str) -> PeerClusterChange {
    PeerClusterChange {
        cluster_id: cluster_id.into(),
        realm: "main".into(),
        session: session.into(),
        displaced_session: String::new(),
        displaced_cluster_id: String::new(),
    }
}

impl Harness {
    fn snapshot(&self, wallet: &str, cluster: &str, session: &str) {
        self.source.lock().unwrap().insert(
            wallet.to_lowercase(),
            PeerClusterSnapshot {
                wallet: wallet.to_lowercase(),
                session: session.to_lowercase(),
                cluster_id: cluster.into(),
                realm: "main".into(),
                pass: 1,
                source_incarnation: "test-source".into(),
            },
        );
    }

    async fn feed(&self, wallet: &str, change: &PeerClusterChange) {
        self.snapshot(wallet, &change.cluster_id, &change.session);
        self.bus.inject(
            &format!("peer.{wallet}.cluster_change"),
            &change.encode_to_vec(),
        );
        self.subscriber.settle().await;
    }

    async fn connect(&self, wallet: &str, session: &str) {
        self.bus
            .inject(&format!("peer.{wallet}.connect"), session.as_bytes());
        self.subscriber.settle().await;
    }

    fn island_changed(&self) -> Vec<(String, IslandChangedMessage)> {
        self.bus
            .published()
            .into_iter()
            .map(|(subject, payload)| {
                (
                    subject,
                    IslandChangedMessage::decode(payload.as_slice()).expect("island_changed"),
                )
            })
            .collect()
    }
}

#[tokio::test]
async fn an_assignment_mints_a_room_token_and_publishes_to_the_session() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;

    assert_eq!(
        h.gateway.minted(),
        vec![MintCall {
            wallet: WALLET.into(),
            session: Some(SESSION_A.into()),
            room: "island-c1".into(),
            ttl_seconds: 60,
            not_before_unix: None,
        }]
    );

    let published = h.island_changed();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}")
    );
    assert_eq!(published[0].1.island_id, "island-c1");
    assert_eq!(
        published[0].1.from_island_id, None,
        "a first assignment names no previous room rather than an empty one"
    );
    assert!(
        published[0].1.peers.is_empty(),
        "peers is empty by design: explorers read only conn_str"
    );
    assert!(published[0].1.conn_str.starts_with("livekit:"));
}

#[tokio::test]
async fn the_next_assignment_names_the_room_the_peer_came_from() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.feed(WALLET, &change("c2", SESSION_A)).await;

    let published = h.island_changed();
    assert_eq!(published[1].1.island_id, "island-c2");
    assert_eq!(published[1].1.from_island_id.as_deref(), Some("island-c1"));
}

#[tokio::test]
async fn an_event_with_no_session_falls_back_to_the_legacy_subject() {
    let h = harness();
    h.feed(WALLET, &change("c1", "")).await;
    assert_eq!(
        h.island_changed()[0].0,
        format!("engine.peer.{WALLET}.island_changed")
    );
}

#[tokio::test]
async fn a_malformed_session_never_becomes_a_subject_token() {
    let h = harness();
    for malformed in ["not-a-session", "0xABCDEF", "0x", "0x11.22"] {
        h.feed(WALLET, &change("c1", malformed)).await;
    }
    for (subject, _) in h.island_changed() {
        assert_eq!(subject, format!("engine.peer.{WALLET}.island_changed"));
    }
}

#[tokio::test]
async fn an_upper_cased_session_is_lower_cased_before_it_is_used() {
    let h = harness();
    h.feed(WALLET, &change("c1", &SESSION_A.to_uppercase()))
        .await;
    assert_eq!(
        h.island_changed()[0].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}")
    );
}

#[tokio::test]
async fn the_wallet_is_taken_from_the_subject_and_lower_cased() {
    let h = harness();
    let mixed = "0xAAAAaaaaBBBBbbbbCCCCccccDDDDddddEEEEeeee";
    h.feed(mixed, &change("c1", SESSION_A)).await;
    assert_eq!(h.gateway.minted()[0].wallet, mixed.to_lowercase());
    assert!(h.island_changed()[0]
        .0
        .starts_with(&format!("engine.peer.{}", mixed.to_lowercase())));
}

#[tokio::test]
async fn a_banned_wallet_is_skipped_entirely() {
    let h = harness();
    h.gateway.denied.lock().unwrap().insert(WALLET.into());
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert!(h.gateway.minted().is_empty());
    assert!(h.bus.published().is_empty());
}

#[tokio::test]
async fn an_access_check_failure_lets_the_peer_through() {
    let h = harness();
    h.gateway.deny_error.store(true, Ordering::Relaxed);
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert_eq!(
        h.gateway.minted().len(),
        1,
        "the gate fails open: a background feed has nobody to report the error to"
    );
}

#[tokio::test]
async fn an_empty_cluster_id_is_dropped_before_anything_is_minted() {
    let h = harness();
    h.feed(WALLET, &change("", SESSION_A)).await;
    assert!(h.gateway.minted().is_empty());
    assert!(h.bus.published().is_empty());
}

#[tokio::test]
async fn an_undecodable_payload_costs_only_that_message() {
    let h = harness();
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &[0xff, 0xff, 0xff],
    );
    h.subscriber.settle().await;
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert_eq!(h.gateway.minted().len(), 1);
}

#[tokio::test]
async fn a_takeover_evicts_the_displaced_session_before_the_replacement_is_minted() {
    let h = harness();
    let mut takeover = change("c1", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    h.feed(WALLET, &takeover).await;

    let removals = h.gateway.removals();
    assert_eq!(removals.len(), 1);
    assert_eq!(removals[0].room, "island-c1");
    assert_eq!(
        removals[0].identity, WALLET,
        "LiveKit identities are wallets, so the wallet is what is removed"
    );

    let minted = h.gateway.minted();
    let boundary = minted[0].not_before_unix.expect("a revocation boundary");
    assert_eq!(
        removals[0].revoke_before,
        Some(boundary as i64),
        "the replacement's nbf and the revocation boundary are the same instant"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        boundary > now || boundary == now + 1,
        "the requested Cloud cutoff is the next whole second; this unit test does not establish revocation"
    );
}

#[tokio::test]
async fn a_displaced_session_that_already_left_is_not_a_failure() {
    let h = harness();
    h.gateway.queue_removals([Ok(Removal::Absent)]);
    let mut takeover = change("c2", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    h.feed(WALLET, &takeover).await;
    assert_eq!(
        h.gateway.removals().len(),
        1,
        "an absence ends the attempts"
    );
    assert_eq!(h.gateway.minted().len(), 1);
}

#[tokio::test]
async fn an_eviction_that_never_completes_does_not_mint_a_replacement() {
    let h = harness();
    h.gateway.queue_removals([
        Err("timeout".to_string()),
        Err("timeout".to_string()),
        Err("timeout".to_string()),
    ]);
    let mut takeover = change("c2", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    h.feed(WALLET, &takeover).await;
    assert_eq!(h.gateway.removals().len(), 3);
    assert!(h.gateway.minted().is_empty());
}

#[tokio::test]
async fn a_takeover_that_names_no_displaced_cluster_removes_nothing() {
    let h = harness();
    let mut takeover = change("c2", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    h.feed(WALLET, &takeover).await;
    assert!(h.gateway.removals().is_empty());
    assert!(h.gateway.minted().is_empty());
}

#[tokio::test]
async fn a_stale_takeover_is_rejected_before_it_can_remove_the_current_owner() {
    let h = harness();
    let mut takeover = change("stale", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "old".into();
    h.snapshot(WALLET, "current", SESSION_B);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &takeover.encode_to_vec(),
    );
    h.subscriber.settle().await;
    assert!(h.gateway.removals().is_empty());
    assert!(h.gateway.minted().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test(start_paused = true)]
async fn self_hosted_takeover_waits_out_old_tokens_removes_again_and_rechecks_source() {
    let mut cfg = config();
    cfg.self_hosted_token_quarantine = true;
    cfg.island_token_ttl_seconds = 1;
    let h = harness_with(cfg);
    let mut takeover = change("c1", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &takeover.encode_to_vec(),
    );
    while h.gateway.removals().is_empty() {
        tokio::task::yield_now().await;
    }
    assert!(h.gateway.minted().is_empty());
    tokio::time::advance(Duration::from_secs(63)).await;
    h.subscriber.settle().await;
    assert_eq!(h.gateway.removals().len(), 2);
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed().len(), 1);
}

#[tokio::test]
async fn self_hosted_cross_room_takeover_does_not_wait_for_a_room_bound_token() {
    let mut cfg = config();
    cfg.self_hosted_token_quarantine = true;
    let h = harness_with(cfg);
    let mut takeover = change("c2", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    tokio::time::timeout(Duration::from_secs(1), h.feed(WALLET, &takeover))
        .await
        .expect("an old-room token cannot displace the owner in the new room");
    assert_eq!(h.gateway.removals().len(), 1);
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_source_change_during_self_hosted_quarantine_discards_the_candidate() {
    let mut cfg = config();
    cfg.self_hosted_token_quarantine = true;
    cfg.island_token_ttl_seconds = 1;
    let h = harness_with(cfg);
    let mut takeover = change("c1", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    takeover.displaced_cluster_id = "c1".into();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &takeover.encode_to_vec(),
    );
    while h.gateway.removals().is_empty() {
        tokio::task::yield_now().await;
    }
    h.snapshot(WALLET, "newer", SESSION_B);
    tokio::time::advance(Duration::from_secs(63)).await;
    h.subscriber.settle().await;
    assert_eq!(h.gateway.removals().len(), 2);
    assert!(h.gateway.minted().is_empty());
    assert!(h.island_changed().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_newer_same_room_takeover_is_not_lost_behind_an_old_quarantine() {
    let mut cfg = config();
    cfg.self_hosted_token_quarantine = true;
    let h = harness_with(cfg);
    let mut first = change("c1", SESSION_B);
    first.displaced_session = SESSION_A.into();
    first.displaced_cluster_id = "c1".into();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &first.encode_to_vec(),
    );
    while h.gateway.removals().is_empty() {
        tokio::task::yield_now().await;
    }

    let mut current = change("c1", SESSION_C);
    current.displaced_session = SESSION_B.into();
    current.displaced_cluster_id = "c1".into();
    h.snapshot(WALLET, "c1", SESSION_C);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &current.encode_to_vec(),
    );
    while h.gateway.removals().len() < 2 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_secs(122)).await;
    h.subscriber.settle().await;

    let assignments = h.island_changed();
    assert_eq!(assignments.len(), 1);
    assert!(assignments[0].0.ends_with(SESSION_C));
    assert_eq!(assignments[0].1.island_id, "island-c1");
}

#[tokio::test(start_paused = true)]
async fn an_accepted_but_stale_successor_cannot_cancel_the_current_quarantine() {
    let mut cfg = config();
    cfg.self_hosted_token_quarantine = true;
    cfg.island_token_ttl_seconds = 1;
    let h = harness_with(cfg);
    let mut current = change("c1", SESSION_B);
    current.displaced_session = SESSION_A.into();
    current.displaced_cluster_id = "c1".into();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &current.encode_to_vec(),
    );
    while h.gateway.removals().is_empty() {
        tokio::task::yield_now().await;
    }

    let mut stale = change("c1", SESSION_C);
    stale.displaced_session = SESSION_B.into();
    stale.displaced_cluster_id = "c1".into();
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &stale.encode_to_vec(),
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(63)).await;
    h.subscriber.settle().await;

    let assignments = h.island_changed();
    assert_eq!(assignments.len(), 1);
    assert!(assignments[0].0.ends_with(SESSION_B));
    assert_eq!(h.gateway.removals().len(), 2);
}

#[tokio::test]
async fn a_publish_that_never_left_records_no_assignment() {
    let h = harness();
    h.bus.set_connected(false);
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.bus.set_connected(true);
    h.feed(WALLET, &change("c2", SESSION_A)).await;

    let published = h.island_changed();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].1.from_island_id, None,
        "an undelivered assignment must not become the next from_island_id"
    );
}

#[tokio::test]
async fn a_refused_publish_records_no_assignment_either() {
    let h = harness();
    h.bus.refuse_publishes(true);
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.bus.refuse_publishes(false);
    h.feed(WALLET, &change("c2", SESSION_A)).await;
    assert_eq!(h.island_changed()[0].1.from_island_id, None);
}

#[tokio::test]
async fn an_unconfirmed_exchange_does_not_commit_a_previous_room_hint() {
    let h = harness();
    h.bus.leave_publishes_unconfirmed(true);
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert_eq!(
        h.bus.published().len(),
        1,
        "unknown does not mean undelivered"
    );
    h.bus.leave_publishes_unconfirmed(false);
    h.feed(WALLET, &change("c2", SESSION_A)).await;
    let published = h.island_changed();
    assert_eq!(published.len(), 2);
    assert_eq!(published[1].1.from_island_id, None);
    h.feed(WALLET, &change("c3", SESSION_A)).await;
    assert_eq!(
        h.island_changed()[2].1.from_island_id.as_deref(),
        Some("island-c2")
    );
}

#[tokio::test]
async fn a_mint_failure_publishes_nothing() {
    let h = harness();
    h.gateway.mint_error.store(true, Ordering::Relaxed);
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert!(h.bus.published().is_empty());
}

#[tokio::test]
async fn a_connect_for_a_wallet_with_no_known_cluster_re_announces_nothing() {
    let h = harness();
    h.connect(WALLET, SESSION_A).await;
    assert!(h.gateway.minted().is_empty());
    assert_eq!(h.gateway.holds_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_connect_re_announces_the_last_island_to_the_connecting_session() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, SESSION_A).await;

    let published = h.island_changed();
    assert_eq!(published.len(), 2);
    assert_eq!(
        published[1].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}")
    );
    assert_eq!(published[1].1.island_id, "island-c1");
    assert_eq!(
        published[1].1.from_island_id.as_deref(),
        Some("island-c1"),
        "the re-announcement names the room the peer was last told to join"
    );
}

#[tokio::test]
async fn a_connect_from_a_displaced_device_is_skipped() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, SESSION_B).await;
    assert_eq!(
        h.island_changed().len(),
        1,
        "handing the room back to a displaced device would put a stale identity beside the live one"
    );
    assert_eq!(h.gateway.holds_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_connect_payload_that_is_not_a_session_key_cannot_use_the_mirror() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, "legacy-connector-payload").await;

    assert_eq!(h.island_changed().len(), 1);
    assert_eq!(h.gateway.holds_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_wallet_already_in_its_room_is_not_re_announced() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.gateway
        .holds
        .lock()
        .unwrap()
        .insert("island-c1".into(), true);
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(h.island_changed().len(), 1);
}

#[tokio::test]
async fn a_participant_lookup_that_fails_suppresses_the_re_announcement() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.gateway.holds_error.store(true, Ordering::Relaxed);
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(
        h.island_changed().len(),
        1,
        "this lookup fails closed: announcing a peer that is still in its room ends its session"
    );
}

#[tokio::test]
async fn a_snapshot_answers_a_reconnect_this_replica_never_minted() {
    let h = harness();
    h.subscriber.peers().record_mirror(
        WALLET,
        MirrorEntry {
            cluster_id: "c9".into(),
            session: SESSION_A.into(),
        },
    );
    h.snapshot(WALLET, "c9", SESSION_A);
    h.connect(WALLET, SESSION_A).await;

    let published = h.island_changed();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].1.island_id, "island-c9");
    assert_eq!(
        published[0].1.from_island_id, None,
        "this replica minted nothing for the wallet, so it names no previous room"
    );
}

#[tokio::test]
async fn a_cluster_change_feeds_the_mirror_as_well_as_the_minting_path() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert_eq!(
        h.subscriber.peers().mirrored(WALLET),
        Some(MirrorEntry {
            cluster_id: "c1".into(),
            session: SESSION_A.into(),
        })
    );
}

#[tokio::test]
async fn a_lost_event_and_an_empty_mirror_recover_from_current_source_state() {
    let h = harness();
    h.snapshot(WALLET, "current", SESSION_A);
    assert!(h.subscriber.peers().mirrored(WALLET).is_none());
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(h.island_changed()[0].1.island_id, "island-current");
    assert!(
        h.gateway.removals().is_empty(),
        "lookup cannot replay takeover"
    );
}

#[tokio::test]
async fn an_unavailable_source_cannot_be_replaced_by_a_stale_mirror() {
    let h = harness();
    h.feed(WALLET, &change("old", SESSION_A)).await;
    h.source.lock().unwrap().clear();
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(h.island_changed().len(), 1);
    assert_eq!(h.gateway.holds_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn snapshot_identity_and_source_fields_are_checked_before_minting() {
    for field in ["wallet", "session", "cluster", "source", "pass", "protobuf"] {
        let h = harness();
        h.snapshot(WALLET, "c1", SESSION_A);
        let mut snapshot = h.source.lock().unwrap().get(WALLET).unwrap().clone();
        match field {
            "wallet" => snapshot.wallet = SESSION_B.into(),
            "session" => snapshot.session = SESSION_B.into(),
            "cluster" => snapshot.cluster_id.clear(),
            "source" => snapshot.source_incarnation.clear(),
            "pass" => snapshot.pass = 0,
            _ => {}
        }
        h.bus.respond_to_requests(Arc::new(move |_, _| {
            Ok(if field == "protobuf" {
                vec![0xff]
            } else {
                snapshot.encode_to_vec()
            })
        }));
        h.connect(WALLET, SESSION_A).await;
        assert!(h.gateway.minted().is_empty(), "accepted invalid {field}");
    }
}

#[tokio::test]
async fn changes_at_the_source_during_mint_discard_the_token_without_a_local_event() {
    for field in [
        "owner", "cluster", "realm", "source", "rollback", "absent", "advance",
    ] {
        let h = harness();
        h.snapshot(WALLET, "c1", SESSION_A);
        h.source.lock().unwrap().get_mut(WALLET).unwrap().pass = 10;
        let release = Arc::new(Notify::new());
        *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
        h.gateway.mint_entered.notified().await;
        {
            let mut source = h.source.lock().unwrap();
            let snapshot = source.get_mut(WALLET).unwrap();
            match field {
                "owner" => snapshot.session = SESSION_B.into(),
                "cluster" => snapshot.cluster_id = "c2".into(),
                "realm" => snapshot.realm = "other".into(),
                "source" => snapshot.source_incarnation = "restarted".into(),
                "rollback" => snapshot.pass = 9,
                "advance" => snapshot.pass = 11,
                "absent" => {
                    source.clear();
                }
                _ => unreachable!(),
            }
        }
        release.notify_one();
        h.subscriber.settle().await;
        assert_eq!(h.gateway.minted().len(), 1);
        assert_eq!(
            h.island_changed().len(),
            usize::from(field == "advance"),
            "{field}"
        );
        assert!(h.gateway.removals().is_empty());
    }
}

#[tokio::test]
async fn another_sessions_announcement_cannot_cancel_the_current_owners_recovery() {
    let h = harness();
    h.snapshot(WALLET, "c1", SESSION_A);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    h.gateway.mint_entered.notified().await;
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
    for session in [SESSION_A, SESSION_B].into_iter().cycle().take(100) {
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), session.as_bytes());
    }
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 2);
    release.notify_one();
    h.subscriber.settle().await;
    assert_eq!(
        h.island_changed().len(),
        1,
        "an unverified socket announcement cannot cancel recovery for Pulse's actual owner"
    );
    assert_eq!(h.gateway.minted()[0].session.as_deref(), Some(SESSION_A));
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
}

#[tokio::test]
async fn a_source_owner_change_rejects_a_held_mint_before_the_new_session_recovers() {
    let h = harness();
    h.snapshot(WALLET, "c1", SESSION_A);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    h.gateway.mint_entered.notified().await;
    h.snapshot(WALLET, "c2", SESSION_B);
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
    *h.gateway.mint_pause.lock().unwrap() = None;
    release.notify_one();
    h.subscriber.settle().await;
    assert_eq!(h.gateway.minted().len(), 2);
    let assignments = h.island_changed();
    assert_eq!(assignments.len(), 1);
    assert!(assignments[0].0.ends_with(SESSION_B));
    assert_eq!(assignments[0].1.island_id, "island-c2");
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
}

#[tokio::test]
async fn a_mirror_event_or_stop_cancels_all_held_recovery_attempts_for_a_wallet() {
    for event in ["mirror", "stop"] {
        let h = harness();
        h.snapshot(WALLET, "c1", SESSION_A);
        h.gateway.mint_parked.store(true, Ordering::Relaxed);
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
        h.gateway.mint_entered.notified().await;
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
        match event {
            "mirror" => h
                .subscriber
                .clone()
                .handle_assignment_mirror(WALLET.into(), change("c2", SESSION_B).encode_to_vec()),
            "stop" => h.subscriber.stop().await,
            _ => unreachable!(),
        }
        tokio::time::timeout(Duration::from_secs(1), h.subscriber.settle())
            .await
            .unwrap();
        assert!(h.island_changed().is_empty(), "{event}");
        assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn repeated_connects_coalesce_and_a_stalled_mint_has_a_recovery_deadline() {
    let h = harness();
    h.snapshot(WALLET, "c1", SESSION_A);
    h.gateway.mint_parked.store(true, Ordering::Relaxed);
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    h.gateway.mint_entered.notified().await;
    for _ in 0..100 {
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    }
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 1);
    tokio::time::advance(RECOVERY_DEADLINE).await;
    h.subscriber.settle().await;
    assert!(h.island_changed().is_empty());
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
    h.gateway.mint_parked.store(false, Ordering::Relaxed);
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(
        h.island_changed().len(),
        1,
        "the expired attempt must release ownership"
    );
}

#[tokio::test]
async fn a_room_join_or_failed_participant_check_during_mint_suppresses_recovery() {
    for lookup_error in [false, true] {
        let h = harness();
        h.snapshot(WALLET, "c1", SESSION_A);
        let release = Arc::new(Notify::new());
        *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
        h.bus
            .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
        h.gateway.mint_entered.notified().await;
        if lookup_error {
            h.gateway.holds_error.store(true, Ordering::Relaxed);
        } else {
            h.gateway
                .holds
                .lock()
                .unwrap()
                .insert("island-c1".into(), true);
        }
        release.notify_one();
        h.subscriber.settle().await;
        assert_eq!(h.gateway.minted().len(), 1);
        assert!(h.island_changed().is_empty());
    }
}

#[tokio::test]
async fn an_attributed_displaced_occupant_allows_current_session_admission_without_removal() {
    let h = harness();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-c1".into(), SESSION_A.into());
    h.connect(WALLET, SESSION_B).await;
    assert_eq!(h.gateway.minted()[0].session.as_deref(), Some(SESSION_B));
    assert_eq!(h.island_changed().len(), 1);
    assert!(h.gateway.removals().is_empty());
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-c1".into(), SESSION_B.into());
    h.connect(WALLET, SESSION_B).await;
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed().len(), 1);
}

#[tokio::test]
async fn a_current_session_join_during_displaced_owner_recovery_discards_the_minted_token() {
    let h = harness();
    h.snapshot(WALLET, "c1", SESSION_B);
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-c1".into(), SESSION_A.into());
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
    h.gateway.mint_entered.notified().await;
    h.gateway
        .held_sessions
        .lock()
        .unwrap()
        .insert("island-c1".into(), SESSION_B.into());
    release.notify_one();
    h.subscriber.settle().await;
    assert_eq!(h.gateway.minted().len(), 1);
    assert!(h.island_changed().is_empty());
    assert!(h.gateway.removals().is_empty());
}

#[tokio::test]
async fn a_disabled_subscriber_takes_no_events_at_all() {
    let h = harness_with(ClusterConfig {
        enabled: false,
        ..config()
    });
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, SESSION_A).await;
    assert!(h.gateway.minted().is_empty());
    assert!(h.bus.published().is_empty());
}

#[tokio::test]
async fn an_unconfigured_broker_leaves_the_subscriber_idle() {
    let bus = Arc::new(crate::nats::DisabledBus);
    let gateway = Arc::new(FakeGateway::default());
    let subscriber = ClusterSubscriber::new(
        bus,
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        config(),
    );
    subscriber.start();
    subscriber.stop().await;
    assert!(gateway.minted().is_empty());
}

#[tokio::test]
async fn a_stalled_wallet_cannot_accumulate_unbounded_cluster_work() {
    let h = harness();
    h.gateway.mint_parked.store(true, Ordering::Relaxed);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("held", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    for index in 0..5_000 {
        h.bus.inject(
            &format!("peer.{WALLET}.cluster_change"),
            &change(&format!("move-{index}"), SESSION_A).encode_to_vec(),
        );
    }
    assert!(
        h.subscriber.in_flight.load(Ordering::SeqCst) <= 16,
        "one stalled wallet retained the whole feed history"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stalled_cluster_mint_expires_and_does_not_prevent_source_recovery() {
    let h = harness();
    h.snapshot(WALLET, "current", SESSION_A);
    h.gateway.mint_parked.store(true, Ordering::Relaxed);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("old", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    tokio::time::advance(wallet_queue::WORK_DEADLINE + Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        h.subscriber.in_flight.load(Ordering::SeqCst),
        0,
        "cluster work held the wallet indefinitely"
    );
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
    h.gateway.mint_parked.store(false, Ordering::Relaxed);
    h.connect(WALLET, SESSION_A).await;
    assert_eq!(h.island_changed()[0].1.island_id, "island-current");
}

#[tokio::test(start_paused = true)]
async fn overload_releases_rejected_recovery_and_eventually_uses_the_current_source() {
    let h = harness();
    h.gateway.mint_parked.store(true, Ordering::Relaxed);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("old", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    for index in 0..100 {
        h.bus.inject(
            &format!("peer.{WALLET}.cluster_change"),
            &change(&format!("move-{index}"), SESSION_A).encode_to_vec(),
        );
    }
    h.snapshot(WALLET, "current", SESSION_B);
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
    assert_eq!(
        h.subscriber.in_flight.load(Ordering::SeqCst),
        wallet_queue::MAX_WALLET_WORK
    );
    let ticket = h
        .subscriber
        .recovery
        .begin(WALLET, SESSION_B)
        .expect("a rejected queue entry must release the recovery coalescer");
    drop(ticket);
    tokio::time::advance(wallet_queue::WORK_DEADLINE).await;
    h.subscriber.settle().await;
    assert!(h.gateway.minted().is_empty());
    assert!(h.island_changed().is_empty());
    h.gateway.mint_parked.store(false, Ordering::Relaxed);
    h.connect(WALLET, SESSION_B).await;
    let assignments = h.island_changed();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].1.island_id, "island-current");
    assert!(assignments[0].0.ends_with(SESSION_B));
    assert_eq!(h.subscriber.queue.outstanding_work(), 0);
}

#[tokio::test]
async fn invalid_or_oversized_feed_cannot_cancel_or_mutate_a_valid_recovery() {
    let h = harness();
    h.snapshot(WALLET, "current", SESSION_A);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_A.as_bytes());
    h.gateway.mint_entered.notified().await;
    let oversized = change(&"x".repeat(MAX_FEED_BYTES), SESSION_B).encode_to_vec();
    assert!(oversized.len() > MAX_FEED_BYTES);
    h.bus
        .inject(&format!("peer.{WALLET}.cluster_change"), &oversized);
    h.bus.inject(
        &format!("peer.{}.cluster_change", "x".repeat(MAX_FEED_SUBJECT_BYTES)),
        &change("bad", SESSION_B).encode_to_vec(),
    );
    h.bus.inject(
        "peer.invalid.cluster_change",
        &change("bad", SESSION_B).encode_to_vec(),
    );
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 1);
    assert!(h.subscriber.peers.mirrored(WALLET).is_none());
    assert!(h.subscriber.peers.mirrored("invalid").is_none());
    release.notify_one();
    h.subscriber.settle().await;
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed()[0].1.island_id, "island-current");
}

#[tokio::test]
async fn all_concurrent_settle_waiters_observe_the_last_completion() {
    let h = harness();
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("current", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    let mut waiters = Vec::new();
    for _ in 0..8 {
        let subscriber = h.subscriber.clone();
        waiters.push(tokio::spawn(async move { subscriber.settle().await }));
    }
    tokio::task::yield_now().await;
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(1), async {
        for waiter in waiters {
            waiter.await.unwrap();
        }
    })
    .await
    .expect("one completion must wake every drain waiter");
    assert_eq!(h.island_changed().len(), 1);
}

#[tokio::test]
async fn stopping_unsubscribes_before_the_drain() {
    let h = harness();
    h.subscriber.stop().await;
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    assert!(
        h.gateway.minted().is_empty(),
        "an event arriving after the stop belongs to another member of the queue group"
    );
}

/// A panic inside one item's work must not take the lane's drainer with it: the completion still
/// runs, so the in-flight count returns to zero and everything queued behind it still runs.
#[tokio::test]
async fn a_panicking_work_item_leaves_the_lane_draining() {
    let h = harness();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = ran.clone();

    h.subscriber.spawn(WALLET.to_string(), |_| {
        Box::pin(async {
            panic!("a work item panicked");
        })
    });
    h.subscriber.spawn(WALLET.to_string(), move |_| {
        Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
        })
    });
    tokio::time::timeout(Duration::from_secs(5), h.subscriber.settle())
        .await
        .expect("a panic must not strand the lane's in-flight count");

    assert!(
        ran.load(Ordering::SeqCst),
        "the item queued behind a panicking one still runs"
    );
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
}

#[tokio::test]
async fn work_for_one_wallet_leaves_no_queue_entry_behind() {
    let h = harness();
    for cluster in ["c1", "c2", "c3"] {
        h.feed(WALLET, &change(cluster, SESSION_A)).await;
    }
    assert_eq!(h.subscriber.queue.tracked_wallets(), 0);
}

/// The lane is FIFO, and its position is taken on the delivery thread, so a burst for one wallet
/// publishes in arrival order and every `from_island_id` names the room the peer actually left.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_for_one_wallet_publishes_in_arrival_order() {
    let h = harness();
    let clusters = ["c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8"];
    for cluster in clusters {
        h.bus.inject(
            &format!("peer.{WALLET}.cluster_change"),
            &change(cluster, SESSION_A).encode_to_vec(),
        );
    }
    h.subscriber.settle().await;

    let published = h.island_changed();
    assert_eq!(published.len(), clusters.len());
    for (index, cluster) in clusters.iter().enumerate() {
        assert_eq!(published[index].1.island_id, format!("island-{cluster}"));
        let previous = index
            .checked_sub(1)
            .map(|before| format!("island-{}", clusters[before]));
        assert_eq!(
            published[index].1.from_island_id, previous,
            "event {index} must chain off the one that arrived before it"
        );
    }
}

/// The drain is bounded, so one request with no timeout of its own cannot hold shutdown open until
/// the orchestrator kills the process.
#[tokio::test]
async fn stopping_gives_up_on_work_that_will_never_finish() {
    let h = harness_with(ClusterConfig {
        drain_timeout_ms: 5,
        ..config()
    });
    h.gateway.mint_parked.store(true, Ordering::Relaxed);
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("c1", SESSION_A).encode_to_vec(),
    );

    tokio::time::timeout(Duration::from_secs(5), h.subscriber.stop())
        .await
        .expect("a parked gateway call must not hold the stop open");
}

#[tokio::test(start_paused = true)]
async fn a_mint_released_after_stop_cannot_publish() {
    for drain_timeout_ms in [0, 5] {
        let h = harness_with(ClusterConfig {
            drain_timeout_ms,
            ..config()
        });
        let release = Arc::new(Notify::new());
        *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
        h.bus.inject(
            &format!("peer.{WALLET}.cluster_change"),
            &change("old", SESSION_A).encode_to_vec(),
        );
        h.gateway.mint_entered.notified().await;
        h.subscriber.stop().await;
        release.notify_one();
        h.subscriber.settle().await;
        assert!(
            h.island_changed().is_empty(),
            "stopped mint published with drain={drain_timeout_ms}"
        );
        assert_eq!(h.subscriber.queue.outstanding_work(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn an_old_stop_cancels_only_its_incarnation_when_start_races_the_drain() {
    let h = harness_with(ClusterConfig {
        drain_timeout_ms: 50,
        ..config()
    });
    let old_release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(old_release.clone());
    for cluster in ["old-active", "old-queued"] {
        h.bus.inject(
            &format!("peer.{WALLET}.cluster_change"),
            &change(cluster, SESSION_A).encode_to_vec(),
        );
    }
    h.gateway.mint_entered.notified().await;
    let subscriber = h.subscriber.clone();
    let stopping = tokio::spawn(async move { subscriber.stop().await });
    tokio::task::yield_now().await;
    assert!(h.subscriber.lifecycle.lock().unwrap().owner.is_none());
    h.subscriber.start();
    let new_release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(new_release.clone());
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("new", SESSION_B).encode_to_vec(),
    );
    tokio::time::advance(Duration::from_millis(50)).await;
    tokio::time::timeout(Duration::from_millis(1), stopping)
        .await
        .expect("old stop must not drain the new incarnation")
        .unwrap();
    h.gateway.mint_entered.notified().await;
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 1);
    assert!(h.island_changed().is_empty());
    old_release.notify_one();
    new_release.notify_one();
    h.subscriber.settle().await;
    let assignments = h.island_changed();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].1.island_id, "island-new");
    assert!(assignments[0].0.ends_with(SESSION_B));
    assert_eq!(h.gateway.minted().len(), 1);
}

#[tokio::test]
async fn callbacks_cloned_before_unsubscribe_cannot_enter_a_restarted_subscriber() {
    let h = harness();
    let old_owner = h
        .subscriber
        .lifecycle
        .lock()
        .unwrap()
        .owner
        .as_ref()
        .unwrap()
        .clone();
    let old_change = h.subscriber.guarded(
        old_owner.clone(),
        "cluster_change",
        |me, owner, wallet, data| {
            me.handle_cluster_change(owner, wallet, data);
        },
    );
    let old_mirror = h
        .subscriber
        .guarded(old_owner, "cluster_change", |me, _, wallet, data| {
            me.handle_assignment_mirror(wallet, data);
        });
    h.subscriber.stop().await;
    h.subscriber.start();
    h.snapshot(WALLET, "new", SESSION_B);
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus
        .inject(&format!("peer.{WALLET}.connect"), SESSION_B.as_bytes());
    h.gateway.mint_entered.notified().await;
    for stale in [old_change, old_mirror] {
        stale(
            &format!("peer.{WALLET}.cluster_change"),
            &change("old", SESSION_A).encode_to_vec(),
        );
    }
    assert!(h.subscriber.peers.mirrored(WALLET).is_none());
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 1);
    release.notify_one();
    h.subscriber.settle().await;
    assert_eq!(h.gateway.minted().len(), 1);
    assert_eq!(h.island_changed()[0].1.island_id, "island-new");
}

#[tokio::test]
async fn graceful_stop_allows_already_accepted_work_to_finish() {
    let h = harness();
    let release = Arc::new(Notify::new());
    *h.gateway.mint_pause.lock().unwrap() = Some(release.clone());
    h.bus.inject(
        &format!("peer.{WALLET}.cluster_change"),
        &change("current", SESSION_A).encode_to_vec(),
    );
    h.gateway.mint_entered.notified().await;
    let subscriber = h.subscriber.clone();
    let stopping = tokio::spawn(async move { subscriber.stop().await });
    tokio::task::yield_now().await;
    assert!(h.subscriber.lifecycle.lock().unwrap().owner.is_none());
    release.notify_one();
    stopping.await.unwrap();
    assert_eq!(h.island_changed()[0].1.island_id, "island-current");
    assert_eq!(h.subscriber.in_flight.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_padded_connect_payload_is_not_a_session_key() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, &format!(" {SESSION_B} ")).await;

    assert_eq!(h.island_changed().len(), 1);
    assert_eq!(h.gateway.holds_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn only_a_lower_cased_ephemeral_address_is_a_session_key() {
    assert!(is_session_key(SESSION_A));
    assert!(!is_session_key(&SESSION_A.to_uppercase()));
    assert!(!is_session_key("0x123"));
    assert!(!is_session_key(""));
    assert!(!is_session_key(
        "0xgggggggggggggggggggggggggggggggggggggggg"
    ));
}

#[test]
fn the_revocation_boundary_is_the_second_after_this_one() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let boundary = next_whole_second_unix();
    assert!(boundary == now + 1 || boundary == now + 2);
}

/// Pins the re-signing the takeover rests on: the payload survives untouched apart from `nbf`,
/// and the signature is still one the API secret verifies.
#[test]
fn moving_a_token_s_not_before_leaves_every_other_claim_alone() {
    let minted = crate::livekit::AccessToken::new(
        "key",
        "secret",
        WALLET,
        crate::livekit::join_grants("island-c1"),
    )
    .to_jwt()
    .unwrap();
    let moved = crate::livekit::with_not_before(&minted, "secret", 1_700_000_000).unwrap();

    let decode = |jwt: &str| -> serde_json::Value {
        let payload = jwt.split('.').nth(1).unwrap();
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
    };
    let before = decode(&minted);
    let after = decode(&moved);
    assert_eq!(after["nbf"], 1_700_000_000u64);
    for claim in ["iss", "sub", "exp", "video"] {
        assert_eq!(
            before[claim], after[claim],
            "{claim} must survive untouched"
        );
    }

    let signed_again = crate::livekit::sign_hs256(
        "secret",
        &URL_SAFE_NO_PAD
            .decode(moved.split('.').next().unwrap())
            .unwrap(),
        &URL_SAFE_NO_PAD
            .decode(moved.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        signed_again, moved,
        "the token must verify under the secret"
    );
}

#[tokio::test]
async fn poisoned_lane_registry_still_drains_new_work() {
    let h = harness();
    let queue = h.subscriber.queue.clone();
    queue.poison_for_test();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = ran.clone();
    h.subscriber.spawn(WALLET.to_string(), move |_| {
        Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
        })
    });
    tokio::time::timeout(Duration::from_secs(5), h.subscriber.settle())
        .await
        .unwrap();
    assert!(ran.load(Ordering::SeqCst));
    assert_eq!(queue.tracked_wallets(), 0);
}
