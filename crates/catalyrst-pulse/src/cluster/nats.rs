//! The broker-backed cluster feed: core NATS, no JetStream.
//!
//! At-most-once on purpose. An assignment is a snapshot of where a peer is now, so a redelivered
//! stale one is worse than a lost one -- the next pass republishes the truth within
//! `pass_interval_ms` anyway. That is also why a failed publish is abandoned rather than retried.
//!
//! The tracker never touches the socket: `publish_*` enqueues into a coalescing outbox and
//! returns, and a task of its own drains it. A stalled broker therefore costs pending messages,
//! never a slowed pass.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_nats::PublishErrorKind;
use tokio::sync::Notify;

use crate::cluster::feed::{
    cluster_change_subject, encode_cluster_change, encode_discovery, encode_topology,
    ClusterFeedPublisher, ClusterOutbox, QueueOutcome, DISCOVERY_SUBJECT,
};
use crate::cluster::{ClusterPass, ClusterSession};

/// Distinct peers that may hold an undelivered assignment at once. Past it the longest-admitted is
/// evicted, which is the only path to genuine loss the feed has -- hence `pulse_nats_dropped_total`.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

pub const DEFAULT_SERVER_NAME: &str = "pulse";

/// What the discovery heartbeat announces when the deployment did not inject its commit. Upstream
/// has exactly one fallback and it is this string, so a consumer never has to handle a blank
/// `commit_hash` as a third state.
pub const DEFAULT_COMMIT_HASH: &str = "unknown";

/// Matches archipelago-core's heartbeat, which is what the subject's consumers expect.
pub const DEFAULT_DISCOVERY_INTERVAL_MS: u64 = 10_000;

/// Long enough that a broker restart is not hammered, short enough that a recovered one is picked
/// up inside two clustering passes. Only ever waited on before a connection has been built at all:
/// once one exists, reconnection is the client's own job.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct NatsFeedOptions {
    pub url: String,
    pub server_name: String,
    pub commit_hash: String,
    pub discovery_interval_ms: u64,
    pub capacity: usize,
}

impl Default for NatsFeedOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            server_name: DEFAULT_SERVER_NAME.to_string(),
            commit_hash: DEFAULT_COMMIT_HASH.to_string(),
            discovery_interval_ms: DEFAULT_DISCOVERY_INTERVAL_MS,
            capacity: DEFAULT_CHANNEL_CAPACITY,
        }
    }
}

struct Shared {
    outbox: Mutex<ClusterOutbox>,
    pending: Notify,
    user_count: AtomicU32,
}

/// Holds the outbox and the handle to the task draining it. Dropping it aborts that task, so the
/// feed lives exactly as long as the server that owns it.
pub struct NatsClusterFeed {
    shared: Arc<Shared>,
    drain: tokio::task::JoinHandle<()>,
}

impl Drop for NatsClusterFeed {
    fn drop(&mut self) {
        self.drain.abort();
    }
}

impl NatsClusterFeed {
    /// Returns immediately: the first connection is established by the drain task, so a broker
    /// that is down at boot delays no startup and needs no ordering against it in the unit file.
    pub fn spawn(options: NatsFeedOptions) -> Self {
        let shared = Arc::new(Shared {
            outbox: Mutex::new(ClusterOutbox::new(options.capacity)),
            pending: Notify::new(),
            user_count: AtomicU32::new(0),
        });
        let drain = tokio::spawn(supervise(shared.clone(), options));
        Self { shared, drain }
    }

    pub fn pending_changes(&self) -> usize {
        self.shared.outbox.lock().unwrap().pending_changes()
    }
}

fn record(outcome: QueueOutcome) {
    match outcome {
        QueueOutcome::Admitted => {}
        QueueOutcome::Superseded => crate::metrics::nats_superseded(),
        QueueOutcome::Dropped => crate::metrics::nats_dropped(),
    }
}

impl ClusterFeedPublisher for NatsClusterFeed {
    fn publish_cluster_change(
        &self,
        wallet: &str,
        cluster_id: &str,
        realm: &str,
        session: &ClusterSession,
    ) {
        let subject = cluster_change_subject(wallet);
        let payload = encode_cluster_change(cluster_id, realm, session);
        record(
            self.shared
                .outbox
                .lock()
                .unwrap()
                .queue_change(subject, payload),
        );
        self.shared.pending.notify_one();
    }

