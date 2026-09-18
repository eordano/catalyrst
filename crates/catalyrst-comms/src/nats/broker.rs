use super::{
    invoke, NatsBus, NatsHandler, NatsSubscription, PublishOutcome, RequestError,
    SubscriptionRegistry,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Notify};
use tokio::task::JoinHandle;
use tokio::time::{timeout, timeout_at, Instant};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
const BROKER_DEADLINE: Duration = Duration::from_secs(2);
pub const OUTBOUND_CAPACITY: usize = 128;
pub const MAX_PUBLISH_BYTES: usize = 64 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 128;
const PENDING: u8 = 0;
const SUBMITTING: u8 = 1;
const CANCELLED: u8 = 2;

struct Registration {
    subject: String,
    queue: Option<String>,
    handler: NatsHandler,
    live: Option<JoinHandle<()>>,
}

struct Link {
    client: OnceLock<async_nats::Client>,
    revision: AtomicU64,
    ready_revision: AtomicU64,
    broken: AtomicBool,
    changed: Notify,
}

impl Default for Link {
    fn default() -> Self {
        Self {
            client: OnceLock::new(),
            revision: AtomicU64::new(1),
            ready_revision: AtomicU64::new(0),
            broken: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }
}

impl Link {
    fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_one();
    }

    fn break_link(&self) {
        self.broken.store(true, Ordering::SeqCst);
        self.invalidate();
    }

    fn connected(&self) -> bool {
        !self.broken.load(Ordering::SeqCst)
            && self.client.get().is_some_and(|client| {
                client.connection_state() == async_nats::connection::State::Connected
            })
    }

    fn ready(&self) -> bool {
        self.ready_revision.load(Ordering::SeqCst) == self.revision.load(Ordering::SeqCst)
            && self.connected()
    }
}

struct Publish {
    subject: Box<str>,
    payload: Box<[u8]>,
    link: Weak<Link>,
    revision: u64,
    deadline: Instant,
    result: oneshot::Sender<PublishOutcome>,
    phase: Arc<AtomicU8>,
}

struct Request {
    subject: Box<str>,
    payload: Box<[u8]>,
    link: Weak<Link>,
    revision: u64,
    deadline: Instant,
    result: oneshot::Sender<Result<Vec<u8>, RequestError>>,
}

enum Outbound {
    Publish(Publish),
    Request(Request),
}

impl Outbound {
    fn disconnected(self) {
        match self {
            Self::Publish(publish) => {
                let _ = publish.result.send(PublishOutcome::NoConnection);
            }
            Self::Request(request) => {
                let _ = request.result.send(Err(RequestError::NoConnection));
            }
        }
    }
}

struct PendingRequest {
    revision: u64,
    deadline: Instant,
    result: oneshot::Sender<Result<Vec<u8>, RequestError>>,
}

struct Requests {
    inbox: String,
    replies: async_nats::Subscriber,
    sequence: u64,
    pending: HashMap<String, PendingRequest>,
}

impl Requests {
    async fn new(client: &async_nats::Client) -> Result<Self, ()> {
        let inbox = client.new_inbox();
        let replies = client
            .subscribe(format!("{inbox}.*"))
            .await
            .map_err(|_| ())?;
        Ok(Self {
            inbox,
            replies,
            sequence: 0,
            pending: HashMap::new(),
        })
    }

