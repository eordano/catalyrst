use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::*;
use crate::nats::InProcessBus;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SESSION_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(Debug, Clone, PartialEq, Eq)]
struct MintCall {
    wallet: String,
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
    holds_error: AtomicBool,
    holds_calls: AtomicUsize,
    mint_parked: AtomicBool,
}

impl FakeGateway {
    fn minted(&self) -> Vec<MintCall> {
        self.minted.lock().unwrap().clone()
    }

    fn removals(&self) -> Vec<RemoveCall> {
        self.removals.lock().unwrap().clone()
    }

    fn queue_removals(&self, outcomes: impl IntoIterator<Item = Result<Removal, String>>) {
        *self.remove_outcomes.lock().unwrap() = outcomes.into_iter().collect();
    }
}

#[async_trait]
impl ClusterGateway for FakeGateway {
    async fn is_denied(&self, wallet: &str) -> Result<bool, GatewayError> {
        if self.deny_error.load(Ordering::Relaxed) {
            return Err(GatewayError("ban store unreachable".into()));
        }
        Ok(self.denied.lock().unwrap().contains(wallet))
    }

    async fn mint_connection_string(
        &self,
        wallet: &str,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError> {
        if self.mint_parked.load(Ordering::Relaxed) {
            futures::future::pending::<()>().await;
        }
        if self.mint_error.load(Ordering::Relaxed) {
            return Err(GatewayError("livekit refused the mint".into()));
        }
        self.minted.lock().unwrap().push(MintCall {
            wallet: wallet.to_string(),
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
            Some(Err(error)) => Err(GatewayError(error)),
            None => Ok(Removal::Removed),
        }
    }

    async fn holds_participant(&self, room: &str, _identity: &str) -> Result<bool, GatewayError> {
        self.holds_calls.fetch_add(1, Ordering::Relaxed);
        if self.holds_error.load(Ordering::Relaxed) {
            return Err(GatewayError("livekit lookup failed".into()));
        }
        Ok(self
            .holds
            .lock()
            .unwrap()
            .get(room)
            .copied()
            .unwrap_or(false))
    }
}

struct Harness {
    bus: Arc<InProcessBus>,
    gateway: Arc<FakeGateway>,
    subscriber: Arc<ClusterSubscriber>,
}

fn config() -> ClusterConfig {
    ClusterConfig {
        nats_url: Some("nats://127.0.0.1:4222".into()),
        enabled: true,
        queue_group: "catalyrst-comms-cluster".into(),
        takeover_retry_delay_ms: 0,
        ..ClusterConfig::default()
    }
}

fn harness_with(config: ClusterConfig) -> Harness {
    let bus = Arc::new(InProcessBus::new());
    let gateway = Arc::new(FakeGateway::default());
    let subscriber = ClusterSubscriber::new(
        bus.clone(),
        gateway.clone(),
        Arc::new(ClusterPeerState::default()),
        config,
    );
    subscriber.start();
    Harness {
        bus,
        gateway,
        subscriber,
    }
}

fn harness() -> Harness {
    harness_with(config())
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
    async fn feed(&self, wallet: &str, change: &PeerClusterChange) {
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
    let mut takeover = change("c2", SESSION_B);
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
        "the boundary is the next whole second, so a token minted in this same second is revoked"
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
async fn an_eviction_is_retried_and_the_replacement_is_still_minted() {
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
    assert_eq!(
        h.gateway.minted().len(),
        1,
        "a takeover the eviction could not complete still hands the new device a token"
    );
}

#[tokio::test]
async fn a_takeover_that_names_no_displaced_cluster_removes_nothing() {
    let h = harness();
    let mut takeover = change("c2", SESSION_B);
    takeover.displaced_session = SESSION_A.into();
    h.feed(WALLET, &takeover).await;
    assert!(h.gateway.removals().is_empty());
    assert_eq!(h.gateway.minted().len(), 1);
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
async fn a_connect_payload_that_is_not_a_session_key_falls_back_to_the_mirror() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, "legacy-connector-payload").await;

    let published = h.island_changed();
    assert_eq!(
        published[1].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}"),
        "an older connector's payload cannot be judged, so the mirror's session stands in"
    );
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

/// The mirror rides the un-grouped copy of the same subject, so it is fed even when the grouped
/// minting subscription belongs to another replica.
#[tokio::test]
async fn the_mirror_answers_a_reconnect_this_replica_never_minted() {
    let h = harness();
    h.subscriber.peers().record_mirror(
        WALLET,
        MirrorEntry {
            cluster_id: "c9".into(),
            session: SESSION_A.into(),
        },
    );
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

/// Upstream lower-cases the connect payload and nothing else. Trimming one into shape would let a
/// padded value be judged against the mirror's session instead of standing aside for it.
#[tokio::test]
async fn a_padded_connect_payload_is_not_a_session_key() {
    let h = harness();
    h.feed(WALLET, &change("c1", SESSION_A)).await;
    h.connect(WALLET, &format!(" {SESSION_B} ")).await;

    let published = h.island_changed();
    assert_eq!(published.len(), 2);
    assert_eq!(
        published[1].0,
        format!("engine.peer.{WALLET}.island_changed.{SESSION_A}"),
        "a payload with whitespace cannot be judged, so the mirror's session stands in"
    );
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
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = queue.lanes.lock().unwrap();
        panic!("poison registry");
    }));
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