    fn publish_topology(&self, pass: &ClusterPass, active_peers: u32) {
        self.shared
            .user_count
            .store(active_peers, Ordering::Relaxed);
        let payload = encode_topology(pass);
        record(self.shared.outbox.lock().unwrap().queue_topology(payload));
        self.shared.pending.notify_one();
    }
}

/// What a client connection event means for `pulse_nats_connected` and
/// `pulse_nats_reconnects_total`. Separated from the recording so the edges can be asserted
/// without a broker: the client reconnects on its own, so these events are the only place the
/// connection state is observable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEdge {
    Opened,
    Reopened,
    Lost,
    None,
}

/// Every open past the first is by definition a reconnect. Tested and set in one step, so two
/// opens racing cannot both read themselves as the first.
pub fn connection_edge(ever_connected: &AtomicBool, event: &async_nats::Event) -> ConnectionEdge {
    match event {
        async_nats::Event::Connected => {
            if ever_connected.swap(true, Ordering::SeqCst) {
                ConnectionEdge::Reopened
            } else {
                ConnectionEdge::Opened
            }
        }
        async_nats::Event::Disconnected | async_nats::Event::Closed => ConnectionEdge::Lost,
        _ => ConnectionEdge::None,
    }
}

/// True for the two kinds that mean this publisher is being refused rather than merely disrupted:
/// the credential rejected and the publish rejected. async-nats folds the second into
/// `ServerError::Other`, so it is matched on the broker's own wording.
pub fn server_error_is_refusal(error: &async_nats::ServerError) -> bool {
    match error {
        async_nats::ServerError::AuthorizationViolation => true,
        async_nats::ServerError::Other(text) => {
            text.to_lowercase().contains("permissions violation")
        }
        _ => false,
    }
}

pub fn apply_connection_event(ever_connected: &AtomicBool, event: async_nats::Event) {
    match connection_edge(ever_connected, &event) {
        ConnectionEdge::Opened => {
            crate::metrics::nats_connected(true);
            tracing::info!("cluster feed connection opened");
        }
        ConnectionEdge::Reopened => {
            crate::metrics::nats_connected(true);
            crate::metrics::nats_reconnected();
            tracing::info!("cluster feed connection reopened");
        }
        ConnectionEdge::Lost => {
            crate::metrics::nats_connected(false);
            tracing::warn!(
                "cluster feed connection lost; the topology and each peer's latest assignment are \
                 retained until it returns"
            );
        }
        ConnectionEdge::None => {}
    }
    match &event {
        async_nats::Event::ServerError(error) if server_error_is_refusal(error) => {
            tracing::error!(error = %error, "cluster feed refused by the broker");
        }
        async_nats::Event::ServerError(error) => {
            tracing::warn!(error = %error, "cluster feed broker error");
        }
        async_nats::Event::ClientError(error) => {
            tracing::warn!(error = %error, "cluster feed client error");
        }
        _ => {}
    }
}

fn connect_options(
    options: &NatsFeedOptions,
    ever_connected: Arc<AtomicBool>,
) -> async_nats::ConnectOptions {
    async_nats::ConnectOptions::new()
        .name(options.server_name.clone())
        .event_callback(move |event| {
            let ever_connected = ever_connected.clone();
            async move { apply_connection_event(&ever_connected, event) }
        })
}

/// Rebuilds the pipeline only for a connection that was never established: once the client exists
/// it reconnects on its own, and `pulse_nats_connected` follows its events rather than this loop.
/// An authentication failure is retried on the same schedule as a refused socket: credentials are
/// rotated under a running server often enough that treating the rejection as permanent would turn
/// a rotation into an operator-visible outage.
async fn supervise(shared: Arc<Shared>, options: NatsFeedOptions) {
    let ever_connected = Arc::new(AtomicBool::new(false));
    loop {
        match connect_options(&options, ever_connected.clone())
            .connect(&options.url)
            .await
        {
            Ok(client) => {
                tracing::info!(url = %options.url, "cluster feed connected");
                drain(&shared, &options, &client).await;
                crate::metrics::nats_connected(false);
            }
            Err(e) => {
                crate::metrics::nats_connected(false);
                tracing::warn!(url = %options.url, error = %e, "cluster feed connect failed");
            }
        }
        tokio::time::sleep(RECONNECT_BACKOFF).await;
    }
}

/// The publish seam, so the drain loop's failure handling is assertable without a broker.
pub trait FeedSink {
    fn send(
        &self,
        subject: String,
        payload: Vec<u8>,
    ) -> impl Future<Output = Option<PublishErrorKind>> + Send;
}

