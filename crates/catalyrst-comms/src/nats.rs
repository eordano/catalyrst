//! The broker seam the Pulse cluster feed arrives on: core NATS, no JetStream.
//!
//! Every consumer sees only "the callback fires or it does not". Connection state, reconnection
//! and backoff live here and nowhere else, so nothing downstream ever branches on the link being
//! up. A publish reports its outcome as a value rather than an error, because "there was no
//! connection to hand this to" is a routine state of a fire-and-forget feed, not a fault.
//!
//! Handlers are invoked from the subscription's own task: one that panics is contained so the
//! remaining subjects keep delivering, mirroring upstream's never-throws contract.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
#[cfg(feature = "nats")]
use std::time::Duration;

/// Long enough that a broker restart is not hammered, short enough that a recovered one is picked
/// up within a few seconds. Only ever waited on before a connection has been built at all: once a
/// client exists, reconnection is its own job.
#[cfg(feature = "nats")]
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Invoked once per message with the concrete subject it arrived on and its payload. Must not
/// panic; one that does is caught, but the message is lost.
pub type NatsHandler = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Delivered,
    /// No connection to hand the write to; the message was discarded rather than queued.
    NoConnection,
    /// The client refused the write: a subject it will not parse, or a payload past its limit.
    Failed,
}

impl PublishOutcome {
    pub fn delivered(self) -> bool {
        matches!(self, PublishOutcome::Delivered)
    }
}

/// Cancels one subscription. Dropping it unsubscribes, so a component that holds its subscriptions
/// in a field stops taking events the moment that field is cleared.
pub struct NatsSubscription {
    registry: Weak<dyn SubscriptionRegistry>,
    id: u64,
}

impl NatsSubscription {
    pub fn unsubscribe(&self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.cancel(self.id);
        }
    }
}

impl Drop for NatsSubscription {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

trait SubscriptionRegistry: Send + Sync {
    fn cancel(&self, id: u64);
}

#[async_trait::async_trait]
pub trait NatsBus: Send + Sync {
    /// Whether a broker is configured at all. Independent of whether the link is up: consumers use
    /// it to decide whether to wire themselves in.
    fn is_enabled(&self) -> bool;

    /// Whether the link is up right now. False during a disconnect/reconnect blip.
    fn is_connected(&self) -> bool;

    /// Whether every registration is live on the broker, i.e. whether this consumer is actually
    /// receiving. A bus with nothing to wait for reports ready.
    fn is_ready(&self) -> bool;

    /// Starts connecting in the background and returns at once.
    ///
    /// Deliberately not awaitable: a real connect stalls for seconds per unreachable address
    /// before giving up, and readiness gates on startup finishing. It retries on its own
    /// regardless, so waiting here would only cost readiness time.
    fn connect(&self);

    /// Registers a handler. Safe to call before [`NatsBus::connect`]: registrations made while
    /// disconnected are activated as soon as the link opens.
    fn subscribe(
        &self,
        subject: &str,
        queue: Option<&str>,
        handler: NatsHandler,
    ) -> NatsSubscription;

    async fn publish(&self, subject: &str, payload: Vec<u8>) -> PublishOutcome;
}

/// Matches a NATS subject against a subscription pattern: `*` stands for exactly one token, `>`
/// for the rest of the subject.
pub fn subject_matches(pattern: &str, subject: &str) -> bool {
    let mut tokens = subject.split('.');
    for part in pattern.split('.') {
        if part == ">" {
            return tokens.next().is_some();
        }
        match tokens.next() {
            Some(token) if part == "*" || part == token => {}
            _ => return false,
        }
    }
    tokens.next().is_none()
}

fn invoke(handler: &NatsHandler, subject: &str, payload: &[u8]) {
    if std::panic::catch_unwind(AssertUnwindSafe(|| handler(subject, payload))).is_err() {
        tracing::error!(subject = %subject, "nats handler panicked; message dropped");
    }
}

/// The real, broker-backed bus.
#[cfg(feature = "nats")]
pub struct BrokerBus {
    shared: Arc<BrokerShared>,
}

#[cfg(feature = "nats")]
struct BrokerRegistration {
    subject: String,
    queue: Option<String>,
    handler: NatsHandler,
    live: Option<tokio::task::JoinHandle<()>>,
    /// Claimed by whichever `activate` call took this registration, so a second one racing it
    /// cannot register the same subject twice and leak the first delivery task.
    activating: bool,
}

#[cfg(feature = "nats")]
struct BrokerShared {
    url: Option<String>,
    name: String,
    client: Mutex<Option<async_nats::Client>>,
    connected: Arc<AtomicBool>,
    registrations: Mutex<HashMap<u64, BrokerRegistration>>,
    next_id: AtomicU64,
    started: AtomicBool,
    ready: AtomicBool,
}

#[cfg(feature = "nats")]
impl SubscriptionRegistry for BrokerShared {
    fn cancel(&self, id: u64) {
        if let Some(registration) = self.registrations.lock().unwrap().remove(&id) {
            if let Some(task) = registration.live {
                task.abort();
            }
        }
    }
}

#[cfg(feature = "nats")]
impl BrokerBus {
    /// A blank or absent URL leaves the bus inert: nothing connects, nothing subscribes, and
    /// `publish` reports every write as undeliverable.
    pub fn new(url: Option<String>, name: impl Into<String>) -> Self {
        Self {
            shared: Arc::new(BrokerShared {
                url: url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
                name: name.into(),
                client: Mutex::new(None),
                connected: Arc::new(AtomicBool::new(false)),
                registrations: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
                started: AtomicBool::new(false),
                ready: AtomicBool::new(false),
            }),
        }
    }
}

#[cfg(feature = "nats")]
#[async_trait::async_trait]
impl NatsBus for BrokerBus {
    fn is_enabled(&self) -> bool {
        self.shared.url.is_some()
    }

    fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }

    fn is_ready(&self) -> bool {
        self.shared.ready.load(Ordering::SeqCst)
    }

    fn connect(&self) {
        if self.shared.url.is_none() || self.shared.started.swap(true, Ordering::SeqCst) {
            return;
        }
        tokio::spawn(supervise(self.shared.clone()));
    }

    fn subscribe(
        &self,
        subject: &str,
        queue: Option<&str>,
        handler: NatsHandler,
    ) -> NatsSubscription {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let registration = BrokerRegistration {
            subject: subject.to_string(),
            queue: queue.map(str::to_string),
            handler,
            live: None,
            activating: false,
        };
        self.shared
            .registrations
            .lock()
            .unwrap()
            .insert(id, registration);
        let client = self.shared.client.lock().unwrap().clone();
        if let Some(client) = client {
            let shared = self.shared.clone();
            tokio::spawn(async move { activate(&shared, id, &client).await });
        }
        NatsSubscription {
            registry: Arc::downgrade(&self.shared) as Weak<dyn SubscriptionRegistry>,
            id,
        }
    }

    /// Reads the client handle rather than the connected flag: async-nats buffers commands across
    /// a disconnect/reconnect blip and flushes them, so the flag would discard a write the client
    /// could still have delivered.
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> PublishOutcome {
        let Some(client) = self.shared.client.lock().unwrap().clone() else {
            return PublishOutcome::NoConnection;
        };
        match client.publish(subject.to_string(), payload.into()).await {
            Ok(()) => PublishOutcome::Delivered,
            Err(error) => {
                tracing::warn!(subject = %subject, %error, "nats publish refused");
                PublishOutcome::Failed
            }
        }
    }
}

/// Takes the registration for `id` if it is neither live nor already being registered by another
/// caller.
///
/// The claim is made under the same lock that reads the registration's state, because the
/// subscribe that follows it is awaited with the lock released: two callers that both merely
/// observed `live == None` would both register the subject with the broker, and the second's write
/// would leak the first's delivery task, delivering every message twice.
#[cfg(feature = "nats")]
fn claim(shared: &Arc<BrokerShared>, id: u64) -> Option<(String, Option<String>, NatsHandler)> {
    let mut registrations = shared.registrations.lock().unwrap();
    let registration = registrations.get_mut(&id)?;
    if registration.live.is_some() || registration.activating {
        return None;
    }
    registration.activating = true;
    Some((
        registration.subject.clone(),
        registration.queue.clone(),
        registration.handler.clone(),
    ))
}

