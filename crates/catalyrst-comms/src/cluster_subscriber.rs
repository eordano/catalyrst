//! Turns Pulse's cluster feed into LiveKit connection strings.
//!
//! Per inbound `peer.*.cluster_change`: extract the wallet from the subject, decode, serialize per
//! wallet, run the platform access gate, evict any session this event displaces, mint a token for
//! the cluster's island room, publish the legacy `IslandChangedMessage` the WS Connector already
//! forwards, and record the assignment so the next event can name the room the peer came from.
//!
//! Per inbound `peer.*.connect`, query Pulse's current wallet/session assignment and revalidate
//! after minting. A cached event mirror alone cannot establish current ownership after a gap.
//!
//! Off unless the subscriber is enabled and a broker is configured; off it subscribes to nothing
//! and is indistinguishable from the component not existing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use prost::Message as _;
use tokio::sync::Notify;

use catalyrst_pulse::decentraland::kernel::comms::v3::IslandChangedMessage;
use catalyrst_pulse::decentraland::pulse::PeerClusterSnapshot;
use catalyrst_pulse::PeerClusterChange;

use crate::assignment_fence::{AssignmentDocument, AssignmentFence, TokenFence};
use crate::config::{ClusterConfig, MAX_ISLAND_TOKEN_TTL_SECONDS};
use crate::livekit::{island_room_name, Removal};
use crate::nats::{NatsBus, NatsHandler, NatsSubscription, PublishOutcome};
use crate::peer_state::{ClusterPeerState, MirrorEntry, PeerAssignment};

pub const CLUSTER_CHANGE_SUBJECT: &str = "peer.*.cluster_change";
pub const PEER_CONNECT_SUBJECT: &str = "peer.*.connect";

const TAKEOVER_ATTEMPTS: u32 = 3;
const RECOVERY_DEADLINE: Duration = Duration::from_secs(10);
// LiveKit protocol's verifier accepts an expired JWT for one minute to match go-jose's former
// DefaultLeeway. The extra two seconds cover NumericDate's whole-second precision and scheduling
// across the expiry boundary. Keep the real-server regression in island_ownership_live aligned.
const LIVEKIT_TOKEN_LEEWAY: Duration = Duration::from_secs(60);
const TOKEN_EXPIRY_SAFETY: Duration = Duration::from_secs(2);
const MAX_FEED_BYTES: usize = 64 * 1024;
const MAX_FEED_SUBJECT_BYTES: usize = 256;

mod recovery;
use recovery::{RecoveryOwners, RecoveryTicket};
mod takeover;
use takeover::{TakeoverOwners, TakeoverTicket};
pub mod wallet_queue;
pub use wallet_queue::WalletQueue;
use wallet_queue::{QueuedWork, WorkOwner};

#[derive(Clone, Copy, thiserror::Error)]
#[error("cluster gateway operation failed")]
pub struct GatewayError;

impl std::fmt::Debug for GatewayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GatewayError")
    }
}

