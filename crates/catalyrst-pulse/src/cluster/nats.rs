//! The broker-backed cluster feed: core NATS, no JetStream.
//!
//! Cluster changes are lossy edges, not a periodic assignment replay. A separate read-only lookup
//! serves a fresh, live, post-debounce wallet/session view; consumers must request it to recover
//! missing state. Topology alone does not reconstruct that ownership.
//!
//! The tracker never touches the socket: `publish_*` enqueues into a coalescing outbox and
//! returns, and a task of its own drains it. A stalled broker therefore costs pending messages,
//! never a slowed pass.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_nats::PublishErrorKind;
use futures_util::StreamExt;
use prost::Message;
use tokio::sync::Notify;

use crate::cluster::control::{
    ControlPositions, ControlSnapshot, DEFAULT_CONTROL_POSITION_CAPACITY,
};
use crate::cluster::feed::{
    cluster_change_subject, encode_cluster_change, encode_discovery, encode_topology,
    ClusterFeedPublisher, ClusterOutbox, CurrentAssignment, CurrentAssignments, QueueOutcome,
    CLUSTER_LOOKUP_SUBJECT, DISCOVERY_SUBJECT,
};
use crate::cluster::{ClusterPass, ClusterSession};

/// Distinct peers that may hold a pending change at once. Capacity eviction is counted, but core
/// NATS delivery can also be lost after a successful local enqueue.
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
const LOOKUP_DEADLINE: Duration = Duration::from_secs(2);
const LOOKUP_CAPACITY: usize = 128;
const MAX_LOOKUP_REPLY_BYTES: usize = 64 * 1024;

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
    current: CurrentAssignments,
    incarnation: OnceLock<String>,
    control: Mutex<ControlPositions>,
    control_enabled: Notify,
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
            current: CurrentAssignments::default(),
            incarnation: OnceLock::new(),
            control: Mutex::new(ControlPositions::default()),
            control_enabled: Notify::new(),
        });
        let drain = tokio::spawn(supervise(shared.clone(), options));
        Self { shared, drain }
    }

    pub fn pending_changes(&self) -> usize {
        self.shared.outbox.lock().unwrap().pending_changes()
    }

    pub fn enable_control_positions(&self, audience: &str) -> Result<(), &'static str> {
        self.shared.control.lock().unwrap().enable(audience)?;
        self.shared.control_enabled.notify_waiters();
        Ok(())
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
    fn control_snapshot(&self) -> ControlSnapshot {
        self.shared.control.lock().unwrap().snapshot()
    }

    fn replace_assignments(
        &self,
        pass: u64,
        valid_for: Duration,
        assignments: Vec<CurrentAssignment>,
    ) {
        self.shared.current.replace(pass, valid_for, assignments);
    }

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
                "cluster feed connection lost; pending changes are bounded and current assignments require lookup"
            );
        }
        ConnectionEdge::None => {}
    }
    match &event {
        async_nats::Event::ServerError(error) if server_error_is_refusal(error) => {
            tracing::error!("cluster feed refused by the broker");
        }
        async_nats::Event::ServerError(_) => {
            tracing::warn!("cluster feed broker error");
        }
        async_nats::Event::ClientError(_) => {
            tracing::warn!("cluster feed client error");
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
        .client_capacity(options.capacity.max(1))
        .subscription_capacity(LOOKUP_CAPACITY.max(DEFAULT_CONTROL_POSITION_CAPACITY))
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
                shared.incarnation.get_or_init(|| client.new_inbox());
                shared.pending.notify_one();
                tracing::info!("cluster feed connected");
                tokio::select! {
                    _ = drain(&shared, &options, &client) => {}
                    _ = serve_lookups(&shared, &client) => {}
                    _ = serve_control_positions(&shared, &client) => {}
                }
                crate::metrics::nats_connected(false);
            }
            Err(_) => {
                crate::metrics::nats_connected(false);
                tracing::warn!("cluster feed connect failed");
            }
        }
        tokio::time::sleep(RECONNECT_BACKOFF).await;
    }
}

