use parking_lot::Mutex;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

mod mailbox;
pub(crate) use mailbox::AssignmentSend;
use mailbox::MailboxSender;
pub use mailbox::{SocketEvents, MAX_ASSIGNMENT_BYTES};

/// What the feed hands a live socket. The feed validates assignments before retaining their
/// bounded wire bytes; messages addressed to nobody cost no decode or mailbox allocation.
#[derive(Clone, Debug)]
pub enum SocketEvent {
    IslandChanged(Vec<u8>),
    Kicked,
}

#[derive(Clone)]
pub struct PeerLink {
    pub session: String,
    pub id: u64,
    assignment_sent: Arc<AtomicBool>,
    tx: Arc<MailboxSender>,
}

/// A broker reconnect announcement tied to the exact socket which authorized it.
///
/// `connection_id` is absent for legacy sockets. It is present for additive sockets so a queued
/// announcement cannot be published on behalf of an older socket which used the same signer
/// session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconciliationPeer {
    pub(crate) address: String,
    pub(crate) session: String,
    pub(crate) id: u64,
    pub(crate) connection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlPositionPermit {
    address: String,
    session: String,
    epoch: u64,
    link_id: u64,
    close: bool,
}

impl ControlPositionPermit {
    pub(crate) fn matches(&self, address: &str, session: &str, epoch: u64, close: bool) -> bool {
        self.address == address
            && self.session == session
            && self.epoch == epoch
            && self.close == close
    }

    pub(crate) fn is_close(&self) -> bool {
        self.close
    }
}

struct V4Peer {
    raw_session: String,
    connection_id: String,
    connection_epoch: u64,
    realm_owner: bool,
    control_position_authorized: bool,
    link: PeerLink,
}

impl PeerLink {
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub fn close(&self) {
        self.tx.close();
    }

    pub fn needs_assignment(&self) -> bool {
        !self.is_closed() && !self.assignment_sent.load(Ordering::SeqCst)
    }

    pub(crate) fn assignment_sent(&self) {
        self.assignment_sent.store(true, Ordering::SeqCst);
    }

    pub(crate) fn assignment_cleared(&self) {
        self.assignment_sent.store(false, Ordering::SeqCst);
    }

    pub fn send(&self, event: SocketEvent) -> bool {
        match event {
            SocketEvent::IslandChanged(bytes) => matches!(
                self.send_assignment(&bytes),
                AssignmentSend::Queued | AssignmentSend::Coalesced
            ),
            SocketEvent::Kicked => self.tx.kick(),
        }
    }

    pub(crate) fn send_assignment(&self, bytes: &[u8]) -> AssignmentSend {
        self.tx.assignment(bytes)
    }