    fn expire(&mut self, link: &Link) {
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(subject, request)| {
                let reason =
                    if !link.ready() || request.revision != link.revision.load(Ordering::SeqCst) {
                        Some(RequestError::NoConnection)
                    } else if request.result.is_closed() || Instant::now() >= request.deadline {
                        Some(RequestError::TimedOut)
                    } else {
                        None
                    };
                reason.map(|reason| (subject.clone(), reason))
            })
            .collect();
        for (subject, reason) in expired {
            if let Some(request) = self.pending.remove(&subject) {
                let _ = request.result.send(Err(reason));
            }
        }
    }

    async fn submit(&mut self, client: &async_nats::Client, link: &Arc<Link>, request: Request) {
        self.expire(link);
        if request.result.is_closed() {
            return;
        }
        let rejected = if !link.ready()
            || request.revision != link.revision.load(Ordering::SeqCst)
            || !request
                .link
                .upgrade()
                .is_some_and(|owner| Arc::ptr_eq(&owner, link))
        {
            Some(RequestError::NoConnection)
        } else if Instant::now() >= request.deadline {
            Some(RequestError::TimedOut)
        } else if self.pending.len() >= MAX_PENDING_REQUESTS {
            Some(RequestError::Overloaded)
        } else {
            None
        };
        if let Some(error) = rejected {
            let _ = request.result.send(Err(error));
            return;
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            link.break_link();
            let _ = request.result.send(Err(RequestError::NoConnection));
            return;
        };
        self.sequence = sequence;
        let reply = format!("{}.{}", self.inbox, sequence);
        let submitted = timeout_at(
            request.deadline,
            client.publish_with_reply(
                request.subject.into_string(),
                reply.clone(),
                request.payload.into_vec().into(),
            ),
        )
        .await;
        match submitted {
            Ok(Ok(())) => {
                self.pending.insert(
                    reply,
                    PendingRequest {
                        revision: request.revision,
                        deadline: request.deadline,
                        result: request.result,
                    },
                );
            }
            Ok(Err(_)) => {
                let _ = request.result.send(Err(RequestError::Invalid));
            }
            Err(_) => {
                let _ = request.result.send(Err(RequestError::TimedOut));
            }
        }
    }

    fn receive(&mut self, link: &Link, reply: async_nats::Message) {
        let Some(request) = self.pending.remove(reply.subject.as_str()) else {
            return;
        };
        let result = if !link.ready() || request.revision != link.revision.load(Ordering::SeqCst) {
            Err(RequestError::NoConnection)
        } else if Instant::now() >= request.deadline {
            Err(RequestError::TimedOut)
        } else if reply.status == Some(async_nats::StatusCode::NO_RESPONDERS) {
            Err(RequestError::NoResponders)
        } else if reply.status.is_some() || reply.payload.len() > MAX_PUBLISH_BYTES {
            Err(RequestError::Invalid)
        } else {
            Ok(reply.payload.to_vec())
        };
        let _ = request.result.send(result);
    }
}

struct Shared {
    url: Option<String>,
    name: String,
    link: Mutex<Option<Arc<Link>>>,
    registrations: Mutex<HashMap<u64, Registration>>,
    next_id: AtomicU64,
    changed: Notify,
    tx: mpsc::Sender<Outbound>,
    rx: Mutex<Option<mpsc::Receiver<Outbound>>>,
}

impl Shared {
    fn link(&self) -> Option<Arc<Link>> {
        self.link.lock().unwrap().clone()
    }

    fn invalidate(&self) {
        if let Some(link) = self.link() {
            link.invalidate();
        }
        self.changed.notify_one();
    }
}

impl SubscriptionRegistry for Shared {
    fn cancel(&self, id: u64) {
        let registration = self.registrations.lock().unwrap().remove(&id);
        if let Some(registration) = registration {
            self.invalidate();
            if let Some(task) = registration.live {
                task.abort();
            }
        }
    }
}

/// Bounded, incarnation-scoped core-NATS exchanges. Only the supervisor activates subscriptions
/// and publishes, so registration races cannot create duplicate delivery loops.
pub struct BrokerBus {
    shared: Arc<Shared>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl BrokerBus {
    pub fn new(url: Option<String>, name: impl Into<String>) -> Self {
        let (tx, rx) = mpsc::channel(OUTBOUND_CAPACITY);
        Self {
            shared: Arc::new(Shared {
                url: url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
                name: name.into(),
                link: Mutex::new(None),
                registrations: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
                changed: Notify::new(),
                tx,
                rx: Mutex::new(Some(rx)),
            }),
            task: Mutex::new(None),
        }
    }
}

impl Drop for BrokerBus {
    fn drop(&mut self) {
        if let Some(link) = self.shared.link() {
            link.break_link();
        }
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
    }
}

#[async_trait::async_trait]
impl NatsBus for BrokerBus {
    fn is_enabled(&self) -> bool {
        self.shared.url.is_some()
    }

