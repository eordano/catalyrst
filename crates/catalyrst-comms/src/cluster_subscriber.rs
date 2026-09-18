//! Turns Pulse's cluster feed into LiveKit connection strings.
//!
//! Per inbound `peer.*.cluster_change`: extract the wallet from the subject, decode, serialize per
//! wallet, run the platform access gate, evict any session this event displaces, mint a token for
//! the cluster's island room, publish the legacy `IslandChangedMessage` the WS Connector already
//! forwards, and record the assignment so the next event can name the room the peer came from.
//!
//! Per inbound `peer.*.connect`, the wallet's last known island is re-announced through the same
//! path: Pulse speaks only when a peer's cluster changes, so without this a client that reconnects
//! standing still is never handed a room.
//!
//! Off unless the subscriber is enabled and a broker is configured; off it subscribes to nothing
//! and is indistinguishable from the component not existing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::FutureExt as _;
use prost::Message as _;
use tokio::sync::Notify;

use catalyrst_pulse::decentraland::kernel::comms::v3::IslandChangedMessage;
use catalyrst_pulse::PeerClusterChange;

use crate::config::ClusterConfig;
use crate::livekit::{island_room_name, Removal};
use crate::nats::{NatsBus, NatsHandler, NatsSubscription, PublishOutcome};
use crate::peer_state::{ClusterPeerState, MirrorEntry, PeerAssignment};

pub const CLUSTER_CHANGE_SUBJECT: &str = "peer.*.cluster_change";
pub const PEER_CONNECT_SUBJECT: &str = "peer.*.connect";

const TAKEOVER_ATTEMPTS: u32 = 3;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct GatewayError(pub String);

/// Everything the subscriber needs beyond the bus: the access gate and the three LiveKit
/// server-API calls the handover rests on.
///
/// Each call surfaces a plain result. The fail-open and fail-closed decisions are deliberately
/// left to the call sites below, because the two call sites make opposite ones for opposite
/// reasons.
#[async_trait]
pub trait ClusterGateway: Send + Sync {
    async fn is_denied(&self, wallet: &str) -> Result<bool, GatewayError>;

    async fn mint_connection_string(
        &self,
        wallet: &str,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError>;

    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError>;

    async fn holds_participant(&self, room: &str, identity: &str) -> Result<bool, GatewayError>;
}

type QueuedWork = futures::future::BoxFuture<'static, ()>;
type Completion = Box<dyn FnOnce() + Send>;

/// Orders work per wallet: an out-of-order mint would publish a stale room and point the next
/// `from_island_id` at a room the peer was never told to join.
///
/// The FIFO position is taken synchronously, inside [`WalletQueue::enqueue`] and therefore on the
/// delivery thread itself, so run order is arrival order. Taking it inside a spawned task instead
/// would only make the work mutually exclusive, not ordered: two tasks spawned in order can reach
/// the lock in either one.
///
/// Local to this process. The queue group has no per-wallet replica affinity, so two events for
/// one wallet landing on different replicas can still publish out of order; scaling this service
/// out needs a per-wallet sequence from Pulse or wallet-affine routing first.
#[derive(Default)]
pub struct WalletQueue {
    lanes: Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<(QueuedWork, Completion)>>>,
}

impl WalletQueue {
    /// Takes the wallet's next FIFO position and returns, without awaiting anything.
    ///
    /// `done` runs after the work has finished AND after the lane has been retired if nothing is
    /// queued behind it, so a caller that waits on `done` observes no lane left over.
    pub fn enqueue(self: &Arc<Self>, wallet: &str, work: QueuedWork, done: Completion) {
        let mut queued = (work, done);
        let mut lanes = self
            .lanes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(lane) = lanes.get(wallet) {
            match lane.send(queued) {
                Ok(()) => return,
                Err(rejected) => queued = rejected.0,
            }
        }
        let (lane, pending) = tokio::sync::mpsc::unbounded_channel();
        let _ = lane.send(queued);
        lanes.insert(wallet.to_string(), lane);
        drop(lanes);

        let me = self.clone();
        let wallet = wallet.to_string();
        tokio::spawn(async move { me.drain(wallet, pending).await });
    }

