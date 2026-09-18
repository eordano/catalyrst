//! The two bounded per-wallet stores the cluster feed needs, kept apart on purpose.
//!
//! Peer state records this replica's last submitted assignment, not a client receipt or join. Its
//! only consumer is the `from_island_id` hint on the next publish. The
//! assignment mirror records what Pulse last published for a wallet regardless of which replica
//! minted it. It is diagnostic history, not recovery authority: reconnects query Pulse directly.
//!
//! Both are reclaimed by TTL alone: Pulse's feed carries no disconnect event, and a departed peer
//! simply stops producing cluster changes. Both are also capped, because a feed that only ever
//! learns new wallets would otherwise grow one permanent slot per address ever seen.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

pub const DEFAULT_PEER_STATE_MAX: usize = 20_000;
pub const DEFAULT_PEER_STATE_TTL_MS: u64 = 60 * 60 * 1000;
pub const DEFAULT_ASSIGNMENT_MIRROR_MAX: usize = 20_000;
pub const DEFAULT_ASSIGNMENT_MIRROR_TTL_MS: u64 = 60 * 60 * 1000;

/// What this replica last published to a wallet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAssignment {
    pub cluster_id: String,
    /// Stored rather than recomputed: `from_island_id` needs the exact previous name.
    pub room: String,
    /// Kept for legibility in logs; the TTL does the actual expiry.
    pub last_seen_ms: i64,
}

/// What Pulse last published for a wallet, on any replica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEntry {
    pub cluster_id: String,
    /// Empty when an older Pulse published no session.
    pub session: String,
}

struct Entries<V> {
    values: HashMap<String, (Instant, u64, V)>,
    recency: VecDeque<(u64, String)>,
    next_stamp: u64,
}

/// A bounded TTL-LRU keyed by wallet, matching the `LRUCache({max, ttl})` upstream keeps these two
/// stores in.
///
/// Both halves of the bound actually reclaim, which is why this is here rather than the workspace's
/// shared [`catalyrst_commons::cache::TtlMap`]: that map enforces its cap only on the read-through
/// path, and a store written by `insert` and read by `get_fresh` would never shed a slot. A read
/// refreshes recency but never the deadline, so the TTL is measured from the write, as upstream's
/// is.
///
/// Recency is a stamped queue superseded lazily, never scanned: at the documented default of
/// 20_000 entries a scan per access would cost more than the work it orders, and it would cost it
/// under the one lock every lane and the delivery task share.
struct BoundedTtlMap<V> {
    inner: Mutex<Entries<V>>,
    ttl: Duration,
    max: usize,
}

impl<V: Clone> BoundedTtlMap<V> {
    fn new(ttl: Duration, max: usize) -> Self {
        Self {
            inner: Mutex::new(Entries {
                values: HashMap::new(),
                recency: VecDeque::new(),
                next_stamp: 0,
            }),
            ttl,
            max: max.max(1),
        }
    }

    fn insert(&self, key: &str, value: V) {
        let mut entries = self.inner.lock().unwrap();
        let stamp = entries.take_stamp();
        entries
            .values
            .insert(key.to_string(), (Instant::now(), stamp, value));
        entries.recency.push_back((stamp, key.to_string()));
        self.reclaim(&mut entries);
    }

    fn get_fresh(&self, key: &str) -> Option<V> {
        let mut entries = self.inner.lock().unwrap();
        let fresh = match entries.values.get(key) {
            Some((at, _, value)) => (at.elapsed() < self.ttl).then(|| value.clone()),
            None => return None,
        };
        match fresh {
            Some(value) => {
                let stamp = entries.take_stamp();
                if let Some((_, current, _)) = entries.values.get_mut(key) {
                    *current = stamp;
                }
                entries.recency.push_back((stamp, key.to_string()));
                self.reclaim(&mut entries);
                Some(value)
            }
            None => {
                entries.values.remove(key);
                self.reclaim(&mut entries);
                None
            }
        }
    }

    fn len(&self) -> usize {
        let mut entries = self.inner.lock().unwrap();
        entries
            .values
            .retain(|_, (at, _, _)| at.elapsed() < self.ttl);
        self.reclaim(&mut entries);
        entries.values.len()
    }

    #[cfg(test)]
    fn recency_len(&self) -> usize {
        self.inner.lock().unwrap().recency.len()
    }