    fn is_connected(&self) -> bool {
        self.shared.link().is_some_and(|link| link.connected())
    }

    fn is_ready(&self) -> bool {
        !self.is_enabled() || self.shared.link().is_some_and(|link| link.ready())
    }

    fn connect(&self) {
        if !self.is_enabled() {
            return;
        }
        let mut task = self.task.lock().unwrap();
        if let Some(rx) = self.shared.rx.lock().unwrap().take() {
            *task = Some(tokio::spawn(supervise(self.shared.clone(), rx)));
        }
    }

    fn subscribe(
        &self,
        subject: &str,
        queue: Option<&str>,
        handler: NatsHandler,
    ) -> NatsSubscription {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        self.shared.registrations.lock().unwrap().insert(
            id,
            Registration {
                subject: subject.into(),
                queue: queue.map(str::to_string),
                handler,
                live: None,
            },
        );
        self.shared.invalidate();
        NatsSubscription {
            registry: Arc::downgrade(&self.shared) as Weak<dyn SubscriptionRegistry>,
            id,
        }
    }

    async fn publish(&self, subject: &str, payload: Vec<u8>) -> PublishOutcome {
        let Some(link) = self.shared.link().filter(|link| link.ready()) else {
            return PublishOutcome::NoConnection;
        };
        if subject.len().saturating_add(payload.len()) > MAX_PUBLISH_BYTES {
            return PublishOutcome::Failed;
        }
        let (result, receive) = oneshot::channel();
        let deadline = Instant::now() + BROKER_DEADLINE;
        let phase = Arc::new(AtomicU8::new(PENDING));
        let publish = Publish {
            subject: subject.into(),
            payload: payload.into_boxed_slice(),
            link: Arc::downgrade(&link),
            revision: link.revision.load(Ordering::SeqCst),
            deadline,
            result,
            phase: phase.clone(),
        };
        if self.shared.tx.try_send(Outbound::Publish(publish)).is_err() {
            return PublishOutcome::Failed;
        }
        let current = Arc::downgrade(&link);
        drop(link);
        match timeout_at(deadline, receive).await {
            Ok(Ok(outcome)) => outcome,
            _ => {
                if phase
                    .compare_exchange(PENDING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return PublishOutcome::Failed;
                }
                if let Some(link) = current.upgrade() {
                    link.break_link();
                }
                PublishOutcome::Unconfirmed
            }
        }
    }

