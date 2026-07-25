use std::collections::{HashMap, HashSet};

use dashmap::DashMap;

#[derive(Default)]
struct PeerState {
    world: String,
    rooms: HashSet<String>,
    world_rooms: usize,
}

#[derive(Default)]
pub struct PeersRegistry {
    peers: DashMap<String, PeerState>,
}

impl PeersRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn peer_joined(&self, wallet: &str, world: &str, room: &str, is_scene_room: bool) {
        let world = world.to_lowercase();
        let mut entry = self.peers.entry(wallet.to_lowercase()).or_default();
        if !entry.world.is_empty() && entry.world != world {
            entry.rooms.clear();
            entry.world_rooms = 0;
        }
        entry.world = world;
        if entry.rooms.insert(room.to_string()) && !is_scene_room {
            entry.world_rooms += 1;
        }
    }

    pub fn peer_left(&self, wallet: &str, room: &str, is_scene_room: bool) {
        let wallet = wallet.to_lowercase();
        let mut drop_peer = false;
        if let Some(mut entry) = self.peers.get_mut(&wallet) {
            if entry.rooms.remove(room) && !is_scene_room {
                entry.world_rooms = entry.world_rooms.saturating_sub(1);
            }
            drop_peer = entry.rooms.is_empty();
        }
        if drop_peer {
            self.peers.remove(&wallet);
        }
    }

    pub fn get_peer_world(&self, wallet: &str) -> Option<String> {
        self.peers
            .get(&wallet.to_lowercase())
            .map(|e| e.world.clone())
    }

    pub fn world_counts(&self) -> Vec<(String, i64)> {
        let mut counts: HashMap<String, i64> = HashMap::new();
        for entry in self.peers.iter() {
            if entry.value().world_rooms > 0 {
                *counts.entry(entry.value().world.clone()).or_insert(0) += 1;
            }
        }
        let mut out: Vec<(String, i64)> = counts.into_iter().collect();
        out.sort();
        out
    }

    pub fn world_participant_count(&self, world: &str) -> i64 {
        let needle = world.to_lowercase();
        self.peers
            .iter()
            .filter(|e| e.value().world == needle && e.value().world_rooms > 0)
            .count() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_room_only_peer_excluded_from_world_counts() {
        let reg = PeersRegistry::new();
        reg.peer_joined("0xAAA", "foo.dcl.eth", "scene-foo.dcl.eth-s1", true);
        assert!(reg.world_counts().is_empty());
        assert_eq!(reg.world_participant_count("foo.dcl.eth"), 0);
        assert_eq!(reg.get_peer_world("0xaaa").as_deref(), Some("foo.dcl.eth"));
    }

    #[test]
    fn world_room_peer_counted_once_even_with_scene_room() {
        let reg = PeersRegistry::new();
        reg.peer_joined("0xAAA", "foo.dcl.eth", "world-foo.dcl.eth", false);
        reg.peer_joined("0xAAA", "foo.dcl.eth", "scene-foo.dcl.eth-s1", true);
        assert_eq!(reg.world_counts(), vec![("foo.dcl.eth".to_string(), 1)]);
        assert_eq!(reg.world_participant_count("foo.dcl.eth"), 1);

        reg.peer_left("0xAAA", "scene-foo.dcl.eth-s1", true);
        assert_eq!(reg.world_counts(), vec![("foo.dcl.eth".to_string(), 1)]);

        reg.peer_left("0xAAA", "world-foo.dcl.eth", false);
        assert!(reg.world_counts().is_empty());
        assert_eq!(reg.get_peer_world("0xaaa"), None);
    }

    #[test]
    fn distinct_world_room_peers_are_summed() {
        let reg = PeersRegistry::new();
        reg.peer_joined("0xAAA", "foo.dcl.eth", "world-foo.dcl.eth", false);
        reg.peer_joined("0xBBB", "foo.dcl.eth", "world-foo.dcl.eth", false);
        reg.peer_joined("0xCCC", "bar.dcl.eth", "world-bar.dcl.eth", false);
        let counts = reg.world_counts();
        assert_eq!(
            counts,
            vec![
                ("bar.dcl.eth".to_string(), 1),
                ("foo.dcl.eth".to_string(), 2)
            ]
        );
    }

    #[test]
    fn moving_worlds_clears_prior_rooms() {
        let reg = PeersRegistry::new();
        reg.peer_joined("0xAAA", "foo.dcl.eth", "world-foo.dcl.eth", false);
        reg.peer_joined("0xAAA", "bar.dcl.eth", "world-bar.dcl.eth", false);
        assert_eq!(reg.world_counts(), vec![("bar.dcl.eth".to_string(), 1)]);
        assert_eq!(reg.get_peer_world("0xaaa").as_deref(), Some("bar.dcl.eth"));
    }
}
