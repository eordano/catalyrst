//! The outbound cluster feed: the publisher seam the tracker hands assignments to, and the
//! coalescing outbox a broker-backed implementation drains.
//!
//! Publisher methods are fire-and-forget hand-offs: they enqueue and return without waiting
//! on a broker, so a stalled or absent one never slows a tracker pass, and none returns an
//! error the caller has to handle.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use prost::Message as _;

use crate::cluster::control::ControlSnapshot;
use crate::cluster::{ClusterPass, ClusterSession};
use crate::decentraland::common::Position;
use crate::decentraland::kernel::comms::v3::{
    IslandData, IslandStatusMessage, ServiceDiscoveryMessage, ServiceStatus,
};
use crate::decentraland::pulse::PeerClusterChange;
use crate::decentraland::pulse::PeerClusterSnapshot;

/// Named "island", not "cluster", on purpose: `engine.islands` is archipelago's topology subject
/// carrying archipelago's `IslandStatusMessage`, and Pulse republishes that shape verbatim. The
/// island vocabulary survives only where it is archipelago's contract.
pub const ISLANDS_SUBJECT: &str = "engine.islands";

pub const DISCOVERY_SUBJECT: &str = "engine.discovery";
pub const CLUSTER_LOOKUP_SUBJECT: &str = "peer.*.cluster_lookup";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentAssignment {
    pub wallet: String,
    pub session: String,
    pub cluster_id: String,
    pub realm: String,
}

struct AssignmentView {
    observed: tokio::time::Instant,
    valid_for: Duration,
    pass: u64,
    assignments: HashMap<String, CurrentAssignment>,
}

/// A replace-only view of live, post-debounce ownership. It is independent of the lossy event
/// outbox and of retained takeover history; absence in the next pass removes an old owner.
#[derive(Default)]
pub struct CurrentAssignments(Mutex<Option<AssignmentView>>);

impl CurrentAssignments {
    pub fn replace(&self, pass: u64, valid_for: Duration, assignments: Vec<CurrentAssignment>) {
        let assignments = assignments
            .into_iter()
            .map(|mut assignment| {
                assignment.wallet.make_ascii_lowercase();
                assignment.session.make_ascii_lowercase();
                (assignment.wallet.clone(), assignment)
            })
            .collect();
        let previous = self.0.lock().unwrap().replace(AssignmentView {
            observed: tokio::time::Instant::now(),
            valid_for,
            pass,
            assignments,
        });
        drop(previous);
    }

    pub fn lookup(&self, wallet: &str, session: &str) -> Option<PeerClusterSnapshot> {
        let current = self.0.lock().unwrap();
        let view = current.as_ref()?;
        if view.observed.elapsed() >= view.valid_for {
            return None;
        }
        let assignment = view.assignments.get(wallet)?;
        if assignment.session != session {
            return None;
        }
        Some(PeerClusterSnapshot {
            wallet: assignment.wallet.clone(),
            session: assignment.session.clone(),
            cluster_id: assignment.cluster_id.clone(),
            realm: assignment.realm.clone(),
            pass: view.pass,
            source_incarnation: String::new(),
        })
    }
}

/// `IslandData.max_peers` belongs to archipelago's shape and Pulse caps cluster size nowhere, so
/// every island goes out carrying zero rather than a bound this server does not enforce.
const NO_PEER_CAP: u32 = 0;

pub trait ClusterFeedPublisher: Send + Sync {
    fn control_snapshot(&self) -> ControlSnapshot {
        ControlSnapshot::default()
    }

    /// A peer's published (post-debounce) cluster assignment changed. `session` names the session
    /// that owns it and, on a takeover, the one it displaced.
    fn publish_cluster_change(
        &self,
        wallet: &str,
        cluster_id: &str,
        realm: &str,
        session: &ClusterSession,
    );

    /// The full cluster topology for a completed pass. `active_peers` is the snapshot board's
    /// active-peer count, not the clustered count: it is what the `engine.discovery` heartbeat
    /// advertises as service load, and a peer with no realm or no live wallet binding is carried
    /// by this server while belonging to no cluster.
    fn publish_topology(&self, pass: &ClusterPass, active_peers: u32);

    /// Replaces the queryable current view after a pass. This must not emit cluster-change events.
    fn replace_assignments(
        &self,
        pass: u64,
        valid_for: Duration,
        assignments: Vec<CurrentAssignment>,
    );
}

/// Stats-only mode: the tracker runs and reports metrics, nothing reaches a broker.
pub struct NoopClusterFeedPublisher;

