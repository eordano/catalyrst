use std::collections::HashMap;

use crate::decentraland::common::Vector3;
use crate::decentraland::pulse::{EmoteStopReason, GlideState, PlayerAnimationFlags, PlayerState};
use crate::interest::{ParcelEncoder, SpatialGrid};

pub const NO_SEQ: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct EmoteState {
    pub emote_id: Option<String>,
    pub start_seq: u32,
    pub start_tick: u32,
    pub duration_ms: Option<u32>,

    pub stop_reason: Option<EmoteStopReason>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeerSnapshot {
    pub seq: u32,
    pub server_tick: u32,

    pub parcel: i32,
    pub local_position: Vector3,
    pub global_position: Vector3,
    pub velocity: Vector3,
    pub rotation_y: f32,

    pub jump_count: i32,
    pub movement_blend: f32,
    pub slide_blend: f32,
    pub head_yaw: Option<f32>,
    pub head_pitch: Option<f32>,
    pub point_at: Option<Vector3>,
    pub animation_flags: i32,
    pub glide_state: i32,

    pub is_teleport: bool,

    pub emote: Option<EmoteState>,

    pub realm: Option<String>,

    pub last_teleport_seq: u32,
}

impl Default for PeerSnapshot {
    fn default() -> Self {
        Self {
            seq: 0,
            server_tick: 0,
            parcel: 0,
            local_position: Vector3::default(),
            global_position: Vector3::default(),
            velocity: Vector3::default(),
            rotation_y: 0.0,
            jump_count: 0,
            movement_blend: 0.0,
            slide_blend: 0.0,
            head_yaw: None,
            head_pitch: None,
            point_at: None,
            animation_flags: PlayerAnimationFlags::None as i32,
            glide_state: GlideState::PropClosed as i32,
            is_teleport: false,
            emote: None,
            realm: None,
            last_teleport_seq: 0,
        }
    }
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

    fn inherit_realm(&self, index: usize) -> Option<String> {
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
        self.peers[id as usize].active = true;
    }

    pub fn clear_active(&mut self, id: u32) {
        let p = &mut self.peers[id as usize];
        p.active = false;
        p.last_seq = NO_SEQ;
        for slot in p.ring.iter_mut() {
            *slot = PeerSnapshot::default();
        }
    }

    pub fn active_peers(&self) -> Vec<u32> {
        self.peers
            .iter()
            .enumerate()
            .filter(|(_, p)| p.active)
            .map(|(i, _)| i as u32)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct EmoteInput {
    pub emote_id: String,
    pub duration_ms: Option<u32>,
    pub start_tick: Option<u32>,
}

fn get_head_yaw(state: &PlayerState) -> Option<f32> {
    state.head_yaw
}

fn get_head_pitch(state: &PlayerState) -> Option<f32> {
    state.head_pitch
}

fn get_point_at(state: &PlayerState) -> Option<Vector3> {
    if state.state_flags & (PlayerAnimationFlags::PointingAt as u32) != 0 {
        state.point_at
    } else {
        None
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
        let local_position = state.position.unwrap_or_default();
        let global_position = encoder.decode_to_global_position(state.parcel_index, local_position);

        let emote_state = emote.map(|e| EmoteState {
            emote_id: Some(e.emote_id),
            start_seq: seq,
            start_tick: e.start_tick.unwrap_or(now),
            duration_ms: e.duration_ms,
            stop_reason: None,
        });

        let snapshot = PeerSnapshot {
            seq,
            server_tick: now,
            parcel: state.parcel_index,
            local_position,
            global_position,
            velocity: state.velocity.unwrap_or_default(),
            rotation_y: state.rotation_y,
            jump_count: state.jump_count,
            movement_blend: state.movement_blend,
            slide_blend: state.slide_blend,
            head_yaw: get_head_yaw(state),
            head_pitch: get_head_pitch(state),
            point_at: get_point_at(state),
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

    pub fn publish_teleport(
        board: &mut SnapshotBoard,
        grid: &mut SpatialGrid,
        encoder: &ParcelEncoder,
        from: u32,
        now: u32,
        parcel_index: i32,
        local_position: Vector3,
        realm: String,
    ) -> PeerSnapshot {
        let seq = board.last_seq(from).wrapping_add(1);
        let global_position = encoder.decode_to_global_position(parcel_index, local_position);

        let mut rotation_y = 0.0;
        let mut head_yaw = None;
        let mut head_pitch = None;
        let mut point_at = None;
        if let Some(prev) = board.try_read(from) {
            rotation_y = prev.rotation_y;
            head_yaw = prev.head_yaw;
            head_pitch = prev.head_pitch;
            point_at = prev.point_at;
        }

        let snapshot = PeerSnapshot {
            seq,
            server_tick: now,
            parcel: parcel_index,
            local_position,
            global_position,
            velocity: Vector3::default(),
            rotation_y,
            jump_count: 0,
            movement_blend: 0.0,
            slide_blend: 0.0,
            head_yaw,
            head_pitch,
            point_at,
            animation_flags: PlayerAnimationFlags::Grounded as i32,
            glide_state: GlideState::PropClosed as i32,
            is_teleport: true,
            emote: None,
            realm: Some(realm),
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
            self.peers_by_wallet.remove(&w.to_lowercase());
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
            realm: seq_realm.map(|s| s.to_string()),
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
}
