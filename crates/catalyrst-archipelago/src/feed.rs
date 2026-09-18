use crate::proto::archipelago::{
    IslandChangedMessage, IslandStatusMessage, ServiceDiscoveryMessage,
};
use crate::registry::{AssignmentSend, MAX_ASSIGNMENT_BYTES};
use crate::state::AppState;
use parking_lot::RwLock;
use prost::Message as _;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Addressed to one device: the last token is the session key the assignment belongs to. Every
/// replica receives every event and the (address, session) lookup is the filter, so a socket held
/// for another session of the same wallet never sees it.
pub const ISLAND_CHANGED_SESSION_SUBJECT: &str = "engine.peer.*.island_changed.*";

/// The session-less subject an older cluster feed publishes on, delivered to the wallet's newest
/// socket on this replica.
pub const ISLAND_CHANGED_SUBJECT: &str = "engine.peer.*.island_changed";

pub const ISLANDS_SUBJECT: &str = "engine.islands";

pub const DISCOVERY_SUBJECT: &str = "engine.discovery";

pub const SUBSCRIPTIONS: [&str; 4] = [
    ISLAND_CHANGED_SESSION_SUBJECT,
    ISLAND_CHANGED_SUBJECT,
    ISLANDS_SUBJECT,
    DISCOVERY_SUBJECT,
];

/// Announces that a session now holds a live socket. The cluster feed is edge-triggered and says
/// nothing while a peer's cluster is unchanged, so without this a client that reconnects standing
/// still would never be told which island to join.
pub fn connect_subject(address: &str) -> String {
    format!("peer.{}.connect", address.to_ascii_lowercase())
}

pub fn disconnect_subject(address: &str) -> String {
    format!("peer.{}.disconnect", address.to_ascii_lowercase())
}

/// How long a service discovery heartbeat stands before the core reads as unhealthy. Absolute
/// delta on purpose: the timestamp is stamped on the publisher's host, so forward clock skew must
/// not read as fresh.
pub const DISCOVERY_FRESHNESS_MS: i64 = 90_000;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct IslandReport {
    pub id: String,
    pub peers: Vec<String>,
    pub max_peers: u32,
    pub center: [f32; 3],
    pub radius: f32,
}