/// Registers one subscription with the broker and hands its delivery loop to a task of its own.
///
/// The subscribe itself is awaited here rather than inside that task, so a caller that goes on to
/// flush knows the broker has the registration; otherwise readiness would mean only that a task
/// had been spawned. A registration that is gone by the time it completes was unsubscribed while
/// the subscribe was in flight, and its delivery task is aborted rather than recorded.
#[cfg(feature = "nats")]
async fn activate(shared: &Arc<BrokerShared>, id: u64, client: &async_nats::Client) {
    let Some((subject, queue, handler)) = claim(shared, id) else {
        return;
    };

    let subscriber = match &queue {
        Some(queue) => client.queue_subscribe(subject.clone(), queue.clone()).await,
        None => client.subscribe(subject.clone()).await,
    };
    let mut subscriber = match subscriber {
        Ok(subscriber) => subscriber,
        Err(error) => {
            if let Some(registration) = shared.registrations.lock().unwrap().get_mut(&id) {
                registration.activating = false;
            }
            tracing::error!(subject = %subject, %error, "nats subscribe failed");
            return;
        }
    };
    tracing::info!(subject = %subject, queue = ?queue, "nats subscription active");

    let live = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(message) = subscriber.next().await {
            invoke(&handler, message.subject.as_str(), &message.payload);
        }
    });
    match shared.registrations.lock().unwrap().get_mut(&id) {
        Some(registration) => {
            registration.activating = false;
            registration.live = Some(live);
        }
        None => live.abort(),
    }
}

