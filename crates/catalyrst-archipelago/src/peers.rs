use crate::ban::BanChecker;
use crate::config::ClusterConfig;
use crate::livekit::LivekitMinter;
use crate::registry::PeersRegistry;
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub type Address = String;
pub type IslandId = String;

const PARCEL_SIZE: f32 = 16.0;

const BAN_SWEEP_CONCURRENCY: usize = 20;

pub fn to_parcel(x: f32, z: f32) -> [i32; 2] {
    [
        (x / PARCEL_SIZE).floor() as i32,
        (z / PARCEL_SIZE).floor() as i32,
    ]
}

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

/// Who is connected to this replica and where they last said they were.
///
/// Positions come from the heartbeats the websocket receives, and `island_id` is recorded from the
/// assignment the cluster feed delivered, never computed here: Pulse owns cluster composition.
///
/// `assignments` is the second map because the feed answers the connect announcement milliseconds
/// after Welcome, long before the client's first heartbeat, and the feed is edge-triggered: a peer
/// standing still is never told its island again. Dropping that first assignment would leave the
/// wallet roomless for the whole session, so it is held here until a heartbeat creates the
/// directory entry that carries it.
pub struct PeerDirectory {
    peers: DashMap<Address, PeerState>,
    assignments: DashMap<Address, IslandId>,
    cfg: ClusterConfig,
    started_at: DateTime<Utc>,
    livekit: Arc<LivekitMinter>,
    ban_checker: Arc<BanChecker>,
    registry: Arc<PeersRegistry>,
}

