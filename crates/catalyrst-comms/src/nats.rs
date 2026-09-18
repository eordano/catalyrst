//! The broker seam the Pulse cluster feed arrives on: core NATS, no JetStream.
//!
//! Every consumer sees only "the callback fires or it does not". Connection state, reconnection
//! and backoff live in `broker` and nowhere else, so nothing downstream ever branches on the link
//! being up. A publish reports its outcome as a value rather than an error, because "there was no
//! connection to hand this to" is a routine state of a fire-and-forget feed, not a fault.
//!
//! Handlers are invoked from the subscription's own task: one that panics is contained so the
//! remaining subjects keep delivering, mirroring upstream's never-throws contract.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// Invoked once per message with the concrete subject it arrived on and its payload. Must not
/// panic; one that does is caught, but the message is lost.
pub type NatsHandler = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// Submitted on a connection that completed a subsequent broker round trip. This is not a
    /// subscriber acknowledgement: core NATS can still have no consumer or refuse a subject.
    Submitted,
    /// No current ready connection; the message was discarded before submission to the SDK.
    NoConnection,
    /// Rejected locally, including overload, invalid subjects and oversized payloads.
    Failed,
    /// The bounded exchange failed after submission became possible. Delivery is unknown;
    /// recovery must reconcile current state, not infer that nothing reached a consumer.
    Unconfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    NoConnection,
    Overloaded,
    Invalid,
    NoResponders,
    TimedOut,
}

pub type NatsResponder = Arc<dyn Fn(&str, &[u8]) -> Result<Vec<u8>, RequestError> + Send + Sync>;

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

    /// Whether subscription commands have crossed a broker round trip on the current link.
    /// This is transport readiness, not confirmation of permissions or current assignment state.
    /// A disabled bus has nothing to wait for and reports ready.
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

    /// A bounded, correlated read-only exchange on one ready connection incarnation. A timeout
    /// says nothing about source ownership and must never authorize a cached fallback or a write.
    async fn request(&self, subject: &str, payload: Vec<u8>) -> Result<Vec<u8>, RequestError>;
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

#[cfg(feature = "nats")]
mod broker;
#[cfg(feature = "nats")]
pub use broker::{BrokerBus, MAX_PENDING_REQUESTS, MAX_PUBLISH_BYTES, OUTBOUND_CAPACITY};

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
    unconfirmed: AtomicBool,
    responder: Mutex<Option<NatsResponder>>,
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
                unconfirmed: AtomicBool::new(false),
                responder: Mutex::new(None),
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

    /// Simulates delivery followed by a failed confirmation. Consumers must not mistake this
    /// outcome for either a confirmed exchange or confirmation that nothing arrived.
    pub fn leave_publishes_unconfirmed(&self, unconfirmed: bool) {
        self.shared
            .unconfirmed
            .store(unconfirmed, Ordering::Relaxed);
    }

    pub fn respond_to_requests(&self, responder: NatsResponder) {
        *self.shared.responder.lock().unwrap() = Some(responder);
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
        self.is_connected()
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
        if self.shared.unconfirmed.load(Ordering::Relaxed) {
            PublishOutcome::Unconfirmed
        } else {
            PublishOutcome::Submitted
        }
    }

    async fn request(&self, subject: &str, payload: Vec<u8>) -> Result<Vec<u8>, RequestError> {
        if !self.is_ready() {
            return Err(RequestError::NoConnection);
        }
        let responder = self.shared.responder.lock().unwrap().clone();
        responder.ok_or(RequestError::NoResponders)?(subject, &payload)
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

    async fn request(&self, _subject: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RequestError> {
        Err(RequestError::NoConnection)
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
            PublishOutcome::Submitted
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
