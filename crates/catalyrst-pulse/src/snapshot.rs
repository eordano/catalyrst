use std::collections::HashMap;
use std::sync::Arc;

use crate::decentraland::common::Vector3;
use crate::decentraland::pulse::{EmoteStopReason, GlideState, PlayerAnimationFlags, PlayerState};
use crate::interest::{ParcelEncoder, SpatialGrid};
use crate::messages::spec;

pub const NO_SEQ: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct EmoteState {
    pub emote_id: Option<Arc<str>>,
    pub start_seq: u32,
    pub start_tick: u32,
    pub duration_ms: Option<u32>,

    pub stop_reason: Option<EmoteStopReason>,
}

/// Canonical per-peer state. Quantized fields hold the client's raw wire codes,
/// relayed verbatim to observers; `global_position` is the decoded world
/// position derived at publish time for AOI queries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeerSnapshot {
    pub seq: u32,
    pub server_tick: u32,

    pub parcel: i32,
    pub position_x: u32,
    pub position_y: u32,
    pub position_z: u32,
    pub global_position: Vector3,
    pub velocity_x: u32,
    pub velocity_y: u32,
    pub velocity_z: u32,
    pub rotation_y: u32,

    pub jump_count: i32,
    pub movement_blend: u32,
    pub slide_blend: u32,
    pub head_yaw: Option<u32>,
    pub head_pitch: Option<u32>,
    pub point_at_x: Option<u32>,
    pub point_at_y: Option<u32>,
    pub point_at_z: Option<u32>,
    pub animation_flags: i32,
    pub glide_state: i32,

    pub is_teleport: bool,

    pub emote: Option<EmoteState>,

    pub realm: Option<Arc<str>>,

    pub last_teleport_seq: u32,
}

impl PeerSnapshot {
    pub fn is_emoting(&self) -> bool {
        matches!(&self.emote, Some(e) if e.emote_id.is_some())
    }
}

struct PeerRing {
    ring: Vec<PeerSnapshot>,
    last_seq: u32,
    active: bool,
}

pub struct SnapshotBoard {
    ring_capacity: usize,
    peers: Vec<PeerRing>,
    active_ids: Vec<u32>,
}

impl SnapshotBoard {
    pub fn new(max_peers: usize, ring_capacity: usize) -> Self {
        let mut peers = Vec::with_capacity(max_peers);
        for _ in 0..max_peers {
            peers.push(PeerRing {
                ring: vec![PeerSnapshot::default(); ring_capacity],
                last_seq: NO_SEQ,
                active: false,
            });
        }
        Self {
            ring_capacity,
            peers,
            active_ids: Vec::new(),
        }
    }

    pub fn publish(&mut self, id: u32, snapshot: PeerSnapshot) {
        let index = id as usize;
        let emote = snapshot
            .emote
            .clone()
            .or_else(|| self.inherit_emote_state(index));
        let realm = snapshot.realm.clone().or_else(|| self.inherit_realm(index));
        let last_teleport_seq = if snapshot.is_teleport {
            snapshot.seq
        } else {
            self.inherit_last_teleport_seq(index)
        };

        let to_write = PeerSnapshot {
            emote,
            realm,
            last_teleport_seq,
            ..snapshot
        };

        let slot = (to_write.seq as usize) % self.ring_capacity;
        let p = &mut self.peers[index];
        p.last_seq = to_write.seq;
        p.ring[slot] = to_write;
    }

    fn inherit_emote_state(&self, index: usize) -> Option<EmoteState> {
        let p = &self.peers[index];
        if p.last_seq == NO_SEQ {
            return None;
        }
        let prev = &p.ring[(p.last_seq as usize) % self.ring_capacity].emote;
        match prev {
            Some(e) if e.stop_reason.is_some() => None,
            other => other.clone(),
        }
    }

    fn inherit_realm(&self, index: usize) -> Option<Arc<str>> {
        let p = &self.peers[index];
        if p.last_seq == NO_SEQ {
            return None;
        }
        p.ring[(p.last_seq as usize) % self.ring_capacity]
            .realm
            .clone()
    }

    fn inherit_last_teleport_seq(&self, index: usize) -> u32 {
        let p = &self.peers[index];
        if p.last_seq == NO_SEQ {
            return 0;
        }
        p.ring[(p.last_seq as usize) % self.ring_capacity].last_teleport_seq
    }

    pub fn last_seq(&self, id: u32) -> u32 {
        self.peers[id as usize].last_seq
    }