impl PeerDirectory {
    pub fn new(
        cfg: ClusterConfig,
        livekit: Arc<LivekitMinter>,
        ban_checker: Arc<BanChecker>,
        registry: Arc<PeersRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            peers: DashMap::new(),
            assignments: DashMap::new(),
            cfg,
            started_at: Utc::now(),
            livekit,
            ban_checker,
            registry,
        })
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
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
        let island_id =
            prev_island.or_else(|| self.assignments.get(&address).map(|v| v.value().clone()));
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
            island_id,
        };
        self.peers.insert(address, state);
    }

    /// Records what the feed told this wallet to join. Always recorded, even for a wallet whose
    /// first heartbeat has not arrived: the assignment is what the LiveKit token and the ban
    /// sweep's participant removal are both read off, and the feed will not repeat it.
    pub fn set_island(&self, address: &str, island_id: &str) {
        self.assignments
            .insert(address.to_string(), island_id.to_string());
        if let Some(mut peer) = self.peers.get_mut(address) {
            peer.island_id = Some(island_id.to_string());
        }
    }

    /// Both maps, so the live socket set is what bounds them: ws teardown and a kick are the only
    /// ways a wallet leaves this replica outright.
    pub fn remove_peer(&self, address: &str) {
        self.peers.remove(address);
        self.assignments.remove(address);
    }

    /// Ends every session of a wallet on this replica and drops it from the directory. The
    /// LiveKit participant is removed too, so a kick reaches the media plane and not only the
    /// signalling socket.
    pub fn kick_peer(&self, address: &str, reason: &str) {
        let room = self.island_of(address);
        tracing::info!(addr = %address, reason, "kicking peer");
        self.registry.kick(address);
        self.remove_peer(address);
        if let (Some(room), true) = (room, self.livekit.is_armed()) {
            let lk = Arc::clone(&self.livekit);
            let addr = address.to_string();
            tokio::spawn(async move {
                lk.remove_participant(&room, &addr).await;
            });
        }
    }

    pub async fn ban_sweep_once(&self) {
        if !self.ban_checker.is_armed() {
            return;
        }
        let mut seen: HashSet<Address> = self.peers.iter().map(|e| e.key().clone()).collect();
        seen.extend(self.registry.wallets());
        let addrs: Vec<Address> = seen.into_iter().collect();
        if addrs.is_empty() {
            return;
        }
        let checker = Arc::clone(&self.ban_checker);
        let banned: Vec<Address> = futures::stream::iter(addrs)
            .map(|addr| {
                let checker = Arc::clone(&checker);
                async move { checker.is_banned_fresh(&addr).await.then_some(addr) }
            })
            .buffer_unordered(BAN_SWEEP_CONCURRENCY)
            .filter_map(|hit| async move { hit })
            .collect()
            .await;
        for addr in banned {
            tracing::info!(addr = %addr, "ban sweep: evicting banned peer from comms");
            self.kick_peer(&addr, "banned");
        }
    }

    pub fn spawn_ban_sweep(self: Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        if !self.ban_checker.is_armed() {
            return None;
        }
        let period = Duration::from_secs(self.cfg.ban_sweep_interval_secs.max(1));
        Some(spawn_periodic(
            "archipelago-ban-sweep",
            period,
            PeriodicCfg::default().after_first_period(),
            CancellationToken::new(),
            move || {
                let this = self.clone();
                async move {
                    this.ban_sweep_once().await;
                    Ok::<(), std::convert::Infallible>(())
                }
            },
        ))
    }

    /// Drops peers whose last heartbeat is older than the configured timeout. Used to be a
    /// side effect of the clustering pass; it survives because the stats surface must not keep
    /// serving a peer whose socket died without a close frame.
    ///
    /// The assignment outlives the directory entry while the socket is still registered: a client
    /// whose heartbeats stall has not left, and the feed is edge-triggered, so retiring its room
    /// here would 403 its next LiveKit token and hide it from the ban sweep's participant removal.
    pub fn expire_stale_once(&self) -> usize {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(self.cfg.heartbeat_timeout_secs as i64);
        let stale: Vec<Address> = self
            .peers
            .iter()
            .filter_map(|e| {
                (now.signed_duration_since(e.value().last_heartbeat) > timeout)
                    .then(|| e.key().clone())
            })
            .collect();
        for addr in &stale {
            self.peers.remove(addr);
            if !self.registry.has_peer(addr) {
                self.assignments.remove(addr);
            }
        }
        stale.len()
    }

    pub fn spawn_expiry(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let period = Duration::from_secs(self.cfg.peer_expiry_interval_secs.max(1));
        spawn_periodic(
            "archipelago-peer-expiry",
            period,
            PeriodicCfg::default().after_first_period(),
            CancellationToken::new(),
            move || {
                let this = self.clone();
                async move {
                    this.expire_stale_once();
                    Ok::<(), std::convert::Infallible>(())
                }
            },
        )
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

    pub fn island_of(&self, address: &str) -> Option<IslandId> {
        self.peers
            .get(address)
            .and_then(|p| p.island_id.clone())
            .or_else(|| self.assignments.get(address).map(|v| v.value().clone()))
    }

    pub fn peers_count(&self) -> usize {
        self.peers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LivekitConfig;

    fn directory(cfg: ClusterConfig) -> (Arc<PeerDirectory>, Arc<PeersRegistry>) {
        let registry = PeersRegistry::new();
        let directory = PeerDirectory::new(
            cfg,
            Arc::new(LivekitMinter::new(LivekitConfig::default())),
            BanChecker::new(None, reqwest::Client::new()),
            Arc::clone(&registry),
        );
        (directory, registry)
    }

    #[test]
    fn a_parcel_is_the_floor_of_the_position_over_the_parcel_size() {
        assert_eq!(to_parcel(0.0, 0.0), [0, 0]);
        assert_eq!(to_parcel(31.9, 16.0), [1, 1]);
        assert_eq!(to_parcel(-1.0, -16.0), [-1, -1]);
    }

    #[test]
    fn a_heartbeat_keeps_the_island_the_feed_recorded() {
        let (peers, _registry) = directory(ClusterConfig::default());
        peers.upsert_peer("0xa".into(), [1.0, 0.0, 1.0], [0, 0], "r".into());
        peers.set_island("0xa", "island-C1");
        peers.upsert_peer("0xa".into(), [2.0, 0.0, 2.0], [0, 0], "r".into());
        assert_eq!(peers.island_of("0xa").as_deref(), Some("island-C1"));
    }

    #[test]
    fn an_assignment_that_lands_before_the_first_heartbeat_is_kept_and_seeds_it() {
        let (peers, _registry) = directory(ClusterConfig::default());
        peers.set_island("0xa", "island-C1");
        assert!(peers.peer("0xa").is_none());
        assert_eq!(peers.island_of("0xa").as_deref(), Some("island-C1"));
        peers.upsert_peer("0xa".into(), [1.0, 0.0, 1.0], [0, 0], "r".into());
        assert_eq!(
            peers.peer("0xa").and_then(|p| p.island_id).as_deref(),
            Some("island-C1")
        );
    }

    #[test]
    fn a_removed_peer_takes_its_assignment_with_it() {
        let (peers, _registry) = directory(ClusterConfig::default());
        peers.set_island("0xa", "island-C1");
        peers.remove_peer("0xa");
        assert_eq!(peers.island_of("0xa"), None);
    }

    #[test]
    fn a_stale_peer_is_expired_and_a_fresh_one_is_kept() {
        let (peers, _registry) = directory(ClusterConfig {
            heartbeat_timeout_secs: 30,
            ..ClusterConfig::default()
        });
        peers.upsert_peer_at(
            "0xstale".into(),
            [0.0, 0.0, 0.0],
            [0, 0],
            "r".into(),
            Utc::now() - chrono::Duration::seconds(31),
        );
        peers.upsert_peer("0xfresh".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
        assert_eq!(peers.expire_stale_once(), 1);
        assert!(peers.peer("0xstale").is_none());
        assert!(peers.peer("0xfresh").is_some());
    }

    #[test]
    fn expiry_keeps_the_assignment_of_a_socket_that_is_still_registered() {
        let (peers, registry) = directory(ClusterConfig {
            heartbeat_timeout_secs: 30,
            ..ClusterConfig::default()
        });
        let (_link, _rx, _) = registry.on_peer_connected("0xa", "0xs1");
        peers.set_island("0xa", "island-C1");
        peers.upsert_peer_at(
            "0xa".into(),
            [0.0, 0.0, 0.0],
            [0, 0],
            "r".into(),
            Utc::now() - chrono::Duration::seconds(31),
        );
        assert_eq!(peers.expire_stale_once(), 1);
        assert!(peers.peer("0xa").is_none());
        assert_eq!(
            peers.island_of("0xa").as_deref(),
            Some("island-C1"),
            "the socket is still held, so the room the feed will not repeat survives"
        );
    }

    #[test]
    fn expiry_drops_the_assignment_once_no_socket_is_left() {
        let (peers, _registry) = directory(ClusterConfig {
            heartbeat_timeout_secs: 30,
            ..ClusterConfig::default()
        });
        peers.set_island("0xa", "island-C1");
        peers.upsert_peer_at(
            "0xa".into(),
            [0.0, 0.0, 0.0],
            [0, 0],
            "r".into(),
            Utc::now() - chrono::Duration::seconds(31),
        );
        assert_eq!(peers.expire_stale_once(), 1);
        assert_eq!(peers.island_of("0xa"), None);
    }

    #[tokio::test]
    async fn a_kick_ends_every_session_and_drops_the_peer() {
        let (peers, registry) = directory(ClusterConfig::default());
        let (_link, mut rx, _) = registry.on_peer_connected("0xa", "0xs1");
        peers.upsert_peer("0xa".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
        peers.kick_peer("0xa", "banned");
        assert!(matches!(
            rx.try_recv(),
            Ok(crate::registry::SocketEvent::Kicked)
        ));
        assert!(peers.peer("0xa").is_none());
    }
}
