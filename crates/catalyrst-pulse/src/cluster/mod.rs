//! Peer clustering over the area-of-interest grid.
//!
//! One pass every `pass_interval_ms` runs weighted union-find with path halving over the occupied
//! cells of one realm's grid at a time, using 8-neighbour adjacency. A cluster cannot span realms
//! because the cells it is built from cannot: realm isolation belongs to `RealmSpatialGrids`, so
//! partitioning costs no realm comparison here. Cost is O(peers + occupied cells) -- no peer-pair
//! tests. Working buffers are fields, cleared rather than reallocated between passes.
//!
//! The pass runs on the server's own cooperative task as a branch of its `select!` loop rather
//! than on a thread of its own: `PulseServer` owns every board through `&mut self`, so there is no
//! worker concurrency to defend against and no lock-free board discipline to keep.

pub mod feed;
#[cfg(feature = "nats")]
pub mod nats;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::decentraland::common::Vector3;
use crate::interest::SpatialGrid;
use crate::realm_grids::RealmSpatialGrids;
use crate::snapshot::{IdentityBoard, SnapshotBoard};

pub use feed::{ClusterFeedPublisher, NoopClusterFeedPublisher, RecordingClusterFeed};

use std::sync::Arc;

/// Cadence of the clustering pass, and the upper bound on how stale a published assignment or a
/// stats read can be.
pub const DEFAULT_PASS_INTERVAL_MS: u64 = 1000;

/// Consecutive passes that must agree on a new assignment before it is published. Temporal
/// hysteresis in place of archipelago's join/leave distance bands, absorbing the cell-boundary
/// noise of clustering on grid cells rather than peer-pair distances.
pub const DEFAULT_DWELL_PASSES: u32 = 3;

pub const DEFAULT_ID_PREFIX: &str = "C";

/// Passes a departed wallet's last published assignment is retained, so a new session arriving
/// inside the window can name it as displaced. Zero disables the annotation.
pub const DEFAULT_SESSION_RETENTION_PASSES: u64 = 300;

/// Tracks upstream `appsettings.json`, which ships `Clusters:Enabled` on with `Nats:Url` empty:
/// the tracker derives clusters and reports metrics everywhere while publishing nothing until a
/// broker URL is injected. Shadow mode is the shipped default, not off.
pub const DEFAULT_CLUSTERS_ENABLED: bool = true;

const NONE: usize = usize::MAX;

/// Fraction of the pass interval a pass may take before it is warned about. A tenth leaves the
/// tick task nine parts in ten for peer simulation, which is what it exists to do.
const OVERRUN_BUDGET_DIVISOR: u64 = 10;

/// Forward half of the 8-neighbourhood: the +X column plus the cell straight ahead in +Z. Every
/// node probes and union is symmetric, so each adjacent pair is still visited exactly once -- half
/// the lookups of the full ring for an identical partition.
const NEIGHBOR_DX: [i32; 4] = [1, 1, 1, 0];
const NEIGHBOR_DZ: [i32; 4] = [-1, 0, 1, 1];

#[derive(Debug, Clone)]
pub struct ClusterOptions {
    pub enabled: bool,
    pub pass_interval_ms: u64,
    pub dwell_passes: u32,
    pub id_prefix: String,
    pub session_retention_passes: u64,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_CLUSTERS_ENABLED,
            pass_interval_ms: DEFAULT_PASS_INTERVAL_MS,
            dwell_passes: DEFAULT_DWELL_PASSES,
            id_prefix: DEFAULT_ID_PREFIX.to_string(),
            session_retention_passes: DEFAULT_SESSION_RETENTION_PASSES,
        }
    }
}

/// The session a published assignment belongs to, and the session it displaced when it is the
/// first publish of a new session for a wallet whose previous session still had a retained
/// assignment. Both displaced fields are `None` on every other publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSession {
    pub session: String,
    pub displaced_session: Option<String>,
    pub displaced_cluster_id: Option<String>,
}

impl ClusterSession {
    pub fn new(session: String) -> Self {
        Self {
            session,
            displaced_session: None,
            displaced_cluster_id: None,
        }
    }

    pub fn is_takeover(&self) -> bool {
        self.displaced_session.is_some()
    }
}

