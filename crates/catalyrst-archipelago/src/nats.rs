//! The broker link: core NATS, no JetStream, fire and forget in both directions.
//!
//! Inbound carries the island assignments comms mints from the cluster feed, plus the topology and
//! service-discovery snapshots the stats surface serves. Outbound carries this connector's own
//! session lifecycle, which is what makes a reconnect that did not move answerable at all.
//!
//! A publish with no live connection is dropped rather than queued: the events are edge-triggered
//! announcements of a socket that exists now, so a backlog delivered after a reconnect would speak
//! for sessions that are already gone.

use crate::config::NatsConfig;
use crate::feed::{dispatch, SUBSCRIPTIONS};
use crate::state::AppState;
use futures::stream::{select_all, StreamExt};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Long enough that a broker restart is not hammered, short enough that a recovered one is picked
/// up within a few seconds. Only ever waited on before a connection has been built at all: once
/// one exists, reconnection is the client's own job.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);

/// The seam every outbound announcement goes through, so the socket layer can be driven without a
/// broker.
pub trait FeedPublisher: Send + Sync {
    /// Hands a message to the broker. `false` means it was dropped because there was no connection
    /// to hand it to; a dropped publish is never an error the caller can retry usefully.
    fn publish(&self, subject: String, payload: Vec<u8>) -> bool;

    fn is_enabled(&self) -> bool;

    fn is_connected(&self) -> bool;

    /// Announcements handed over with no live connection. Surfaced on `/stats/health` so a
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

type Outbound = (String, Vec<u8>);

pub struct NatsBus {
    cfg: NatsConfig,
    tx: mpsc::UnboundedSender<Outbound>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Outbound>>>,
    connected: Arc<AtomicBool>,
    dropped: AtomicU64,
}

impl NatsBus {
    pub fn new(cfg: NatsConfig) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            cfg,
            tx,
            rx: Mutex::new(Some(rx)),
            connected: Arc::new(AtomicBool::new(false)),
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
        if !self.is_connected() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if self.tx.send((subject, payload)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn is_enabled(&self) -> bool {
        self.cfg.url.is_some()
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

fn connect_options(cfg: &NatsConfig) -> async_nats::ConnectOptions {
    async_nats::ConnectOptions::new().name(cfg.server_name.clone())
}

/// Rebuilds the pipeline only for a connection that was never established or has ended: once the
/// client exists it reconnects on its own. An authentication failure is retried on the same
/// schedule as a refused socket, so a credential rotation under a running broker is not an outage.
async fn supervise(
    bus: Arc<NatsBus>,
    url: String,
    mut rx: mpsc::UnboundedReceiver<Outbound>,
    state: AppState,
) {
    loop {
        match connect_options(&bus.cfg).connect(&url).await {
            Ok(client) => {
                tracing::info!(url = %url, "archipelago feed connected");
                bus.connected.store(true, Ordering::SeqCst);
                run(&client, &mut rx, &state).await;
                bus.connected.store(false, Ordering::SeqCst);
                tracing::warn!(url = %url, "archipelago feed link ended; rebuilding");
            }
            Err(e) => {
                bus.connected.store(false, Ordering::SeqCst);
                tracing::warn!(url = %url, error = %e, "archipelago feed connect failed");
            }
        }
        tokio::time::sleep(RECONNECT_BACKOFF).await;
    }
}

/// Runs until the subscriptions are gone or the client refuses to publish at all. Every other
/// publish failure costs its one message: the subjects are announcements of a moment, and a
/// retried one would speak for a socket that may already have closed.
async fn run(
    client: &async_nats::Client,
    rx: &mut mpsc::UnboundedReceiver<Outbound>,
    state: &AppState,
) {
    let mut subscribers = Vec::with_capacity(SUBSCRIPTIONS.len());
    for subject in SUBSCRIPTIONS {
        match client.subscribe(subject).await {
            Ok(subscriber) => subscribers.push(subscriber),
            Err(e) => {
                tracing::error!(subject = %subject, error = %e, "cannot subscribe; rebuilding the link");
                return;
            }
        }
    }
    tracing::info!(
        subjects = SUBSCRIPTIONS.len(),
        "archipelago feed subscribed; island assignments now reach sockets"
    );
    while rx.try_recv().is_ok() {}
    let reannounced = reannounce_all(state, state.publisher.as_ref());
    tracing::info!(
        sessions = reannounced,
        "re-announced every live session on the recovered link"
    );
    let mut inbound = select_all(subscribers);
    loop {
        tokio::select! {
            message = inbound.next() => {
                let Some(message) = message else { return };
                dispatch(state, message.subject.as_str(), &message.payload);
            }
            outbound = rx.recv() => {
                let Some((subject, payload)) = outbound else { return };
                if let Err(e) = client.publish(subject.clone(), payload.into()).await {
                    let gone = matches!(e.kind(), async_nats::PublishErrorKind::Send);
                    tracing::warn!(subject = %subject, error = %e, connection_lost = gone, "feed publish failed; dropping");
                    if gone {
                        return;
                    }
                }
            }
        }
    }
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
        .snapshot()
        .into_iter()
        .filter(|(address, session)| {
            publisher.publish(
                crate::feed::connect_subject(address),
                session.as_bytes().to_vec(),
            )
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
