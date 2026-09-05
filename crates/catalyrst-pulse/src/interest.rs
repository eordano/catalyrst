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

    pub fn parcel_size(&self) -> i32 {
        self.parcel_size
    }
}

/// Tracks upstream Pulse `SpatialHashAreaOfInterest.CellSize` (appsettings.json); move it only in
/// lockstep. A scene listener's cover is sized from it, so a cell the size of a parcel would put
/// the per-tick cost back at O(announced parcels).
pub const SPATIAL_GRID_CELL_SIZE: f32 = 100.0;

/// Track upstream Pulse `SpatialHashAreaOfInterest` in appsettings.json, the `IAreaOfInterest`
/// its Program.cs registers; the `SpatialAreaOfInterest` section beside it (20/50/100) is config
/// upstream never serves. Move these only in lockstep with upstream: they decide how far every
/// player sees.
pub const DEFAULT_AOI_TIER0_RADIUS: f32 = 30.0;
pub const DEFAULT_AOI_TIER1_RADIUS: f32 = 60.0;
pub const DEFAULT_AOI_MAX_RADIUS: f32 = 200.0;

/// Ceiling on `PULSE_AOI_MAX_RADIUS`, 4x upstream's registered MaxRadius. Upstream bounds the
/// radius only from below (ScanCellRadius * CellSize must cover it) and widens the ring by hand;
/// ours is derived per query, so without this a typo would silently turn every tick into a
/// (2*ceil(r/cell)+1)^2 cell scan per observer. Raise it only with a measured tick budget.
pub const AOI_MAX_RADIUS_CEILING: f32 = 4.0 * DEFAULT_AOI_MAX_RADIUS;

/// Maps an announced scene-listener AoI to the deduped `SpatialGrid` cell keys covering it, one
/// inclusive parcel rect at a time. Computed once per announcement; immutable thereafter.
///
/// A rect's cover is the contiguous cell range between its two corners, so the work is
/// O(covered cells) rather than O(parcels): a 64x64 rect covers 121 cells, not 4096 parcels times
/// four corners each. The range has no gaps because a parcel is smaller than a grid cell, so
/// advancing one parcel moves the cell coordinate by at most one.
///
/// The closed max corner may over-cover one neighbouring cell when a parcel edge lands exactly on
/// a cell boundary, and realms share one grid coordinate space so their cells overlap outright;
/// both are harmless, the simulation filters candidates realm- and parcel-exact.
#[derive(Debug, Clone, Copy)]
pub struct SceneListenerCellMapper {
    parcel_size: i32,
    inverse_cell_size: f32,
}

impl SceneListenerCellMapper {
    pub fn new(grid: &SpatialGrid, encoder: &ParcelEncoder) -> Self {
        Self {
            parcel_size: encoder.parcel_size(),
            inverse_cell_size: grid.inverse_cell_size,
        }
    }

    /// Adds every grid cell key covering the inclusive parcel rect to `keys`. Coordinates must
    /// already be bounds-checked; this does no validation of its own.
    pub fn add_covering_cells(
        &self,
        keys: &mut std::collections::HashSet<i64>,
        min_x: i32,
        min_z: i32,
        max_x: i32,
        max_z: i32,
    ) {
        let min_cell_x = cell_coord((min_x * self.parcel_size) as f32, self.inverse_cell_size);
        let min_cell_z = cell_coord((min_z * self.parcel_size) as f32, self.inverse_cell_size);
        let max_cell_x = cell_coord(
            ((max_x + 1) * self.parcel_size) as f32,
            self.inverse_cell_size,
        );
        let max_cell_z = cell_coord(
            ((max_z + 1) * self.parcel_size) as f32,
            self.inverse_cell_size,
        );
        for cell_z in min_cell_z..=max_cell_z {
            for cell_x in min_cell_x..=max_cell_x {
                keys.insert(SpatialGrid::pack_key(cell_x, cell_z));
            }
        }
    }
}