/// Metadata for one cluster in a single pass. `centroid` is the mean of member positions and
/// `radius` the distance from it to the farthest member, both on the XZ plane -- the geometry
/// archipelago reported.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterInfo {
    pub id: String,
    pub realm: String,
    pub count: usize,
    pub centroid: Vector3,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClusterPeerInfo {
    pub peer: u32,
    pub wallet: String,
    pub cluster_id: String,
    pub realm: String,
    pub position: Vector3,
    pub parcel: i32,
}

/// The result of one clustering pass. Never mutated after construction.
#[derive(Debug, Default, Clone)]
pub struct ClusterPass {
    pub clusters: Vec<ClusterInfo>,
    pub peers: Vec<ClusterPeerInfo>,
    cluster_id_by_peer: Vec<Option<String>>,
}

impl ClusterPass {
    /// The cluster this peer belonged to as of this pass, or `None` if it was unassigned (no
    /// realm, or not in any grid when the pass ran). Indexed by peer slot, so it answers in
    /// constant time rather than scanning `peers`.
    pub fn cluster_id(&self, peer: u32) -> Option<&str> {
        self.cluster_id_by_peer
            .get(peer as usize)
            .and_then(|id| id.as_deref())
    }
}

/// Holds the latest pass. Kept as its own type rather than a bare field for symmetry with
/// upstream's board, and because a future stats endpoint reads exactly this.
#[derive(Default)]
pub struct ClusterBoard {
    current: ClusterPass,
}

impl ClusterBoard {
    pub fn current(&self) -> &ClusterPass {
        &self.current
    }

    pub fn publish(&mut self, pass: ClusterPass) {
        self.current = pass;
    }
}

/// A peer as observed by one pass, resolved once so later steps never re-read the boards. Carries
/// no realm: the realm belongs to the node the member was collected into, and every member of a
/// node shares it.
#[derive(Debug, Clone)]
struct PassMember {
    peer: u32,
    wallet: String,
    session: String,
    position: Vector3,
    parcel: i32,
    is_teleport: bool,
}

struct PassNode {
    realm: usize,
    cell_key: i64,
    member_start: usize,
    member_count: usize,
    parent: usize,
    tree_size: usize,
    component: usize,
}

struct PassComponent {
    realm: usize,
    member_start: usize,
    member_count: usize,
    inherited_id: Option<String>,
    inherited_overlap: usize,
    id: String,
}

/// What the tracker carries about one peer slot between passes.
///
/// Two notions of "previous cluster", deliberately kept apart: `previous_pass_cluster_id` is what
/// the last pass computed, which is what sticky-ID inheritance measures overlap against;
/// `published_cluster_id` is what the feed was last told, read only by the debounce. Conflating
/// them starves the debounce -- a fragment mid-debounce would look unassigned to inheritance and
/// be minted a fresh ID every pass, so its candidate would never repeat.
#[derive(Default, Clone)]
struct PeerClusterState {
    previous_pass_cluster_id: Option<String>,
    published_cluster_id: Option<String>,
    published_realm: Option<String>,
    candidate_cluster_id: Option<String>,
    candidate_streak: u32,
    last_seen_pass: u64,
}

/// A wallet's last published assignment and the session that published it, retained across the
/// peer-slot change of a duplicate-session eviction. `last_seen_pass` tracks when the wallet was
/// last seen rather than last published: a stationary peer publishes once and never again, and its
/// entry must not expire underneath it.
struct WalletAssignment {
    cluster_id: String,
    session: String,
    last_seen_pass: u64,
}

/// Bookkeeping for one cluster ID that exists or existed. `last_live_pass` equal to the current
/// pass marks the cluster live and makes `claimant` meaningful; anything older is pruned.
struct ClusterRecord {
    creation_seq: u64,
    last_live_pass: u64,
    claimant: usize,
}

pub struct ClusterTracker {
    options: ClusterOptions,
    publisher: Arc<dyn ClusterFeedPublisher>,
    board: ClusterBoard,

    pass_realms: Vec<String>,
    pass_cell_keys: Vec<i64>,
    node_index_by_key: HashMap<i64, usize>,
    nodes: Vec<PassNode>,
    members: Vec<PassMember>,
    components: Vec<PassComponent>,
    ordered_members: Vec<usize>,
    overlap_counts: HashMap<String, usize>,

    peer_states: Vec<PeerClusterState>,
    cluster_records: HashMap<String, ClusterRecord>,
    assignment_by_wallet: HashMap<String, WalletAssignment>,

    pass_number: u64,
    next_cluster_number: u64,
}