impl ClusterFeedPublisher for NoopClusterFeedPublisher {
    fn publish_cluster_change(&self, _: &str, _: &str, _: &str, _: &ClusterSession) {}

    fn publish_topology(&self, _: &ClusterPass, _: u32) {}

    fn replace_assignments(&self, _: u64, _: Duration, _: Vec<CurrentAssignment>) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedChange {
    pub wallet: String,
    pub cluster_id: String,
    pub realm: String,
    pub session: ClusterSession,
}

/// In-process fake: records every hand-off with zero network I/O, so the tracker's publish
/// decisions can be asserted without a broker.
#[derive(Default)]
pub struct RecordingClusterFeed {
    changes: Mutex<Vec<RecordedChange>>,
    topologies: Mutex<Vec<Vec<(String, usize)>>>,
    active_peers: Mutex<Vec<u32>>,
    current: CurrentAssignments,
    control: Mutex<ControlSnapshot>,
}

impl RecordingClusterFeed {
    pub fn changes(&self) -> Vec<RecordedChange> {
        self.changes.lock().unwrap().clone()
    }

    pub fn topologies(&self) -> Vec<Vec<(String, usize)>> {
        self.topologies.lock().unwrap().clone()
    }

    pub fn active_peers(&self) -> Vec<u32> {
        self.active_peers.lock().unwrap().clone()
    }

    pub fn current(&self, wallet: &str, session: &str) -> Option<PeerClusterSnapshot> {
        self.current.lookup(wallet, session)
    }

    pub fn clear(&self) {
        self.changes.lock().unwrap().clear();
        self.topologies.lock().unwrap().clear();
        self.active_peers.lock().unwrap().clear();
    }

    pub fn set_control_snapshot(&self, snapshot: ControlSnapshot) {
        *self.control.lock().unwrap() = snapshot;
    }
}

impl ClusterFeedPublisher for RecordingClusterFeed {
    fn control_snapshot(&self) -> ControlSnapshot {
        self.control.lock().unwrap().clone()
    }
    fn replace_assignments(
        &self,
        pass: u64,
        valid_for: Duration,
        assignments: Vec<CurrentAssignment>,
    ) {
        self.current.replace(pass, valid_for, assignments);
    }

    fn publish_cluster_change(
        &self,
        wallet: &str,
        cluster_id: &str,
        realm: &str,
        session: &ClusterSession,
    ) {
        self.changes.lock().unwrap().push(RecordedChange {
            wallet: wallet.to_string(),
            cluster_id: cluster_id.to_string(),
            realm: realm.to_string(),
            session: session.clone(),
        });
    }

    fn publish_topology(&self, pass: &ClusterPass, active_peers: u32) {
        self.active_peers.lock().unwrap().push(active_peers);
        self.topologies.lock().unwrap().push(
            pass.clusters
                .iter()
                .map(|c| (c.id.clone(), c.count))
                .collect(),
        );
    }
}

/// Lower-cased once at the publish boundary, so one wallet always maps to one subject whatever
/// checksum casing the auth chain carried. The subject is also the coalescing key, so per-subject
/// latest-wins is exactly per-peer latest-wins.
pub fn cluster_change_subject(wallet: &str) -> String {
    format!("peer.{}.cluster_change", wallet.to_lowercase())
}

pub fn encode_cluster_change(cluster_id: &str, realm: &str, session: &ClusterSession) -> Vec<u8> {
    PeerClusterChange {
        cluster_id: cluster_id.to_string(),
        realm: realm.to_string(),
        session: session.session.clone(),
        displaced_session: session.displaced_session.clone().unwrap_or_default(),
        displaced_cluster_id: session.displaced_cluster_id.clone().unwrap_or_default(),
    }
    .encode_to_vec()
}

/// Projects a pass onto archipelago's `IslandStatusMessage`, the shape archipelago-core published.
pub fn encode_topology(pass: &ClusterPass) -> Vec<u8> {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    let mut data: Vec<IslandData> = Vec::with_capacity(pass.clusters.len());
    for cluster in &pass.clusters {
        by_id.insert(cluster.id.as_str(), data.len());
        data.push(IslandData {
            id: cluster.id.clone(),
            peers: Vec::new(),
            max_peers: NO_PEER_CAP,
            center: Some(Position {
                x: cluster.centroid.x,
                y: cluster.centroid.y,
                z: cluster.centroid.z,
            }),
            radius: cluster.radius as f64,
        });
    }
    for peer in &pass.peers {
        if let Some(&index) = by_id.get(peer.cluster_id.as_str()) {
            data[index].peers.push(peer.wallet.clone());
        }
    }
    IslandStatusMessage { data }.encode_to_vec()
}

pub fn encode_discovery(
    server_name: &str,
    commit_hash: &str,
    current_time_ms: u64,
    user_count: u32,
) -> Vec<u8> {
    ServiceDiscoveryMessage {
        server_name: server_name.to_string(),
        status: Some(ServiceStatus {
            current_time: current_time_ms,
            commit_hash: Some(commit_hash.to_string()),
            user_count,
        }),
    }
    .encode_to_vec()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueOutcome {
    /// Admitted as a new subject; nothing was lost.
    Admitted,
    /// Replaced an undelivered message for the same subject. Harmless -- the replacement carries
    /// strictly fresher state.
    Superseded,
    /// Admitted, and the longest-admitted pending subject was evicted to make room. The only path
    /// to genuine loss the outbox itself has.
    Dropped,
}

/// The outbox keeps the two feeds apart because they supersede differently. `engine.islands` is a
/// whole-world snapshot, so it lives in a single latest-wins slot and never competes with an
/// assignment. `peer.{addr}.cluster_change` supersedes per peer only, so changes are held one per
/// peer: a peer's newer assignment replaces its own older one, and one peer's event never
/// displaces another's -- which a shared FIFO with oldest-first eviction could not guarantee.
pub struct ClusterOutbox {
    capacity: usize,
    pending_change_by_subject: HashMap<String, Vec<u8>>,
    /// Admission order, not waiting order: a supersede leaves it untouched, so the evicted subject
    /// may be holding a message written a moment ago.
    change_order: VecDeque<String>,
    pending_topology: Option<Vec<u8>>,
}

impl ClusterOutbox {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pending_change_by_subject: HashMap::new(),
            change_order: VecDeque::new(),
            pending_topology: None,
        }
    }