    /// Sheds expired entries from the least-recently-used end, then whatever is still over the cap.
    ///
    /// A queued stamp that no longer matches the value's is a superseded duplicate: it is dropped
    /// without touching the value, which is what keeps insert and read off any scan. The slack
    /// those duplicates leave is bounded by one compaction, so a read-heavy wallet set cannot grow
    /// the queue without bound.
    fn reclaim(&self, entries: &mut Entries<V>) {
        while let Some(front) = entries.front_state(self.ttl) {
            match front {
                Front::Fresh => break,
                Front::Expired => {
                    let (_, key) = entries.recency.pop_front().expect("front");
                    entries.values.remove(&key);
                }
                Front::Superseded => {
                    entries.recency.pop_front();
                }
            }
        }
        while entries.values.len() > self.max {
            let Some((stamp, key)) = entries.recency.pop_front() else {
                break;
            };
            if entries
                .values
                .get(&key)
                .is_some_and(|(_, current, _)| *current == stamp)
            {
                entries.values.remove(&key);
            }
        }
        if entries.recency.len() > 4 * self.max {
            let Entries {
                values, recency, ..
            } = entries;
            recency.retain(|(stamp, key)| {
                values
                    .get(key)
                    .is_some_and(|(_, current, _)| current == stamp)
            });
        }
    }
}

enum Front {
    Fresh,
    Expired,
    Superseded,
}

impl<V> Entries<V> {
    fn take_stamp(&mut self) -> u64 {
        let stamp = self.next_stamp;
        self.next_stamp += 1;
        stamp
    }

    fn front_state(&self, ttl: Duration) -> Option<Front> {
        let (stamp, key) = self.recency.front()?;
        match self.values.get(key) {
            Some((at, current, _)) if current == stamp => Some(if at.elapsed() >= ttl {
                Front::Expired
            } else {
                Front::Fresh
            }),
            _ => Some(Front::Superseded),
        }
    }
}

pub struct ClusterPeerState {
    assignments: BoundedTtlMap<PeerAssignment>,
    mirror: BoundedTtlMap<MirrorEntry>,
}

impl ClusterPeerState {
    pub fn new(
        peer_state_max: usize,
        peer_state_ttl_ms: u64,
        mirror_max: usize,
        mirror_ttl_ms: u64,
    ) -> Self {
        Self {
            assignments: BoundedTtlMap::new(
                Duration::from_millis(peer_state_ttl_ms),
                peer_state_max,
            ),
            mirror: BoundedTtlMap::new(Duration::from_millis(mirror_ttl_ms), mirror_max),
        }
    }

    pub fn assignment(&self, wallet: &str) -> Option<PeerAssignment> {
        self.assignments.get_fresh(wallet)
    }

    pub fn record_assignment(&self, wallet: &str, assignment: PeerAssignment) {
        self.assignments.insert(wallet, assignment);
    }

    pub fn mirrored(&self, wallet: &str) -> Option<MirrorEntry> {
        self.mirror.get_fresh(wallet)
    }

    pub fn record_mirror(&self, wallet: &str, entry: MirrorEntry) {
        self.mirror.insert(wallet, entry);
    }

    pub fn tracked_assignments(&self) -> usize {
        self.assignments.len()
    }

    pub fn tracked_mirror_entries(&self) -> usize {
        self.mirror.len()
    }

    #[cfg(test)]
    fn assignment_recency_len(&self) -> usize {
        self.assignments.recency_len()
    }
}

