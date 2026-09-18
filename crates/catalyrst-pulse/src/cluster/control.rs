use std::collections::HashMap;
#[cfg(any(feature = "nats", test))]
use std::time::{Duration, Instant};

#[cfg(any(feature = "nats", test))]
use catalyrst_types::control_position::ControlPosition;

use crate::decentraland::common::Vector3;

const SUPPORTED_TRANSPORT_PEERS: usize = 4095 + 4096;
const CONTROL_POSITION_CHURN_RESERVE: usize = SUPPORTED_TRANSPORT_PEERS;
pub const DEFAULT_CONTROL_POSITION_CAPACITY: usize =
    SUPPORTED_TRANSPORT_PEERS + CONTROL_POSITION_CHURN_RESERVE;
#[cfg(any(feature = "nats", test))]
pub const CONTROL_POSITION_TTL: Duration = Duration::from_secs(10);
#[cfg(any(feature = "nats", test))]
pub const CONTROL_TOMBSTONE_TTL: Duration = Duration::from_secs(300);
#[cfg(any(feature = "nats", test))]
const CONTROL_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub struct ControlMember {
    pub slot: u32,
    pub wallet: String,
    pub session: String,
    pub epoch: u64,
    pub realm: String,
    pub position: Vector3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlSnapshot {
    pub members: Vec<ControlMember>,
    pub owners: HashMap<String, ControlOwner>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlOwner {
    pub session: String,
    pub closed: bool,
}

#[cfg(any(feature = "nats", test))]
struct Entry {
    slot: u32,
    session: String,
    epoch: u64,
    sequence: u64,
    realm: String,
    position: Option<Vector3>,
    closed: bool,
    observed: Instant,
}

#[cfg(any(feature = "nats", test))]
pub(crate) struct ControlPositions {
    audience: Option<String>,
    live_ttl: Duration,
    tombstone_ttl: Duration,
    entries: HashMap<String, Entry>,
    free_slots: Vec<u32>,
    last_sweep: Instant,
}

#[cfg(any(feature = "nats", test))]
impl Default for ControlPositions {
    fn default() -> Self {
        Self::new(
            DEFAULT_CONTROL_POSITION_CAPACITY,
            CONTROL_POSITION_TTL,
            CONTROL_TOMBSTONE_TTL,
        )
    }
}

#[cfg(any(feature = "nats", test))]
impl ControlPositions {
    pub(crate) fn new(capacity: usize, live_ttl: Duration, tombstone_ttl: Duration) -> Self {
        let capacity = capacity.min(u32::MAX as usize);
        Self {
            audience: None,
            live_ttl,
            tombstone_ttl,
            entries: HashMap::new(),
            free_slots: (0..capacity as u32).rev().collect(),
            last_sweep: Instant::now(),
        }
    }

    pub(crate) fn enable(&mut self, audience: &str) -> Result<(), &'static str> {
        if audience.is_empty() || audience.len() > 256 {
            return Err("control position audience must be between 1 and 256 bytes");
        }
        match self.audience.as_deref() {
            Some(current) if current != audience => {
                Err("control position audience cannot change after enable")
            }
            Some(_) => Ok(()),
            None => {
                self.audience = Some(audience.to_string());
                Ok(())
            }
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.audience.is_some()
    }

    pub(crate) fn ingest(&mut self, update: ControlPosition) -> bool {
        self.ingest_at(update, Instant::now())
    }

    fn ingest_at(&mut self, update: ControlPosition, now: Instant) -> bool {
        if self.audience.as_deref() != Some(update.audience.as_str()) || update.encode().is_none() {
            return false;
        }
        let wallet = update.wallet.to_ascii_lowercase();
        if let Some(current) = self.entries.get_mut(&wallet) {
            expire_entry(current, now, self.live_ttl);
            if (update.epoch, update.sequence) <= (current.epoch, current.sequence) {
                return false;
            }
            current.session = update.session.to_ascii_lowercase();
            current.epoch = update.epoch;
            current.sequence = update.sequence;
            current.realm = update.realm;
            current.position = update.position.map(vector);
            current.closed = current.position.is_none();
            current.observed = now;
            return true;
        }

        self.sweep_if_due(now);
        if self.free_slots.is_empty() {
            self.expire_at(now);
        }
        let Some(slot) = self.allocate_slot() else {
            return false;
        };
        self.entries.insert(
            wallet,
            Entry {
                slot,
                session: update.session.to_ascii_lowercase(),
                epoch: update.epoch,
                sequence: update.sequence,
                realm: update.realm,
                position: update.position.map(vector),
                closed: update.position.is_none(),
                observed: now,
            },
        );
        true
    }

    pub(crate) fn snapshot(&mut self) -> ControlSnapshot {
        self.snapshot_at(Instant::now())
    }

    fn snapshot_at(&mut self, now: Instant) -> ControlSnapshot {
        self.expire_at(now);
        let mut snapshot = ControlSnapshot::default();
        snapshot.members.reserve(self.entries.len());
        snapshot.owners.reserve(self.entries.len());
        for (wallet, entry) in &self.entries {
            snapshot.owners.insert(
                wallet.clone(),
                ControlOwner {
                    session: entry.session.clone(),
                    closed: entry.closed,
                },
            );
            let Some(position) = entry.position else {
                continue;
            };
            snapshot.members.push(ControlMember {
                slot: entry.slot,
                wallet: wallet.clone(),
                session: entry.session.clone(),
                epoch: entry.epoch,
                realm: entry.realm.clone(),
                position,
            });
        }
        snapshot.members.sort_unstable_by_key(|member| member.slot);
        snapshot
    }

    fn expire_at(&mut self, now: Instant) {
        for entry in self.entries.values_mut() {
            expire_entry(entry, now, self.live_ttl);
        }
        let mut removed = Vec::new();
        self.entries.retain(|_, entry| {
            let retain = entry.position.is_some()
                || now.saturating_duration_since(entry.observed) < self.tombstone_ttl;
            if !retain {
                removed.push(entry.slot);
            }
            retain
        });
        self.free_slots.extend(removed);
        self.last_sweep = now;
    }

    fn sweep_if_due(&mut self, now: Instant) {
        if now.saturating_duration_since(self.last_sweep) >= CONTROL_SWEEP_INTERVAL {
            self.expire_at(now);
        }
    }

    fn allocate_slot(&mut self) -> Option<u32> {
        if let Some(slot) = self.free_slots.pop() {
            return Some(slot);
        }
        None
    }
}

#[cfg(any(feature = "nats", test))]
fn expire_entry(entry: &mut Entry, now: Instant, live_ttl: Duration) {
    if entry.position.is_some() && now.saturating_duration_since(entry.observed) >= live_ttl {
        entry.position = None;
        entry.closed = false;
        entry.observed = now;
    }
}

#[cfg(any(feature = "nats", test))]
fn vector(position: [f32; 3]) -> Vector3 {
    Vector3 {
        x: position[0],
        y: position[1],
        z: position[2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(
        wallet: u8,
        epoch: u64,
        sequence: u64,
        position: Option<[f32; 3]>,
    ) -> ControlPosition {
        ControlPosition {
            audience: "realm-a".into(),
            wallet: format!("0x{wallet:040x}"),
            session: format!("0x{:040x}", wallet + 100),
            epoch,
            sequence,
            realm: if position.is_some() {
                "main".into()
            } else {
                String::new()
            },
            position,
        }
    }

    #[test]
    fn ordering_audience_release_and_expiry_are_fail_closed() {
        let now = Instant::now();
        let mut positions =
            ControlPositions::new(2, Duration::from_secs(10), Duration::from_secs(30));
        positions.enable("realm-a").unwrap();
        let mut first = update(1, 7, 10, Some([1.0, 2.0, 3.0]));
        assert!(positions.ingest_at(first.clone(), now));
        assert!(!positions.ingest_at(first.clone(), now));
        first.sequence = 9;
        assert!(!positions.ingest_at(first.clone(), now));
        first.epoch = 6;
        first.sequence = 100;
        assert!(!positions.ingest_at(first, now));

        let mut wrong = update(2, 1, 1, Some([0.0; 3]));
        wrong.audience = "realm-b".into();
        assert!(!positions.ingest_at(wrong, now));

        assert_eq!(positions.snapshot_at(now).members.len(), 1);
        assert!(positions.ingest_at(update(1, 7, 11, None), now));
        let released = positions.snapshot_at(now);
        assert!(released.members.is_empty());
        assert_eq!(released.owners.len(), 1);
        assert!(released.owners.values().next().unwrap().closed);
        assert!(!positions.ingest_at(update(1, 7, 10, Some([4.0, 5.0, 6.0])), now));
        assert!(positions
            .snapshot_at(now + Duration::from_secs(31))
            .owners
            .is_empty());
    }

    #[test]
    fn capacity_retains_live_members_and_tombstone_high_water() {
        let now = Instant::now();
        let mut positions =
            ControlPositions::new(2, Duration::from_secs(10), Duration::from_secs(30));
        positions.enable("realm-a").unwrap();
        assert!(positions.ingest_at(update(1, 1, 1, Some([0.0; 3])), now));
        assert!(positions.ingest_at(update(2, 1, 2, Some([0.0; 3])), now));
        assert!(!positions.ingest_at(update(3, 1, 3, Some([0.0; 3])), now));
        assert_eq!(positions.snapshot_at(now).members.len(), 2);

        assert!(positions.ingest_at(update(1, 1, 4, None), now));
        assert!(!positions.ingest_at(update(3, 1, 5, Some([0.0; 3])), now));
        assert!(!positions.ingest_at(update(1, 1, 3, Some([0.0; 3])), now));
        let snapshot = positions.snapshot_at(now);
        assert_eq!(snapshot.members.len(), 1);
        assert!(snapshot.owners.contains_key(&format!("0x{:040x}", 1)));

        positions.snapshot_at(now + Duration::from_secs(31));
        assert!(positions.ingest_at(
            update(3, 1, 5, Some([0.0; 3])),
            now + Duration::from_secs(31)
        ));
    }

    #[test]
    fn ingress_sweeps_are_throttled_but_capacity_reclaims_expired_tombstones() {
        let now = Instant::now();
        let mut positions =
            ControlPositions::new(1, Duration::from_secs(1), Duration::from_secs(2));
        positions.enable("realm-a").unwrap();
        positions.last_sweep = now;
        assert!(positions.ingest_at(update(1, 1, 1, Some([0.0; 3])), now));
        assert!(positions.ingest_at(
            update(1, 1, 2, Some([1.0; 3])),
            now + Duration::from_millis(100)
        ));
        assert_eq!(positions.last_sweep, now);

        assert!(positions.ingest_at(update(1, 1, 3, None), now));
        assert!(positions.ingest_at(
            update(2, 1, 1, Some([0.0; 3])),
            now + Duration::from_secs(3)
        ));
    }

    #[test]
    fn default_capacity_covers_all_supported_transport_peers_and_equal_churn() {
        assert_eq!(SUPPORTED_TRANSPORT_PEERS, 8191);
        assert_eq!(
            DEFAULT_CONTROL_POSITION_CAPACITY,
            SUPPORTED_TRANSPORT_PEERS * 2
        );
    }

    #[test]
    fn a_missed_live_heartbeat_becomes_a_tombstone_before_it_is_forgotten() {
        let now = Instant::now();
        let mut positions =
            ControlPositions::new(1, Duration::from_secs(10), Duration::from_secs(30));
        positions.enable("realm-a").unwrap();
        assert!(positions.ingest_at(update(1, 1, 1, Some([0.0; 3])), now));
        let expired_live = positions.snapshot_at(now + Duration::from_secs(10));
        assert!(expired_live.members.is_empty());
        assert_eq!(expired_live.owners.len(), 1);
        assert!(!expired_live.owners.values().next().unwrap().closed);
        let forgotten = positions.snapshot_at(now + Duration::from_secs(40));
        assert!(forgotten.owners.is_empty());
    }
}