    pub fn queue_change(&mut self, subject: String, payload: Vec<u8>) -> QueueOutcome {
        if let Some(pending) = self.pending_change_by_subject.get_mut(&subject) {
            *pending = payload;
            return QueueOutcome::Superseded;
        }

        let mut dropped = false;
        while self.pending_change_by_subject.len() >= self.capacity {
            let Some(evicted) = self.change_order.pop_front() else {
                break;
            };
            if self.pending_change_by_subject.remove(&evicted).is_some() {
                dropped = true;
            }
        }

        self.pending_change_by_subject
            .insert(subject.clone(), payload);
        self.change_order.push_back(subject);

        if dropped {
            QueueOutcome::Dropped
        } else {
            QueueOutcome::Admitted
        }
    }

    /// Latest wins: an undelivered snapshot is worthless once a newer one exists. Held apart from
    /// the changes, so it never counts against capacity.
    pub fn queue_topology(&mut self, payload: Vec<u8>) -> QueueOutcome {
        let superseded = self.pending_topology.is_some();
        self.pending_topology = Some(payload);
        if superseded {
            QueueOutcome::Superseded
        } else {
            QueueOutcome::Admitted
        }
    }

    /// A pending topology snapshot ahead of any pending change, then changes in admission order.
    /// That priority is the whole of it and not an ordering between the two feeds: a snapshot
    /// admitted after a change preempts it, and one superseded before the drain gets here is never
    /// handed out at all.
    pub fn try_dequeue_next(&mut self) -> Option<(String, Vec<u8>)> {
        if let Some(topology) = self.pending_topology.take() {
            return Some((ISLANDS_SUBJECT.to_string(), topology));
        }
        while let Some(subject) = self.change_order.pop_front() {
            if let Some(payload) = self.pending_change_by_subject.remove(&subject) {
                return Some((subject, payload));
            }
        }
        None
    }

    pub fn pending_changes(&self) -> usize {
        self.pending_change_by_subject.len()
    }

