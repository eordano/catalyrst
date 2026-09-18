//! The broker link: core NATS, no JetStream or durable consumer acknowledgement.
//!
//! Inbound carries the island assignments comms mints from the cluster feed, plus the topology and
//! service-discovery snapshots the stats surface serves. Outbound carries this connector's own
//! session lifecycle, which is what makes a reconnect that did not move answerable at all.
//!
//! A publish with no live connection is dropped rather than queued: the events are edge-triggered
//! announcements of a socket that exists now, so a backlog delivered after a reconnect would speak
//! for sessions that are already gone.
//! A bounded outbox carries immediate announcements; periodic live-session snapshots bypass it.
//! A private-inbox round trip confirms broker progress, not downstream token delivery.

use crate::config::NatsConfig;
use crate::feed::{dispatch, SUBSCRIPTIONS};
use crate::registry::{ControlPositionPermit, PeersRegistry, ReconciliationPeer};
use crate::state::AppState;
use catalyrst_types::control_position::{ControlPosition, SUBJECT as CONTROL_POSITION_SUBJECT};
use futures::stream::{select_all, StreamExt};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio::time::Instant;

mod reconciliation;
use reconciliation::Reconciliation;

/// Long enough that a broker restart is not hammered, short enough that a recovered one is picked
/// up within a few seconds. Only ever waited on before a connection has been built at all: once
/// one exists, reconnection is the client's own job.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(5);
const RECONCILE_PACING: Duration = Duration::from_millis(10);
const BROKER_DEADLINE: Duration = Duration::from_secs(2);
pub const OUTBOUND_CAPACITY: usize = 1024;
const MAX_ANNOUNCEMENT_BYTES: usize = 1024;

/// The seam every outbound announcement goes through, so the socket layer can be driven without a
/// broker.
pub trait FeedPublisher: Send + Sync {
    /// Accepts an announcement into the local bounded queue, not a broker/consumer acknowledgement.
    /// `false` means unavailable, overloaded or oversized; live sessions are reconciled later.
    fn publish(&self, subject: String, payload: Vec<u8>) -> bool;

    /// Publishes a socket-scoped connect announcement. Implementations which retain an outbox
    /// must preserve the target so it can be checked again immediately before publication.
    fn publish_connect(&self, target: ReconciliationPeer) -> bool {
        self.publish(
            crate::feed::connect_subject(&target.address),
            target.session.into_bytes(),
        )
    }

    fn publish_control_position(&self, announcement: ControlPositionAnnouncement) -> bool {
        self.publish(CONTROL_POSITION_SUBJECT.to_owned(), announcement.payload)
    }

    fn is_enabled(&self) -> bool;

    fn is_connected(&self) -> bool;

    /// Announcements discarded on disconnect, overload, stale ownership or failed delivery. Surfaced on `/stats/health` so a
    /// roomless deployment can be told from a healthy one without reading the log.
    fn dropped(&self) -> u64 {
        0
    }
}

/// What a deployment without a broker URL gets: every announcement is dropped and nothing is
/// subscribed, which is byte-identical to not wiring the feed at all.
pub struct NoopPublisher;

impl FeedPublisher for NoopPublisher {
    fn publish(&self, _subject: String, _payload: Vec<u8>) -> bool {
        false
    }

    fn is_enabled(&self) -> bool {
        false
    }

    fn is_connected(&self) -> bool {
        false
    }
}

/// The in-process fake: records what would have gone to the broker, in order.
#[derive(Default)]
pub struct RecordingPublisher {
    sent: Mutex<Vec<(String, Vec<u8>)>>,
}

impl RecordingPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn sent(&self) -> Vec<(String, Vec<u8>)> {
        self.sent.lock().clone()
    }

    pub fn subjects(&self) -> Vec<String> {
        self.sent.lock().iter().map(|(s, _)| s.clone()).collect()
    }
}