impl GatewayError {
    pub fn new(_private_detail: impl Into<String>) -> Self {
        Self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IslandOccupant {
    Absent,
    Session(String),
    Unattributed,
}

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
        session: Option<&str>,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError>;

    async fn mint_fenced_connection_string(
        &self,
        _wallet: &str,
        _room: &str,
        _ttl_seconds: u64,
        _fence: &TokenFence,
    ) -> Result<String, GatewayError> {
        Err(GatewayError::new(
            "gateway does not support fenced island tokens",
        ))
    }

    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError>;

    async fn holds_participant(&self, room: &str, identity: &str) -> Result<bool, GatewayError>;

    async fn island_occupant(
        &self,
        room: &str,
        wallet: &str,
    ) -> Result<IslandOccupant, GatewayError> {
        self.holds_participant(room, wallet).await.map(|held| {
            if held {
                IslandOccupant::Unattributed
            } else {
                IslandOccupant::Absent
            }
        })
    }
}

pub struct ClusterSubscriber {
    bus: Arc<dyn NatsBus>,
    gateway: Arc<dyn ClusterGateway>,
    peers: Arc<ClusterPeerState>,
    config: ClusterConfig,
    queue: Arc<WalletQueue>,
    lifecycle: Mutex<Lifecycle>,
    in_flight: AtomicUsize,
    idle: Notify,
    recovery: Arc<RecoveryOwners>,
    takeovers: Arc<TakeoverOwners>,
    assignment_fence: Option<Arc<dyn AssignmentFence>>,
}

#[derive(Default)]
struct Lifecycle {
    subscriptions: Vec<NatsSubscription>,
    owner: Option<Arc<WorkOwner>>,
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
/// LiveKit Cloud revokes tokens whose `nbf` predates the boundary at second granularity, and the minter
/// stamps `nbf` with the mint second, so "now" would spare a token minted in this same second. The
/// next whole second is the boundary every already-minted token is before, and the replacement is
/// minted with its `nbf` set to it: LiveKit's `nbf` leeway leaves it usable at once.
/// Self-hosted LiveKit ignores the cutoff; this value does not protect against old-token reuse there.
fn next_whole_second_unix() -> u64 {
    now_unix_ms().div_euclid(1000) as u64 + 1
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn now_unix_seconds() -> u64 {
    now_unix_ms().div_euclid(1000).max(0) as u64
}

fn fenced_token_expires_at_unix(ttl_seconds: u64) -> u64 {
    now_unix_seconds().saturating_add(ttl_seconds)
}

fn fenced_token_is_fresh(expires_at_unix: Option<u64>) -> bool {
    expires_at_unix.is_some_and(|expires_at| {
        expires_at > now_unix_seconds().saturating_add(TOKEN_EXPIRY_SAFETY.as_secs())
    })
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
        Self::new_with_assignment_fence(bus, gateway, peers, config, None)
    }

    pub fn new_with_assignment_fence(
        bus: Arc<dyn NatsBus>,
        gateway: Arc<dyn ClusterGateway>,
        peers: Arc<ClusterPeerState>,
        mut config: ClusterConfig,
        assignment_fence: Option<Arc<dyn AssignmentFence>>,
    ) -> Arc<Self> {
        config.island_token_ttl_seconds = config
            .island_token_ttl_seconds
            .min(MAX_ISLAND_TOKEN_TTL_SECONDS);
        Arc::new(Self {
            bus,
            gateway,
            peers,
            config,
            queue: Arc::new(WalletQueue::default()),
            lifecycle: Mutex::new(Lifecycle::default()),
            in_flight: AtomicUsize::new(0),
            idle: Notify::new(),
            recovery: Arc::new(RecoveryOwners::default()),
            takeovers: Arc::new(TakeoverOwners::default()),
            assignment_fence,
        })
    }

    pub fn peers(&self) -> &Arc<ClusterPeerState> {
        &self.peers
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.config.enabled && self.bus.is_ready()
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
        let mut lifecycle = self.lifecycle.lock().unwrap();
        if lifecycle.owner.is_some() {
            return;
        }
        let owner = Arc::new(WorkOwner::default());
        lifecycle.owner = Some(owner.clone());
        lifecycle.subscriptions.push(self.bus.subscribe(
            CLUSTER_CHANGE_SUBJECT,
            Some(&queue_group),
            self.guarded(
                owner.clone(),
                "cluster_change",
                |me, owner, wallet, data| me.handle_cluster_change(owner, wallet, data),
            ),
        ));
        lifecycle.subscriptions.push(self.bus.subscribe(
            CLUSTER_CHANGE_SUBJECT,
            None,
            self.guarded(owner.clone(), "cluster_change", |me, _, wallet, data| {
                me.handle_assignment_mirror(wallet, data)
            }),
        ));
        lifecycle.subscriptions.push(self.bus.subscribe(
            PEER_CONNECT_SUBJECT,
            Some(&queue_group),
            self.guarded(owner, "connect", |me, owner, wallet, data| {
                me.handle_peer_connect(owner, wallet, data)
            }),
        ));
        drop(lifecycle);

        self.bus.connect();
        tracing::info!(queue_group = %queue_group, "cluster subscriber started");
    }

    /// Stops taking events, then drains whatever is mid-flight.
    ///
    /// In that order on purpose: an event arriving during the drain lands on another member of the
    /// queue group instead of being minted against a connection that is about to close.
    ///
    /// The graceful drain is bounded by `CLUSTER_DRAIN_TIMEOUT_MS`; 0 cancels immediately.
    /// After that grace, pending work is discarded and running futures are cancelled and joined.
    /// Only this incarnation is drained/cancelled, even when start races the old stop.
    /// Already submitted external side effects cannot be undone by cancelling their waiter.
    ///
    /// A bounded grace is required because
    /// this service has no stop timeout of its own, and a mint parked on a LiveKit request or a
    /// publish parked on a broker that is gone would otherwise hold shutdown open until the
    /// orchestrator kills the process.
    pub async fn stop(&self) {
        let owner = {
            let mut lifecycle = self.lifecycle.lock().unwrap();
            let owner = lifecycle.owner.take();
            lifecycle.subscriptions.clear();
            self.recovery.clear();
            owner
        };
        let Some(owner) = owner else { return };
        let budget = self.config.drain_timeout_ms;
        if budget > 0
            && tokio::time::timeout(Duration::from_millis(budget), owner.settle())
                .await
                .is_err()
        {
            tracing::warn!(
                in_flight = self.in_flight.load(Ordering::SeqCst),
                drain_timeout_ms = budget,
                "gave up draining the cluster subscriber"
            );
        }
        self.queue.cancel(&owner);
        owner.settle().await;
        tracing::info!("cluster subscriber stopped taking events");
    }

    /// Resolves once nothing is in flight.
    pub async fn settle(&self) {
        loop {
            let idle = self.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if self.in_flight.load(Ordering::SeqCst) == 0 {
                return;
            }
            idle.await;
        }
    }

    /// Wraps a callback so the wallet is parsed once and nothing escapes into the delivery loop:
    /// a panic there would stop delivery on every subject sharing the connection, not just this
    /// one.
    fn guarded(
        self: &Arc<Self>,
        owner: Arc<WorkOwner>,
        what: &'static str,
        handle: fn(Arc<Self>, Arc<WorkOwner>, String, Vec<u8>),
    ) -> NatsHandler {
        let weak = Arc::downgrade(self);
        Arc::new(move |subject: &str, data: &[u8]| {
            if subject.len() > MAX_FEED_SUBJECT_BYTES || data.len() > MAX_FEED_BYTES {
                crate::metrics::cluster_feed_oversized();
                return;
            }
            let Some(me) = Weak::upgrade(&weak) else {
                return;
            };
            let lifecycle = me.lifecycle.lock().unwrap();
            if !lifecycle
                .owner
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &owner))
            {
                return;
            }
            let Some(wallet) = wallet_from_subject(subject).filter(|wallet| is_session_key(wallet))
            else {
                crate::metrics::cluster_feed_invalid_subject();
                tracing::warn!(what, "feed subject does not contain a wallet address");
                return;
            };
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handle(me.clone(), owner.clone(), wallet, data.to_vec());
            }))
            .is_err()
            {
                tracing::error!(what, "cluster feed callback panicked");
            }
        })
    }

    #[cfg(test)]
    fn spawn<F>(self: &Arc<Self>, wallet: String, work: F)
    where
        F: FnOnce(Arc<Self>) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
    {
        let lifecycle = self.lifecycle.lock().unwrap();
        self.spawn_scoped(lifecycle.owner.as_ref().unwrap().clone(), wallet, work);
    }

    /// Queues one message's work, taking its place in the wallet's lane before returning, so the
    /// order this is called in is the order the work runs in.
    fn spawn_scoped<F>(self: &Arc<Self>, owner: Arc<WorkOwner>, wallet: String, work: F) -> bool
    where
        F: FnOnce(Arc<Self>) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
    {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let me = self.clone();
        let queued: QueuedWork = Box::pin(async move { work(me.clone()).await });
        let settled = self.clone();
        self.queue.enqueue_scoped(
            owner,
            &wallet,
            queued,
            Box::new(move || {
                settled.in_flight.fetch_sub(1, Ordering::SeqCst);
                settled.idle.notify_waiters();
            }),
        )
    }

    /// Decodes one feed event and queues its work.
    ///
    /// The empty-`cluster_id` guard sits after the received counter because such an event is a
    /// payload problem rather than a decode one, and unguarded it would dump every such peer into
    /// one shared `island-` room.
    fn handle_cluster_change(
        self: Arc<Self>,
        owner: Arc<WorkOwner>,
        wallet: String,
        data: Vec<u8>,
    ) {
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

        self.recovery.invalidate(&wallet);

        let takeover = (self.config.self_hosted_token_quarantine
            && !change.displaced_session.is_empty()
            && change.displaced_cluster_id == change.cluster_id)
            .then(|| {
                self.takeovers
                    .prepare(&wallet, &change.cluster_id, &change.session, &change.realm)
            });
        let (registration, ticket) = takeover
            .map(|(registration, ticket)| (Some(registration), Some(ticket)))
            .unwrap_or_default();
        let for_wallet = wallet.clone();
        let accepted = self.spawn_scoped(owner, wallet, move |me| {
            Box::pin(async move {
                me.process_cluster_change_inner(&for_wallet, &change, ticket)
                    .await
            })
        });
        if accepted {
            if let Some(registration) = registration {
                registration.commit();
            }
        }
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
        self.recovery.invalidate(&wallet);
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
    /// Session shape is strict, including whitespace. Repeated announcements for a currently
    /// recovering session are coalesced, so a slow mint is not starved by periodic retries.
    fn handle_peer_connect(self: Arc<Self>, owner: Arc<WorkOwner>, wallet: String, data: Vec<u8>) {
        crate::metrics::cluster_connect_received();
        let session = String::from_utf8_lossy(&data).to_lowercase();
        if !is_session_key(&wallet) || !is_session_key(&session) {
            crate::metrics::cluster_reannounce_unresolved();
            return;
        }
        let Some(ticket) = self.recovery.begin(&wallet, &session) else {
            return;
        };
        let for_wallet = wallet.clone();
        self.spawn_scoped(owner, wallet, move |me| {
            Box::pin(async move { me.recover_guarded(&for_wallet, &session, ticket).await })
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
            Err(_) => {
                crate::metrics::cluster_access_check_failed();
                tracing::warn!(%wallet, reason = "gateway failure", "access check failed; allowing");
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
    /// Only a submitted exchange updates the previous-room hint. A failed or unconfirmed exchange
    /// cannot establish that hint. Even submission is not a client receipt or a successful join:
    /// core NATS has no consumer acknowledgement and current-state reconciliation is still needed.
    pub async fn process_cluster_change(&self, wallet: &str, change: &PeerClusterChange) {
        self.process_cluster_change_inner(wallet, change, None)
            .await;
    }

    async fn process_cluster_change_inner(
        &self,
        wallet: &str,
        change: &PeerClusterChange,
        takeover: Option<TakeoverTicket>,
    ) {
        if self.is_denied(wallet).await {
            crate::metrics::cluster_banned_skipped();
            tracing::info!(
                %wallet,
                cluster_id = %change.cluster_id,
                "skipping a banned wallet's cluster assignment"
            );
            return;
        }

        if self.assignment_fence.is_some()
            && self.process_fenced_cluster_change(wallet, change).await
        {
            return;
        }

        let revocation_boundary = if change.displaced_session.is_empty() {
            None
        } else {
            Some(next_whole_second_unix())
        };
        let takeover_source = if let Some(boundary) = revocation_boundary {
            let Some(expected) = self.lookup_current(wallet, &change.session).await else {
                crate::metrics::cluster_recovery_stale();
                return;
            };
            if expected.cluster_id != change.cluster_id || expected.realm != change.realm {
                crate::metrics::cluster_recovery_stale();
                return;
            }
            if !self
                .evict_displaced_session(wallet, change, boundary, takeover.as_ref())
                .await
            {
                return;
            }
            let Some(current) = self.lookup_current(wallet, &change.session).await else {
                crate::metrics::cluster_recovery_stale();
                return;
            };
            if !snapshot_extends(&expected, &current) {
                crate::metrics::cluster_recovery_stale();
                return;
            }
            Some(current)
        } else {
            None
        };

        let room = island_room_name(&change.cluster_id);

        let conn_str = match self
            .gateway
            .mint_connection_string(
                wallet,
                is_session_key(&change.session).then_some(change.session.as_str()),
                &room,
                self.config.island_token_ttl_seconds,
                revocation_boundary,
            )
            .await
        {
            Ok(conn_str) => conn_str,
            Err(_) => {
                tracing::error!(%wallet, %room, reason = "gateway failure", "cannot mint an island token");
                return;
            }
        };
        crate::metrics::cluster_token_minted();

        if let Some(expected) = takeover_source {
            let Some(current) = self.lookup_current(wallet, &change.session).await else {
                crate::metrics::cluster_recovery_stale();
                return;
            };
            if !snapshot_extends(&expected, &current) {
                crate::metrics::cluster_recovery_stale();
                return;
            }
        }

        self.publish_assignment(wallet, &change.cluster_id, &change.session, conn_str)
            .await;
    }

    /// Serializes the authority row from the live-source lookup through token mint and publish.
    /// This fences the control owner and assignment publication, not LiveKit admission: LiveKit
    /// does not interpret the token fence and an already issued JWT remains usable until expiry.
    /// Consequently, owner replacements that would require destructive SFU work fail closed.
    ///
    /// Returns false for work the authority does not cover, which the caller takes down the
    /// legacy path: a wallet no v4 socket has claimed, or a session that cannot be a v4 owner.
    /// The same server admits unmodified v3 clients, and they write no authority row.
    async fn process_fenced_cluster_change(
        &self,
        wallet: &str,
        change: &PeerClusterChange,
    ) -> bool {
        if !is_session_key(wallet) || !is_session_key(&change.session) {
            return false;
        }
        let authority = self
            .assignment_fence
            .as_ref()
            .expect("fenced path requires authority");
        let lease = match authority.lock_realm(wallet, &change.session).await {
            Ok(lease) => lease,
            Err(crate::assignment_fence::FenceError::Unowned) => return false,
            Err(error) => {
                crate::metrics::cluster_recovery_stale();
                tracing::warn!(%wallet, %error, "realm assignment authority rejected cluster work");
                return true;
            }
        };
        let Some(snapshot) = self.lookup_current(wallet, &change.session).await else {
            crate::metrics::cluster_reannounce_unresolved();
            return true;
        };
        if snapshot.cluster_id != change.cluster_id || snapshot.realm != change.realm {
            crate::metrics::cluster_recovery_stale();
            return true;
        }
        if !change.displaced_session.is_empty() {
            crate::metrics::cluster_takeover_failed();
            tracing::warn!(
                %wallet,
                displaced_session = %change.displaced_session,
                "fenced takeover withheld: LiveKit removal is not session-conditional"
            );
            return true;
        }

        let room = island_room_name(&change.cluster_id);
        let fence = lease.token_fence();
        let token_expires_at_unix =
            fenced_token_expires_at_unix(self.config.island_token_ttl_seconds);
        let conn_str = match self
            .gateway
            .mint_fenced_connection_string(
                wallet,
                &room,
                self.config.island_token_ttl_seconds,
                &fence,
            )
            .await
        {
            Ok(conn_str) => conn_str,
            Err(_) => {
                tracing::error!(%wallet, %room, "cannot mint a fenced island token");
                return true;
            }
        };
        crate::metrics::cluster_token_minted();
        let Some(current) = self.lookup_current(wallet, &change.session).await else {
            crate::metrics::cluster_recovery_stale();
            return true;
        };
        if !snapshot_extends(&snapshot, &current) {
            crate::metrics::cluster_recovery_stale();
            return true;
        }
        let assignment = AssignmentDocument {
            island_id: room,
            connection_string: conn_str,
            from_island_id: lease.previous_island(),
            token_expires_at_unix: Some(token_expires_at_unix),
            peers: Default::default(),
        };
        if let Err(error) = lease.commit(assignment).await {
            crate::metrics::cluster_publish_failed();
            tracing::warn!(%wallet, %error, "fenced realm assignment commit rejected");
            return true;
        }
        crate::metrics::cluster_published();
        tracing::info!(
            %wallet,
            source_incarnation = %snapshot.source_incarnation,
            source_pass = snapshot.pass,
            owner_epoch = fence.owner_epoch,
            assignment_revision = fence.assignment_revision,
            fencing_token = fence.fencing_token,
            "fenced realm assignment committed"
        );
        true
    }

    async fn publish_assignment(
        &self,
        wallet: &str,
        cluster_id: &str,
        session: &str,
        conn_str: String,
    ) {
        let room = island_room_name(cluster_id);

        let message = IslandChangedMessage {
            island_id: room.clone(),
            conn_str,
            from_island_id: self.peers.assignment(wallet).map(|previous| previous.room),
            peers: HashMap::new(),
            assignment_context: None,
        };

        let subject = if is_session_key(session) {
            format!("engine.peer.{wallet}.island_changed.{session}")
        } else {
            format!("engine.peer.{wallet}.island_changed")
        };

        match self.bus.publish(&subject, message.encode_to_vec()).await {
            PublishOutcome::Submitted => {}
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
            PublishOutcome::Unconfirmed => {
                crate::metrics::cluster_publish_failed();
                crate::metrics::cluster_publish_unconfirmed();
                tracing::warn!(%wallet, "island_changed broker exchange unconfirmed; delivery unknown");
                return;
            }
        }

        crate::metrics::cluster_published();
        self.peers.record_assignment(
            wallet,
            PeerAssignment {
                cluster_id: cluster_id.into(),
                room,
                last_seen_ms: now_unix_ms(),
            },
        );
    }

    /// Requests removal by wallet and a Cloud revocation cutoff. Self-hosted LiveKit ignores the
    /// cutoff, so a same-room takeover waits out the bounded token lifetime plus the verifier's
    /// leeway and removes the identity again before any replacement token is published. Tokens
    /// are room-bound, so cross-room moves do not hold the new room behind that quarantine.
    /// Removal is not conditional on participant SID; source checks around minting reject a
    /// changed Pulse owner, but they do not provide a distributed ownership lease.
    ///
    /// Retried because this runs on a background feed with nobody to report a transient LiveKit
    /// error to, and a failure here leaves two sessions in comms until one of them leaves.
    async fn evict_displaced_session(
        &self,
        wallet: &str,
        change: &PeerClusterChange,
        revoke_before: u64,
        takeover: Option<&TakeoverTicket>,
    ) -> bool {
        if change.displaced_cluster_id.is_empty() {
            crate::metrics::cluster_takeover_failed();
            tracing::warn!(
                %wallet,
                displaced_session = %change.displaced_session,
                "cannot evict a displaced session: the event names no displaced cluster"
            );
            return false;
        }

        let room = island_room_name(&change.displaced_cluster_id);
        if self
            .remove_displaced_participant(wallet, &room, revoke_before)
            .await
            .is_none()
        {
            return false;
        }
        if self.config.self_hosted_token_quarantine
            && change.displaced_cluster_id == change.cluster_id
        {
            let quarantine = Duration::from_secs(self.config.island_token_ttl_seconds)
                .saturating_add(LIVEKIT_TOKEN_LEEWAY)
                .saturating_add(TOKEN_EXPIRY_SAFETY);
            if let Some(takeover) = takeover {
                let deadline = tokio::time::Instant::now() + quarantine;
                loop {
                    tokio::select! {
                        biased;
                        successor = takeover.superseded() => {
                            let Some(current) = self.lookup_current(wallet, &successor.session).await else {
                                continue;
                            };
                            if current.cluster_id == successor.cluster && current.realm == successor.realm {
                                crate::metrics::cluster_recovery_stale();
                                return false;
                            }
                        }
                        _ = tokio::time::sleep_until(deadline) => break,
                    }
                }
            } else {
                tokio::time::sleep(quarantine).await;
            }
            if self
                .remove_displaced_participant(wallet, &room, revoke_before)
                .await
                .is_none()
            {
                return false;
            }
        }
        true
    }

    async fn remove_displaced_participant(
        &self,
        wallet: &str,
        room: &str,
        revoke_before: u64,
    ) -> Option<Removal> {
        for attempt in 1..=TAKEOVER_ATTEMPTS {
            match self
                .gateway
                .remove_participant(room, wallet, Some(revoke_before as i64))
                .await
            {
                Ok(Removal::Removed) => {
                    crate::metrics::cluster_takeover_evicted();
                    return Some(Removal::Removed);
                }
                Ok(Removal::Absent) => {
                    crate::metrics::cluster_takeover_absent();
                    tracing::debug!(
                        %wallet,
                        %room,
                        "the displaced session had already left; nothing to remove"
                    );
                    return Some(Removal::Absent);
                }
                Err(_) => {
                    if attempt == TAKEOVER_ATTEMPTS {
                        crate::metrics::cluster_takeover_failed();
                        tracing::warn!(
                            %wallet,
                            %room,
                            reason = "gateway failure",
                            "cannot evict a displaced session after every attempt"
                        );
                        return None;
                    }
                    let delay = self.config.takeover_retry_delay_ms * attempt as u64;
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
        None
    }

    /// Reconcile only an exact session, never a legacy or cached guess. A snapshot is not a lease:
    /// source restart, revision rollback or changed assignment during minting discards the candidate.
    /// Local change events cancel the whole attempt; another socket's connect is not ownership
    /// evidence and cannot cancel it. Legacy recovery is suppressed by a matching or unattributed
    /// SFU participant. Fenced recovery refills a cleared claim even when an old matching participant
    /// remains, but suppresses a healthy matching participant once the authority already holds the
    /// room. Other occupants rely on fenced LiveKit admission, never a blind RemoveParticipant.
    pub async fn process_peer_connect(&self, wallet: &str, session: &str) {
        if !is_session_key(wallet) || !is_session_key(session) {
            return;
        }
        let Some(ticket) = self.recovery.begin(wallet, session) else {
            return;
        };
        self.recover_guarded(wallet, session, ticket).await;
    }

    async fn recover_guarded(&self, wallet: &str, session: &str, ticket: RecoveryTicket) {
        tokio::select! {
            biased;
            _ = ticket.cancelled() => crate::metrics::cluster_recovery_stale(),
            _ = tokio::time::sleep(RECOVERY_DEADLINE) => crate::metrics::cluster_reannounce_unresolved(),
            _ = self.recover_current(wallet, session) => {}
        }
    }

    async fn lookup_current(&self, wallet: &str, session: &str) -> Option<PeerClusterSnapshot> {
        if !is_session_key(wallet) || !is_session_key(session) {
            return None;
        }
        let payload = self
            .bus
            .request(
                &format!("peer.{wallet}.cluster_lookup"),
                session.as_bytes().to_vec(),
            )
            .await
            .ok()?;
        let snapshot = PeerClusterSnapshot::decode(payload.as_slice()).ok()?;
        (snapshot.wallet == wallet
            && snapshot.session == session
            && !snapshot.cluster_id.is_empty()
            && !snapshot.source_incarnation.is_empty()
            && snapshot.source_incarnation.len() <= 256
            && snapshot.pass > 0)
            .then_some(snapshot)
    }

    async fn recover_current(&self, wallet: &str, session: &str) {
        let Some(snapshot) = self.lookup_current(wallet, session).await else {
            crate::metrics::cluster_reannounce_unresolved();
            return;
        };
        if self.is_denied(wallet).await {
            crate::metrics::cluster_banned_skipped();
            return;
        }
        if self.assignment_fence.is_some()
            && self
                .process_fenced_recovery(wallet, session, &snapshot)
                .await
        {
            return;
        }
        let room = island_room_name(&snapshot.cluster_id);
        if !self.recovery_room_available(wallet, session, &room).await {
            return;
        }

        crate::metrics::cluster_reannounce_attempted();
        let conn_str = match self
            .gateway
            .mint_connection_string(
                wallet,
                Some(session),
                &room,
                self.config.island_token_ttl_seconds,
                None,
            )
            .await
        {
            Ok(conn_str) => conn_str,
            Err(_) => return,
        };
        crate::metrics::cluster_token_minted();
        if !self.recovery_room_available(wallet, session, &room).await {
            return;
        }
        let Some(current) = self.lookup_current(wallet, session).await else {
            crate::metrics::cluster_recovery_stale();
            return;
        };
        if current.source_incarnation != snapshot.source_incarnation
            || current.pass < snapshot.pass
            || current.cluster_id != snapshot.cluster_id
            || current.realm != snapshot.realm
        {
            crate::metrics::cluster_recovery_stale();
            return;
        }
        self.publish_assignment(wallet, &current.cluster_id, session, conn_str)
            .await;
    }

    /// Rebuilds an assignment for a socket that owns the shared v4 authority row. `Unowned` is
    /// the only result that returns false and permits legacy recovery. A current-room assignment is
    /// reused while its token is fresh, or while its exact-session participant remains healthy. An
    /// expired, unstamped assignment is renewed once the exact participant is gone. A newly claimed
    /// row has no assignment, so an old participant with the same session must not suppress refill.
    async fn process_fenced_recovery(
        &self,
        wallet: &str,
        session: &str,
        initial: &PeerClusterSnapshot,
    ) -> bool {
        let authority = self
            .assignment_fence
            .as_ref()
            .expect("fenced recovery requires authority");
        let lease = match authority.lock_realm(wallet, session).await {
            Ok(lease) => lease,
            Err(crate::assignment_fence::FenceError::Unowned) => return false,
            Err(error) => {
                crate::metrics::cluster_recovery_stale();
                tracing::warn!(%wallet, %error, "realm assignment authority rejected recovery");
                return true;
            }
        };
        let Some(snapshot) = self.lookup_current(wallet, session).await else {
            crate::metrics::cluster_reannounce_unresolved();
            return true;
        };
        if !snapshot_extends(initial, &snapshot) {
            crate::metrics::cluster_recovery_stale();
            return true;
        }
        let room = island_room_name(&snapshot.cluster_id);
        let previous_island = lease.previous_island();
        let previous_is_current = previous_island.as_deref() == Some(room.as_str());
        let previous_token_is_fresh = fenced_token_is_fresh(lease.previous_token_expires_at_unix());
        crate::metrics::cluster_reannounce_attempted();
        let Some(occupant) = self.fenced_recovery_room_occupant(wallet, &room).await else {
            return true;
        };
        if previous_is_current
            && (matches!(&occupant, IslandOccupant::Session(owner) if owner == session)
                || (previous_token_is_fresh
                    && matches!(
                        occupant,
                        IslandOccupant::Absent | IslandOccupant::Session(_)
                    )))
        {
            crate::metrics::cluster_reannounce_suppressed();
            return true;
        }
        let fence = lease.token_fence();
        let token_expires_at_unix =
            fenced_token_expires_at_unix(self.config.island_token_ttl_seconds);
        let conn_str = match self
            .gateway
            .mint_fenced_connection_string(
                wallet,
                &room,
                self.config.island_token_ttl_seconds,
                &fence,
            )
            .await
        {
            Ok(conn_str) => conn_str,
            Err(_) => {
                tracing::error!(%wallet, %room, "cannot mint a fenced island token for recovery");
                return true;
            }
        };
        crate::metrics::cluster_token_minted();
        if self
            .fenced_recovery_room_occupant(wallet, &room)
            .await
            .is_none()
        {
            return true;
        }
        let Some(current) = self.lookup_current(wallet, session).await else {
            crate::metrics::cluster_recovery_stale();
            return true;
        };
        if !snapshot_extends(&snapshot, &current) {
            crate::metrics::cluster_recovery_stale();
            return true;
        }
        let assignment = AssignmentDocument {
            island_id: room,
            connection_string: conn_str,
            from_island_id: previous_island,
            token_expires_at_unix: Some(token_expires_at_unix),
            peers: Default::default(),
        };
        if let Err(error) = lease.commit(assignment).await {
            crate::metrics::cluster_publish_failed();
            tracing::warn!(%wallet, %error, "fenced realm recovery commit rejected");
            return true;
        }
        crate::metrics::cluster_published();
        tracing::info!(
            %wallet,
            source_incarnation = %current.source_incarnation,
            source_pass = current.pass,
            owner_epoch = fence.owner_epoch,
            assignment_revision = fence.assignment_revision,
            fencing_token = fence.fencing_token,
            "fenced realm assignment recovered"
        );
        true
    }

    async fn fenced_recovery_room_occupant(
        &self,
        wallet: &str,
        room: &str,
    ) -> Option<IslandOccupant> {
        match self.gateway.island_occupant(room, wallet).await {
            Ok(occupant @ (IslandOccupant::Absent | IslandOccupant::Session(_))) => Some(occupant),
            Ok(IslandOccupant::Unattributed) => {
                crate::metrics::cluster_reannounce_suppressed();
                None
            }
            Err(_) => {
                crate::metrics::cluster_reannounce_check_failed();
                tracing::warn!(
                    %wallet,
                    %room,
                    reason = "gateway failure",
                    "cannot safely recover a fenced island assignment"
                );
                None
            }
        }
    }

    async fn recovery_room_available(&self, wallet: &str, session: &str, room: &str) -> bool {
        let occupant = match self.gateway.island_occupant(room, wallet).await {
            Ok(occupant) => occupant,
            Err(_) => {
                crate::metrics::cluster_reannounce_check_failed();
                tracing::warn!(
                    %wallet,
                    %room,
                    reason = "gateway failure",
                    "cannot tell whether the wallet already holds its island; not re-announcing"
                );
                return false;
            }
        };
        if matches!(occupant, IslandOccupant::Unattributed)
            || matches!(&occupant, IslandOccupant::Session(owner) if owner == session)
        {
            crate::metrics::cluster_reannounce_suppressed();
            return false;
        }
        true
    }
}

fn snapshot_extends(expected: &PeerClusterSnapshot, current: &PeerClusterSnapshot) -> bool {
    current.source_incarnation == expected.source_incarnation
        && current.pass >= expected.pass
        && current.cluster_id == expected.cluster_id
        && current.realm == expected.realm
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
