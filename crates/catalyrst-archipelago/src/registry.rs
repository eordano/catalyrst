use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// What the feed hands a live socket. The island payload travels encoded: only the socket that
/// owns the assignment decodes it, so a message addressed to nobody costs no decode at all.
#[derive(Clone, Debug)]
pub enum SocketEvent {
    IslandChanged(Vec<u8>),
    Kicked,
}

#[derive(Clone)]
pub struct PeerLink {
    pub session: String,
    pub id: u64,
    closed: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<SocketEvent>,
}

impl PeerLink {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst) || self.tx.is_closed()
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn send(&self, event: SocketEvent) -> bool {
        self.tx.send(event).is_ok()
    }
}

/// The registry of sockets connected to this replica, keyed by wallet and then by session key.
///
/// Every replica receives every island assignment, and this registry is the filter: only the
/// replica holding the (wallet, session) an assignment is addressed to forwards it. Keys are
/// exact strings, so callers register and look up with the same lower-cased forms.
#[derive(Default)]
pub struct PeersRegistry {
    peers: Mutex<HashMap<String, Vec<PeerLink>>>,
    next_id: AtomicU64,
}

impl PeersRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Registers a session's socket and returns the link it replaced, if this exact
    /// (address, session) already held one: that is the device's own socket from a drop this
    /// replica has not noticed, and the caller ends it.
    pub fn on_peer_connected(
        &self,
        address: &str,
        session: &str,
    ) -> (
        PeerLink,
        mpsc::UnboundedReceiver<SocketEvent>,
        Option<PeerLink>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let link = PeerLink {
            session: session.to_string(),
            id: self.next_id.fetch_add(1, Ordering::SeqCst) + 1,
            closed: Arc::new(AtomicBool::new(false)),
            tx,
        };
        let mut peers = self.peers.lock();
        let sessions = peers.entry(address.to_string()).or_default();
        let previous = sessions
            .iter()
            .position(|held| held.session == session)
            .map(|at| sessions.remove(at));
        sessions.push(link.clone());
        (link, rx, previous)
    }

    /// Removes a session's socket, but only while the registry still points at this exact one.
    /// After a same-session reconnect the previous socket closes later, and without the identity
    /// check its close would evict the live socket from the feed and from the ban sweep.
    pub fn on_peer_disconnected(&self, address: &str, session: &str, id: u64) {
        let mut peers = self.peers.lock();
        let Some(sessions) = peers.get_mut(address) else {
            return;
        };
        sessions.retain(|held| !(held.session == session && held.id == id));
        if sessions.is_empty() {
            peers.remove(address);
        }
    }

    pub fn peer_link(&self, address: &str, session: &str) -> Option<PeerLink> {
        self.peers
            .lock()
            .get(address)?
            .iter()
            .find(|held| held.session == session)
            .cloned()
    }

    /// The most recently registered socket of any session of the wallet. Used only for the
    /// legacy, session-less island subject.
    pub fn newest_link(&self, address: &str) -> Option<PeerLink> {
        self.peers.lock().get(address)?.last().cloned()
    }

    pub fn has_peer(&self, address: &str) -> bool {
        self.peers.lock().contains_key(address)
    }

    pub fn peer_count(&self) -> usize {
        self.peers.lock().values().map(Vec::len).sum()
    }

    pub fn sessions_of(&self, address: &str) -> Vec<PeerLink> {
        self.peers.lock().get(address).cloned().unwrap_or_default()
    }

    /// Every (address, session) this replica currently holds. The ban sweep re-checks each one,
    /// and the broker link re-announces each one after a reconnect: the cluster feed is
    /// edge-triggered, so a session whose connect announcement was lost while the link was down
    /// is never given a room until it moves.
    pub fn snapshot(&self) -> Vec<(String, String)> {
        let peers = self.peers.lock();
        let mut out = Vec::with_capacity(peers.len());
        for (address, sessions) in peers.iter() {
            for link in sessions {
                out.push((address.clone(), link.session.clone()));
            }
        }
        out
    }

    /// Ends every session of a wallet on this replica. Returns how many were reached.
    pub fn kick(&self, address: &str) -> usize {
        self.sessions_of(address)
            .into_iter()
            .filter(|link| link.send(SocketEvent::Kicked))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_reachable_by_its_own_key_only() {
        let registry = PeersRegistry::new();
        let (link, _rx, previous) = registry.on_peer_connected("0xa", "0xs1");
        assert!(previous.is_none());
        assert_eq!(
            registry.peer_link("0xa", "0xs1").map(|l| l.id),
            Some(link.id)
        );
        assert!(registry.peer_link("0xa", "0xs2").is_none());
        assert!(registry.has_peer("0xa"));
        assert_eq!(registry.peer_count(), 1);
    }

    #[test]
    fn a_second_session_leaves_the_first_alone_and_becomes_the_newest() {
        let registry = PeersRegistry::new();
        let (first, _rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (second, _rx_b, previous) = registry.on_peer_connected("0xa", "0xs2");
        assert!(previous.is_none());
        assert_eq!(
            registry.peer_link("0xa", "0xs1").map(|l| l.id),
            Some(first.id)
        );
        assert_eq!(registry.newest_link("0xa").map(|l| l.id), Some(second.id));
        assert_eq!(registry.peer_count(), 2);
    }

    #[test]
    fn the_same_session_reconnecting_hands_back_the_socket_it_replaced() {
        let registry = PeersRegistry::new();
        let (first, _rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (second, _rx_b, previous) = registry.on_peer_connected("0xa", "0xs1");
        assert_eq!(previous.map(|l| l.id), Some(first.id));
        assert_eq!(
            registry.peer_link("0xa", "0xs1").map(|l| l.id),
            Some(second.id)
        );
        assert_eq!(registry.peer_count(), 1);
    }

    #[test]
    fn a_stale_close_does_not_evict_the_live_socket() {
        let registry = PeersRegistry::new();
        let (first, _rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (second, _rx_b, _) = registry.on_peer_connected("0xa", "0xs1");
        registry.on_peer_disconnected("0xa", "0xs1", first.id);
        assert_eq!(
            registry.peer_link("0xa", "0xs1").map(|l| l.id),
            Some(second.id)
        );
        registry.on_peer_disconnected("0xa", "0xs1", second.id);
        assert!(!registry.has_peer("0xa"));
    }

    #[test]
    fn a_snapshot_names_every_live_address_and_session() {
        let registry = PeersRegistry::new();
        let (_a, _rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (_b, _rx_b, _) = registry.on_peer_connected("0xa", "0xs2");
        let (_c, _rx_c, _) = registry.on_peer_connected("0xb", "0xs3");
        let mut snapshot = registry.snapshot();
        snapshot.sort();
        assert_eq!(
            snapshot,
            vec![
                ("0xa".to_string(), "0xs1".to_string()),
                ("0xa".to_string(), "0xs2".to_string()),
                ("0xb".to_string(), "0xs3".to_string()),
            ]
        );
        registry.on_peer_disconnected("0xb", "0xs3", _c.id);
        assert_eq!(registry.snapshot().len(), 2);
    }

    #[test]
    fn a_kick_reaches_every_session_of_the_wallet() {
        let registry = PeersRegistry::new();
        let (_a, mut rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (_b, mut rx_b, _) = registry.on_peer_connected("0xa", "0xs2");
        assert_eq!(registry.kick("0xa"), 2);
        assert!(matches!(rx_a.try_recv(), Ok(SocketEvent::Kicked)));
        assert!(matches!(rx_b.try_recv(), Ok(SocketEvent::Kicked)));
        assert_eq!(registry.kick("0xb"), 0);
    }
}