/// Rebuilds only a connection that was never established: once the client exists it reconnects on
/// its own and restores its subscriptions, and the connected gauge follows its events rather than
/// this loop. An authentication failure is retried on the same schedule as a refused socket, so a
/// credential rotation under a running broker is not an outage.
#[cfg(feature = "nats")]
async fn supervise(shared: Arc<BrokerShared>) {
    let url = shared.url.clone().unwrap_or_default();
    loop {
        let connected = shared.connected.clone();
        let options = async_nats::ConnectOptions::new()
            .name(shared.name.clone())
            .event_callback(move |event| {
                let connected = connected.clone();
                async move { apply_connection_event(&connected, event) }
            });
        match options.connect(&url).await {
            Ok(client) => {
                tracing::info!(url = %url, "cluster feed broker connected");
                *shared.client.lock().unwrap() = Some(client.clone());
                shared.connected.store(true, Ordering::Relaxed);
                crate::metrics::nats_connected(true);
                let ids: Vec<u64> = shared
                    .registrations
                    .lock()
                    .unwrap()
                    .keys()
                    .copied()
                    .collect();
                for id in ids {
                    activate(&shared, id, &client).await;
                }
                if let Err(error) = client.flush().await {
                    tracing::warn!(%error, "cluster feed subscriptions were not acknowledged");
                }
                shared.ready.store(true, Ordering::SeqCst);
                return;
            }
            Err(error) => {
                crate::metrics::nats_connected(false);
                tracing::warn!(url = %url, %error, "cluster feed broker connect failed");
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

#[cfg(feature = "nats")]
fn apply_connection_event(connected: &AtomicBool, event: async_nats::Event) {
    match event {
        async_nats::Event::Connected => {
            connected.store(true, Ordering::Relaxed);
            crate::metrics::nats_connected(true);
            tracing::info!("cluster feed broker link up");
        }
        async_nats::Event::Disconnected | async_nats::Event::Closed => {
            connected.store(false, Ordering::Relaxed);
            crate::metrics::nats_connected(false);
            tracing::warn!("cluster feed broker link down");
        }
        async_nats::Event::ServerError(error) => {
            tracing::error!(%error, "cluster feed broker error")
        }
        async_nats::Event::ClientError(error) => {
            tracing::warn!(%error, "cluster feed client error")
        }
        _ => {}
    }
}

struct InProcessSub {
    subject: String,
    queue: Option<String>,
    handler: NatsHandler,
}

struct InProcessShared {
    subs: Mutex<HashMap<u64, InProcessSub>>,
    next_id: AtomicU64,
    published: Mutex<Vec<(String, Vec<u8>)>>,
    connected: AtomicBool,
    refuse: AtomicBool,
}

impl SubscriptionRegistry for InProcessShared {
    fn cancel(&self, id: u64) {
        self.subs.lock().unwrap().remove(&id);
    }
}

/// The in-process fake: one loopback bus with the same subject wildcards and queue-group
/// semantics as the broker, so the whole subscriber can be driven with no broker anywhere.
pub struct InProcessBus {
    shared: Arc<InProcessShared>,
}

impl Default for InProcessBus {
    fn default() -> Self {
        Self::new()
    }
}

impl InProcessBus {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(InProcessShared {
                subs: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
                published: Mutex::new(Vec::new()),
                connected: AtomicBool::new(true),
                refuse: AtomicBool::new(false),
            }),
        }
    }

    /// Everything published so far, in order, including messages that were also delivered to a
    /// local subscriber.
    pub fn published(&self) -> Vec<(String, Vec<u8>)> {
        self.shared.published.lock().unwrap().clone()
    }

    /// Simulates the link going away: publishes report [`PublishOutcome::NoConnection`] and
    /// nothing is delivered.
    pub fn set_connected(&self, up: bool) {
        self.shared.connected.store(up, Ordering::Relaxed);
    }

    /// Simulates a write the broker refuses, which is a different outcome from having no link.
    pub fn refuse_publishes(&self, refuse: bool) {
        self.shared.refuse.store(refuse, Ordering::Relaxed);
    }

    /// Delivers a message to the matching subscribers without recording it as published: how a
    /// test stands in for Pulse, which publishes from another process.
    ///
    /// One member of each queue group is picked, like the broker's own distribution.
    pub fn inject(&self, subject: &str, payload: &[u8]) {
        let handlers: Vec<NatsHandler> = {
            let subs = self.shared.subs.lock().unwrap();
            let mut grouped: HashMap<(String, String), NatsHandler> = HashMap::new();
            let mut ungrouped: Vec<NatsHandler> = Vec::new();
            for sub in subs.values() {
                if !subject_matches(&sub.subject, subject) {
                    continue;
                }
                match &sub.queue {
                    Some(queue) => {
                        grouped
                            .entry((sub.subject.clone(), queue.clone()))
                            .or_insert_with(|| sub.handler.clone());
                    }
                    None => ungrouped.push(sub.handler.clone()),
                }
            }
            grouped.into_values().chain(ungrouped).collect()
        };
        for handler in handlers {
            invoke(&handler, subject, payload);
        }
    }
}

#[async_trait::async_trait]
impl NatsBus for InProcessBus {
    fn is_enabled(&self) -> bool {
        true
    }

    fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn connect(&self) {}

    fn subscribe(
        &self,
        subject: &str,
        queue: Option<&str>,
        handler: NatsHandler,
    ) -> NatsSubscription {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        self.shared.subs.lock().unwrap().insert(
            id,
            InProcessSub {
                subject: subject.to_string(),
                queue: queue.map(str::to_string),
                handler,
            },
        );
        NatsSubscription {
            registry: Arc::downgrade(&self.shared) as Weak<dyn SubscriptionRegistry>,
            id,
        }
    }

    async fn publish(&self, subject: &str, payload: Vec<u8>) -> PublishOutcome {
        if !self.shared.connected.load(Ordering::Relaxed) {
            return PublishOutcome::NoConnection;
        }
        if self.shared.refuse.load(Ordering::Relaxed) {
            return PublishOutcome::Failed;
        }
        self.shared
            .published
            .lock()
            .unwrap()
            .push((subject.to_string(), payload.clone()));
        self.inject(subject, &payload);
        PublishOutcome::Delivered
    }
}

/// The bus a deployment with no broker configured gets: enabled nowhere, connected never.
pub struct DisabledBus;

#[async_trait::async_trait]
impl NatsBus for DisabledBus {
    fn is_enabled(&self) -> bool {
        false
    }

    fn is_connected(&self) -> bool {
        false
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn connect(&self) {}

    fn subscribe(
        &self,
        _subject: &str,
        _queue: Option<&str>,
        _handler: NatsHandler,
    ) -> NatsSubscription {
        NatsSubscription {
            registry: Weak::<InProcessShared>::new() as Weak<dyn SubscriptionRegistry>,
            id: 0,
        }
    }

    async fn publish(&self, _subject: &str, _payload: Vec<u8>) -> PublishOutcome {
        PublishOutcome::NoConnection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn a_wildcard_stands_for_exactly_one_token() {
        assert!(subject_matches(
            "peer.*.cluster_change",
            "peer.0xabc.cluster_change"
        ));
        assert!(!subject_matches(
            "peer.*.cluster_change",
            "peer.0xabc.connect"
        ));
        assert!(!subject_matches(
            "peer.*.cluster_change",
            "peer.0xabc.extra.cluster_change"
        ));
        assert!(!subject_matches("peer.*.cluster_change", "peer.0xabc"));
        assert!(subject_matches(
            "engine.peer.*.island_changed",
            "engine.peer.0xa.island_changed"
        ));
        assert!(!subject_matches(
            "engine.peer.*.island_changed",
            "engine.peer.0xa.island_changed.0xb"
        ));
        assert!(subject_matches("peer.>", "peer.0xabc.cluster_change"));
        assert!(!subject_matches("peer.>", "peer"));
    }

    #[tokio::test]
    async fn a_queue_group_delivers_to_one_member_and_an_ungrouped_copy_to_every_one() {
        let bus = InProcessBus::new();
        let grouped = Arc::new(AtomicUsize::new(0));
        let ungrouped = Arc::new(AtomicUsize::new(0));
        let mut subs = Vec::new();
        for _ in 0..3 {
            let seen = grouped.clone();
            subs.push(bus.subscribe(
                "peer.*.cluster_change",
                Some("mint"),
                Arc::new(move |_, _| {
                    seen.fetch_add(1, Ordering::Relaxed);
                }),
            ));
            let seen = ungrouped.clone();
            subs.push(bus.subscribe(
                "peer.*.cluster_change",
                None,
                Arc::new(move |_, _| {
                    seen.fetch_add(1, Ordering::Relaxed);
                }),
            ));
        }

        bus.inject("peer.0xabc.cluster_change", b"payload");

        assert_eq!(grouped.load(Ordering::Relaxed), 1);
        assert_eq!(ungrouped.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn dropping_the_handle_stops_delivery() {
        let bus = InProcessBus::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let subscription = bus.subscribe(
            "peer.*.connect",
            None,
            Arc::new(move |_, _| {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        bus.inject("peer.0xabc.connect", b"x");
        drop(subscription);
        bus.inject("peer.0xabc.connect", b"x");
        assert_eq!(seen.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_panicking_handler_does_not_stop_the_others() {
        let bus = InProcessBus::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let _boom = bus.subscribe(
            "peer.*.connect",
            None,
            Arc::new(|_, _| panic!("handler fault")),
        );
        let _ok = bus.subscribe(
            "peer.*.connect",
            None,
            Arc::new(move |_, _| {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        bus.inject("peer.0xabc.connect", b"x");
        assert_eq!(seen.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_publish_with_no_link_is_reported_apart_from_one_the_broker_refuses() {
        let bus = InProcessBus::new();
        assert_eq!(
            bus.publish("engine.peer.0xa.island_changed", vec![1]).await,
            PublishOutcome::Delivered
        );
        bus.refuse_publishes(true);
        assert_eq!(
            bus.publish("engine.peer.0xa.island_changed", vec![1]).await,
            PublishOutcome::Failed
        );
        bus.refuse_publishes(false);
        bus.set_connected(false);
        assert_eq!(
            bus.publish("engine.peer.0xa.island_changed", vec![1]).await,
            PublishOutcome::NoConnection
        );
        assert_eq!(bus.published().len(), 1);
    }

    #[cfg(feature = "nats")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn only_one_of_several_racing_activations_registers_a_subscription() {
        let bus = BrokerBus::new(Some("nats://127.0.0.1:4222".into()), "test");
        let _subscription =
            bus.subscribe("peer.*.cluster_change", Some("mint"), Arc::new(|_, _| {}));
        let shared = bus.shared.clone();

        let claimed = Arc::new(AtomicUsize::new(0));
        let mut racers = Vec::new();
        for _ in 0..8 {
            let shared = shared.clone();
            let claimed = claimed.clone();
            racers.push(tokio::spawn(async move {
                if claim(&shared, 0).is_some() {
                    claimed.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }
        for racer in racers {
            racer.await.unwrap();
        }

        assert_eq!(
            claimed.load(Ordering::SeqCst),
            1,
            "a second claim would register the same subject twice and deliver every message twice"
        );
    }

    /// Covers the claim handshake alone. `activate`'s subscribe-failure branch, which clears the
    /// same flag so a transient broker refusal cannot wedge a registration unclaimable, needs a
    /// broker that refuses a subscribe: cover it in the env-gated live suite, not here.
    #[cfg(feature = "nats")]
    #[tokio::test]
    async fn a_released_claim_is_takeable_again() {
        let bus = BrokerBus::new(Some("nats://127.0.0.1:4222".into()), "test");
        let _subscription = bus.subscribe("peer.*.cluster_change", None, Arc::new(|_, _| {}));
        let shared = bus.shared.clone();

        assert!(claim(&shared, 0).is_some());
        shared
            .registrations
            .lock()
            .unwrap()
            .get_mut(&0)
            .unwrap()
            .activating = false;
        assert!(claim(&shared, 0).is_some());
    }

    #[tokio::test]
    async fn a_disabled_bus_subscribes_to_nothing_and_delivers_nothing() {
        let bus = DisabledBus;
        assert!(!bus.is_enabled());
        let _subscription = bus.subscribe("peer.*.cluster_change", None, Arc::new(|_, _| {}));
        assert_eq!(
            bus.publish("engine.peer.0xa.island_changed", vec![1]).await,
            PublishOutcome::NoConnection
        );
    }
}