impl FeedPublisher for RecordingPublisher {
    fn publish(&self, subject: String, payload: Vec<u8>) -> bool {
        self.sent.lock().push((subject, payload));
        true
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn is_connected(&self) -> bool {
        true
    }
}

enum Outbound {
    Raw(Box<str>, Box<[u8]>),
    Connect(ReconciliationPeer),
    ControlPosition(ControlPositionAnnouncement),
}

pub struct ControlPositionAnnouncement {
    payload: Vec<u8>,
    permit: ControlPositionPermit,
}

impl ControlPositionAnnouncement {
    pub(crate) fn new(record: ControlPosition, permit: ControlPositionPermit) -> Option<Self> {
        let close = record.position.is_none();
        if !permit.matches(&record.wallet, &record.session, record.epoch, close) {
            return None;
        }
        Some(Self {
            payload: record.encode()?,
            permit,
        })
    }
}

struct Link {
    client: Mutex<Option<async_nats::Client>>,
    ready_revision: AtomicU64,
    revision: AtomicU64,
    changed: Notify,
}

impl Default for Link {
    fn default() -> Self {
        Self {
            client: Mutex::new(None),
            ready_revision: AtomicU64::new(0),
            revision: AtomicU64::new(1),
            changed: Notify::new(),
        }
    }
}

impl Link {
    fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_one();
    }

    fn is_connected(&self) -> bool {
        self.ready_revision.load(Ordering::SeqCst) == self.revision.load(Ordering::SeqCst)
            && self.client.lock().as_ref().is_some_and(|client| {
                client.connection_state() == async_nats::connection::State::Connected
            })
    }
}

struct LinkLifetime(Arc<Link>);

impl Drop for LinkLifetime {
    fn drop(&mut self) {
        self.0.invalidate();
        self.0.client.lock().take();
    }
}

struct BrokerFence {
    inbox: String,
    replies: async_nats::Subscriber,
    sequence: u64,
}

impl BrokerFence {
    async fn new(client: &async_nats::Client) -> Option<Self> {
        let inbox = client.new_inbox();
        let replies = client.subscribe(inbox.clone()).await.ok()?;
        Some(Self {
            inbox,
            replies,
            sequence: 0,
        })
    }

    async fn confirm(&mut self, client: &async_nats::Client) -> Result<(), ()> {
        self.sequence = self.sequence.checked_add(1).ok_or(())?;
        let expected = self.sequence.to_be_bytes();
        client
            .publish(self.inbox.clone(), expected.to_vec().into())
            .await
            .map_err(|_| ())?;
        while let Some(reply) = self.replies.next().await {
            if reply.payload.as_ref() == expected {
                return Ok(());
            }
        }
        Err(())
    }
}

pub struct NatsBus {
    cfg: NatsConfig,
    tx: mpsc::Sender<Outbound>,
    rx: Mutex<Option<mpsc::Receiver<Outbound>>>,
    link: Mutex<Arc<Link>>,
    dropped: AtomicU64,
}

impl NatsBus {
    pub fn new(cfg: NatsConfig) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(OUTBOUND_CAPACITY);
        Arc::new(Self {
            cfg,
            tx,
            rx: Mutex::new(Some(rx)),
            link: Mutex::new(Arc::new(Link::default())),
            dropped: AtomicU64::new(0),
        })
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Starts the link. Returns immediately: the first connection is established by the
    /// supervising task, so a broker that is down at boot delays no startup and needs no ordering
    /// against it in the unit file.
    pub fn start(self: &Arc<Self>, state: AppState) -> Option<tokio::task::JoinHandle<()>> {
        let url = self.cfg.url.clone()?;
        let rx = self.rx.lock().take()?;
        let bus = Arc::clone(self);
        Some(tokio::spawn(async move {
            supervise(bus, url, rx, state).await;
        }))
    }
}

impl FeedPublisher for NatsBus {
    fn dropped(&self) -> u64 {
        self.dropped_count()
    }