/// Decodes a topology snapshot into the shape the stats endpoints serve. The publisher reports an
/// uncapped `max_peers` of 0 and its own id scheme: nothing here parses or compares either.
pub fn decode_islands_report(data: &[u8]) -> Result<Vec<IslandReport>, prost::DecodeError> {
    let message = IslandStatusMessage::decode(data)?;
    Ok(message
        .data
        .into_iter()
        .filter_map(|island| {
            let center = island.center?;
            Some(IslandReport {
                id: island.id,
                peers: island.peers,
                max_peers: island.max_peers,
                center: [center.x, center.y, center.z],
                radius: island.radius as f32,
            })
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    /// Accepted by the socket mailbox, not a WebSocket write or a client receipt.
    Delivered,
    /// Replaced a pending absolute assignment; arrival order is not a source revision.
    Coalesced,
    TooLarge,
    NoSessionSocket,
    NoSocket,
    Topology,
    Discovery,
    Undecodable,
    Ignored,
}

/// The latest topology and service-discovery snapshots the feed carried, and the counters that
/// tell a silent feed apart from one whose events reach no socket.
#[derive(Default)]
pub struct FeedCache {
    islands: RwLock<Vec<IslandReport>>,
    discovery: RwLock<Option<ServiceDiscoveryMessage>>,
    delivered: AtomicU64,
    no_session_socket: AtomicU64,
    deduplicated: AtomicU64,
    undecodable: AtomicU64,
    coalesced: AtomicU64,
    assignment_oversized: AtomicU64,
    socket_write_timeout: AtomicU64,
    socket_assignments_written: AtomicU64,
}

impl FeedCache {
    pub fn islands(&self) -> Vec<IslandReport> {
        self.islands.read().clone()
    }

    pub fn island(&self, id: &str) -> Option<IslandReport> {
        self.islands
            .read()
            .iter()
            .find(|island| island.id == id)
            .cloned()
    }

    pub fn islands_count(&self) -> usize {
        self.islands.read().len()
    }

    pub fn on_islands(&self, islands: Vec<IslandReport>) {
        *self.islands.write() = islands;
    }

    pub fn on_discovery(&self, message: ServiceDiscoveryMessage) {
        *self.discovery.write() = Some(message);
    }

    pub fn is_core_healthy(&self, now_ms: i64) -> bool {
        self.discovery
            .read()
            .as_ref()
            .and_then(|m| m.status.as_ref())
            .is_some_and(|status| {
                (now_ms - status.current_time as i64).abs() < DISCOVERY_FRESHNESS_MS
            })
    }

    pub fn core_user_count(&self) -> u32 {
        self.discovery
            .read()
            .as_ref()
            .and_then(|m| m.status.as_ref())
            .map(|status| status.user_count)
            .unwrap_or(0)
    }

    pub fn delivered_count(&self) -> u64 {
        self.delivered.load(Ordering::Relaxed)
    }

    pub fn no_session_socket_count(&self) -> u64 {
        self.no_session_socket.load(Ordering::Relaxed)
    }

    /// Counted by the socket layer, which owns the per-socket window: the feed delivered the
    /// message, the socket decided it had already forwarded that room.
    pub fn on_deduplicated(&self) {
        self.deduplicated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn deduplicated_count(&self) -> u64 {
        self.deduplicated.load(Ordering::Relaxed)
    }

    pub fn undecodable_count(&self) -> u64 {
        self.undecodable.load(Ordering::Relaxed)
    }

    pub fn coalesced_count(&self) -> u64 {
        self.coalesced.load(Ordering::Relaxed)
    }

    pub fn assignment_oversized_count(&self) -> u64 {
        self.assignment_oversized.load(Ordering::Relaxed)
    }

    pub fn socket_write_timeout_count(&self) -> u64 {
        self.socket_write_timeout.load(Ordering::Relaxed)
    }

    pub(crate) fn on_socket_write_timeout(&self) {
        self.socket_write_timeout.fetch_add(1, Ordering::Relaxed);
    }

    pub fn socket_assignments_written_count(&self) -> u64 {
        self.socket_assignments_written.load(Ordering::Relaxed)
    }

    pub(crate) fn on_socket_assignment_written(&self) {
        self.socket_assignments_written
            .fetch_add(1, Ordering::Relaxed);
    }
}

/// Routes one feed message. Never fails: the publishers are out of this process, so a malformed
/// payload costs its own message and nothing else.
pub fn dispatch(state: &AppState, subject: &str, data: &[u8]) -> Routed {
    let tokens: Vec<&str> = subject.split('.').collect();
    match tokens.as_slice() {
        ["engine", "peer", address, "island_changed", session] => {
            route_to_session(state, address, session, data)
        }
        ["engine", "peer", address, "island_changed"] => route_to_newest(state, address, data),
        ["engine", "islands"] => match decode_islands_report(data) {
            Ok(islands) => {
                state.feed.on_islands(islands);
                Routed::Topology
            }
            Err(e) => undecodable(state, subject, &e),
        },
        ["engine", "discovery"] => match ServiceDiscoveryMessage::decode(data) {
            Ok(message) => {
                state.feed.on_discovery(message);
                Routed::Discovery
            }
            Err(e) => undecodable(state, subject, &e),
        },
        _ => Routed::Ignored,
    }
}

fn undecodable(state: &AppState, subject: &str, error: &prost::DecodeError) -> Routed {
    state.feed.undecodable.fetch_add(1, Ordering::Relaxed);
    tracing::warn!(subject = %subject, error = %error, "cannot decode a feed message");
    Routed::Undecodable
}

fn route_to_session(state: &AppState, address: &str, session: &str, data: &[u8]) -> Routed {
    let address = address.to_ascii_lowercase();
    let session = session.to_ascii_lowercase();
    let Some(link) = state.registry.peer_link(&address, &session) else {
        // Only a replica that holds the wallet under another session can tell a stale session
        // apart from ordinary fan-out to the replicas that do not hold the wallet at all.
        if state.registry.has_peer(&address) {
            state.feed.no_session_socket.fetch_add(1, Ordering::Relaxed);
            return Routed::NoSessionSocket;
        }
        return Routed::NoSocket;
    };
    deliver(state, &address, &link, data)
}

fn route_to_newest(state: &AppState, address: &str, data: &[u8]) -> Routed {
    let address = address.to_ascii_lowercase();
    let Some(link) = state.registry.newest_link(&address) else {
        return Routed::NoSocket;
    };
    deliver(state, &address, &link, data)
}

fn deliver(
    state: &AppState,
    address: &str,
    link: &crate::registry::PeerLink,
    data: &[u8],
) -> Routed {
    if data.len() > MAX_ASSIGNMENT_BYTES {
        state
            .feed
            .assignment_oversized
            .fetch_add(1, Ordering::Relaxed);
        return Routed::TooLarge;
    }
    let message = match IslandChangedMessage::decode(data) {
        Ok(message) => message,
        Err(e) => {
            state.feed.undecodable.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(addr = %address, error = %e, "cannot decode an island assignment");
            return Routed::Undecodable;
        }
    };
    let outcome = match link.send_assignment(data) {
        AssignmentSend::Queued => Routed::Delivered,
        AssignmentSend::Coalesced => {
            state.feed.coalesced.fetch_add(1, Ordering::Relaxed);
            Routed::Coalesced
        }
        AssignmentSend::TooLarge => {
            state
                .feed
                .assignment_oversized
                .fetch_add(1, Ordering::Relaxed);
            return Routed::TooLarge;
        }
        AssignmentSend::Closed => return Routed::NoSocket,
    };
    state.peers.set_island(address, &message.island_id);
    state.feed.delivered.fetch_add(1, Ordering::Relaxed);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::archipelago::{IslandData, ServiceStatus};
    use crate::proto::Position;

    fn status_message(current_time: u64, user_count: u32) -> Vec<u8> {
        ServiceDiscoveryMessage {
            server_name: "pulse".into(),
            status: Some(ServiceStatus {
                current_time,
                commit_hash: Some("deadbeef".into()),
                user_count,
            }),
        }
        .encode_to_vec()
    }

    fn islands_message() -> Vec<u8> {
        IslandStatusMessage {
            data: vec![
                IslandData {
                    id: "island-C1".into(),
                    peers: vec!["0xa".into(), "0xb".into()],
                    max_peers: 0,
                    center: Some(Position {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                    }),
                    radius: 8.0,
                },
                IslandData {
                    id: "centerless".into(),
                    peers: vec![],
                    max_peers: 0,
                    center: None,
                    radius: 0.0,
                },
            ],
        }
        .encode_to_vec()
    }

    #[test]
    fn a_topology_snapshot_drops_an_island_without_a_center() {
        let report = decode_islands_report(&islands_message()).expect("decodes");
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].id, "island-C1");
        assert_eq!(report[0].peers, vec!["0xa".to_string(), "0xb".to_string()]);
        assert_eq!(report[0].max_peers, 0);
        assert_eq!(report[0].center, [1.0, 2.0, 3.0]);
        assert_eq!(report[0].radius, 8.0);
    }

    #[test]
    fn the_core_is_healthy_only_while_its_heartbeat_is_fresh() {
        let cache = FeedCache::default();
        assert!(!cache.is_core_healthy(1_000_000));
        assert_eq!(cache.core_user_count(), 0);

        let message = ServiceDiscoveryMessage::decode(status_message(1_000_000, 42).as_slice())
            .expect("decodes");
        cache.on_discovery(message);
        assert!(cache.is_core_healthy(1_000_000));
        assert!(cache.is_core_healthy(1_000_000 + DISCOVERY_FRESHNESS_MS - 1));
        assert!(!cache.is_core_healthy(1_000_000 + DISCOVERY_FRESHNESS_MS));
        assert!(
            !cache.is_core_healthy(1_000_000 - DISCOVERY_FRESHNESS_MS),
            "a heartbeat from the future is not fresh either"
        );
        assert_eq!(cache.core_user_count(), 42);
    }

    #[test]
    fn the_connect_subject_is_lower_cased() {
        assert_eq!(connect_subject("0xAB"), "peer.0xab.connect");
        assert_eq!(disconnect_subject("0xAB"), "peer.0xab.disconnect");
    }
}
