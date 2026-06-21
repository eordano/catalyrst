//! Shared RPC server context — holds the DB, the per-user `UserUpdate` pubsub,
//! the transport identity map, and the in-process event queue that the
//! SendEvent processor drains.
//!
//! Single-node analogue of upstream's Redis-backed channel + events queue:
//! upstream fans `UserUpdate`s through a Redis channel keyed by user address
//! and pushes `Event`s onto a Redis list consumed by the event processor; here
//! both are in-process (a `tokio::broadcast` per address and an unbounded mpsc).

use crate::db::Db;
use crate::proto::{Event, UserUpdate};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

const CHANNEL_CAP: usize = 256;

/// Per-user `UserUpdate` fan-out. The Subscribe RPC's generator is fed from the
/// receiver for the connection's authenticated address.
#[derive(Clone)]
pub struct UserUpdatePubSub {
    channels: Arc<DashMap<String, broadcast::Sender<UserUpdate>>>,
}

impl UserUpdatePubSub {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
        }
    }

    pub fn subscribe(&self, address: &str) -> broadcast::Receiver<UserUpdate> {
        self.channels
            .entry(address.to_lowercase())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAP).0)
            .subscribe()
    }

    /// Publish a `UserUpdate` to its `user_address` channel (upstream's
    /// `redis_channel_publisher.publish` + subscriber fan-out).
    pub fn publish(&self, update: UserUpdate) {
        let addr = update.user_address.to_lowercase();
        if let Some(sender) = self.channels.get(&addr) {
            let _ = sender.send(update);
        }
    }
}

impl Default for UserUpdatePubSub {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ContextInner {
    pub db: Arc<Db>,
    pubsub: UserUpdatePubSub,
    /// transport id -> authenticated (lowercased) user address.
    identities: DashMap<u32, String>,
    /// Submission side of the in-process events queue (SendEvent -> processor).
    events_tx: mpsc::UnboundedSender<Event>,
}

#[derive(Clone)]
pub struct Context(Arc<ContextInner>);

impl Context {
    /// Build the context, returning it alongside the receiver end of the events
    /// queue (the processor task owns the receiver).
    pub fn new(db: Arc<Db>) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let ctx = Self(Arc::new(ContextInner {
            db,
            pubsub: UserUpdatePubSub::new(),
            identities: DashMap::new(),
            events_tx,
        }));
        (ctx, events_rx)
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.0.db
    }

    pub fn pubsub(&self) -> &UserUpdatePubSub {
        &self.0.pubsub
    }

    pub fn register_identity(&self, transport_id: u32, address: String) {
        self.0
            .identities
            .insert(transport_id, address.to_lowercase());
    }

    pub fn forget_identity(&self, transport_id: u32) {
        self.0.identities.remove(&transport_id);
    }

    pub fn identity(&self, transport_id: u32) -> Option<String> {
        self.0.identities.get(&transport_id).map(|r| r.clone())
    }

    /// Push an event onto the in-process queue for the processor (upstream
    /// `events_queue.push`). Returns false only if the processor is gone.
    pub fn push_event(&self, event: Event) -> bool {
        self.0.events_tx.send(event).is_ok()
    }
}

pub type SharedContext = Context;