async fn serve_control_positions(shared: &Shared, client: &async_nats::Client) {
    loop {
        let enabled = shared.control_enabled.notified();
        tokio::pin!(enabled);
        enabled.as_mut().enable();
        if shared.control.lock().unwrap().is_enabled() {
            break;
        }
        enabled.await;
    }
    let Ok(mut updates) = client
        .subscribe(catalyrst_types::control_position::SUBJECT)
        .await
    else {
        return;
    };
    while let Some(message) = updates.next().await {
        let Some(update) =
            catalyrst_types::control_position::ControlPosition::decode(&message.payload)
        else {
            continue;
        };
        shared.control.lock().unwrap().ingest(update);
    }
}

fn lookup_target(subject: &str, payload: &[u8]) -> Option<(String, String)> {
    fn address(value: &str) -> bool {
        value.len() == 42
            && value.starts_with("0x")
            && value.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
    }
    let wallet = subject
        .strip_prefix("peer.")?
        .strip_suffix(".cluster_lookup")?;
    let session = std::str::from_utf8(payload).ok()?;
    (address(wallet) && address(session))
        .then(|| (wallet.to_ascii_lowercase(), session.to_ascii_lowercase()))
}

fn private_reply(subject: &str) -> bool {
    subject.len() <= 256
        && subject.strip_prefix("_INBOX.").is_some_and(|tail| {
            tail.split('.').all(|token| {
                !token.is_empty()
                    && token
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        })
}

async fn serve_lookups(shared: &Shared, client: &async_nats::Client) {
    let Some(incarnation) = shared.incarnation.get() else {
        return;
    };
    let Ok(Ok(mut requests)) =
        tokio::time::timeout(LOOKUP_DEADLINE, client.subscribe(CLUSTER_LOOKUP_SUBJECT)).await
    else {
        return;
    };
    while let Some(request) = requests.next().await {
        let Some(reply) = request
            .reply
            .filter(|subject| private_reply(subject.as_str()))
        else {
            continue;
        };
        let Some((wallet, session)) = lookup_target(request.subject.as_str(), &request.payload)
        else {
            continue;
        };
        crate::metrics::cluster_lookup_requested();
        let Some(mut snapshot) = shared.current.lookup(&wallet, &session) else {
            continue;
        };
        snapshot.source_incarnation = incarnation.clone();
        let payload = snapshot.encode_to_vec();
        if payload.len() > MAX_LOOKUP_REPLY_BYTES {
            continue;
        }
        if !matches!(
            tokio::time::timeout(LOOKUP_DEADLINE, client.publish(reply, payload.into())).await,
            Ok(Ok(()))
        ) {
            tracing::warn!("cluster lookup reply could not be submitted; rebuilding link");
            return;
        }
        crate::metrics::cluster_lookup_submitted();
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

    #[test]
    fn lookup_targets_are_exact_addresses_and_replies_stay_in_private_inboxes() {
        let wallet = format!("0x{:040x}", 1);
        let session = format!("0x{:040x}", 2);
        let subject = format!("peer.{wallet}.cluster_lookup");
        assert_eq!(
            lookup_target(&subject, session.as_bytes()),
            Some((wallet, session.clone()))
        );
        for invalid in [
            format!("{session} "),
            format!("{session}.extra"),
            "*".into(),
            String::new(),
        ] {
            assert!(lookup_target(&subject, invalid.as_bytes()).is_none());
        }
        assert!(lookup_target("peer.*.cluster_lookup", session.as_bytes()).is_none());
        assert!(private_reply("_INBOX.aB12.003"));
        for invalid in [
            "peer.0xa.cluster_change",
            "_INBOX.",
            "_INBOX.*",
            "_INBOX.a.>",
            "_INBOX.a..b",
        ] {
            assert!(!private_reply(invalid));
        }
    }

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
            current: CurrentAssignments::default(),
            incarnation: OnceLock::new(),
            control: Mutex::new(ControlPositions::default()),
            control_enabled: Notify::new(),
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