impl ClusterTracker {
    pub fn new(
        options: ClusterOptions,
        max_peers: usize,
        publisher: Arc<dyn ClusterFeedPublisher>,
    ) -> Self {
        Self {
            options,
            publisher,
            board: ClusterBoard::default(),
            pass_realms: Vec::new(),
            pass_cell_keys: Vec::new(),
            node_index_by_key: HashMap::new(),
            nodes: Vec::new(),
            members: Vec::new(),
            components: Vec::new(),
            ordered_members: Vec::new(),
            overlap_counts: HashMap::new(),
            peer_states: vec![PeerClusterState::default(); max_peers],
            cluster_records: HashMap::new(),
            assignment_by_wallet: HashMap::new(),
            pass_number: 0,
            next_cluster_number: 0,
        }
    }

    pub fn options(&self) -> &ClusterOptions {
        &self.options
    }

    pub fn pass_interval_ms(&self) -> u64 {
        self.options.pass_interval_ms
    }

    pub fn board(&self) -> &ClusterPass {
        self.board.current()
    }

    pub fn pass_number(&self) -> u64 {
        self.pass_number
    }

    /// One clustering pass, start to finish.
    pub fn run_pass(
        &mut self,
        grids: &RealmSpatialGrids,
        board: &SnapshotBoard,
        identity: &IdentityBoard,
    ) {
        let started = std::time::Instant::now();

        self.pass_number += 1;

        self.collect_and_union_realms(grids, board, identity);
        self.group_components();
        self.assign_sticky_ids();

        let pass = self.build_pass();

        self.remember_computed_assignments();
        self.board.publish(pass);
        self.publisher
            .publish_topology(self.board.current(), board.active_peers().len() as u32);

        let reassignments = self.publish_assignment_changes();
        self.forget_vanished_peers();
        self.forget_expired_sessions();

        self.record_pass_metrics(started, reassignments);
        self.warn_on_overrun(started);
    }

    /// The pass shares the server's tick task, so one that eats an appreciable slice of the pass
    /// interval is delaying peer simulation for every connected client. Warned at a tenth of the
    /// interval, which is where `pulse_cluster_pass_duration_us_total` stops being background cost.
    fn warn_on_overrun(&self, started: std::time::Instant) {
        let budget_us = (self.options.pass_interval_ms * 1_000) / OVERRUN_BUDGET_DIVISOR;
        let elapsed_us = started.elapsed().as_micros() as u64;
        if budget_us > 0 && elapsed_us > budget_us {
            tracing::warn!(
                elapsed_us,
                budget_us,
                peers = self.board.current().peers.len(),
                clusters = self.board.current().clusters.len(),
                "clustering pass overran its share of the tick"
            );
        }
    }

    fn record_pass_metrics(&self, started: std::time::Instant, reassignments: u64) {
        let mut peers = 0usize;
        let mut largest = 0usize;
        for component in &self.components {
            crate::metrics::cluster_size(component.member_count);
            peers += component.member_count;
            largest = largest.max(component.member_count);
        }
        crate::metrics::cluster_pass(
            started.elapsed().as_micros() as u64,
            reassignments,
            self.components.len(),
            peers,
            largest,
        );
    }

    /// Builds and unions the cell graph, realm by realm. Nodes and members accumulate across
    /// realms -- later steps work on the pass as a whole -- while the key index that neighbour
    /// probes consult holds one realm at a time, which is what confines every union to one realm.
    fn collect_and_union_realms(
        &mut self,
        grids: &RealmSpatialGrids,
        board: &SnapshotBoard,
        identity: &IdentityBoard,
    ) {
        self.nodes.clear();
        self.members.clear();
        self.pass_realms.clear();

        let mut realms: Vec<(&str, &SpatialGrid)> = grids.realm_grids().collect();
        realms.sort_unstable_by_key(|(realm, _)| *realm);

        for (realm, grid) in realms {
            self.node_index_by_key.clear();
            let realm_index = self.pass_realms.len();
            self.pass_realms.push(realm.to_string());
            let realm_first_node = self.nodes.len();
            self.collect_realm_nodes(realm_index, grid, board, identity);
            self.union_realm_neighbors(realm_first_node);
        }
    }