/// Immutable scene-listener descriptor stamped onto `PeerState` by the scene-listener handshake
/// and replaced wholesale by `SceneListenerUpdate`, so a reader that has resolved it always sees
/// one consistent AoI. A peer carrying it never publishes snapshots (invisible to players) and
/// observes a parcel set per realm instead of a radius around its own position: every world
/// numbers its parcels from 0,0, so a parcel only means something together with its realm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneListenerState {
    /// Announced parcel set per realm: the parcel-exact visibility filter.
    pub parcels_by_realm: std::collections::HashMap<String, std::collections::HashSet<i32>>,
    /// Deduped, sorted `SpatialGrid` cell keys covering every announced realm's parcels. The
    /// grid is one global coordinate space, so realms overlap in it; that only ever over-covers,
    /// and candidates are filtered realm- and parcel-exact, so the cost is a lookup, never
    /// correctness.
    cell_keys: Vec<i64>,
}

impl SceneListenerState {
    pub fn new(
        parcels_by_realm: std::collections::HashMap<String, std::collections::HashSet<i32>>,
        cell_keys: std::collections::HashSet<i64>,
    ) -> Self {
        let mut cell_keys: Vec<i64> = cell_keys.into_iter().collect();
        cell_keys.sort_unstable();
        Self {
            parcels_by_realm,
            cell_keys,
        }
    }

    /// Whether a subject standing in `parcel` of `realm` is observed.
    pub fn observes(&self, realm: Option<&str>, parcel: i32) -> bool {
        realm
            .and_then(|r| self.parcels_by_realm.get(r))
            .is_some_and(|parcels| parcels.contains(&parcel))
    }

    pub fn realm_count(&self) -> usize {
        self.parcels_by_realm.len()
    }

    /// Total announced parcels across every realm: logging and metrics only.
    pub fn parcel_count(&self) -> usize {
        self.parcels_by_realm
            .values()
            .map(std::collections::HashSet::len)
            .sum()
    }

    /// Covering cells, the per-tick lookup count: logging and metrics only.
    pub fn cell_count(&self) -> usize {
        self.cell_keys.len()
    }

    pub fn cell_keys(&self) -> &[i64] {
        &self.cell_keys
    }

    /// Fills the collector with every subject standing inside the AoI, all at TIER_0 (a parcel
    /// set has no distance to tier by): the occupants of the precomputed covering cells, filtered
    /// on (realm, parcel) exactly. Neither filter is redundant: the cover over-approximates (a
    /// 100-unit cell holds ~6x6 parcels), and every realm numbers its parcels from 0,0, so two
    /// cohosted worlds share both cells and parcel indices. A peer occupies exactly one grid cell
    /// and the keys are deduped, so a subject can never be collected twice.
    pub fn get_visible_subjects(
        &self,
        board: &SnapshotBoard,
        grid: &SpatialGrid,
        observer: u32,
        collector: &mut InterestCollector,
    ) {
        for &key in &self.cell_keys {
            for &subject in grid.peers_in_cell(key) {
                #[cfg(test)]
                SCENE_LISTENER_EXAMINED.with(|c| c.set(c.get() + 1));

                if subject == observer {
                    continue;
                }
                let Some(s) = board.try_read(subject) else {
                    continue;
                };
                if !self.observes(s.realm.as_deref(), s.parcel) {
                    continue;
                }
                collector.add(subject, PeerViewSimulationTier::TIER_0);
            }
        }
    }

    /// Builds a descriptor from bare parcel indices, covering each as a 1x1 rect. Tests announce
    /// by index; the server announces rects and covers each in O(cells).
    #[cfg(test)]
    pub fn from_parcels(
        parcels_by_realm: std::collections::HashMap<String, std::collections::HashSet<i32>>,
        encoder: &ParcelEncoder,
        mapper: &SceneListenerCellMapper,
    ) -> Self {
        let mut keys = std::collections::HashSet::new();
        for &parcel in parcels_by_realm.values().flatten() {
            let (x, z) = encoder.decode(parcel);
            mapper.add_covering_cells(&mut keys, x, z, x, z);
        }
        Self::new(parcels_by_realm, keys)
    }