    /// Runs one wallet's lane to exhaustion, then retires it.
    ///
    /// The emptiness check that retires the lane is made under the same lock `enqueue` sends
    /// under, so a send racing the retirement either lands in this lane or creates the next one.
    ///
    /// A panicking work item is caught rather than allowed to unwind out of the lane: an escaped
    /// panic would drop the completions of this item and of everything queued behind it, leaving
    /// the in-flight count permanently above zero and `settle` unable to resolve again.
    async fn drain(
        &self,
        wallet: String,
        mut pending: tokio::sync::mpsc::UnboundedReceiver<(QueuedWork, Completion)>,
    ) {
        let Ok((mut work, mut done)) = pending.try_recv() else {
            self.lanes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&wallet);
            while let Ok((_, orphaned)) = pending.try_recv() {
                orphaned();
            }
            return;
        };
        loop {
            if std::panic::AssertUnwindSafe(work)
                .catch_unwind()
                .await
                .is_err()
            {
                tracing::error!(%wallet, "cluster work panicked; the lane continues");
            }
            let next = {
                let mut lanes = self
                    .lanes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                match pending.try_recv() {
                    Ok(next) => Some(next),
                    Err(_) => {
                        lanes.remove(&wallet);
                        None
                    }
                }
            };
            done();
            match next {
                Some((next_work, next_done)) => {
                    work = next_work;
                    done = next_done;
                }
                None => break,
            }
        }
        while let Ok((_, orphaned)) = pending.try_recv() {
            orphaned();
        }
    }