    pub fn try_read(&self, id: u32) -> Option<&PeerSnapshot> {
        let p = &self.peers[id as usize];
        if !p.active || p.last_seq == NO_SEQ {
            return None;
        }
        Some(&p.ring[(p.last_seq as usize) % self.ring_capacity])
    }

    pub fn try_read_seq(&self, id: u32, seq: u32) -> Option<&PeerSnapshot> {
        let p = &self.peers[id as usize];
        if !p.active || p.last_seq == NO_SEQ {
            return None;
        }
        let snap = &p.ring[(seq as usize) % self.ring_capacity];
        if snap.seq == seq {
            Some(snap)
        } else {
            None
        }
    }

    pub fn is_emoting(&self, id: u32) -> bool {
        self.try_read(id).map(|s| s.is_emoting()).unwrap_or(false)
    }

    pub fn set_active(&mut self, id: u32) {
        if !self.peers[id as usize].active {
            self.peers[id as usize].active = true;
            if let Err(pos) = self.active_ids.binary_search(&id) {
                self.active_ids.insert(pos, id);
            }
        }
    }

    pub fn clear_active(&mut self, id: u32) {
        let p = &mut self.peers[id as usize];
        p.active = false;
        p.last_seq = NO_SEQ;
        for slot in p.ring.iter_mut() {
            *slot = PeerSnapshot::default();
        }
        if let Ok(pos) = self.active_ids.binary_search(&id) {
            self.active_ids.remove(pos);
        }
    }

    pub fn active_peers(&self) -> &[u32] {
        &self.active_ids
    }
}

#[derive(Debug, Clone)]
pub struct EmoteInput {
    pub emote_id: String,
    pub duration_ms: Option<u32>,
    pub start_tick: Option<u32>,
}

fn decode_local_position(position_x: u32, position_y: u32, position_z: u32) -> Vector3 {
    Vector3 {
        x: spec::POSITION_X.decode(position_x),
        y: spec::POSITION_Y.decode(position_y),
        z: spec::POSITION_Z.decode(position_z),
    }
}

pub struct PeerSnapshotPublisher;

impl PeerSnapshotPublisher {
    pub fn publish_from_player_state(
        board: &mut SnapshotBoard,
        grid: &mut SpatialGrid,
        encoder: &ParcelEncoder,
        from: u32,
        now: u32,
        state: &PlayerState,
        emote: Option<EmoteInput>,
    ) -> PeerSnapshot {
        let seq = board.last_seq(from).wrapping_add(1);
        let local_position =
            decode_local_position(state.position_x, state.position_y, state.position_z);
        let global_position = encoder.decode_to_global_position(state.parcel_index, local_position);

        let emote_state = emote.map(|e| EmoteState {
            emote_id: Some(e.emote_id.into()),
            start_seq: seq,
            start_tick: e.start_tick.unwrap_or(now),
            duration_ms: e.duration_ms,
            stop_reason: None,
        });

        let pointing = state.state_flags & (PlayerAnimationFlags::PointingAt as u32) != 0;

        let snapshot = PeerSnapshot {
            seq,
            server_tick: now,
            parcel: state.parcel_index,
            position_x: state.position_x,
            position_y: state.position_y,
            position_z: state.position_z,
            global_position,
            velocity_x: state.velocity_x,
            velocity_y: state.velocity_y,
            velocity_z: state.velocity_z,
            rotation_y: state.rotation_y,
            jump_count: state.jump_count,
            movement_blend: state.movement_blend,
            slide_blend: state.slide_blend,
            head_yaw: state.head_yaw,
            head_pitch: state.head_pitch,
            point_at_x: state.point_at_x.filter(|_| pointing),
            point_at_y: state.point_at_y.filter(|_| pointing),
            point_at_z: state.point_at_z.filter(|_| pointing),
            animation_flags: state.state_flags as i32,
            glide_state: state.glide_state,
            is_teleport: false,
            emote: emote_state,
            realm: None,
            last_teleport_seq: 0,
        };

        board.publish(from, snapshot.clone());
        grid.set(from, global_position);
        snapshot
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish_teleport(
        board: &mut SnapshotBoard,
        grid: &mut SpatialGrid,
        encoder: &ParcelEncoder,
        from: u32,
        now: u32,
        parcel_index: i32,
        position_x: u32,
        position_y: u32,
        position_z: u32,
        realm: String,
    ) -> PeerSnapshot {
        let seq = board.last_seq(from).wrapping_add(1);
        let local_position = decode_local_position(position_x, position_y, position_z);
        let global_position = encoder.decode_to_global_position(parcel_index, local_position);

        let mut rotation_y = 0;
        let mut head_yaw = None;
        let mut head_pitch = None;
        let mut point_at_x = None;
        let mut point_at_y = None;
        let mut point_at_z = None;
        if let Some(prev) = board.try_read(from) {
            rotation_y = prev.rotation_y;
            head_yaw = prev.head_yaw;
            head_pitch = prev.head_pitch;
            point_at_x = prev.point_at_x;
            point_at_y = prev.point_at_y;
            point_at_z = prev.point_at_z;
        }

        let snapshot = PeerSnapshot {
            seq,
            server_tick: now,
            parcel: parcel_index,
            position_x,
            position_y,
            position_z,
            global_position,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
            rotation_y,
            jump_count: 0,
            movement_blend: 0,
            slide_blend: 0,
            head_yaw,
            head_pitch,
            point_at_x,
            point_at_y,
            point_at_z,
            animation_flags: PlayerAnimationFlags::Grounded as i32,
            glide_state: GlideState::PropClosed as i32,
            is_teleport: true,
            emote: None,
            realm: Some(realm.into()),
            last_teleport_seq: 0,
        };

        board.publish(from, snapshot.clone());
        grid.set(from, global_position);
        snapshot
    }
}

#[derive(Default)]
pub struct IdentityBoard {
    wallets_by_peer: Vec<Option<String>>,
    peers_by_wallet: HashMap<String, u32>,
}

impl IdentityBoard {
    pub fn new(max_peers: usize) -> Self {
        Self {
            wallets_by_peer: vec![None; max_peers],
            peers_by_wallet: HashMap::new(),
        }
    }