    /// Pre-optimization full scan, kept as the parity oracle for `get_visible_subjects`.
    #[cfg(test)]
    pub fn get_visible_subjects_linear(
        &self,
        board: &SnapshotBoard,
        observer: u32,
        collector: &mut InterestCollector,
    ) {
        for &subject in board.active_peers() {
            if subject == observer {
                continue;
            }
            let Some(s) = board.try_read(subject) else {
                continue;
            };
            if !self.observes(s.realm.as_deref(), s.parcel) {
                continue;
            }
            collector.add(subject, PeerViewSimulationTier::TIER_0);
        }
    }
}

// Thread-local so parallel test threads do not cross-count.
#[cfg(test)]
thread_local! {
    pub static SCENE_LISTENER_EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Uniform-grid spatial index over peer world positions, kept in sync by the snapshot
/// publisher so AOI queries scan nearby cells instead of every active peer.
pub struct SpatialGrid {
    inverse_cell_size: f32,
    cells: std::collections::HashMap<i64, Vec<u32>>,
    peer_cell: std::collections::HashMap<u32, i64>,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            inverse_cell_size: 1.0 / cell_size,
            cells: std::collections::HashMap::new(),
            peer_cell: std::collections::HashMap::new(),
        }
    }

    /// Cell coordinate containing the world coordinate `v` on one axis. Paired with `pack_key`
    /// this decomposes `key`, so a caller covering a contiguous world region can walk cell
    /// coordinates directly instead of probing every point inside it.
    pub fn cell_coord(&self, v: f32) -> i32 {
        cell_coord(v, self.inverse_cell_size)
    }

    /// Packs a cell coordinate pair into the key `peers_in_cell` takes.
    pub fn pack_key(x: i32, z: i32) -> i64 {
        ((x as i64) << 32) | (z as u32 as i64)
    }

    pub fn key(&self, position: Vector3) -> i64 {
        Self::pack_key(self.cell_coord(position.x), self.cell_coord(position.z))
    }

    /// Membership only: callers still apply the active/realm/self filters per candidate.
    pub fn peers_in_cell(&self, key: i64) -> &[u32] {
        self.cells.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn set(&mut self, peer: u32, position: Vector3) {
        let k = self.key(position);
        if self.peer_cell.get(&peer) == Some(&k) {
            return;
        }
        self.remove(peer);
        self.cells.entry(k).or_default().push(peer);
        self.peer_cell.insert(peer, k);
    }

    pub fn remove(&mut self, peer: u32) {
        if let Some(k) = self.peer_cell.remove(&peer) {
            if let Some(bucket) = self.cells.get_mut(&k) {
                if let Some(pos) = bucket.iter().position(|&p| p == peer) {
                    bucket.swap_remove(pos);
                }
                if bucket.is_empty() {
                    self.cells.remove(&k);
                }
            }
        }
    }

    /// Cells to scan on each side of the observer's cell to cover `radius`: upstream's
    /// `ScanCellRadius`, derived per query rather than configured so it can never sit below the
    /// `ceil(MaxRadius / CellSize)` floor upstream's options validator has to enforce by hand.
    pub fn scan_cell_radius(&self, radius: f32) -> i32 {
        (radius * self.inverse_cell_size).ceil() as i32
    }

    /// Yields every peer in the cell ring covering `radius` around `center`. Candidates outside
    /// the exact radius are still yielded; the caller applies the precise distance test.
    pub fn for_candidates_in_radius(&self, center: Vector3, radius: f32, mut f: impl FnMut(u32)) {
        let cx = self.cell_coord(center.x);
        let cz = self.cell_coord(center.z);
        let r = self.scan_cell_radius(radius);
        for gz in (cz - r)..=(cz + r) {
            for gx in (cx - r)..=(cx + r) {
                if let Some(bucket) = self.cells.get(&Self::pack_key(gx, gz)) {
                    for &peer in bucket {
                        f(peer);
                    }
                }
            }
        }
    }
}