    pub fn tracked_wallets(&self) -> usize {
        self.lanes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

pub struct ClusterSubscriber {
    bus: Arc<dyn NatsBus>,
    gateway: Arc<dyn ClusterGateway>,
    peers: Arc<ClusterPeerState>,
    config: ClusterConfig,
    queue: Arc<WalletQueue>,
    subscriptions: Mutex<Vec<NatsSubscription>>,
    in_flight: AtomicUsize,
    idle: Notify,
}

/// The lower-cased ephemeral address shape Pulse publishes as `session` and the WS Connector
/// registers a socket under. A value that is not one must never become a subject token.
pub fn is_session_key(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value.as_bytes()[2..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

/// The start of the second after the current one, as a revocation boundary.
///
/// LiveKit revokes tokens whose `nbf` predates the boundary at second granularity, and the minter
/// stamps `nbf` with the mint second, so "now" would spare a token minted in this same second. The
/// next whole second is the boundary every already-minted token is before, and the replacement is
/// minted with its `nbf` set to it: LiveKit's `nbf` leeway leaves it usable at once.
fn next_whole_second_unix() -> u64 {
    now_unix_ms().div_euclid(1000) as u64 + 1
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn wallet_from_subject(subject: &str) -> Option<String> {
    subject
        .split('.')
        .nth(1)
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
}

impl ClusterSubscriber {
    pub fn new(
        bus: Arc<dyn NatsBus>,
        gateway: Arc<dyn ClusterGateway>,
        peers: Arc<ClusterPeerState>,
        config: ClusterConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            bus,
            gateway,
            peers,
            config,
            queue: Arc::new(WalletQueue::default()),
            subscriptions: Mutex::new(Vec::new()),
            in_flight: AtomicUsize::new(0),
            idle: Notify::new(),
        })
    }

    pub fn peers(&self) -> &Arc<ClusterPeerState> {
        &self.peers
    }

    /// Registers the three subscriptions and kicks off the connection.
    ///
    /// The minting subscription is queue-grouped: without the group every replica would mint and
    /// publish, handing one client as many `island_changed` messages as there are replicas, each
    /// carrying a different token. The mirror copy is deliberately un-grouped so every replica
    /// agrees on every wallet's last published assignment, and the connect subscription is grouped
    /// like minting, because un-grouped every replica would answer the same reconnect and every
    /// join after the first would evict the one before it.
    pub fn start(self: &Arc<Self>) {
        if !self.config.enabled {
            tracing::info!(
                "cluster subscriber disabled (CLUSTER_SUBSCRIBER_ENABLED is not \"true\")"
            );
            return;
        }
        if !self.bus.is_enabled() {
            tracing::info!("cluster subscriber enabled but no broker is configured; staying idle");
            return;
        }

        let queue_group = self.config.queue_group.clone();
        let mut subscriptions = self.subscriptions.lock().unwrap();
        subscriptions.push(self.bus.subscribe(
            CLUSTER_CHANGE_SUBJECT,
            Some(&queue_group),
            self.guarded("cluster_change", |me, wallet, data| {
                me.handle_cluster_change(wallet, data)
            }),
        ));
        subscriptions.push(self.bus.subscribe(
            CLUSTER_CHANGE_SUBJECT,
            None,
            self.guarded("cluster_change", |me, wallet, data| {
                me.handle_assignment_mirror(wallet, data)
            }),
        ));
        subscriptions.push(self.bus.subscribe(
            PEER_CONNECT_SUBJECT,
            Some(&queue_group),
            self.guarded("connect", |me, wallet, data| {
                me.handle_peer_connect(wallet, data)
            }),
        ));
        drop(subscriptions);

        self.bus.connect();
        tracing::info!(queue_group = %queue_group, "cluster subscriber started");
    }

    /// Stops taking events, then drains whatever is mid-flight.
    ///
    /// In that order on purpose: an event arriving during the drain lands on another member of the
    /// queue group instead of being minted against a connection that is about to close.
    ///
    /// The drain is bounded by `CLUSTER_DRAIN_TIMEOUT_MS`, and a configured 0 skips it entirely:
    /// this service has no stop timeout of its own, and a mint parked on a LiveKit request or a
    /// publish parked on a broker that is gone would otherwise hold shutdown open until the
    /// orchestrator kills the process.
    pub async fn stop(&self) {
        let had = {
            let mut subscriptions = self.subscriptions.lock().unwrap();
            let had = !subscriptions.is_empty();
            subscriptions.clear();
            had
        };
        let budget = self.config.drain_timeout_ms;
        if budget > 0
            && tokio::time::timeout(Duration::from_millis(budget), self.settle())
                .await
                .is_err()
        {
            tracing::warn!(
                in_flight = self.in_flight.load(Ordering::SeqCst),
                drain_timeout_ms = budget,
                "gave up draining the cluster subscriber"
            );
        }
        if had {
            tracing::info!("cluster subscriber stopped taking events");
        }
    }

    /// Resolves once nothing is in flight.
    pub async fn settle(&self) {
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            self.idle.notified().await;
        }
    }

    /// Wraps a callback so the wallet is parsed once and nothing escapes into the delivery loop:
    /// a panic there would stop delivery on every subject sharing the connection, not just this
    /// one.
    fn guarded(
        self: &Arc<Self>,
        what: &'static str,
        handle: fn(Arc<Self>, String, Vec<u8>),
    ) -> NatsHandler {
        let weak = Arc::downgrade(self);
        Arc::new(move |subject: &str, data: &[u8]| {
            let Some(me) = Weak::upgrade(&weak) else {
                return;
            };
            let Some(wallet) = wallet_from_subject(subject) else {
                tracing::warn!(subject = %subject, what, "cannot extract a wallet from the subject");
                return;
            };
            handle(me, wallet, data.to_vec());
        })
    }

    /// Queues one message's work, taking its place in the wallet's lane before returning, so the
    /// order this is called in is the order the work runs in.
    fn spawn<F>(self: &Arc<Self>, wallet: String, work: F)
    where
        F: FnOnce(Arc<Self>) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
    {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let me = self.clone();
        let queued: QueuedWork = Box::pin(async move { work(me.clone()).await });
        let settled = self.clone();
        self.queue.enqueue(
            &wallet,
            queued,
            Box::new(move || {
                settled.in_flight.fetch_sub(1, Ordering::SeqCst);
                settled.idle.notify_one();
            }),
        );
    }

    /// Decodes one feed event and queues its work.
    ///
    /// The empty-`cluster_id` guard sits after the received counter because such an event is a
    /// payload problem rather than a decode one, and unguarded it would dump every such peer into
    /// one shared `island-` room.
    fn handle_cluster_change(self: Arc<Self>, wallet: String, data: Vec<u8>) {
        let change = match PeerClusterChange::decode(data.as_slice()) {
            Ok(change) => normalize(change),
            Err(error) => {
                tracing::error!(%wallet, %error, "cannot decode a cluster_change");
                return;
            }
        };
        crate::metrics::cluster_event_received();

        if change.cluster_id.is_empty() {
            tracing::warn!(%wallet, "cannot process a cluster_change with an empty cluster_id");
            return;
        }

        let for_wallet = wallet.clone();
        self.spawn(wallet, move |me| {
            Box::pin(async move { me.process_cluster_change(&for_wallet, &change).await })
        });
    }

    /// Refreshes the mirror only; minting stays exclusive to the queue-grouped subscription. The
    /// write is synchronous, so a connect queued behind this event already sees the entry.
    fn handle_assignment_mirror(self: Arc<Self>, wallet: String, data: Vec<u8>) {
        let Ok(change) = PeerClusterChange::decode(data.as_slice()).map(normalize) else {
            return;
        };
        if change.cluster_id.is_empty() {
            return;
        }
        self.peers.record_mirror(
            &wallet,
            MirrorEntry {
                cluster_id: change.cluster_id,
                session: change.session,
            },
        );
    }

    /// Queues a reconnect on the same lane as cluster changes, so within one process a reconnect
    /// cannot interleave with a move for the same wallet.
    ///
    /// The payload is decoded and lower-cased and nothing else: a value with surrounding
    /// whitespace is not a session key, and trimming one into shape would let it be judged against
    /// the mirror's session instead of standing aside for it.
    fn handle_peer_connect(self: Arc<Self>, wallet: String, data: Vec<u8>) {
        crate::metrics::cluster_connect_received();
        let session = String::from_utf8_lossy(&data).to_lowercase();
        let for_wallet = wallet.clone();
        self.spawn(wallet, move |me| {
            Box::pin(async move { me.process_peer_connect(&for_wallet, &session).await })
        });
    }

    /// The one gate in this service that fails open: a background feed has no caller to return an
    /// error to, and failing closed would stop island formation for everyone during an outage of
    /// the store behind it. Nothing is remembered on failure, so the next event retries.
    ///
    /// Only this platform's ban store is consulted. Upstream also ORs in a deny-list lookup, for
    /// which this platform has no equivalent store; see the stream notes.
    async fn is_denied(&self, wallet: &str) -> bool {
        match self.gateway.is_denied(wallet).await {
            Ok(denied) => denied,
            Err(error) => {
                crate::metrics::cluster_access_check_failed();
                tracing::warn!(%wallet, %error, "access check failed; allowing");
                false
            }
        }
    }

    /// Gates, evicts, mints and publishes one assignment.
    ///
    /// A repeat assignment is not suppressed: Pulse re-announces a cluster only after forgetting a
    /// peer, which is a reconnect that needs a fresh token.
    ///
    /// The publish is addressed to the session whenever the event names a valid one. An older
    /// Pulse sends none, and a malformed value must never become a subject token, so both fall
    /// back to the legacy four-token subject the connector delivers to the wallet's newest socket.
    ///
    /// A refused publish and a dropped one both count as a failed publish, because both mean the
    /// client was not told, and neither records the assignment: a falsely recorded one would point
    /// the next `from_island_id` at a room this peer was never handed.
    pub async fn process_cluster_change(&self, wallet: &str, change: &PeerClusterChange) {
        if self.is_denied(wallet).await {
            crate::metrics::cluster_banned_skipped();
            tracing::info!(
                %wallet,
                cluster_id = %change.cluster_id,
                "skipping a banned wallet's cluster assignment"
            );
            return;
        }

        let revocation_boundary = if change.displaced_session.is_empty() {
            None
        } else {
            Some(next_whole_second_unix())
        };
        if let Some(boundary) = revocation_boundary {
            self.evict_displaced_session(wallet, change, boundary).await;
        }

        let room = island_room_name(&change.cluster_id);

        let conn_str = match self
            .gateway
            .mint_connection_string(
                wallet,
                &room,
                self.config.island_token_ttl_seconds,
                revocation_boundary,
            )
            .await
        {
            Ok(conn_str) => conn_str,
            Err(error) => {
                tracing::error!(%wallet, %room, %error, "cannot mint an island token");
                return;
            }
        };
        crate::metrics::cluster_token_minted();

        let message = IslandChangedMessage {
            island_id: room.clone(),
            conn_str,
            from_island_id: self.peers.assignment(wallet).map(|previous| previous.room),
            peers: HashMap::new(),
        };

        let subject = if is_session_key(&change.session) {
            format!("engine.peer.{wallet}.island_changed.{}", change.session)
        } else {
            format!("engine.peer.{wallet}.island_changed")
        };

        match self.bus.publish(&subject, message.encode_to_vec()).await {
            PublishOutcome::Delivered => {}
            PublishOutcome::Failed => {
                crate::metrics::cluster_publish_failed();
                tracing::error!(%wallet, %subject, "island_changed publish refused");
                return;
            }
            PublishOutcome::NoConnection => {
                crate::metrics::cluster_publish_failed();
                tracing::error!(%wallet, %subject, "island_changed dropped: no broker connection");
                return;
            }
        }

        crate::metrics::cluster_published();
        self.peers.record_assignment(
            wallet,
            PeerAssignment {
                cluster_id: change.cluster_id.clone(),
                room,
                last_seen_ms: now_unix_ms(),
            },
        );
    }

    /// Removes the displaced session's participant from the room it was last published into and
    /// revokes every token minted for the wallet before the boundary.
    ///
    /// Retried because this runs on a background feed with nobody to report a transient LiveKit
    /// error to, and a failure here leaves two sessions in comms until one of them leaves.
    async fn evict_displaced_session(
        &self,
        wallet: &str,
        change: &PeerClusterChange,
        revoke_before: u64,
    ) {
        if change.displaced_cluster_id.is_empty() {
            crate::metrics::cluster_takeover_failed();
            tracing::warn!(
                %wallet,
                displaced_session = %change.displaced_session,
                "cannot evict a displaced session: the event names no displaced cluster"
            );
            return;
        }

        let room = island_room_name(&change.displaced_cluster_id);
        for attempt in 1..=TAKEOVER_ATTEMPTS {
            match self
                .gateway
                .remove_participant(&room, wallet, Some(revoke_before as i64))
                .await
            {
                Ok(Removal::Removed) => {
                    crate::metrics::cluster_takeover_evicted();
                    return;
                }
                Ok(Removal::Absent) => {
                    crate::metrics::cluster_takeover_absent();
                    tracing::debug!(
                        %wallet,
                        %room,
                        "the displaced session had already left; nothing to remove"
                    );
                    return;
                }
                Err(error) => {
                    if attempt == TAKEOVER_ATTEMPTS {
                        crate::metrics::cluster_takeover_failed();
                        tracing::warn!(
                            %wallet,
                            %room,
                            %error,
                            "cannot evict a displaced session after every attempt"
                        );
                        return;
                    }
                    let delay = self.config.takeover_retry_delay_ms * attempt as u64;
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
    }

    /// Re-announces the wallet's island because its comms session just started.
    ///
    /// A connect from a device other than the one Pulse last published for is a displaced session
    /// coming back; handing it the room would put it next to the live one under a single identity.
    /// A payload that is not a session key comes from an older connector and cannot be judged
    /// either way, so the mirror's own session stands in for it.
    ///
    /// The LiveKit lookup fails CLOSED, unlike the ban gate, and for the opposite reason. Only the
    /// signalling socket has to have dropped for this event to fire, so the peer is often still in
    /// its room; announcing it there again puts two participants under one identity and LiveKit
    /// ends the live one. Reading "cannot tell" as "not in the room" would do that to every
    /// reconnecting peer at once, which is exactly when this lookup is most likely to fail. A
    /// stranded peer costs one more reconnect; the other way costs it its session.
    pub async fn process_peer_connect(&self, wallet: &str, session: &str) {
        let Some(entry) = self.peers.mirrored(wallet) else {
            crate::metrics::cluster_reannounce_unresolved();
            return;
        };

        if is_session_key(session) && !entry.session.is_empty() && entry.session != session {
            crate::metrics::cluster_reannounce_skipped_other_session();
            return;
        }

        let room = island_room_name(&entry.cluster_id);
        let already_in_room = match self.gateway.holds_participant(&room, wallet).await {
            Ok(held) => held,
            Err(error) => {
                crate::metrics::cluster_reannounce_check_failed();
                tracing::warn!(
                    %wallet,
                    %room,
                    %error,
                    "cannot tell whether the wallet already holds its island; not re-announcing"
                );
                return;
            }
        };
        if already_in_room {
            crate::metrics::cluster_reannounce_suppressed();
            return;
        }

        crate::metrics::cluster_reannounce_attempted();
        let session = if is_session_key(session) {
            session.to_string()
        } else {
            entry.session.clone()
        };
        self.process_cluster_change(
            wallet,
            &PeerClusterChange {
                cluster_id: entry.cluster_id,
                realm: String::new(),
                session,
                displaced_session: String::new(),
                displaced_cluster_id: String::new(),
            },
        )
        .await;
    }
}

/// Lower-cases the session once at decode time, so minting, the mirror and the session-addressed
/// subject all agree on its casing.
fn normalize(mut change: PeerClusterChange) -> PeerClusterChange {
    change.session = change.session.to_lowercase();
    change.displaced_session = change.displaced_session.to_lowercase();
    change
}

#[cfg(test)]
mod tests;