    /// Cells are visited in key order, and realms in name order, so one pass over one world state
    /// always produces the same components in the same sequence. Upstream leaves this to dictionary
    /// enumeration order; pinning it is what makes minting -- and therefore the creation-sequence
    /// tie-break between two clusters born in the same pass -- reproducible instead of a property
    /// of this process's hash seed. The partition itself is order-independent either way.
    fn collect_realm_nodes(
        &mut self,
        realm: usize,
        grid: &SpatialGrid,
        board: &SnapshotBoard,
        identity: &IdentityBoard,
    ) {
        self.pass_cell_keys.clear();
        self.pass_cell_keys
            .extend(grid.occupied_cells().map(|(key, _)| key));
        self.pass_cell_keys.sort_unstable();

        for index in 0..self.pass_cell_keys.len() {
            let key = self.pass_cell_keys[index];
            let first_member = self.members.len();
            for &peer in grid.peers_in_cell(key) {
                self.try_collect_member(peer, board, identity);
            }
            let member_count = self.members.len() - first_member;
            if member_count == 0 {
                continue;
            }
            self.add_node(realm, key, first_member, member_count);
        }
    }

    /// Resolves one occupant into a member, or skips it. A peer whose snapshot is unreadable,
    /// whose wallet is unknown, or which is no longer that wallet's live binding cannot be
    /// published as a cluster member: a binding goes stale when a duplicate-session eviction
    /// rebinds the wallet to a replacement peer while the outgoing one is still in the grid.
    /// A peer already collected this pass is skipped too, so every later step can assume a peer
    /// appears at most once. Collection, not publication, is what refreshes a wallet's retention
    /// clock: refreshing on publish would expire a stationary peer's entry, since it publishes
    /// once and never again.
    fn try_collect_member(&mut self, peer: u32, board: &SnapshotBoard, identity: &IdentityBoard) {
        if self.peer_states[peer as usize].last_seen_pass == self.pass_number {
            return;
        }
        let Some(snapshot) = board.try_read(peer) else {
            return;
        };
        let Some(wallet) = identity.wallet_by_peer(peer) else {
            return;
        };
        if identity.peer_by_wallet(wallet) != Some(peer) {
            return;
        }
        let session = identity.session_by_peer(peer).unwrap_or(wallet).to_string();

        self.peer_states[peer as usize].last_seen_pass = self.pass_number;

        if let Some(seen) = self.assignment_by_wallet.get_mut(&wallet.to_lowercase()) {
            seen.last_seen_pass = self.pass_number;
        }

        self.members.push(PassMember {
            peer,
            wallet: wallet.to_string(),
            session,
            position: snapshot.global_position,
            parcel: snapshot.parcel,
            is_teleport: snapshot.is_teleport,
        });
    }

    fn add_node(&mut self, realm: usize, cell_key: i64, member_start: usize, member_count: usize) {
        let index = self.nodes.len();
        self.node_index_by_key.insert(cell_key, index);
        self.nodes.push(PassNode {
            realm,
            cell_key,
            member_start,
            member_count,
            parent: index,
            tree_size: 1,
            component: NONE,
        });
    }

    /// Cell adjacency is the whole join test: two peers in adjacent cells are between 0 and
    /// `2 * cell_size * sqrt(2)` apart, and the resulting boundary noise is absorbed by the dwell
    /// debounce rather than by a second distance threshold.
    fn union_realm_neighbors(&mut self, first_node: usize) {
        for node in first_node..self.nodes.len() {
            let (x, z) = SpatialGrid::unpack_key(self.nodes[node].cell_key);
            for i in 0..NEIGHBOR_DX.len() {
                let key = SpatialGrid::pack_key(x + NEIGHBOR_DX[i], z + NEIGHBOR_DZ[i]);
                if let Some(&neighbor) = self.node_index_by_key.get(&key) {
                    union(&mut self.nodes, node, neighbor);
                }
            }
        }
    }

    /// Turns union-find roots into dense components, then lays every component's members out in
    /// one contiguous run so later steps walk them without a second indirection.
    fn group_components(&mut self) {
        self.components.clear();
        self.ordered_members.clear();

        for node in 0..self.nodes.len() {
            let root = find(&mut self.nodes, node);
            let mut component = self.nodes[root].component;
            if component == NONE {
                component = self.components.len();
                self.components.push(PassComponent {
                    realm: self.nodes[node].realm,
                    member_start: 0,
                    member_count: 0,
                    inherited_id: None,
                    inherited_overlap: 0,
                    id: String::new(),
                });
                self.nodes[root].component = component;
            }
            self.nodes[node].component = component;
            self.components[component].member_count += self.nodes[node].member_count;
        }

        let mut cursor = 0usize;
        for component in self.components.iter_mut() {
            component.member_start = cursor;
            cursor += component.member_count;
            component.member_count = 0;
        }
        self.ordered_members.resize(cursor, 0);
        for node in 0..self.nodes.len() {
            let component = self.nodes[node].component;
            let start = self.nodes[node].member_start;
            for offset in 0..self.nodes[node].member_count {
                let slot = self.components[component].member_start
                    + self.components[component].member_count;
                self.ordered_members[slot] = start + offset;
                self.components[component].member_count += 1;
            }
        }
    }

