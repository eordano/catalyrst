//! Spatial interest management — a port of `InterestManagement/` (`ParcelEncoder`,
//! `SpatialGrid`, `SpatialAreaOfInterest`, `IInterestCollector`) plus
//! `Peers/Diff/PeerViewSimulationTier`.
//!
//! An observer sees only subjects in the same realm and within [`SpatialAreaOfInterest`]'s
//! max radius, each tagged with a [`PeerViewSimulationTier`] (0/1/2 by distance) that
//! governs how often and in how much detail it is updated. This is what makes peers receive
//! only in-interest state rather than all state unconditionally.

use crate::decentraland::common::Vector3;
use crate::snapshot::SnapshotBoard;

/// How detailed the data sent about a subject to an observer is
/// (`Peers/Diff/PeerViewSimulationTier`). Lower = closer = more frequent + more fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerViewSimulationTier(pub u8);

impl PeerViewSimulationTier {
    pub const TIER_0: PeerViewSimulationTier = PeerViewSimulationTier(0);
    pub const TIER_1: PeerViewSimulationTier = PeerViewSimulationTier(1);
    pub const TIER_2: PeerViewSimulationTier = PeerViewSimulationTier(2);

    pub fn value(&self) -> u8 {
        self.0
    }
}

/// Options for the parcel/global coordinate mapping (`ParcelEncoderOptions`). Defaults match
/// upstream `appsettings`-less defaults (Genesis City + 2 parcel padding, 16m parcels).
#[derive(Debug, Clone)]
pub struct ParcelEncoderOptions {
    pub min_parcel_x: i32,
    pub min_parcel_z: i32,
    pub max_parcel_x: i32,
    pub max_parcel_z: i32,
    pub padding: i32,
    pub parcel_size: i32,
}

impl Default for ParcelEncoderOptions {
    fn default() -> Self {
        Self {
            min_parcel_x: -150,
            min_parcel_z: -150,
            max_parcel_x: 163,
            max_parcel_z: 158,
            padding: 2,
            parcel_size: 16,
        }
    }
}

/// Maps a (parcel_index, local position) pair to a global position and back
/// (`InterestManagement/ParcelEncoder.cs`).
pub struct ParcelEncoder {
    min_x: i32,
    min_z: i32,
    width: i32,
    parcel_size: i32,
    max_index_exclusive: i32,
}

impl ParcelEncoder {
    pub fn new(options: ParcelEncoderOptions) -> Self {
        let padding = options.padding;
        let min_x = options.min_parcel_x - padding;
        let min_z = options.min_parcel_z - padding;
        let max_x = options.max_parcel_x + padding;
        let max_z = options.max_parcel_z + padding;
        let width = max_x - min_x + 1;
        let height = max_z - min_z + 1;
        Self {
            min_x,
            min_z,
            width,
            parcel_size: options.parcel_size,
            max_index_exclusive: width * height,
        }
    }

    /// A parcel index is valid iff `0 <= index < max_index_exclusive`.
    pub fn is_valid_index(&self, index: i32) -> bool {
        (index as u32) < (self.max_index_exclusive as u32)
    }

    pub fn decode(&self, index: i32) -> (i32, i32) {
        let x = (index % self.width) + self.min_x;
        let z = (index / self.width) + self.min_z;
        (x, z)
    }

    pub fn decode_to_global_position(&self, index: i32, local_position: Vector3) -> Vector3 {
        let (x, z) = self.decode(index);
        Vector3 {
            x: (x * self.parcel_size) as f32 + local_position.x,
            y: local_position.y,
            z: (z * self.parcel_size) as f32 + local_position.z,
        }
    }
}

/// Uniform spatial hash grid keyed by cell coordinate (`InterestManagement/SpatialGrid.cs`).
/// Kept for parity / future broad-phase use; [`SpatialAreaOfInterest`] currently scans the
/// active set directly, exactly like the upstream `SpatialAreaOfInterest`.
pub struct SpatialGrid {
    inverse_cell_size: f32,
    parcel_size: i32,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            inverse_cell_size: 1.0 / cell_size,
            parcel_size: 16,
        }
    }

    fn cell_coord(&self, v: f32) -> i32 {
        (v * self.inverse_cell_size).floor() as i32
    }

    /// Pack a 2D cell into the upstream `((long)x << 32) | (uint)z` key.
    pub fn key(&self, position: Vector3) -> i64 {
        let x = self.cell_coord(position.x);
        let z = self.cell_coord(position.z);
        ((x as i64) << 32) | (z as u32 as i64)
    }

    /// Position is updated as part of every publish; the encoder owns the actual cell math.
    pub fn set(&mut self, _peer: u32, _position: Vector3) {
        let _ = self.parcel_size; // grid is positional only; kept for API parity
    }

    pub fn remove(&mut self, _peer: u32) {}
}

/// Radius tiers for spatial AoI (`SpatialAreaOfInterestOptions`). Defaults match upstream.
#[derive(Debug, Clone)]
pub struct SpatialAreaOfInterestOptions {
    pub tier0_radius: f32,
    pub tier1_radius: f32,
    pub max_radius: f32,
}

impl Default for SpatialAreaOfInterestOptions {
    fn default() -> Self {
        Self {
            tier0_radius: 20.0,
            tier1_radius: 50.0,
            max_radius: 100.0,
        }
    }
}

