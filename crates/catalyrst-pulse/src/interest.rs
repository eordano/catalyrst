use crate::decentraland::common::Vector3;
use crate::snapshot::SnapshotBoard;

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

pub struct ParcelEncoder {
    min_x: i32,
    min_z: i32,
    width: i32,
    height: i32,
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
            height,
            parcel_size: options.parcel_size,
            max_index_exclusive: width * height,
        }
    }

    pub fn is_valid_index(&self, index: i32) -> bool {
        (index as u32) < (self.max_index_exclusive as u32)
    }

    pub fn encode(&self, x: i32, z: i32) -> i32 {
        x - self.min_x + (z - self.min_z) * self.width
    }

    pub fn is_valid_coordinate(&self, x: i32, z: i32) -> bool {
        x >= self.min_x
            && x < self.min_x + self.width
            && z >= self.min_z
            && z < self.min_z + self.height
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

/// Immutable scene-listener descriptor stamped onto `PeerState` at handshake. A peer carrying it
/// never publishes snapshots (invisible to players) and observes a fixed parcel set instead of a
/// radius around its own position. Changing the set requires reconnecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneListenerState {
    pub realm: String,
    pub parcels: std::collections::HashSet<i32>,
}

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

    pub fn key(&self, position: Vector3) -> i64 {
        let x = self.cell_coord(position.x);
        let z = self.cell_coord(position.z);
        ((x as i64) << 32) | (z as u32 as i64)
    }

    pub fn set(&mut self, _peer: u32, _position: Vector3) {
        let _ = self.parcel_size;
    }

    pub fn remove(&mut self, _peer: u32) {}
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterestEntry {
    pub subject: u32,
    pub tier: PeerViewSimulationTier,
}

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

        for &subject in board.active_peers() {
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
        place(&mut board, 0, v3(0.0, 0.0), "r");
        place(&mut board, 1, v3(10.0, 0.0), "r");
        place(&mut board, 2, v3(30.0, 0.0), "r");
        place(&mut board, 3, v3(70.0, 0.0), "r");
        place(&mut board, 4, v3(150.0, 0.0), "r");

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