impl Default for ClusterPeerState {
    fn default() -> Self {
        Self::new(
            DEFAULT_PEER_STATE_MAX,
            DEFAULT_PEER_STATE_TTL_MS,
            DEFAULT_ASSIGNMENT_MIRROR_MAX,
            DEFAULT_ASSIGNMENT_MIRROR_TTL_MS,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(cluster: &str) -> PeerAssignment {
        PeerAssignment {
            cluster_id: cluster.to_string(),
            room: format!("island-{cluster}"),
            last_seen_ms: 0,
        }
    }

    #[test]
    fn the_two_stores_never_answer_for_each_other() {
        let state = ClusterPeerState::default();
        state.record_assignment("0xa", assignment("c1"));
        state.record_mirror(
            "0xb",
            MirrorEntry {
                cluster_id: "c2".into(),
                session: "0xs".into(),
            },
        );

        assert_eq!(state.assignment("0xa").unwrap().room, "island-c1");
        assert!(state.mirrored("0xa").is_none());
        assert!(state.assignment("0xb").is_none());
        assert_eq!(state.mirrored("0xb").unwrap().cluster_id, "c2");
    }

    #[test]
    fn a_new_assignment_replaces_the_previous_one() {
        let state = ClusterPeerState::default();
        state.record_assignment("0xa", assignment("c1"));
        state.record_assignment("0xa", assignment("c2"));
        assert_eq!(state.assignment("0xa").unwrap().cluster_id, "c2");
        assert_eq!(state.tracked_assignments(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_entry_older_than_its_ttl_is_gone() {
        let state = ClusterPeerState::new(16, 50, 16, 50);
        state.record_assignment("0xa", assignment("c1"));
        assert!(state.assignment("0xa").is_some());
        tokio::time::advance(Duration::from_millis(80)).await;
        assert!(
            state.assignment("0xa").is_none(),
            "TTL is the only reclamation path the feed has"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_reclaims_the_slot_rather_than_only_refusing_the_value() {
        let state = ClusterPeerState::new(16, 50, 16, 50);
        for index in 0..4 {
            state.record_assignment(&format!("0x{index}"), assignment("c1"));
            state.record_mirror(
                &format!("0x{index}"),
                MirrorEntry {
                    cluster_id: "c1".into(),
                    session: String::new(),
                },
            );
        }
        assert_eq!(state.tracked_assignments(), 4);

        tokio::time::advance(Duration::from_millis(80)).await;

        assert_eq!(
            state.tracked_assignments(),
            0,
            "an expired entry is reclaimed, not merely refused on read"
        );
        assert_eq!(state.tracked_mirror_entries(), 0);
    }

    #[test]
    fn the_cap_evicts_the_least_recently_used_wallet() {
        let state = ClusterPeerState::new(3, DEFAULT_PEER_STATE_TTL_MS, 3, 60_000);
        state.record_assignment("0xa", assignment("c1"));
        state.record_assignment("0xb", assignment("c1"));
        state.record_assignment("0xc", assignment("c1"));
        assert!(state.assignment("0xa").is_some());

        state.record_assignment("0xd", assignment("c1"));

        assert_eq!(state.tracked_assignments(), 3);
        assert!(
            state.assignment("0xb").is_none(),
            "the least recently used wallet is the one that goes"
        );
        assert!(state.assignment("0xa").is_some());
        assert!(state.assignment("0xc").is_some());
        assert!(state.assignment("0xd").is_some());
    }

    #[test]
    fn repeated_reads_keep_every_entry_and_a_bounded_recency_queue() {
        let max = 512;
        let state = ClusterPeerState::new(
            max,
            DEFAULT_PEER_STATE_TTL_MS,
            max,
            DEFAULT_ASSIGNMENT_MIRROR_TTL_MS,
        );
        for index in 0..max {
            state.record_assignment(&format!("0x{index}"), assignment("c1"));
        }
        for _ in 0..8 {
            for index in 0..max {
                assert!(state.assignment(&format!("0x{index}")).is_some());
            }
        }

        let queued = state.assignment_recency_len();
        assert!(
            queued <= 4 * max,
            "superseded recency stamps must be bounded on the read path too, not only when a write or a size query reclaims: {queued}"
        );
        assert_eq!(state.tracked_assignments(), max);
    }

    #[test]
    fn a_read_of_a_missing_wallet_records_nothing() {
        let state = ClusterPeerState::new(8, DEFAULT_PEER_STATE_TTL_MS, 8, 60_000);
        assert!(state.assignment("0xabsent").is_none());
        assert_eq!(state.tracked_assignments(), 0);
        assert_eq!(state.assignment_recency_len(), 0);
    }
    #[test]
    fn size_excludes_expired_entries_behind_a_fresh_lru_front() {
        let map = BoundedTtlMap::new(Duration::from_secs(30), 2);
        map.insert("old", 1);
        map.insert("fresh", 2);
        assert_eq!(map.get_fresh("old"), Some(1));
        map.inner.lock().unwrap().values.get_mut("old").unwrap().0 =
            Instant::now() - Duration::from_secs(31);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get_fresh("fresh"), Some(2));
    }

    #[test]
    fn compaction_removes_hot_key_stamps_behind_an_untouched_front() {
        let map = BoundedTtlMap::new(Duration::from_secs(30), 2);
        map.insert("anchor", 1);
        map.insert("hot", 2);
        for _ in 0..8 {
            assert_eq!(map.get_fresh("hot"), Some(2));
        }
        assert!(map.recency_len() <= 4);
        assert_eq!(map.len(), 2);
    }
}