impl FeedSink for async_nats::Client {
    async fn send(&self, subject: String, payload: Vec<u8>) -> Option<PublishErrorKind> {
        self.publish(subject, payload.into())
            .await
            .err()
            .map(|e| e.kind())
    }
}

/// `Send` is the client's command channel being closed, which is the one publish failure that says
/// this client is finished. A payload the broker will not take and a subject it will not parse are
/// properties of the one message, so the feed drops it and keeps draining -- upstream's
/// PUBLISH_FAILED-and-continue.
pub fn connection_is_gone(kind: PublishErrorKind) -> bool {
    matches!(kind, PublishErrorKind::Send)
}

/// Records a publish outcome and says whether the drain loop may keep going.
pub fn record_publish(subject: &str, error: Option<PublishErrorKind>) -> bool {
    match error {
        None => {
            crate::metrics::nats_published();
            true
        }
        Some(kind) => {
            crate::metrics::nats_publish_failed();
            let gone = connection_is_gone(kind);
            tracing::warn!(
                subject = %subject,
                error = %kind,
                connection_lost = gone,
                "cluster feed publish failed; dropping"
            );
            !gone
        }
    }
}

/// Runs until the client itself is gone; every other publish failure costs its one message. The
/// failed message is dropped rather than held, so a broker outage cannot replay a backlog of stale
/// assignments once it ends.
async fn drain<S: FeedSink + Sync>(shared: &Arc<Shared>, options: &NatsFeedOptions, sink: &S) {
    let mut discovery = (options.discovery_interval_ms > 0).then(|| {
        let mut timer = tokio::time::interval(Duration::from_millis(options.discovery_interval_ms));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer
    });
    loop {
        tokio::select! {
            _ = shared.pending.notified() => {
                while let Some((subject, payload)) = next(shared) {
                    let error = sink.send(subject.clone(), payload).await;
                    if !record_publish(&subject, error) {
                        return;
                    }
                }
            }
            _ = next_heartbeat(discovery.as_mut()) => {
                let payload = encode_discovery(
                    &options.server_name,
                    &options.commit_hash,
                    chrono::Utc::now().timestamp_millis().max(0) as u64,
                    shared.user_count.load(Ordering::Relaxed),
                );
                let error = sink.send(DISCOVERY_SUBJECT.to_string(), payload).await;
                if !record_publish(DISCOVERY_SUBJECT, error) {
                    return;
                }
            }
        }
    }
}

/// Never resolves when the heartbeat is off, which keeps a disabled interval out of the select
/// without a second copy of the loop.
async fn next_heartbeat(timer: Option<&mut tokio::time::Interval>) {
    match timer {
        Some(timer) => {
            timer.tick().await;
        }
        None => std::future::pending().await,
    }
}