    pub fn has_topology(&self) -> bool {
        self.pending_topology.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(n: u8) -> Vec<u8> {
        vec![n]
    }

    #[test]
    fn a_fresher_change_for_one_peer_supersedes_its_own_older_one() {
        let mut outbox = ClusterOutbox::new(8);
        assert_eq!(
            outbox.queue_change("peer.a.cluster_change".into(), payload(1)),
            QueueOutcome::Admitted
        );
        assert_eq!(
            outbox.queue_change("peer.a.cluster_change".into(), payload(2)),
            QueueOutcome::Superseded
        );
        assert_eq!(outbox.pending_changes(), 1);
        assert_eq!(
            outbox.try_dequeue_next(),
            Some(("peer.a.cluster_change".to_string(), payload(2)))
        );
        assert_eq!(outbox.try_dequeue_next(), None);
    }

    #[test]
    fn past_capacity_the_longest_admitted_subject_is_evicted() {
        let mut outbox = ClusterOutbox::new(2);
        outbox.queue_change("peer.a.cluster_change".into(), payload(1));
        outbox.queue_change("peer.b.cluster_change".into(), payload(2));
        outbox.queue_change("peer.a.cluster_change".into(), payload(3));
        assert_eq!(
            outbox.queue_change("peer.c.cluster_change".into(), payload(4)),
            QueueOutcome::Dropped
        );
        let drained: Vec<String> = std::iter::from_fn(|| outbox.try_dequeue_next())
            .map(|(s, _)| s)
            .collect();
        assert_eq!(
            drained,
            vec![
                "peer.b.cluster_change".to_string(),
                "peer.c.cluster_change".to_string()
            ]
        );
    }

    #[test]
    fn the_topology_slot_is_latest_wins_and_never_counts_against_capacity() {
        let mut outbox = ClusterOutbox::new(1);
        assert_eq!(outbox.queue_topology(payload(1)), QueueOutcome::Admitted);
        assert_eq!(outbox.queue_topology(payload(2)), QueueOutcome::Superseded);
        assert_eq!(
            outbox.queue_change("peer.a.cluster_change".into(), payload(9)),
            QueueOutcome::Admitted
        );
        assert!(outbox.has_topology());
        assert_eq!(outbox.pending_changes(), 1);
    }

    #[test]
    fn a_pending_topology_dequeues_ahead_of_a_change_admitted_before_it() {
        let mut outbox = ClusterOutbox::new(8);
        outbox.queue_change("peer.a.cluster_change".into(), payload(1));
        outbox.queue_topology(payload(2));
        assert_eq!(
            outbox.try_dequeue_next(),
            Some((ISLANDS_SUBJECT.to_string(), payload(2)))
        );
        assert_eq!(
            outbox.try_dequeue_next(),
            Some(("peer.a.cluster_change".to_string(), payload(1)))
        );
    }

    #[test]
    fn the_subject_lower_cases_the_wallet_once() {
        assert_eq!(
            cluster_change_subject("0xAbCd"),
            "peer.0xabcd.cluster_change"
        );
    }

    #[test]
    fn an_absent_displaced_session_encodes_as_the_proto3_empty_string() {
        let bytes =
            encode_cluster_change("C1", "realm", &ClusterSession::new("0xsession".to_string()));
        let decoded = PeerClusterChange::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.cluster_id, "C1");
        assert_eq!(decoded.realm, "realm");
        assert_eq!(decoded.session, "0xsession");
        assert_eq!(decoded.displaced_session, "");
        assert_eq!(decoded.displaced_cluster_id, "");
    }

    #[test]
    fn a_takeover_names_the_displaced_session_and_cluster() {
        let session = ClusterSession {
            session: "0xnew".to_string(),
            displaced_session: Some("0xold".to_string()),
            displaced_cluster_id: Some("C7".to_string()),
        };
        let decoded =
            PeerClusterChange::decode(encode_cluster_change("C9", "realm", &session).as_slice())
                .unwrap();
        assert_eq!(decoded.session, "0xnew");
        assert_eq!(decoded.displaced_session, "0xold");
        assert_eq!(decoded.displaced_cluster_id, "C7");
    }
}

#[cfg(test)]
mod current_tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_current_view_expires_at_its_deadline_and_is_replace_only() {
        let current = CurrentAssignments::default();
        let assignment = CurrentAssignment {
            wallet: "0xAB".into(),
            session: "0xCD".into(),
            cluster_id: "C1".into(),
            realm: "realm".into(),
        };
        current.replace(7, Duration::from_secs(2), vec![assignment.clone()]);
        assert_eq!(current.lookup("0xab", "0xcd").unwrap().pass, 7);
        assert!(current.lookup("0xab", "0xce").is_none());
        tokio::time::advance(Duration::from_millis(1999)).await;
        assert!(current.lookup("0xab", "0xcd").is_some());
        tokio::time::advance(Duration::from_millis(1)).await;
        assert!(current.lookup("0xab", "0xcd").is_none());
        current.replace(8, Duration::from_secs(2), vec![assignment]);
        assert_eq!(current.lookup("0xab", "0xcd").unwrap().pass, 8);
        current.replace(9, Duration::from_secs(2), vec![]);
        assert!(current.lookup("0xab", "0xcd").is_none());
    }
}