    pub fn set(&mut self, id: u32, wallet: String) {
        self.peers_by_wallet.insert(wallet.to_lowercase(), id);
        self.wallets_by_peer[id as usize] = Some(wallet);
    }

    pub fn wallet_by_peer(&self, id: u32) -> Option<&str> {
        self.wallets_by_peer[id as usize].as_deref()
    }

    pub fn peer_by_wallet(&self, wallet: &str) -> Option<u32> {
        self.peers_by_wallet.get(&wallet.to_lowercase()).copied()
    }

    pub fn remove(&mut self, id: u32) {
        if let Some(w) = self.wallets_by_peer[id as usize].take() {
            // Value-checked: after a duplicate-session eviction rebinds the wallet to the
            // replacement peer, a delayed cleanup of the evicted peer must not delete that live
            // forward mapping.
            let key = w.to_lowercase();
            if self.peers_by_wallet.get(&key) == Some(&id) {
                self.peers_by_wallet.remove(&key);
            }
        }
    }
}

#[derive(Default)]
pub struct ProfileBoard {
    versions: Vec<i32>,
}

impl ProfileBoard {
    pub fn new(max_peers: usize) -> Self {
        Self {
            versions: vec![0; max_peers],
        }
    }

    pub fn set(&mut self, id: u32, version: i32) {
        self.versions[id as usize] = version;
    }

    pub fn get(&self, id: u32) -> i32 {
        self.versions[id as usize]
    }