    fn publish(&self, subject: String, payload: Vec<u8>) -> bool {
        if !self.is_connected() || subject.len() + payload.len() > MAX_ANNOUNCEMENT_BYTES {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if self
            .tx
            .try_send(Outbound::Raw(
                subject.into_boxed_str(),
                payload.into_boxed_slice(),
            ))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn publish_connect(&self, target: ReconciliationPeer) -> bool {
        let bytes = target.address.len() + target.session.len() + "peer..connect".len();
        if !self.is_connected() || bytes > MAX_ANNOUNCEMENT_BYTES {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if self.tx.try_send(Outbound::Connect(target)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn publish_control_position(&self, announcement: ControlPositionAnnouncement) -> bool {
        if !self.is_connected()
            || CONTROL_POSITION_SUBJECT.len() + announcement.payload.len() > MAX_ANNOUNCEMENT_BYTES
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if self
            .tx
            .try_send(Outbound::ControlPosition(announcement))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn is_enabled(&self) -> bool {
        self.cfg.url.is_some()
    }

    fn is_connected(&self) -> bool {
        self.link.lock().is_connected()
    }
}

fn connect_options(cfg: &NatsConfig, link: &Arc<Link>) -> async_nats::ConnectOptions {
    let link = Arc::downgrade(link);
    async_nats::ConnectOptions::new()
        .name(cfg.server_name.clone())
        .client_capacity(OUTBOUND_CAPACITY)
        .subscription_capacity(OUTBOUND_CAPACITY)
        .event_callback(move |event| {
            let link = link.clone();
            async move {
                if matches!(
                    event,
                    async_nats::Event::Connected
                        | async_nats::Event::Disconnected
                        | async_nats::Event::Closed
                        | async_nats::Event::SlowConsumer(_)
                ) {
                    if let Some(link) = link.upgrade() {
                        link.invalidate();
                    }
                }
            }
        })
}

/// Rebuilds the pipeline only for a connection that was never established or has ended: once the
/// client exists it reconnects on its own. An authentication failure is retried on the same
/// schedule as a refused socket, so a credential rotation under a running broker is not an outage.
async fn supervise(
    bus: Arc<NatsBus>,
    url: String,
    mut rx: mpsc::Receiver<Outbound>,
    state: AppState,
) {
    loop {
        let link = Arc::new(Link::default());
        *bus.link.lock() = link.clone();
        let lifetime = LinkLifetime(link.clone());
        match connect_options(&bus.cfg, &link).connect(&url).await {
            Ok(client) => {
                *link.client.lock() = Some(client.clone());
                tracing::info!("archipelago feed socket connected; awaiting subscriptions");
                run(&client, &mut rx, &state, &bus, &link).await;
                tracing::warn!("archipelago feed link ended; rebuilding");
            }
            Err(_) => {
                tracing::warn!("archipelago feed connect failed");
            }
        }
        drop(lifetime);
        discard_pending(&mut rx, &bus);
        tokio::time::sleep(RECONNECT_BACKOFF).await;
    }
}

/// Runs until subscriptions end or a broker acknowledgement fails its bounded deadline.
/// Recovery rebuilds from the registry instead of retrying a historical announcement.
async fn run(
    client: &async_nats::Client,
    rx: &mut mpsc::Receiver<Outbound>,
    state: &AppState,
    bus: &NatsBus,
    link: &Link,
) {
    let mut subscribers = Vec::with_capacity(SUBSCRIPTIONS.len());
    for subject in SUBSCRIPTIONS {
        match client.subscribe(subject).await {
            Ok(subscriber) => subscribers.push(subscriber),
            Err(_) => {
                tracing::error!(subject = %subject, reason = "subscribe failed", "cannot subscribe; rebuilding the link");
                return;
            }
        }
    }
    let Some(mut fence) = BrokerFence::new(client).await else {
        return;
    };
    tracing::info!(
        subjects = SUBSCRIPTIONS.len(),
        "archipelago feed subscriptions queued; awaiting broker acknowledgement"
    );
    discard_pending(rx, bus);
    let mut pending = Reconciliation::new(Instant::now());
    let mut next_announcement = Instant::now();
    let mut reconcile = tokio::time::interval(RECONCILE_INTERVAL);
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut inbound = select_all(subscribers);
    loop {
        tokio::select! {
            _ = link.changed.notified() => {
                discard_pending(rx, bus);
                pending.reset(Instant::now());
                if !restore_link(client, link, &mut fence).await {
                    if client.connection_state() == async_nats::connection::State::Disconnected {
                        continue;
                    }
                    return;
                }
                pending.refill(&state.registry, Instant::now());
            }
            _ = reconcile.tick() => {
                if !restore_link(client, link, &mut fence).await {
                    if client.connection_state() != async_nats::connection::State::Connected {
                        continue;
                    }
                    return;
                }
                pending.refill(&state.registry, Instant::now());
            }
            message = inbound.next() => {
                let Some(message) = message else { return };
                dispatch(state, message.subject.as_str(), &message.payload);
            }
            outbound = rx.recv() => {
                let Some(outbound) = outbound else { return };
                let (subject, payload) = match outbound {
                    Outbound::Raw(subject, payload) => {
                        if !announcement_is_current(state, &subject, &payload) {
                            bus.dropped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        (subject.into_string(), payload.into_vec())
                    }
                    Outbound::Connect(target) => {
                        if !state.registry.reconciliation_peer_is_current(&target) {
                            bus.dropped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        (
                            crate::feed::connect_subject(&target.address),
                            target.session.as_bytes().to_vec(),
                        )
                    }
                    Outbound::ControlPosition(announcement) => {
                        if !control_position_announcement_is_current(
                            &state.registry,
                            &announcement,
                        ) {
                            bus.dropped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        (CONTROL_POSITION_SUBJECT.to_owned(), announcement.payload)
                    }
                };
                if !link.is_connected() || !publish_confirmed(client, subject, payload, &mut fence).await {
                    bus.dropped.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
            _ = tokio::time::sleep_until(next_announcement), if !pending.is_empty() && link.is_connected() => {
                if let Some(target) = pending.next(&state.registry) {
                    if !publish_confirmed(client, crate::feed::connect_subject(&target.address), target.session.as_bytes().to_vec(), &mut fence).await {
                        bus.dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                next_announcement = Instant::now() + RECONCILE_PACING;
            }
        }
    }
}

fn control_position_announcement_is_current(
    registry: &PeersRegistry,
    announcement: &ControlPositionAnnouncement,
) -> bool {
    announcement.permit.is_close()
        || registry.control_position_permit_is_current(&announcement.permit)
}

fn discard_pending(rx: &mut mpsc::Receiver<Outbound>, bus: &NatsBus) {
    while rx.try_recv().is_ok() {
        bus.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

async fn restore_link(client: &async_nats::Client, link: &Link, fence: &mut BrokerFence) -> bool {
    if client.connection_state() != async_nats::connection::State::Connected {
        return false;
    }
    let revision = link.revision.load(Ordering::SeqCst);
    if matches!(
        tokio::time::timeout(BROKER_DEADLINE, fence.confirm(client)).await,
        Ok(Ok(()))
    ) && revision == link.revision.load(Ordering::SeqCst)
        && client.connection_state() == async_nats::connection::State::Connected
    {
        link.ready_revision.store(revision, Ordering::SeqCst);
        link.is_connected()
    } else {
        false
    }
}

async fn publish_confirmed(
    client: &async_nats::Client,
    subject: String,
    payload: Vec<u8>,
    fence: &mut BrokerFence,
) -> bool {
    matches!(
        tokio::time::timeout(BROKER_DEADLINE, async {
            client
                .publish(subject, payload.into())
                .await
                .map_err(|_| ())?;
            fence.confirm(client).await
        })
        .await,
        Ok(Ok(()))
    )
}

fn announcement_is_current(state: &AppState, subject: &str, payload: &[u8]) -> bool {
    let mut tokens = subject.split('.');
    match (tokens.next(), tokens.next(), tokens.next(), tokens.next()) {
        (Some("peer"), Some(address), Some("connect"), None) => std::str::from_utf8(payload)
            .ok()
            .and_then(|session| state.registry.peer_link(address, session))
            .is_some_and(|peer| !peer.is_closed()),
        (Some("peer"), Some(address), Some("disconnect"), None) => {
            disconnect_is_current(&state.registry, address)
        }
        _ => false,
    }
}

fn disconnect_is_current(registry: &crate::registry::PeersRegistry, address: &str) -> bool {
    !registry.has_any_peer(address)
}

/// Publishes one `peer.{address}.connect` for every session this replica holds, and returns how
/// many the publisher accepted.
///
/// Announcements made while the link was down are dropped, not queued, and the cluster feed is
/// edge-triggered: without this pass a session that completed its handshake during an outage --
/// including every socket accepted while the first connection was still being dialled at boot --
/// stays roomless until its cluster changes on its own.
pub fn reannounce_all(state: &AppState, publisher: &dyn FeedPublisher) -> usize {
    state
        .registry
        .reconciliation_snapshot(false)
        .into_iter()
        .filter(|target| publisher.publish_connect(target.clone()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::PeersRegistry;

    fn control_record(epoch: u64, sequence: u64, position: Option<[f32; 3]>) -> ControlPosition {
        ControlPosition {
            audience: "realm-a".into(),
            wallet: "0x000000000000000000000000000000000000000a".into(),
            session: "0x000000000000000000000000000000000000000b".into(),
            epoch,
            sequence,
            realm: position.map(|_| "main".to_owned()).unwrap_or_default(),
            position,
        }
    }

    #[test]
    fn a_publish_without_a_connection_is_dropped_and_counted() {
        let bus = NatsBus::new(NatsConfig {
            url: Some("nats://127.0.0.1:4222".into()),
            ..NatsConfig::default()
        });
        assert!(bus.is_enabled());
        assert!(!bus.is_connected());
        assert!(!bus.publish("peer.0xa.connect".into(), b"0xs1".to_vec()));
        assert_eq!(bus.dropped_count(), 1);
    }

    #[test]
    fn an_unconfigured_bus_publishes_nothing_and_subscribes_nothing() {
        let bus = NatsBus::new(NatsConfig::default());
        assert!(!bus.is_enabled());
        assert!(!bus.publish("peer.0xa.connect".into(), b"0xs1".to_vec()));
        assert_eq!(bus.dropped_count(), 1);
    }

    #[test]
    fn the_recording_publisher_keeps_what_it_was_handed() {
        let publisher = RecordingPublisher::new();
        assert!(publisher.publish("peer.0xa.connect".into(), b"0xs1".to_vec()));
        assert_eq!(publisher.subjects(), vec!["peer.0xa.connect".to_string()]);
        assert_eq!(publisher.sent()[0].1, b"0xs1".to_vec());
    }

    #[test]
    fn a_queued_disconnect_is_stale_while_an_additive_socket_remains_live() {
        let registry = PeersRegistry::new();
        let (first, _events) = registry.on_v4_peer_connected("0xa", "0xs", "old");
        let (_current, _events) = registry.on_v4_peer_connected("0xa", "0xs", "current");
        registry.on_v4_peer_disconnected("0xa", first.id);

        assert!(!disconnect_is_current(&registry, "0xa"));
    }

    #[test]
    fn control_positions_require_an_exact_live_permit_and_an_authorized_close_survives_removal() {
        let registry = PeersRegistry::new();
        let record = control_record(7, 1, Some([1.0, 2.0, 3.0]));
        let (link, _events) = registry.on_v4_peer_connected_with_realm_epoch(
            &record.wallet,
            &record.session,
            "connection-7",
            true,
            record.epoch,
        );
        let permit = registry
            .control_position_permit(&record.wallet, &record.session, record.epoch, false)
            .unwrap();
        let live = ControlPositionAnnouncement::new(record.clone(), permit).unwrap();
        assert!(control_position_announcement_is_current(&registry, &live));

        link.close();
        assert!(!control_position_announcement_is_current(&registry, &live));
        let (_newer, _events) = registry.on_v4_peer_connected_with_realm_epoch(
            &record.wallet,
            &record.session,
            "connection-8",
            true,
            8,
        );
        let close_record = control_record(7, 2, None);
        let close_permit = registry
            .control_position_permit(
                &close_record.wallet,
                &close_record.session,
                close_record.epoch,
                true,
            )
            .unwrap();
        let close = ControlPositionAnnouncement::new(close_record, close_permit).unwrap();
        assert!(!control_position_announcement_is_current(&registry, &live));
        registry.on_v4_peer_disconnected(&record.wallet, link.id);
        assert!(control_position_announcement_is_current(&registry, &close));

        let publisher = RecordingPublisher::new();
        assert!(publisher.publish_control_position(close));
        assert_eq!(publisher.subjects(), vec![CONTROL_POSITION_SUBJECT]);
        assert!(ControlPosition::decode(&publisher.sent()[0].1).is_some());
    }
}