    async fn request(&self, subject: &str, payload: Vec<u8>) -> Result<Vec<u8>, RequestError> {
        let Some(link) = self.shared.link().filter(|link| link.ready()) else {
            return Err(RequestError::NoConnection);
        };
        if subject.len().saturating_add(payload.len()) > MAX_PUBLISH_BYTES {
            return Err(RequestError::Invalid);
        }
        let (result, receive) = oneshot::channel();
        let deadline = Instant::now() + BROKER_DEADLINE;
        let revision = link.revision.load(Ordering::SeqCst);
        self.shared
            .tx
            .try_send(Outbound::Request(Request {
                subject: subject.into(),
                payload: payload.into_boxed_slice(),
                link: Arc::downgrade(&link),
                revision,
                deadline,
                result,
            }))
            .map_err(|_| RequestError::Overloaded)?;
        let owner = Arc::downgrade(&link);
        drop(link);
        let result = timeout_at(deadline, receive)
            .await
            .map_err(|_| RequestError::TimedOut)?
            .unwrap_or(Err(RequestError::NoConnection));
        if !owner
            .upgrade()
            .is_some_and(|link| link.ready() && link.revision.load(Ordering::SeqCst) == revision)
        {
            return Err(RequestError::NoConnection);
        }
        result
    }
}

struct LinkLifetime(Arc<Shared>, Arc<Link>);

impl Drop for LinkLifetime {
    fn drop(&mut self) {
        self.1.break_link();
        self.0.link.lock().unwrap().take();
        for registration in self.0.registrations.lock().unwrap().values_mut() {
            if let Some(task) = registration.live.take() {
                task.abort();
            }
        }
        crate::metrics::nats_connected(false);
    }
}

struct Fence {
    inbox: String,
    replies: async_nats::Subscriber,
    sequence: u64,
}

impl Fence {
    async fn new(client: &async_nats::Client) -> Result<Self, ()> {
        let inbox = client.new_inbox();
        let replies = client.subscribe(inbox.clone()).await.map_err(|_| ())?;
        Ok(Self {
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

fn options(shared: &Shared, link: &Arc<Link>) -> async_nats::ConnectOptions {
    let link = Arc::downgrade(link);
    async_nats::ConnectOptions::new()
        .name(shared.name.clone())
        .client_capacity(OUTBOUND_CAPACITY)
        .subscription_capacity(1024)
        .event_callback(move |event| {
            let link = link.clone();
            async move {
                if let Some(link) = link.upgrade() {
                    match event {
                        async_nats::Event::Connected | async_nats::Event::Disconnected => {
                            link.invalidate()
                        }
                        async_nats::Event::Closed
                        | async_nats::Event::SlowConsumer(_)
                        | async_nats::Event::ServerError(_)
                        | async_nats::Event::ClientError(_) => link.break_link(),
                        _ => {}
                    }
                }
            }
        })
}

async fn supervise(shared: Arc<Shared>, mut rx: mpsc::Receiver<Outbound>) {
    loop {
        let link = Arc::new(Link::default());
        *shared.link.lock().unwrap() = Some(link.clone());
        let lifetime = LinkLifetime(shared.clone(), link.clone());
        match options(&shared, &link)
            .connect(shared.url.as_ref().unwrap())
            .await
        {
            Ok(client) => {
                let _ = link.client.set(client);
                tracing::info!("cluster feed socket connected; awaiting broker round trip");
                run(&shared, &link, &mut rx).await;
                tracing::warn!("cluster feed broker exchange ended; rebuilding");
            }
            Err(_) => tracing::warn!("cluster feed broker connect failed"),
        }
        drop(lifetime);
        drop(link);
        while let Ok(outbound) = rx.try_recv() {
            outbound.disconnected();
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn activate(shared: &Shared, link: &Arc<Link>) -> Result<(), ()> {
    let registrations: Vec<_> = shared
        .registrations
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, registration)| registration.live.is_none())
        .map(|(id, registration)| {
            (
                *id,
                registration.subject.clone(),
                registration.queue.clone(),
                registration.handler.clone(),
            )
        })
        .collect();
    let client = link.client.get().ok_or(())?;
    for (id, subject, queue, handler) in registrations {
        let mut subscriber = match queue {
            Some(queue) => client.queue_subscribe(subject, queue).await,
            None => client.subscribe(subject).await,
        }
        .map_err(|_| ())?;
        let current_link = Arc::downgrade(link);
        let live = tokio::spawn(async move {
            while let Some(message) = subscriber.next().await {
                invoke(&handler, message.subject.as_str(), &message.payload);
            }
            if let Some(link) = current_link.upgrade() {
                link.break_link();
            }
        });
        match shared.registrations.lock().unwrap().get_mut(&id) {
            Some(registration) => registration.live = Some(live),
            None => live.abort(),
        }
    }
    Ok(())
}

async fn restore(shared: &Shared, link: &Arc<Link>, fence: &mut Fence) -> Result<(), ()> {
    if !link.connected() {
        crate::metrics::nats_connected(false);
        return Ok(());
    }
    let revision = link.revision.load(Ordering::SeqCst);
    timeout(BROKER_DEADLINE, async {
        activate(shared, link).await?;
        fence.confirm(link.client.get().ok_or(())?).await
    })
    .await
    .map_err(|_| ())??;
    let registrations_live = shared
        .registrations
        .lock()
        .unwrap()
        .values()
        .all(|registration| {
            registration
                .live
                .as_ref()
                .is_some_and(|task| !task.is_finished())
        });
    if registrations_live {
        link.ready_revision.store(revision, Ordering::SeqCst);
    }
    crate::metrics::nats_connected(link.ready());
    Ok(())
}

async fn run(shared: &Shared, link: &Arc<Link>, rx: &mut mpsc::Receiver<Outbound>) {
    let client = link.client.get().unwrap();
    let Ok(Ok(mut fence)) = timeout(BROKER_DEADLINE, Fence::new(client)).await else {
        return;
    };
    let Ok(Ok(mut requests)) = timeout(BROKER_DEADLINE, Requests::new(client)).await else {
        return;
    };
    let mut expiry = tokio::time::interval(Duration::from_millis(100));
    expiry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut probe = tokio::time::interval(PROBE_INTERVAL);
    probe.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        if link.broken.load(Ordering::SeqCst) {
            return;
        }
        tokio::select! {
            _ = expiry.tick() => requests.expire(link),
            reply = requests.replies.next() => {
                let Some(reply) = reply else { return };
                requests.receive(link, reply);
            }
            _ = probe.tick() => {
                if restore(shared, link, &mut fence).await.is_err() { return; }
            }
            _ = link.changed.notified() => {
                if link.broken.load(Ordering::SeqCst) || restore(shared, link, &mut fence).await.is_err() { return; }
            }
            _ = shared.changed.notified() => {
                if restore(shared, link, &mut fence).await.is_err() { return; }
            }
            outbound = rx.recv() => {
                let Some(outbound) = outbound else { return };
                let publish = match outbound {
                    Outbound::Request(request) => {
                        requests.submit(client, link, request).await;
                        continue;
                    }
                    Outbound::Publish(publish) => publish,
                };
                if publish.result.is_closed() { continue; }
                if !link.ready() || publish.revision != link.revision.load(Ordering::SeqCst)
                    || !publish.link.upgrade().is_some_and(|owner| Arc::ptr_eq(&owner, link)) {
                    let _ = publish.result.send(PublishOutcome::NoConnection);
                    continue;
                }
                if Instant::now() >= publish.deadline {
                    let _ = publish.result.send(PublishOutcome::Failed);
                    continue;
                }
                if publish.phase.compare_exchange(PENDING, SUBMITTING, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                    let _ = publish.result.send(PublishOutcome::Failed);
                    continue;
                }
                let outcome = timeout_at(publish.deadline, async {
                    client.publish(publish.subject.into_string(), publish.payload.into_vec().into())
                        .await.map_err(|_| PublishOutcome::Failed)?;
                    fence.confirm(client).await.map_err(|_| PublishOutcome::Unconfirmed)?;
                    if link.ready() && publish.revision == link.revision.load(Ordering::SeqCst) {
                        Ok(PublishOutcome::Submitted)
                    } else {
                        Err(PublishOutcome::Unconfirmed)
                    }
                }).await.unwrap_or(Err(PublishOutcome::Unconfirmed)).unwrap_or_else(|outcome| outcome);
                let _ = publish.result.send(outcome);
                if outcome == PublishOutcome::Unconfirmed { return; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_probe_cannot_overwrite_newer_link_invalidation() {
        let link = Link::default();
        let probed = link.revision.load(Ordering::SeqCst);
        link.invalidate();
        link.ready_revision.store(probed, Ordering::SeqCst);
        assert_ne!(
            link.ready_revision.load(Ordering::SeqCst),
            link.revision.load(Ordering::SeqCst)
        );
        assert!(!link.ready());
    }

    #[tokio::test]
    async fn registrations_are_owned_by_the_supervisor_and_cancelled_before_activation() {
        let bus = Arc::new(BrokerBus::new(Some("nats://127.0.0.1:1".into()), "test"));
        let mut racers = Vec::new();
        for _ in 0..8 {
            let bus = bus.clone();
            racers.push(tokio::spawn(async move {
                bus.subscribe("peer.*.cluster_change", Some("mint"), Arc::new(|_, _| {}))
            }));
        }
        let mut subscriptions = Vec::new();
        for racer in racers {
            subscriptions.push(racer.await.unwrap());
        }
        let registrations = bus.shared.registrations.lock().unwrap();
        assert_eq!(registrations.len(), 8);
        assert!(registrations
            .values()
            .all(|registration| registration.live.is_none()));
        drop(registrations);
        drop(subscriptions);
        assert!(bus.shared.registrations.lock().unwrap().is_empty());
    }
}