    pub fn remove(&mut self, id: u32) {
        self.versions[id as usize] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3(x: f32, y: f32, z: f32) -> Vector3 {
        Vector3 { x, y, z }
    }

    fn movement(seq_realm: Option<&str>) -> PeerSnapshot {
        PeerSnapshot {
            seq: 0,
            global_position: v3(1.0, 0.0, 1.0),
            realm: seq_realm.map(Arc::from),
            ..Default::default()
        }
    }

    #[test]
    fn last_seq_starts_at_sentinel_then_increments() {
        let mut board = SnapshotBoard::new(4, 8);
        board.set_active(0);
        assert_eq!(board.last_seq(0), NO_SEQ);
        board.publish(
            0,
            PeerSnapshot {
                seq: 0,
                ..movement(Some("r"))
            },
        );
        assert_eq!(board.last_seq(0), 0);
        board.publish(
            0,
            PeerSnapshot {
                seq: 1,
                ..movement(Some("r"))
            },
        );
        assert_eq!(board.last_seq(0), 1);
    }

    #[test]
    fn try_read_requires_active_slot() {
        let mut board = SnapshotBoard::new(4, 8);

        board.publish(
            0,
            PeerSnapshot {
                seq: 0,
                ..movement(Some("r"))
            },
        );
        assert!(board.try_read(0).is_none());
        board.set_active(0);
        assert!(board.try_read(0).is_some());
    }

    #[test]
    fn historical_read_by_seq_and_ring_wrap() {
        let mut board = SnapshotBoard::new(2, 4);
        board.set_active(0);
        for seq in 0..6u32 {
            board.publish(
                0,
                PeerSnapshot {
                    seq,
                    ..movement(Some("r"))
                },
            );
        }

        assert!(board.try_read_seq(0, 5).is_some());
        assert!(board.try_read_seq(0, 2).is_some());
        assert!(
            board.try_read_seq(0, 1).is_none(),
            "ring wrap evicts old seq"
        );
        assert!(board.try_read_seq(0, 0).is_none());
    }

    #[test]
    fn realm_carries_forward_after_teleport() {
        let mut board = SnapshotBoard::new(2, 8);
        board.set_active(0);

        board.publish(
            0,
            PeerSnapshot {
                seq: 0,
                is_teleport: true,
                realm: Some("realm-a".into()),
                ..Default::default()
            },
        );

        board.publish(
            0,
            PeerSnapshot {
                seq: 1,
                realm: None,
                ..Default::default()
            },
        );
        assert_eq!(board.try_read(0).unwrap().realm.as_deref(), Some("realm-a"));
    }

    #[test]
    fn last_teleport_seq_carries_forward() {
        let mut board = SnapshotBoard::new(2, 8);
        board.set_active(0);
        board.publish(
            0,
            PeerSnapshot {
                seq: 3,
                is_teleport: true,
                realm: Some("r".into()),
                ..Default::default()
            },
        );
        assert_eq!(board.try_read(0).unwrap().last_teleport_seq, 3);
        board.publish(
            0,
            PeerSnapshot {
                seq: 4,
                ..Default::default()
            },
        );

        assert_eq!(board.try_read(0).unwrap().last_teleport_seq, 3);
    }

    #[test]
    fn emote_carries_forward_then_stop_consumed() {
        let mut board = SnapshotBoard::new(2, 8);
        board.set_active(0);

        board.publish(
            0,
            PeerSnapshot {
                seq: 0,
                emote: Some(EmoteState {
                    emote_id: Some("wave".into()),
                    start_seq: 0,
                    start_tick: 100,
                    duration_ms: None,
                    stop_reason: None,
                }),
                realm: Some("r".into()),
                ..Default::default()
            },
        );

        board.publish(
            0,
            PeerSnapshot {
                seq: 1,
                ..Default::default()
            },
        );
        assert!(board.is_emoting(0));
        let carried = board.try_read(0).unwrap().emote.clone().unwrap();
        assert_eq!(carried.emote_id.as_deref(), Some("wave"));
        assert_eq!(
            carried.start_seq, 0,
            "carry-forward keeps original start_seq"
        );

        board.publish(
            0,
            PeerSnapshot {
                seq: 2,
                emote: Some(EmoteState {
                    emote_id: None,
                    start_seq: 0,
                    start_tick: 100,
                    duration_ms: None,
                    stop_reason: Some(EmoteStopReason::Cancelled),
                }),
                ..Default::default()
            },
        );

        board.publish(
            0,
            PeerSnapshot {
                seq: 3,
                ..Default::default()
            },
        );
        assert!(!board.is_emoting(0));
        assert!(board.try_read(0).unwrap().emote.is_none());
    }

    #[test]
    fn clear_active_resets_slot() {
        let mut board = SnapshotBoard::new(2, 8);
        board.set_active(0);
        board.publish(
            0,
            PeerSnapshot {
                seq: 0,
                ..movement(Some("r"))
            },
        );
        assert!(board.try_read(0).is_some());
        board.clear_active(0);
        assert!(board.try_read(0).is_none());
        assert_eq!(board.last_seq(0), NO_SEQ);
        assert!(!board.active_peers().contains(&0));
    }

    #[test]
    fn identity_board_roundtrip_case_insensitive() {
        let mut b = IdentityBoard::new(4);
        b.set(2, "0xABC".into());
        assert_eq!(b.wallet_by_peer(2), Some("0xABC"));
        assert_eq!(b.peer_by_wallet("0xabc"), Some(2));
        b.remove(2);
        assert_eq!(b.wallet_by_peer(2), None);
        assert_eq!(b.peer_by_wallet("0xabc"), None);
    }

    #[test]
    fn identity_board_remove_preserves_live_rebound_wallet() {
        let mut b = IdentityBoard::new(8);
        b.set(2, "0xW".into());
        b.set(5, "0xW".into()); // duplicate-session rebind to the replacement peer
        b.remove(2); // delayed cleanup of the evicted peer must not clobber the live binding
        assert_eq!(b.peer_by_wallet("0xw"), Some(5));
        assert_eq!(b.wallet_by_peer(2), None);
        b.remove(5);
        assert_eq!(b.peer_by_wallet("0xw"), None);
    }
}