/// A single (subject, tier) entry in an interest result set (`InterestEntry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterestEntry {
    pub subject: u32,
    pub tier: PeerViewSimulationTier,
}

/// List-backed collector, reused across ticks (`InterestCollector`).
#[derive(Default)]
pub struct InterestCollector {
    pub entries: Vec<InterestEntry>,
}

impl InterestCollector {
    pub fn add(&mut self, subject: u32, tier: PeerViewSimulationTier) {
        self.entries.push(InterestEntry { subject, tier });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

/// Realm-gated, distance-tiered area of interest (`SpatialAreaOfInterest.cs`). Subjects in a
/// different realm, or beyond `max_radius`, are invisible; the rest are tagged TIER_0/1/2 by
/// horizontal (XZ) distance to the observer.
pub struct SpatialAreaOfInterest {
    tier0_sq: f32,
    tier1_sq: f32,
    max_distance_sq: f32,
}

impl SpatialAreaOfInterest {
    pub fn new(options: SpatialAreaOfInterestOptions) -> Self {
        Self {
            tier0_sq: options.tier0_radius * options.tier0_radius,
            tier1_sq: options.tier1_radius * options.tier1_radius,
            max_distance_sq: options.max_radius * options.max_radius,
        }
    }

    /// Fill `collector` with (subject, tier) for every active peer visible to `observer`.
    /// An observer with no realm sees nobody (matches "invisible until first teleport").
    pub fn get_visible_subjects(
        &self,
        board: &SnapshotBoard,
        observer: u32,
        observer_realm: Option<&str>,
        observer_pos: Vector3,
        collector: &mut InterestCollector,
    ) {
        let Some(observer_realm) = observer_realm else {
            return;
        };

        for subject in board.active_peers() {
            if subject == observer {
                continue;
            }
            let Some(subject_snapshot) = board.try_read(subject) else {
                continue;
            };
            if subject_snapshot.realm.as_deref() != Some(observer_realm) {
                continue;
            }

            let dx = subject_snapshot.global_position.x - observer_pos.x;
            let dz = subject_snapshot.global_position.z - observer_pos.z;
            let dist_sq = dx * dx + dz * dz;

            if dist_sq > self.max_distance_sq {
                continue;
            }

            let tier = if dist_sq <= self.tier0_sq {
                PeerViewSimulationTier::TIER_0
            } else if dist_sq <= self.tier1_sq {
                PeerViewSimulationTier::TIER_1
            } else {
                PeerViewSimulationTier::TIER_2
            };

            collector.add(subject, tier);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::PeerSnapshot;

    fn v3(x: f32, z: f32) -> Vector3 {
        Vector3 { x, y: 0.0, z }
    }

    fn place(board: &mut SnapshotBoard, id: u32, pos: Vector3, realm: &str) {
        board.set_active(id);
        board.publish(
            id,
            PeerSnapshot {
                seq: 0,
                global_position: pos,
                realm: Some(realm.into()),
                ..Default::default()
            },
        );
    }

    #[test]
    fn parcel_encode_decode_roundtrips_global_position() {
        let enc = ParcelEncoder::new(ParcelEncoderOptions::default());
        // origin parcel (0,0) -> index, then back to global.
        let idx = (0 - (-150 - 2)) + (0 - (-150 - 2)) * (163 + 2 - (-150 - 2) + 1);
        let g = enc.decode_to_global_position(idx, v3(3.0, 4.0));
        assert_eq!(g.x, 3.0);
        assert_eq!(g.z, 4.0);
    }

    #[test]
    fn observer_without_realm_sees_nobody() {
        let mut board = SnapshotBoard::new(8, 8);
        place(&mut board, 1, v3(0.0, 0.0), "r");
        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, 0, None, v3(0.0, 0.0), &mut c);
        assert_eq!(c.count(), 0);
    }

    #[test]
    fn different_realm_is_invisible() {
        let mut board = SnapshotBoard::new(8, 8);
        place(&mut board, 0, v3(0.0, 0.0), "realm-a");
        place(&mut board, 1, v3(1.0, 1.0), "realm-b");
        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, 0, Some("realm-a"), v3(0.0, 0.0), &mut c);
        assert_eq!(c.count(), 0);
    }

    #[test]
    fn distance_tiers_and_max_radius_cutoff() {
        let mut board = SnapshotBoard::new(8, 8);
        place(&mut board, 0, v3(0.0, 0.0), "r"); // observer
        place(&mut board, 1, v3(10.0, 0.0), "r"); // within tier0 (<=20)
        place(&mut board, 2, v3(30.0, 0.0), "r"); // tier1 (<=50)
        place(&mut board, 3, v3(70.0, 0.0), "r"); // tier2 (<=100)
        place(&mut board, 4, v3(150.0, 0.0), "r"); // beyond max -> invisible

        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, 0, Some("r"), v3(0.0, 0.0), &mut c);

        let mut got: Vec<(u32, u8)> = c
            .entries
            .iter()
            .map(|e| (e.subject, e.tier.value()))
            .collect();
        got.sort();
        assert_eq!(got, vec![(1, 0), (2, 1), (3, 2)]);
    }
}