    pub(crate) async fn cancelled(&self) {
        self.tx.cancelled().await;
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
    v4_peers: Mutex<HashMap<String, Vec<V4Peer>>>,
    next_id: AtomicU64,
    admissions: Mutex<AdmissionState>,
}

#[derive(Default)]
struct AdmissionState {
    next_sequence: u64,
    wallets: HashMap<String, WalletAdmissions>,
}

#[derive(Default)]
struct WalletAdmissions {
    pending: BTreeSet<u64>,
    committed: HashMap<String, u64>,
}

pub(crate) struct AdmissionTicket {
    registry: Arc<PeersRegistry>,
    address: String,
    sequence: u64,
    active: bool,
}

impl AdmissionState {
    fn retire(&mut self, address: &str, sequence: u64) {
        let remove_wallet = if let Some(wallet) = self.wallets.get_mut(address) {
            wallet.pending.remove(&sequence);
            if let Some(oldest) = wallet.pending.first().copied() {
                wallet.committed.retain(|_, committed| *committed > oldest);
            } else {
                wallet.committed.clear();
            }
            wallet.pending.is_empty() && wallet.committed.is_empty()
        } else {
            false
        };
        if remove_wallet {
            self.wallets.remove(address);
        }
    }
}

impl AdmissionTicket {
    pub(crate) fn commit(
        mut self,
        session: &str,
    ) -> Option<(PeerLink, SocketEvents, Option<PeerLink>)> {
        let registry = Arc::clone(&self.registry);
        let mut admissions = registry.admissions.lock();
        self.active = false;

        let superseded = admissions
            .wallets
            .get(&self.address)
            .and_then(|wallet| wallet.committed.get(session))
            .is_some_and(|committed| *committed > self.sequence);
        if superseded {
            admissions.retire(&self.address, self.sequence);
            return None;
        }

        admissions
            .wallets
            .get_mut(&self.address)
            .expect("an active admission ticket remains registered")
            .committed
            .insert(session.to_owned(), self.sequence);
        let connected = registry.on_peer_connected(&self.address, session);
        admissions.retire(&self.address, self.sequence);
        Some(connected)
    }
}

impl Drop for AdmissionTicket {
    fn drop(&mut self) {
        if self.active {
            self.registry
                .admissions
                .lock()
                .retire(&self.address, self.sequence);
        }
    }
}

impl PeersRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn begin_admission(self: &Arc<Self>, address: &str) -> Option<AdmissionTicket> {
        let mut admissions = self.admissions.lock();
        let sequence = admissions.next_sequence.checked_add(1)?;
        admissions.next_sequence = sequence;
        admissions
            .wallets
            .entry(address.to_owned())
            .or_default()
            .pending
            .insert(sequence);
        Some(AdmissionTicket {
            registry: Arc::clone(self),
            address: address.to_owned(),
            sequence,
            active: true,
        })
    }

    /// Registers a session's socket and returns the link it replaced, if this exact
    /// (address, session) already held one: that is the device's own socket from a drop this
    /// replica has not noticed, and the caller ends it.
    pub fn on_peer_connected(
        &self,
        address: &str,
        session: &str,
    ) -> (PeerLink, SocketEvents, Option<PeerLink>) {
        let (link, rx) = self.new_link(session);
        let mut peers = self.peers.lock();
        let sessions = peers.entry(address.to_string()).or_default();
        let previous = sessions
            .iter()
            .position(|held| held.session == session)
            .map(|at| sessions.remove(at));
        sessions.push(link.clone());
        (link, rx, previous)
    }

    pub fn on_v4_peer_connected(
        &self,
        address: &str,
        session: &str,
        connection_id: &str,
    ) -> (PeerLink, SocketEvents) {
        self.on_v4_peer_connected_with_realm(address, session, connection_id, true)
    }

    pub fn on_v4_peer_connected_with_realm(
        &self,
        address: &str,
        session: &str,
        connection_id: &str,
        realm_owner: bool,
    ) -> (PeerLink, SocketEvents) {
        self.on_v4_peer_connected_with_realm_epoch(
            address,
            session,
            connection_id,
            realm_owner,
            self.next_id.load(Ordering::SeqCst).saturating_add(1),
        )
    }

    pub fn on_v4_peer_connected_with_realm_epoch(
        &self,
        address: &str,
        session: &str,
        connection_id: &str,
        realm_owner: bool,
        connection_epoch: u64,
    ) -> (PeerLink, SocketEvents) {
        let registry_session = v4_registry_session(session, connection_id);
        let (link, rx) = self.new_link(&registry_session);
        let mut peers = self.v4_peers.lock();
        let sessions = peers.entry(address.to_string()).or_default();
        // Registration can arrive after a newer authority claim completed. Only the highest
        // observed authenticated realm epoch for this wallet remains eligible for broker
        // reconciliation; demoted links stay alive for their independent non-realm lanes.
        let mut eligible_realm = realm_owner;
        if realm_owner {
            for peer in sessions.iter_mut().filter(|peer| peer.realm_owner) {
                if peer.connection_epoch < connection_epoch {
                    peer.realm_owner = false;
                } else {
                    eligible_realm = false;
                }
            }
        }
        sessions.push(V4Peer {
            raw_session: session.to_string(),
            connection_id: connection_id.to_string(),
            connection_epoch,
            realm_owner: eligible_realm,
            control_position_authorized: realm_owner,
            link: link.clone(),
        });
        (link, rx)
    }