fn cell_coord(v: f32, inverse_cell_size: f32) -> i32 {
    (v * inverse_cell_size).floor() as i32
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
            tier0_radius: DEFAULT_AOI_TIER0_RADIUS,
            tier1_radius: DEFAULT_AOI_TIER1_RADIUS,
            max_radius: DEFAULT_AOI_MAX_RADIUS,
        }
    }
}

impl SpatialAreaOfInterestOptions {
    /// Finite, non-negative and 0 <= tier0 <= tier1 <= max <= `AOI_MAX_RADIUS_CEILING`.
    /// Upstream's validator only checks `MaxRadius >= 0` and that the scan cover reaches it; the
    /// ordering is ours because `classify` resolves tiers in that order, so a swapped pair
    /// silently empties a tier, and the ceiling is ours because the scan ring is derived from
    /// the radius rather than configured beside it.
    pub fn validate(&self) -> anyhow::Result<()> {
        for (name, radius) in [
            ("PULSE_AOI_TIER0_RADIUS", self.tier0_radius),
            ("PULSE_AOI_TIER1_RADIUS", self.tier1_radius),
            ("PULSE_AOI_MAX_RADIUS", self.max_radius),
        ] {
            if !radius.is_finite() || radius < 0.0 {
                anyhow::bail!("{name} must be a finite radius >= 0, got {radius}");
            }
        }
        if self.max_radius > AOI_MAX_RADIUS_CEILING {
            let ring = 2.0 * (self.max_radius / SPATIAL_GRID_CELL_SIZE).ceil() + 1.0;
            anyhow::bail!(
                "PULSE_AOI_MAX_RADIUS must be <= {AOI_MAX_RADIUS_CEILING}, got {}: that radius \
                 would scan a {ring}x{ring} cell ring per observer per tick",
                self.max_radius
            );
        }
        if !(self.tier0_radius <= self.tier1_radius && self.tier1_radius <= self.max_radius) {
            anyhow::bail!(
                "player AoI radii must satisfy tier0 <= tier1 <= max, got {} / {} / {}",
                self.tier0_radius,
                self.tier1_radius,
                self.max_radius
            );
        }
        Ok(())
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

    fn classify(&self, dist_sq: f32) -> Option<PeerViewSimulationTier> {
        if dist_sq > self.max_distance_sq {
            None
        } else if dist_sq <= self.tier0_sq {
            Some(PeerViewSimulationTier::TIER_0)
        } else if dist_sq <= self.tier1_sq {
            Some(PeerViewSimulationTier::TIER_1)
        } else {
            Some(PeerViewSimulationTier::TIER_2)
        }
    }

    pub fn get_visible_subjects(
        &self,
        board: &SnapshotBoard,
        grid: &SpatialGrid,
        observer: u32,
        observer_realm: Option<&str>,
        observer_pos: Vector3,
        collector: &mut InterestCollector,
    ) {
        let Some(observer_realm) = observer_realm else {
            return;
        };

        let max_radius = self.max_distance_sq.sqrt();
        grid.for_candidates_in_radius(observer_pos, max_radius, |subject| {
            #[cfg(test)]
            CANDIDATES_EXAMINED.with(|c| c.set(c.get() + 1));

            if subject == observer {
                return;
            }
            let Some(subject_snapshot) = board.try_read(subject) else {
                return;
            };
            if subject_snapshot.realm.as_deref() != Some(observer_realm) {
                return;
            }

            let dx = subject_snapshot.global_position.x - observer_pos.x;
            let dz = subject_snapshot.global_position.z - observer_pos.z;
            let dist_sq = dx * dx + dz * dz;

            if let Some(tier) = self.classify(dist_sq) {
                collector.add(subject, tier);
            }
        });
    }

    /// Pre-optimization full scan, kept as the parity oracle for `get_visible_subjects`.
    #[cfg(test)]
    pub fn get_visible_subjects_linear(
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
            if let Some(tier) = self.classify(dist_sq) {
                collector.add(subject, tier);
            }
        }
    }
}