/// The lock is taken and released inside this call so no guard is ever held across an await.
fn next(shared: &Arc<Shared>) -> Option<(String, Vec<u8>)> {
    shared.outbox.lock().unwrap().try_dequeue_next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric_value(rendered: &str, name: &str) -> Option<f64> {
        rendered
            .lines()
            .filter(|line| !line.starts_with('#'))
            .find_map(|line| line.strip_prefix(name)?.trim().parse().ok())
    }

    #[test]
    fn a_reconnect_drives_the_gauge_down_and_back_up_and_counts_once() {
        let ever = AtomicBool::new(false);
        let rendered = crate::metrics::render_recorded(|| {
            apply_connection_event(&ever, async_nats::Event::Connected);
            apply_connection_event(&ever, async_nats::Event::Disconnected);
            apply_connection_event(&ever, async_nats::Event::Connected);
        });

        assert_eq!(metric_value(&rendered, "pulse_nats_connected"), Some(1.0));
        assert_eq!(
            metric_value(&rendered, "pulse_nats_reconnects_total"),
            Some(1.0)
        );
    }

    #[test]
    fn the_first_open_is_not_a_reconnect() {
        let ever = AtomicBool::new(false);
        assert_eq!(
            connection_edge(&ever, &async_nats::Event::Connected),
            ConnectionEdge::Opened
        );
        assert_eq!(
            connection_edge(&ever, &async_nats::Event::Connected),
            ConnectionEdge::Reopened
        );
        assert_eq!(
            connection_edge(&ever, &async_nats::Event::Disconnected),
            ConnectionEdge::Lost
        );
        assert_eq!(
            connection_edge(&ever, &async_nats::Event::LameDuckMode),
            ConnectionEdge::None
        );
    }

    #[test]
    fn a_rejected_credential_and_a_rejected_publish_are_refusals() {
        assert!(server_error_is_refusal(
            &async_nats::ServerError::AuthorizationViolation
        ));
        assert!(server_error_is_refusal(&async_nats::ServerError::Other(
            "Permissions Violation for Publish to \"engine.islands\"".to_string()
        )));
        assert!(!server_error_is_refusal(&async_nats::ServerError::Other(
            "unknown protocol operation".to_string()
        )));
    }

    #[test]
    fn a_per_message_failure_keeps_the_feed_and_only_counts_publish_failed() {
        let rendered = crate::metrics::render_recorded(|| {
            assert!(record_publish(
                "engine.islands",
                Some(PublishErrorKind::MaxPayloadExceeded)
            ));
            assert!(record_publish(
                "peer.0xa.cluster_change",
                Some(PublishErrorKind::InvalidSubject)
            ));
        });

        assert_eq!(
            metric_value(&rendered, "pulse_nats_publish_failed_total"),
            Some(2.0)
        );
        assert_eq!(metric_value(&rendered, "pulse_nats_published_total"), None);
        assert_eq!(metric_value(&rendered, "pulse_nats_dropped_total"), None);
    }

    #[test]
    fn a_dead_client_ends_the_drain() {
        assert!(!record_publish(
            "engine.islands",
            Some(PublishErrorKind::Send)
        ));
        assert!(connection_is_gone(PublishErrorKind::Send));
        assert!(!connection_is_gone(PublishErrorKind::MaxPayloadExceeded));
        assert!(!connection_is_gone(PublishErrorKind::InvalidSubject));
    }

    struct FlakySink {
        sent: Mutex<Vec<String>>,
        reject: Option<PublishErrorKind>,
    }

    impl FeedSink for FlakySink {
        async fn send(&self, subject: String, _: Vec<u8>) -> Option<PublishErrorKind> {
            self.sent.lock().unwrap().push(subject);
            self.reject
        }
    }

    fn feed_shared(capacity: usize) -> Arc<Shared> {
        Arc::new(Shared {
            outbox: Mutex::new(ClusterOutbox::new(capacity)),
            pending: Notify::new(),
            user_count: AtomicU32::new(0),
        })
    }

    #[tokio::test]
    async fn the_drain_loop_survives_a_per_message_failure_and_stops_on_a_dead_client() {
        let options = NatsFeedOptions {
            discovery_interval_ms: 0,
            ..Default::default()
        };
        let shared = feed_shared(8);
        {
            let mut outbox = shared.outbox.lock().unwrap();
            outbox.queue_topology(vec![9]);
            for peer in 0..3u32 {
                outbox.queue_change(format!("peer.0x{peer}.cluster_change"), vec![1, 2, 3]);
            }
        }
        shared.pending.notify_one();
        let sink = FlakySink {
            sent: Mutex::new(Vec::new()),
            reject: Some(PublishErrorKind::MaxPayloadExceeded),
        };
        let survived =
            tokio::time::timeout(Duration::from_millis(50), drain(&shared, &options, &sink))
                .await
                .is_err();
        assert!(survived, "a rejected payload must not end the drain loop");
        assert_eq!(
            sink.sent.lock().unwrap().len(),
            4,
            "every queued message is attempted despite the first one failing"
        );

        let shared = feed_shared(8);
        shared
            .outbox
            .lock()
            .unwrap()
            .queue_change("peer.0xa.cluster_change".to_string(), vec![1]);
        shared.pending.notify_one();
        let sink = FlakySink {
            sent: Mutex::new(Vec::new()),
            reject: Some(PublishErrorKind::Send),
        };
        tokio::time::timeout(Duration::from_millis(500), drain(&shared, &options, &sink))
            .await
            .expect("a dead client must end the drain loop");
    }

    #[test]
    fn the_server_name_reaches_the_connection_options() {
        let options = NatsFeedOptions {
            server_name: "pulse-a".to_string(),
            ..Default::default()
        };
        let built = connect_options(&options, Arc::new(AtomicBool::new(false)));
        assert!(
            format!("{built:?}").contains("pulse-a"),
            "the configured server name must reach the client connection name"
        );
    }
}