    fn member_range(&self, component: usize) -> std::ops::Range<usize> {
        let info = &self.components[component];
        info.member_start..info.member_start + info.member_count
    }

    /// Gives each component the ID of the previous cluster it shares the most members with, so a
    /// crowd that splits or merges keeps a stable identity across passes. Ties resolve to the
    /// older cluster; when two components claim the same ID the larger overlap keeps it and the
    /// other takes a fresh one.
    fn assign_sticky_ids(&mut self) {
        for component in 0..self.components.len() {
            self.find_best_inherited_id(component);
        }
        for component in 0..self.components.len() {
            self.resolve_inheritance_conflict(component);
        }
        for component in 0..self.components.len() {
            let id = match self.components[component].inherited_id.take() {
                Some(inherited) => inherited,
                None => self.mint_cluster_id(),
            };
            self.components[component].id = id;
        }
        self.prune_vanished_clusters();
    }

    /// Majority overlap with the previous pass's assignments, tie-broken to the older cluster.
    /// Only a still-registered ID can be inherited; anything else no longer exists.
    fn find_best_inherited_id(&mut self, component: usize) {
        self.overlap_counts.clear();
        for i in self.member_range(component) {
            let peer = self.members[self.ordered_members[i]].peer;
            let Some(previous) = self.peer_states[peer as usize]
                .previous_pass_cluster_id
                .clone()
            else {
                continue;
            };
            *self.overlap_counts.entry(previous).or_insert(0) += 1;
        }

        let mut best_id: Option<String> = None;
        let mut best_count = 0usize;
        let mut best_creation_seq = u64::MAX;
        for (cluster_id, count) in &self.overlap_counts {
            let Some(record) = self.cluster_records.get(cluster_id) else {
                continue;
            };
            if *count < best_count {
                continue;
            }
            if *count == best_count && record.creation_seq >= best_creation_seq {
                continue;
            }
            best_id = Some(cluster_id.clone());
            best_count = *count;
            best_creation_seq = record.creation_seq;
        }

        self.components[component].inherited_id = best_id;
        self.components[component].inherited_overlap = best_count;
    }

    /// Settles two components inheriting the same ID and marks the surviving claim live for this
    /// pass. The loser's inherited ID is cleared, which leaves it to be minted a fresh one. On an
    /// exact overlap tie the component discovered first keeps the ID.
    fn resolve_inheritance_conflict(&mut self, component: usize) {
        let Some(inherited) = self.components[component].inherited_id.clone() else {
            return;
        };
        let record = &self.cluster_records[&inherited];
        if record.last_live_pass != self.pass_number {
            self.claim_cluster_id(&inherited, component);
            return;
        }
        let claimant = record.claimant;
        if self.components[component].inherited_overlap
            > self.components[claimant].inherited_overlap
        {
            self.components[claimant].inherited_id = None;
            self.claim_cluster_id(&inherited, component);
        } else {
            self.components[component].inherited_id = None;
        }
    }

    fn claim_cluster_id(&mut self, cluster_id: &str, component: usize) {
        if let Some(record) = self.cluster_records.get_mut(cluster_id) {
            record.last_live_pass = self.pass_number;
            record.claimant = component;
        }
    }

    /// Registers the new ID as live for this pass immediately: inheritance only reads the previous
    /// pass's assignments, so a freshly minted ID cannot be contested within its own pass.
    fn mint_cluster_id(&mut self) -> String {
        self.next_cluster_number += 1;
        let id = format!("{}{}", self.options.id_prefix, self.next_cluster_number);
        self.cluster_records.insert(
            id.clone(),
            ClusterRecord {
                creation_seq: self.next_cluster_number,
                last_live_pass: self.pass_number,
                claimant: NONE,
            },
        );
        id
    }

