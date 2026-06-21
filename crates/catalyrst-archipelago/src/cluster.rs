use crate::config::ClusterConfig;
use crate::livekit::{LivekitGrant, LivekitMinter};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;

pub type Address = String;
pub type IslandId = String;

const PARCEL_SIZE: f32 = 16.0;

pub fn to_parcel(x: f32, z: f32) -> [i32; 2] {
    [
        (x / PARCEL_SIZE).floor() as i32,
        (z / PARCEL_SIZE).floor() as i32,
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position3D(pub f32, pub f32, pub f32);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parcel(pub i32, pub i32);

#[derive(Clone, Debug, Serialize)]
pub struct PeerState {
    pub address: Address,
    pub position: [f32; 3],
    pub parcel: [i32; 2],
    pub realm: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_heartbeat: DateTime<Utc>,
    pub island_id: Option<IslandId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Island {
    pub id: IslandId,
    pub center: [f32; 3],
    pub radius: f32,
    pub peers_count: usize,

    pub max_peers: usize,
    pub peers: Vec<Address>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClusterEvent {
    #[serde(rename = "island_changed")]
    IslandChanged {
        address: Address,
        island_id: IslandId,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_island_id: Option<IslandId>,
        peers: Vec<Address>,
        #[serde(skip_serializing_if = "Option::is_none")]
        livekit: Option<LivekitGrant>,
    },
    #[serde(rename = "peer_left")]
    PeerLeft { address: Address },
}

pub struct Cluster {
    peers: DashMap<Address, PeerState>,
    islands: RwLock<HashMap<IslandId, Island>>,
    cfg: ClusterConfig,
    tx: broadcast::Sender<ClusterEvent>,
    next_island: parking_lot::Mutex<u64>,
    started_at: DateTime<Utc>,
    livekit: Arc<LivekitMinter>,
    // Latest ws-connection generation per address. A closing socket may only
    // remove the peer it registered itself: an abandoned older socket's late
    // cleanup must NOT delete the entry a newer reconnect just created
    // (observed as a cascading reconnect failure on world->genesis returns).
    conn_gens: DashMap<Address, u64>,
    conn_counter: parking_lot::Mutex<u64>,
    last_kicked_recluster: parking_lot::Mutex<std::time::Instant>,
}

impl Cluster {
    pub fn new(cfg: ClusterConfig, livekit: Arc<LivekitMinter>) -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            peers: DashMap::new(),
            islands: RwLock::new(HashMap::new()),
            cfg,
            tx,
            next_island: parking_lot::Mutex::new(0),
            started_at: Utc::now(),
            livekit,
            conn_gens: DashMap::new(),
            conn_counter: parking_lot::Mutex::new(0),
            last_kicked_recluster: parking_lot::Mutex::new(
                std::time::Instant::now() - std::time::Duration::from_secs(60),
            ),
        })
    }

    /// Registers a freshly authenticated ws connection for `address` and
    /// returns its generation token. Pass it back to
    /// [`Cluster::remove_peer_if_conn`] on socket close.
    pub fn register_conn(&self, address: &str) -> u64 {
        let mut c = self.conn_counter.lock();
        *c += 1;
        let gen = *c;
        drop(c);
        self.conn_gens.insert(address.to_string(), gen);
        gen
    }

    /// Removes the peer only if `gen` is still the address' latest connection —
    /// a stale socket's cleanup leaves a newer reconnect's registration alone.
    pub fn remove_peer_if_conn(&self, address: &str, gen: u64) {
        let is_current = self
            .conn_gens
            .get(address)
            .map(|g| *g.value() == gen)
            .unwrap_or(false);
        if is_current {
            self.conn_gens.remove(address);
            self.remove_peer(address);
        }
    }

    /// Current island assignment for a peer (id + member list), if any.
    pub fn island_of(&self, address: &str) -> Option<(IslandId, Vec<Address>)> {
        let island_id = self.peers.get(address)?.island_id.clone()?;
        let islands = self.islands.read();
        let island = islands.get(&island_id)?;
        Some((island_id, island.peers.clone()))
    }

    /// Debounced on-demand recluster: a freshly heartbeating peer otherwise
    /// waits up to `recluster_interval_secs` for its first island. The
    /// resulting IslandChanged broadcast delivers the assignment.
    pub fn kick_recluster(&self) {
        {
            let mut last = self.last_kicked_recluster.lock();
            if last.elapsed() < std::time::Duration::from_millis(200) {
                return;
            }
            *last = std::time::Instant::now();
        }
        self.recluster_once();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.tx.subscribe()
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    pub fn livekit(&self) -> &LivekitMinter {
        &self.livekit
    }

    pub fn upsert_peer(
        &self,
        address: Address,
        position: [f32; 3],
        parcel: [i32; 2],
        realm: String,
    ) {
        self.upsert_peer_at(address, position, parcel, realm, Utc::now());
    }

    pub fn upsert_peer_at(
        &self,
        address: Address,
        position: [f32; 3],
        parcel: [i32; 2],
        realm: String,
        last_heartbeat: DateTime<Utc>,
    ) {
        let prev = self.peers.get(&address);
        let prev_island = prev.as_ref().and_then(|p| p.island_id.clone());
        let prev_ts = prev.as_ref().map(|p| p.last_heartbeat);
        drop(prev);
        let new_ts = match prev_ts {
            Some(t) if t > last_heartbeat => t,
            _ => last_heartbeat,
        };
        let state = PeerState {
            address: address.clone(),
            position,
            parcel,
            realm,
            last_heartbeat: new_ts,
            island_id: prev_island,
        };
        self.peers.insert(address, state);
    }

    pub fn remove_peer(&self, address: &str) {
        if let Some((_, state)) = self.peers.remove(address) {
            if let Some(island_id) = state.island_id.clone() {
                let mut islands = self.islands.write();
                if let Some(island) = islands.get_mut(&island_id) {
                    island.peers.retain(|a| a != address);
                    island.peers_count = island.peers.len();
                    if island.peers.is_empty() {
                        islands.remove(&island_id);
                    }
                }
            }
            let _ = self.tx.send(ClusterEvent::PeerLeft {
                address: address.to_string(),
            });
        }
    }

    pub fn peers_snapshot(&self) -> Vec<PeerState> {
        self.peers.iter().map(|e| e.value().clone()).collect()
    }

    pub fn peers_by_address(&self) -> HashMap<Address, PeerState> {
        self.peers
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    pub fn peer(&self, address: &str) -> Option<PeerState> {
        self.peers.get(address).map(|e| e.value().clone())
    }

    pub fn islands_snapshot(&self) -> Vec<Island> {
        self.islands.read().values().cloned().collect()
    }

    pub fn island(&self, id: &str) -> Option<Island> {
        self.islands.read().get(id).cloned()
    }

    pub fn peers_count(&self) -> usize {
        self.peers.len()
    }

    pub fn islands_count(&self) -> usize {
        self.islands.read().len()
    }

    fn next_island_id(&self) -> IslandId {
        let mut n = self.next_island.lock();
        *n += 1;
        format!("I{}", *n)
    }

    pub fn recluster_once(&self) {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(self.cfg.heartbeat_timeout_secs as i64);
        let stale: Vec<Address> = self
            .peers
            .iter()
            .filter_map(|e| {
                if now.signed_duration_since(e.value().last_heartbeat) > timeout {
                    Some(e.key().clone())
                } else {
                    None
                }
            })
            .collect();
        for addr in stale {
            self.remove_peer(&addr);
        }

        let mut all_peers: Vec<PeerState> = self.peers.iter().map(|e| e.value().clone()).collect();
        all_peers.sort_by(|a, b| a.address.cmp(&b.address));

        let radius = self.cfg.island_radius_parcels;
        let radius_sq = radius * radius;
        let max_peers = self.cfg.island_max_peers;

        let mut visited: HashSet<Address> = HashSet::new();
        let mut groups: Vec<Vec<PeerState>> = Vec::new();

        for seed in &all_peers {
            if visited.contains(&seed.address) {
                continue;
            }
            let mut group: Vec<PeerState> = Vec::new();
            let mut queue: Vec<PeerState> = vec![seed.clone()];
            visited.insert(seed.address.clone());
            while let Some(cur) = queue.pop() {
                group.push(cur.clone());
                if group.len() >= max_peers {
                    break;
                }
                for other in &all_peers {
                    if visited.contains(&other.address) {
                        continue;
                    }
                    let dx = (other.parcel[0] - cur.parcel[0]) as f32;
                    let dz = (other.parcel[1] - cur.parcel[1]) as f32;
                    if dx * dx + dz * dz <= radius_sq {
                        visited.insert(other.address.clone());
                        queue.push(other.clone());
                    }
                }
            }
            groups.push(group);
        }

        let mut new_islands: HashMap<IslandId, Island> = HashMap::new();
        let mut peer_assignment: HashMap<Address, IslandId> = HashMap::new();
        let prev_assignments: HashMap<Address, Option<IslandId>> = self
            .peers
            .iter()
            .map(|e| (e.key().clone(), e.value().island_id.clone()))
            .collect();

        let prev_islands = self.islands.read().clone();

        for group in groups {
            if group.is_empty() {
                continue;
            }
            let existing_id: Option<IslandId> = group
                .iter()
                .filter_map(|p| prev_assignments.get(&p.address).and_then(|x| x.clone()))
                .max_by_key(|id| prev_islands.get(id).map(|i| i.peers_count).unwrap_or(0));
            let island_id = existing_id.unwrap_or_else(|| self.next_island_id());

            let mut sum = [0f32; 3];
            for p in &group {
                sum[0] += p.position[0];
                sum[1] += p.position[1];
                sum[2] += p.position[2];
            }
            let n = group.len() as f32;
            let center = [sum[0] / n, sum[1] / n, sum[2] / n];
            let mut max_r2 = 0f32;
            for p in &group {
                let dx = p.position[0] - center[0];
                let dz = p.position[2] - center[2];
                let r2 = dx * dx + dz * dz;
                if r2 > max_r2 {
                    max_r2 = r2;
                }
            }
            let peers: Vec<Address> = group.iter().map(|p| p.address.clone()).collect();
            for a in &peers {
                peer_assignment.insert(a.clone(), island_id.clone());
            }
            new_islands.insert(
                island_id.clone(),
                Island {
                    id: island_id,
                    center,
                    radius: max_r2.sqrt(),
                    peers_count: peers.len(),
                    max_peers,
                    peers,
                },
            );
        }

        let mut changed: Vec<(Address, IslandId, Option<IslandId>, Vec<Address>)> = Vec::new();
        for (addr, new_id) in &peer_assignment {
            let prev = prev_assignments.get(addr).and_then(|x| x.clone());
            if prev.as_ref() != Some(new_id) {
                if let Some(island) = new_islands.get(new_id) {
                    changed.push((addr.clone(), new_id.clone(), prev, island.peers.clone()));
                }
            }
        }

        for mut e in self.peers.iter_mut() {
            let addr = e.key().clone();
            let p = e.value_mut();
            p.island_id = peer_assignment.get(&addr).cloned();
        }
        *self.islands.write() = new_islands;

        for (address, island_id, from_island_id, peers) in changed {
            tracing::info!(addr = %address, island = %island_id, from = ?from_island_id, members = peers.len(), "island assigned");
            let livekit = if self.livekit.is_armed() {
                Some(self.livekit.mint(&address, &island_id))
            } else {
                None
            };
            let _ = self.tx.send(ClusterEvent::IslandChanged {
                address,
                island_id,
                from_island_id,
                peers,
                livekit,
            });
        }
    }

    pub fn spawn_periodic(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval_secs = self.cfg.recluster_interval_secs.max(1);
        tokio::task::spawn(async move {
            let mut tick = interval(Duration::from_secs(interval_secs));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            tick.tick().await;
            loop {
                tick.tick().await;
                self.recluster_once();
            }
        })
    }
}