// Thread-local so parallel test threads do not cross-count.
#[cfg(test)]
thread_local! {
    pub static CANDIDATES_EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::PeerSnapshot;

    // Upstream's `ScanCellRadius` at its shipped radii and cell size. Ours is derived per query
    // (`SpatialGrid::scan_cell_radius`), so this only exists for the parity pin.
    const UPSTREAM_SCAN_CELL_RADIUS: i32 = 2;

    fn v3(x: f32, z: f32) -> Vector3 {
        Vector3 { x, y: 0.0, z }
    }

    fn place(
        board: &mut SnapshotBoard,
        grid: &mut SpatialGrid,
        id: u32,
        pos: Vector3,
        realm: &str,
    ) {
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
        grid.set(id, pos);
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
    fn default_radii_and_scan_ring_track_upstream_spatial_hash_aoi() {
        let options = SpatialAreaOfInterestOptions::default();
        assert_eq!(
            (
                options.tier0_radius,
                options.tier1_radius,
                options.max_radius
            ),
            (30.0, 60.0, 200.0),
            "upstream SpatialHashAreaOfInterest Tier0Radius / Tier1Radius / MaxRadius"
        );
        assert_eq!(SPATIAL_GRID_CELL_SIZE, 100.0, "upstream CellSize");
        assert_eq!(
            SpatialGrid::new(SPATIAL_GRID_CELL_SIZE).scan_cell_radius(options.max_radius),
            UPSTREAM_SCAN_CELL_RADIUS,
            "the derived ring must scan exactly the (2N+1)^2 cells upstream configures"
        );
        options.validate().unwrap();
    }

    #[test]
    fn radii_validation_rejects_negative_nan_and_misordered() {
        let ok = SpatialAreaOfInterestOptions::default();
        let cases = [
            SpatialAreaOfInterestOptions {
                tier0_radius: -1.0,
                ..ok.clone()
            },
            SpatialAreaOfInterestOptions {
                max_radius: f32::NAN,
                ..ok.clone()
            },
            SpatialAreaOfInterestOptions {
                tier1_radius: f32::INFINITY,
                ..ok.clone()
            },
            SpatialAreaOfInterestOptions {
                tier0_radius: 70.0,
                ..ok.clone()
            },
            SpatialAreaOfInterestOptions {
                max_radius: 50.0,
                ..ok.clone()
            },
        ];
        for bad in cases {
            assert!(bad.validate().is_err(), "{bad:?} must be rejected");
        }
        SpatialAreaOfInterestOptions {
            tier0_radius: 0.0,
            tier1_radius: 0.0,
            max_radius: 0.0,
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn max_radius_ceiling_bounds_the_scan_ring() {
        let ok = SpatialAreaOfInterestOptions::default();
        assert_eq!(AOI_MAX_RADIUS_CEILING, 4.0 * DEFAULT_AOI_MAX_RADIUS);
        SpatialAreaOfInterestOptions {
            max_radius: AOI_MAX_RADIUS_CEILING,
            ..ok.clone()
        }
        .validate()
        .unwrap();
        assert_eq!(
            SpatialGrid::new(SPATIAL_GRID_CELL_SIZE).scan_cell_radius(AOI_MAX_RADIUS_CEILING),
            8,
            "the widest ring validate() admits is 17x17 cells"
        );

        for typo in [AOI_MAX_RADIUS_CEILING + 1.0, 1000.0, 20_000.0, f32::MAX] {
            let err = SpatialAreaOfInterestOptions {
                max_radius: typo,
                ..ok.clone()
            }
            .validate()
            .unwrap_err();
            assert!(
                err.to_string()
                    .starts_with("PULSE_AOI_MAX_RADIUS must be <= 800, got"),
                "{typo}: {err}"
            );
        }
        let err = SpatialAreaOfInterestOptions {
            max_radius: 1000.0,
            ..ok.clone()
        }
        .validate()
        .unwrap_err();
        assert!(err.to_string().contains("21x21"), "{err}");

        let scaled = SpatialAreaOfInterestOptions {
            tier0_radius: 300.0,
            tier1_radius: 600.0,
            max_radius: 2000.0,
        };
        assert!(
            scaled.validate().is_err(),
            "a well-ordered triple is still bounded by the ceiling"
        );
    }

    #[test]
    fn observer_without_realm_sees_nobody() {
        let mut board = SnapshotBoard::new(8, 8);
        let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
        place(&mut board, &mut grid, 1, v3(0.0, 0.0), "r");
        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, &grid, 0, None, v3(0.0, 0.0), &mut c);
        assert_eq!(c.count(), 0);
    }

    #[test]
    fn different_realm_is_invisible() {
        let mut board = SnapshotBoard::new(8, 8);
        let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
        place(&mut board, &mut grid, 0, v3(0.0, 0.0), "realm-a");
        place(&mut board, &mut grid, 1, v3(1.0, 1.0), "realm-b");
        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, &grid, 0, Some("realm-a"), v3(0.0, 0.0), &mut c);
        assert_eq!(c.count(), 0);
    }

    // Every tier boundary is inclusive on its own side; the observer sits at the origin so each
    // subject's x is its distance. The two farthest straddle the max radius one unit apart.
    #[test]
    fn distance_tiers_and_max_radius_cutoff() {
        let mut board = SnapshotBoard::new(8, 8);
        let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
        place(&mut board, &mut grid, 0, v3(0.0, 0.0), "r");
        place(&mut board, &mut grid, 1, v3(10.0, 0.0), "r");
        place(&mut board, &mut grid, 2, v3(30.0, 0.0), "r");
        place(&mut board, &mut grid, 3, v3(45.0, 0.0), "r");
        place(&mut board, &mut grid, 4, v3(60.0, 0.0), "r");
        place(&mut board, &mut grid, 5, v3(150.0, 0.0), "r");
        place(&mut board, &mut grid, 6, v3(200.0, 0.0), "r");
        place(&mut board, &mut grid, 7, v3(201.0, 0.0), "r");

        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        let mut c = InterestCollector::default();
        aoi.get_visible_subjects(&board, &grid, 0, Some("r"), v3(0.0, 0.0), &mut c);

        let mut got: Vec<(u32, u8)> = c
            .entries
            .iter()
            .map(|e| (e.subject, e.tier.value()))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![(1, 0), (2, 0), (3, 1), (4, 1), (5, 2), (6, 2)],
            "30 / 60 / 200 inclusive; 201 is invisible"
        );
    }

    // Runs at the shipped cell size and at one that does not divide the max radius: 200 / 100
    // is a whole 2, so only the 48-unit grid (200 / 48 = 4.17 cells) can tell a floored ring
    // from a ceiled one, and a floored ring drops peers standing in the outer strip.
    #[test]
    fn grid_query_matches_linear_scan() {
        let mut board = SnapshotBoard::new(600, 8);
        let mut placed: Vec<(u32, Vector3)> = Vec::new();
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        let mut rng = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for id in 0..500u32 {
            let x = (rng() % 4000) as f32 - 2000.0;
            let z = (rng() % 4000) as f32 - 2000.0;
            let realm = if rng() % 2 == 0 { "realm-a" } else { "realm-b" };
            board.set_active(id);
            board.publish(
                id,
                PeerSnapshot {
                    seq: 0,
                    global_position: v3(x, z),
                    realm: Some(realm.into()),
                    ..Default::default()
                },
            );
            placed.push((id, v3(x, z)));
        }
        let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
        for cell_size in [SPATIAL_GRID_CELL_SIZE, 48.0] {
            let mut grid = SpatialGrid::new(cell_size);
            for &(id, pos) in &placed {
                grid.set(id, pos);
            }
            for &(ox, oz, realm) in &[(0.0f32, 0.0f32, "realm-a"), (500.0, -300.0, "realm-b")] {
                let mut grid_c = InterestCollector::default();
                aoi.get_visible_subjects(&board, &grid, 999, Some(realm), v3(ox, oz), &mut grid_c);
                let mut lin_c = InterestCollector::default();
                aoi.get_visible_subjects_linear(&board, 999, Some(realm), v3(ox, oz), &mut lin_c);

                let mut a: Vec<(u32, u8)> = grid_c
                    .entries
                    .iter()
                    .map(|e| (e.subject, e.tier.value()))
                    .collect();
                let mut b: Vec<(u32, u8)> = lin_c
                    .entries
                    .iter()
                    .map(|e| (e.subject, e.tier.value()))
                    .collect();
                a.sort();
                b.sort();
                assert!(!a.is_empty(), "the seeded board must exercise the ring");
                assert_eq!(
                    a, b,
                    "grid (cell {cell_size}) and linear outputs must be set-equal (with tiers)"
                );
            }
        }
    }

    // N peers in fixed-size clusters of C, each cluster 1km apart (beyond the 200-unit max radius
    // and its 500-unit-wide scan ring): a linear scan examines N*(N-1), the grid examines only
    // clustermates.
    #[test]
    fn aoi_candidate_checks_scale_with_local_density() {
        const C: u32 = 10;

        fn examined_for(n: u32) -> usize {
            let mut board = SnapshotBoard::new((n + 1) as usize, 8);
            let mut grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
            for id in 0..n {
                let cluster = id / C;
                let off = (id % C) as f32;
                let cx = cluster as f32 * 1000.0;
                place(&mut board, &mut grid, id, v3(cx + off, off), "r");
            }
            let aoi = SpatialAreaOfInterest::new(SpatialAreaOfInterestOptions::default());
            CANDIDATES_EXAMINED.with(|c| c.set(0));
            for observer in 0..n {
                let pos = board.try_read(observer).unwrap().global_position;
                let mut c = InterestCollector::default();
                aoi.get_visible_subjects(&board, &grid, observer, Some("r"), pos, &mut c);
            }
            CANDIDATES_EXAMINED.with(|c| c.get())
        }

        let n = 200u32;
        let examined = examined_for(n);

        assert!(
            examined <= (n as usize) * (C as usize + 5),
            "examined {examined} should scale with cluster size C={C}, not N*N"
        );
        assert!(
            examined < (n as usize) * (n as usize) / 4,
            "examined {examined} must be far below the N^2={} linear-scan cost",
            n * n
        );

        let examined_2n = examined_for(2 * n);
        let per_observer = examined as f64 / n as f64;
        let per_observer_2n = examined_2n as f64 / (2 * n) as f64;
        assert!(
            (per_observer_2n - per_observer).abs() < 2.0,
            "per-observer examined must be independent of total N ({per_observer} vs {per_observer_2n})"
        );
    }

    // ParcelEncoderOptions default: parcel size 16. Grid cell 100, so parcel (0,0) spans world
    // [0,16)^2 inside cell (0,0).
    const PARCEL_SIZE: f32 = 16.0;

    fn mapper_fixture() -> (SpatialGrid, SceneListenerCellMapper) {
        let grid = SpatialGrid::new(SPATIAL_GRID_CELL_SIZE);
        let mapper = SceneListenerCellMapper::new(
            &grid,
            &ParcelEncoder::new(ParcelEncoderOptions::default()),
        );
        (grid, mapper)
    }

    fn cover(
        mapper: &SceneListenerCellMapper,
        min_x: i32,
        min_z: i32,
        max_x: i32,
        max_z: i32,
    ) -> std::collections::HashSet<i64> {
        let mut keys = std::collections::HashSet::new();
        mapper.add_covering_cells(&mut keys, min_x, min_z, max_x, max_z);
        keys
    }

    fn reachable(grid: &SpatialGrid, keys: &std::collections::HashSet<i64>, peer: u32) -> bool {
        keys.iter().any(|&k| grid.peers_in_cell(k).contains(&peer))
    }

    #[test]
    fn single_parcel_inside_one_cell_covers_that_cell() {
        let (mut grid, mapper) = mapper_fixture();
        let keys = cover(&mapper, 1, 1, 1, 1);
        grid.set(7, v3(20.0, 20.0));
        assert!(
            reachable(&grid, &keys, 7),
            "a peer standing inside the parcel must be reachable through the covering cell keys"
        );
    }

    #[test]
    fn parcel_straddling_cell_boundary_covers_both_cells() {
        let (mut grid, mapper) = mapper_fixture();
        let keys = cover(&mapper, 6, 0, 6, 0);
        grid.set(1, v3(97.0, 5.0));
        grid.set(2, v3(105.0, 5.0));
        assert!(reachable(&grid, &keys, 1));
        assert!(reachable(&grid, &keys, 2));
    }

    #[test]
    fn adjacent_parcels_in_same_cell_dedupe_keys() {
        let (_, mapper) = mapper_fixture();
        let keys = cover(&mapper, 1, 1, 2, 1);
        assert!(
            keys.len() <= 4,
            "two adjacent interior parcels must not multiply covering cells, got {}",
            keys.len()
        );
    }

    // The cover is derived from the rect's two corners rather than from each parcel's four
    // corners; that shortcut is only valid while the two agree exactly. An 8x8 rect spans world
    // [0,128)^2, crossing both cell boundaries at 100.
    #[test]
    fn rect_cover_matches_every_parcel_inside_it() {
        let (grid, mapper) = mapper_fixture();
        let from_rect = cover(&mapper, 0, 0, 7, 7);
        let mut from_parcels = std::collections::HashSet::new();
        for z in 0..=7 {
            for x in 0..=7 {
                let min_x = x as f32 * PARCEL_SIZE;
                let min_z = z as f32 * PARCEL_SIZE;
                from_parcels.insert(grid.key(v3(min_x, min_z)));
                from_parcels.insert(grid.key(v3(min_x + PARCEL_SIZE, min_z)));
                from_parcels.insert(grid.key(v3(min_x, min_z + PARCEL_SIZE)));
                from_parcels.insert(grid.key(v3(min_x + PARCEL_SIZE, min_z + PARCEL_SIZE)));
            }
        }
        assert_eq!(from_rect, from_parcels);
    }

    #[test]
    fn separate_rects_accumulate_into_one_cover() {
        let (_, mapper) = mapper_fixture();
        let mut keys = std::collections::HashSet::new();
        mapper.add_covering_cells(&mut keys, 0, 0, 0, 0);
        mapper.add_covering_cells(&mut keys, 40, 40, 40, 40);
        let expected: std::collections::HashSet<i64> = cover(&mapper, 0, 0, 0, 0)
            .union(&cover(&mapper, 40, 40, 40, 40))
            .copied()
            .collect();
        assert_eq!(
            keys, expected,
            "accumulating rects into a shared set must union their covers, not replace them"
        );
    }

    // The full-budget shape: 4096 parcels is 64x64 x 16 units = 1024 units a side, eleven
    // 100-unit cells with the closed max corner.
    #[test]
    fn full_budget_rect_covers_bounded_cells() {
        let (_, mapper) = mapper_fixture();
        let keys = cover(&mapper, 0, 0, 63, 63);
        assert_eq!(keys.len(), 121);
    }
}