    /// Drops bookkeeping for clusters that no longer exist, so the registry cannot grow without
    /// bound. Every component holds exactly one distinct ID, so matching counts mean nothing
    /// vanished.
    fn prune_vanished_clusters(&mut self) {
        if self.cluster_records.len() == self.components.len() {
            return;
        }
        let pass = self.pass_number;
        self.cluster_records
            .retain(|_, record| record.last_live_pass == pass);
    }

    fn build_pass(&mut self) -> ClusterPass {
        let mut clusters = Vec::with_capacity(self.components.len());
        let mut peers = Vec::with_capacity(self.members.len());
        let mut cluster_id_by_peer = vec![None; self.peer_states.len()];

        for component in 0..self.components.len() {
            let id = self.components[component].id.clone();
            let realm = self.pass_realms[self.components[component].realm].clone();
            let range = self.member_range(component);
            let count = range.len();

            let mut sum = Vector3::default();
            for i in range.clone() {
                let position = self.members[self.ordered_members[i]].position;
                sum.x += position.x;
                sum.y += position.y;
                sum.z += position.z;
            }
            let centroid = Vector3 {
                x: sum.x / count as f32,
                y: sum.y / count as f32,
                z: sum.z / count as f32,
            };

            let mut radius_squared = 0f32;
            for i in range {
                let member = &self.members[self.ordered_members[i]];
                let dx = member.position.x - centroid.x;
                let dz = member.position.z - centroid.z;
                radius_squared = radius_squared.max(dx * dx + dz * dz);
                peers.push(ClusterPeerInfo {
                    peer: member.peer,
                    wallet: member.wallet.clone(),
                    cluster_id: id.clone(),
                    realm: realm.clone(),
                    position: member.position,
                    parcel: member.parcel,
                });
                cluster_id_by_peer[member.peer as usize] = Some(id.clone());
            }

            clusters.push(ClusterInfo {
                id,
                realm,
                count,
                centroid,
                radius: radius_squared.sqrt(),
            });
        }

        ClusterPass {
            clusters,
            peers,
            cluster_id_by_peer,
        }
    }

    /// Records what this pass computed, so the next pass can measure cluster-identity overlap
    /// against it. Kept separate from the published assignment: a fragment mid-debounce must still
    /// inherit its own ID rather than be minted a new one every pass.
    fn remember_computed_assignments(&mut self) {
        for component in 0..self.components.len() {
            let id = self.components[component].id.clone();
            for i in self.member_range(component) {
                let peer = self.members[self.ordered_members[i]].peer;
                self.peer_states[peer as usize].previous_pass_cluster_id = Some(id.clone());
            }
        }
    }

    fn publish_assignment_changes(&mut self) -> u64 {
        let mut reassignments = 0;
        for component in 0..self.components.len() {
            let id = self.components[component].id.clone();
            let realm = self.pass_realms[self.components[component].realm].clone();
            for i in self.member_range(component) {
                let member = self.ordered_members[i];
                if self.try_publish_assignment(member, &id, &realm) {
                    reassignments += 1;
                }
            }
        }
        reassignments
    }

    /// Emits a feed event for one peer if its assignment -- cluster and realm together -- differs
    /// from the last one published, and either the change is exempt from the debounce or the peer
    /// has dwelled long enough. The debounce is bypassed on first assignment, teleport, realm
    /// change and deletion of the peer's previous cluster: cases where the published assignment is
    /// already known to be wrong, so waiting would only prolong it.
    fn try_publish_assignment(&mut self, member: usize, cluster_id: &str, realm: &str) -> bool {
        let peer = self.members[member].peer as usize;
        let realm_changed = self.peer_states[peer].published_realm.as_deref() != Some(realm);

        if !realm_changed
            && self.peer_states[peer].published_cluster_id.as_deref() == Some(cluster_id)
        {
            self.peer_states[peer].candidate_cluster_id = None;
            self.peer_states[peer].candidate_streak = 0;
            return false;
        }

        let previously_published = self.peer_states[peer].published_cluster_id.clone();
        let immediate = previously_published.is_none()
            || self.members[member].is_teleport
            || realm_changed
            || !self.is_cluster_live(previously_published.as_deref().unwrap_or_default());

        if !immediate && !self.has_dwelled(peer, cluster_id) {
            return false;
        }

        self.peer_states[peer].published_cluster_id = Some(cluster_id.to_string());
        self.peer_states[peer].published_realm = Some(realm.to_string());
        self.peer_states[peer].candidate_cluster_id = None;
        self.peer_states[peer].candidate_streak = 0;

        let session = self.session_for(member);

        let wallet = self.members[member].wallet.clone();
        self.assignment_by_wallet.insert(
            wallet.to_lowercase(),
            WalletAssignment {
                cluster_id: cluster_id.to_string(),
                session: self.members[member].session.clone(),
                last_seen_pass: self.pass_number,
            },
        );

        self.publisher
            .publish_cluster_change(&wallet, cluster_id, realm, &session);

        if session.is_takeover() {
            crate::metrics::cluster_takeover();
        }

        true
    }