    fn new_link(&self, session: &str) -> (PeerLink, SocketEvents) {
        let (tx, rx) = MailboxSender::channel();
        let link = PeerLink {
            session: session.to_string(),
            id: self.next_id.fetch_add(1, Ordering::SeqCst) + 1,
            assignment_sent: Arc::new(AtomicBool::new(false)),
            tx,
        };
        (link, rx)
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

    pub fn on_v4_peer_disconnected(&self, address: &str, id: u64) {
        let mut peers = self.v4_peers.lock();
        let Some(sessions) = peers.get_mut(address) else {
            return;
        };
        sessions.retain(|held| held.link.id != id);
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

    pub fn has_any_peer(&self, address: &str) -> bool {
        self.has_peer(address) || self.v4_peers.lock().contains_key(address)
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

    pub(crate) fn reconciliation_snapshot(&self, unassigned_only: bool) -> Vec<ReconciliationPeer> {
        let mut snapshot: Vec<_> = self
            .peers
            .lock()
            .iter()
            .flat_map(|(address, sessions)| {
                sessions
                    .iter()
                    .filter(|link| {
                        !link.is_closed() && (!unassigned_only || link.needs_assignment())
                    })
                    .map(|link| ReconciliationPeer {
                        address: address.clone(),
                        session: link.session.clone(),
                        id: link.id,
                        connection_id: None,
                    })
            })
            .collect();
        snapshot.extend(self.v4_peers.lock().iter().flat_map(|(address, peers)| {
            peers
                .iter()
                .filter(|peer| {
                    peer.realm_owner
                        && !peer.link.is_closed()
                        && (!unassigned_only || peer.link.needs_assignment())
                })
                .map(|peer| ReconciliationPeer {
                    address: address.clone(),
                    session: peer.raw_session.clone(),
                    id: peer.link.id,
                    connection_id: Some(peer.connection_id.clone()),
                })
        }));
        snapshot
    }

    /// True only while this exact legacy or realm-owning additive socket is still live.
    /// Feed dispatch continues to use `peer_link`, which deliberately only sees legacy sockets.
    pub(crate) fn reconciliation_peer_is_current(&self, target: &ReconciliationPeer) -> bool {
        match target.connection_id.as_deref() {
            None => self
                .peer_link(&target.address, &target.session)
                .is_some_and(|peer| peer.id == target.id && !peer.is_closed()),
            Some(connection_id) => self
                .v4_peers
                .lock()
                .get(&target.address)
                .is_some_and(|peers| {
                    peers.iter().any(|peer| {
                        peer.realm_owner
                            && peer.raw_session == target.session
                            && peer.connection_id == connection_id
                            && peer.link.id == target.id
                            && !peer.link.is_closed()
                    })
                }),
        }
    }

    pub(crate) fn control_position_permit(
        &self,
        address: &str,
        session: &str,
        epoch: u64,
        close: bool,
    ) -> Option<ControlPositionPermit> {
        let peers = self.v4_peers.lock();
        let peer = peers.get(address)?.iter().find(|peer| {
            (if close {
                peer.control_position_authorized
            } else {
                peer.realm_owner
            }) && peer.raw_session == session
                && peer.connection_epoch == epoch
                && (close || !peer.link.is_closed())
        })?;
        Some(ControlPositionPermit {
            address: address.to_owned(),
            session: session.to_owned(),
            epoch,
            link_id: peer.link.id,
            close,
        })
    }

    pub(crate) fn control_position_permit_is_current(
        &self,
        permit: &ControlPositionPermit,
    ) -> bool {
        !permit.close
            && self
                .v4_peers
                .lock()
                .get(&permit.address)
                .is_some_and(|peers| {
                    peers.iter().any(|peer| {
                        peer.realm_owner
                            && peer.raw_session == permit.session
                            && peer.connection_epoch == permit.epoch
                            && peer.link.id == permit.link_id
                            && !peer.link.is_closed()
                    })
                })
    }

    pub(crate) fn reconciliation_peer_needs_assignment(&self, target: &ReconciliationPeer) -> bool {
        match target.connection_id.as_deref() {
            None => self
                .peer_link(&target.address, &target.session)
                .is_some_and(|peer| peer.id == target.id && peer.needs_assignment()),
            Some(connection_id) => self
                .v4_peers
                .lock()
                .get(&target.address)
                .is_some_and(|peers| {
                    peers.iter().any(|peer| {
                        peer.realm_owner
                            && peer.raw_session == target.session
                            && peer.connection_id == connection_id
                            && peer.link.id == target.id
                            && peer.link.needs_assignment()
                    })
                }),
        }
    }

    /// Every wallet that holds a socket on this replica, on either listener. The ban sweep
    /// re-checks each one: a v4 socket that has sent no position is in no other list.
    pub fn wallets(&self) -> Vec<String> {
        let mut wallets: Vec<String> = self.peers.lock().keys().cloned().collect();
        wallets.extend(self.v4_peers.lock().keys().cloned());
        wallets.sort_unstable();
        wallets.dedup();
        wallets
    }

    /// Ends every session of a wallet on this replica, on either listener. Returns how many
    /// were reached.
    pub fn kick(&self, address: &str) -> usize {
        let legacy = self
            .sessions_of(address)
            .into_iter()
            .filter(|link| link.send(SocketEvent::Kicked))
            .count();
        let v4 = self
            .v4_peers
            .lock()
            .get(address)
            .into_iter()
            .flatten()
            .filter(|peer| peer.link.send(SocketEvent::Kicked))
            .count();
        legacy + v4
    }
}

fn v4_registry_session(session: &str, connection_id: &str) -> String {
    format!("v4:{session}:{connection_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_target(address: &str, session: &str, id: u64) -> ReconciliationPeer {
        ReconciliationPeer {
            address: address.into(),
            session: session.into(),
            id,
            connection_id: None,
        }
    }

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
    fn reconciliation_snapshot_excludes_closed_receivers_and_retains_socket_incarnations() {
        let registry = PeersRegistry::new();
        let (closed, _rx_a, _) = registry.on_peer_connected("0xa", "0xs1");
        let (_, rx_b, _) = registry.on_peer_connected("0xb", "0xs2");
        let (first, _rx_c, _) = registry.on_peer_connected("0xc", "0xs3");
        closed.close();
        drop(rx_b);
        assert_eq!(
            registry.reconciliation_snapshot(true),
            vec![legacy_target("0xc", "0xs3", first.id)]
        );
        let (second, _rx_d, _) = registry.on_peer_connected("0xc", "0xs3");
        assert_ne!(first.id, second.id);
        assert_eq!(
            registry.reconciliation_snapshot(true),
            vec![legacy_target("0xc", "0xs3", second.id)]
        );
        first.assignment_sent();
        assert!(
            second.needs_assignment(),
            "a stale socket cannot acknowledge its replacement"
        );
        second.assignment_sent();
        assert!(registry.reconciliation_snapshot(true).is_empty());
        assert_eq!(
            registry.reconciliation_snapshot(false),
            vec![legacy_target("0xc", "0xs3", second.id)],
            "a delivered assignment does not prove that later moves will arrive"
        );
    }

    #[test]
    fn reconciliation_includes_a_live_v4_realm_owner_by_raw_session() {
        let registry = PeersRegistry::new();
        let (link, _events) = registry.on_v4_peer_connected("0xa", "0xs1", "conn-1");

        assert_eq!(
            registry.reconciliation_snapshot(true),
            vec![ReconciliationPeer {
                address: "0xa".into(),
                session: "0xs1".into(),
                id: link.id,
                connection_id: Some("conn-1".into()),
            }]
        );
    }

    #[test]
    fn reconciliation_excludes_scene_only_and_stale_realm_socket_incarnations() {
        let registry = PeersRegistry::new();
        let (first, _events) =
            registry.on_v4_peer_connected_with_realm("0xa", "0xs1", "realm-old", true);
        let (_scene, _events) =
            registry.on_v4_peer_connected_with_realm("0xa", "0xs2", "scene-only", false);
        assert!(
            registry.peer_link("0xa", "0xs1").is_none(),
            "legacy feed assignment routing must not acquire additive sockets"
        );
        let old = registry.reconciliation_snapshot(true).pop().unwrap();
        assert_eq!(old.id, first.id);
        let (current, _events) =
            registry.on_v4_peer_connected_with_realm("0xa", "0xs1", "realm-new", true);

        assert!(
            !registry.reconciliation_peer_is_current(&old),
            "a queued predecessor must not announce after its realm claim was replaced"
        );
        assert_eq!(
            registry.reconciliation_snapshot(true),
            vec![ReconciliationPeer {
                address: "0xa".into(),
                session: "0xs1".into(),
                id: current.id,
                connection_id: Some("realm-new".into()),
            }]
        );
        registry.on_v4_peer_disconnected("0xa", current.id);
        assert!(registry.reconciliation_snapshot(true).is_empty());
    }

    #[test]
    fn a_demoted_realm_socket_keeps_only_its_close_permit_and_scene_only_gets_none() {
        let registry = PeersRegistry::new();
        let (_old, _events) =
            registry.on_v4_peer_connected_with_realm_epoch("0xa", "0xs-old", "realm-old", true, 7);
        let (_scene, _events) = registry.on_v4_peer_connected_with_realm_epoch(
            "0xa",
            "0xs-scene",
            "scene-only",
            false,
            8,
        );
        let (_current, _events) = registry.on_v4_peer_connected_with_realm_epoch(
            "0xa",
            "0xs-current",
            "realm-current",
            true,
            9,
        );

        assert!(registry
            .control_position_permit("0xa", "0xs-old", 7, false)
            .is_none());
        assert!(registry
            .control_position_permit("0xa", "0xs-old", 7, true)
            .is_some());
        assert!(registry
            .control_position_permit("0xa", "0xs-scene", 8, false)
            .is_none());
        assert!(registry
            .control_position_permit("0xa", "0xs-scene", 8, true)
            .is_none());
        assert!(registry
            .control_position_permit("0xa", "0xs-current", 9, false)
            .is_some());
    }

    #[test]
    fn a_delayed_lower_epoch_registration_cannot_displace_the_current_realm_owner() {
        let registry = PeersRegistry::new();
        let (current, _events) =
            registry.on_v4_peer_connected_with_realm_epoch("0xa", "0xs1", "realm-current", true, 2);
        let queued = registry.reconciliation_snapshot(true).pop().unwrap();
        let (_late, _events) =
            registry.on_v4_peer_connected_with_realm_epoch("0xa", "0xs1", "realm-late", true, 1);

        assert!(registry.reconciliation_peer_is_current(&queued));
        assert_eq!(
            registry.reconciliation_snapshot(true),
            vec![ReconciliationPeer {
                address: "0xa".into(),
                session: "0xs1".into(),
                id: current.id,
                connection_id: Some("realm-current".into()),
            }]
        );
    }

    #[test]
    fn a_newer_realm_epoch_from_another_signing_session_replaces_the_old_owner() {
        let registry = PeersRegistry::new();
        let (old, _events) =
            registry.on_v4_peer_connected_with_realm_epoch("0xa", "0xs-old", "realm-old", true, 1);
        let (current, _events) =
            registry.on_v4_peer_connected_with_realm_epoch("0xa", "0xs-new", "realm-new", true, 2);
        let (_late, _events) = registry.on_v4_peer_connected_with_realm_epoch(
            "0xa",
            "0xs-late",
            "realm-late",
            true,
            1,
        );
        let queued = registry.reconciliation_snapshot(true);

        assert!(
            !registry.reconciliation_peer_is_current(&ReconciliationPeer {
                address: "0xa".into(),
                session: "0xs-old".into(),
                id: old.id,
                connection_id: Some("realm-old".into()),
            })
        );
        assert_eq!(
            queued,
            vec![ReconciliationPeer {
                address: "0xa".into(),
                session: "0xs-new".into(),
                id: current.id,
                connection_id: Some("realm-new".into()),
            }]
        );
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

    #[test]
    fn a_kick_reaches_the_wallets_v4_sockets_too() {
        let registry = PeersRegistry::new();
        let (_legacy, mut rx_legacy, _) = registry.on_peer_connected("0xa", "0xs1");
        let (_first, mut rx_first) = registry.on_v4_peer_connected("0xa", "0xs2", "conn-1");
        let (_second, mut rx_second) = registry.on_v4_peer_connected("0xa", "0xs2", "conn-2");
        let (_other, mut rx_other) = registry.on_v4_peer_connected("0xb", "0xs3", "conn-3");
        assert_eq!(registry.kick("0xa"), 3);
        assert!(matches!(rx_legacy.try_recv(), Ok(SocketEvent::Kicked)));
        assert!(matches!(rx_first.try_recv(), Ok(SocketEvent::Kicked)));
        assert!(matches!(rx_second.try_recv(), Ok(SocketEvent::Kicked)));
        assert!(rx_other.try_recv().is_err());
    }

    #[test]
    fn the_wallets_of_both_listeners_are_named_once() {
        let registry = PeersRegistry::new();
        let (_legacy, _rx_legacy, _) = registry.on_peer_connected("0xa", "0xs1");
        let (_both, _rx_both) = registry.on_v4_peer_connected("0xa", "0xs2", "conn-1");
        let (v4_only, _rx_v4) = registry.on_v4_peer_connected("0xb", "0xs3", "conn-2");
        assert_eq!(
            registry.wallets(),
            vec!["0xa".to_string(), "0xb".to_string()]
        );
        registry.on_v4_peer_disconnected("0xb", v4_only.id);
        assert_eq!(registry.wallets(), vec!["0xa".to_string()]);
    }

    #[test]
    fn an_older_admission_cannot_replace_a_newer_commit_for_the_same_session() {
        let registry = PeersRegistry::new();
        let older = registry.begin_admission("0xa").unwrap();
        let newer = registry.begin_admission("0xa").unwrap();
        let (current, _events, previous) = newer.commit("0xs1").unwrap();
        assert!(previous.is_none());
        assert!(older.commit("0xs1").is_none());
        assert_eq!(
            registry.peer_link("0xa", "0xs1").map(|link| link.id),
            Some(current.id)
        );
        assert!(registry.admissions.lock().wallets.is_empty());
    }

    #[test]
    fn admission_order_does_not_merge_independent_sessions() {
        let registry = PeersRegistry::new();
        let older = registry.begin_admission("0xa").unwrap();
        let newer = registry.begin_admission("0xa").unwrap();
        assert!(newer.commit("0xs2").is_some());
        assert!(older.commit("0xs1").is_some());
        assert!(registry.peer_link("0xa", "0xs1").is_some());
        assert!(registry.peer_link("0xa", "0xs2").is_some());
    }

    #[test]
    fn cancelling_a_newer_unauthenticated_attempt_does_not_block_the_older_one() {
        let registry = PeersRegistry::new();
        let older = registry.begin_admission("0xa").unwrap();
        let newer = registry.begin_admission("0xa").unwrap();
        drop(newer);
        assert!(older.commit("0xs1").is_some());
        assert!(registry.admissions.lock().wallets.is_empty());
    }

    #[test]
    fn a_newer_commit_blocks_an_older_ticket_after_the_socket_disconnects() {
        let registry = PeersRegistry::new();
        let older = registry.begin_admission("0xa").unwrap();
        let newer = registry.begin_admission("0xa").unwrap();
        let (current, _events, _) = newer.commit("0xs1").unwrap();
        registry.on_peer_disconnected("0xa", "0xs1", current.id);
        assert!(older.commit("0xs1").is_none());
        assert!(registry.peer_link("0xa", "0xs1").is_none());
        assert!(registry.admissions.lock().wallets.is_empty());
    }
}