    /// Names the session this publish belongs to and, when the wallet's retained assignment was
    /// published by a different session, that session and the cluster it was last published into.
    /// A same-session reconnect names nothing: only the session key tells one device's return from
    /// another device's arrival. Deliberately not gated on the displaced cluster still existing --
    /// a peer that was alone took its cluster with it, while the room it was in outlives it.
    fn session_for(&self, member: usize) -> ClusterSession {
        let session = self.members[member].session.clone();
        if self.options.session_retention_passes == 0 {
            return ClusterSession::new(session);
        }
        let retained = self
            .assignment_by_wallet
            .get(&self.members[member].wallet.to_lowercase());
        match retained {
            Some(retained) if !retained.session.eq_ignore_ascii_case(&session) => ClusterSession {
                session,
                displaced_session: Some(retained.session.clone()),
                displaced_cluster_id: Some(retained.cluster_id.clone()),
            },
            _ => ClusterSession::new(session),
        }
    }

    fn is_cluster_live(&self, cluster_id: &str) -> bool {
        self.cluster_records
            .get(cluster_id)
            .is_some_and(|record| record.last_live_pass == self.pass_number)
    }

    fn has_dwelled(&mut self, peer: usize, cluster_id: &str) -> bool {
        if self.peer_states[peer].candidate_cluster_id.as_deref() == Some(cluster_id) {
            self.peer_states[peer].candidate_streak += 1;
        } else {
            self.peer_states[peer].candidate_cluster_id = Some(cluster_id.to_string());
            self.peer_states[peer].candidate_streak = 1;
        }
        self.peer_states[peer].candidate_streak >= self.options.dwell_passes
    }

    /// Clears carried-over state for peers absent from this pass. Mandatory rather than tidy: a
    /// peer slot is recycled, so state left behind would be inherited by the next wallet on that
    /// slot and make its first assignment look unchanged.
    fn forget_vanished_peers(&mut self) {
        let pass = self.pass_number;
        for state in self.peer_states.iter_mut() {
            if state.last_seen_pass == pass || state.last_seen_pass == 0 {
                continue;
            }
            *state = PeerClusterState::default();
        }
    }

    /// Drops retained assignments for wallets absent for more than `session_retention_passes`,
    /// which also bounds the map to concurrent wallets plus recently departed ones. Past the
    /// window there is no participant left to name as displaced.
    fn forget_expired_sessions(&mut self) {
        let pass = self.pass_number;
        let retention = self.options.session_retention_passes;
        self.assignment_by_wallet
            .retain(|_, assignment| pass - assignment.last_seen_pass <= retention);
    }
}

/// Path halving: compresses the path without the second traversal full path compression needs.
fn find(nodes: &mut [PassNode], mut node: usize) -> usize {
    while nodes[node].parent != node {
        let grandparent = nodes[nodes[node].parent].parent;
        nodes[node].parent = grandparent;
        node = grandparent;
    }
    node
}

fn union(nodes: &mut [PassNode], a: usize, b: usize) {
    let mut root_a = find(nodes, a);
    let mut root_b = find(nodes, b);
    if root_a == root_b {
        return;
    }
    if nodes[root_a].tree_size < nodes[root_b].tree_size {
        std::mem::swap(&mut root_a, &mut root_b);
    }
    nodes[root_b].parent = root_a;
    nodes[root_a].tree_size += nodes[root_b].tree_size;
}

impl crate::server::PulseServer {
    /// One clustering pass against the server's own boards, or nothing when clustering is off.
    /// Driven by the server loop's own timer, never by the per-packet path.
    pub fn run_cluster_pass(&mut self) {
        let Some(tracker) = self.clusters.as_mut() else {
            return;
        };
        tracker.run_pass(&self.grids, &self.board, &self.identity);
    }
}
