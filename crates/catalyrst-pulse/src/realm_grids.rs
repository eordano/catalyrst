//! One `SpatialGrid` per realm, and the router that keeps each peer in exactly one of them.
//!
//! Realm isolation is structural rather than a per-candidate predicate: a grid holds one realm's
//! peers and nothing else, so neither interest management nor cluster derivation ever compares
//! realms. Cell math stays realm-independent, so a caller that only needs to name cells (the
//! scene-listener cover) needs no grid at all.
//!
//! Per-peer bookkeeping lives here rather than inside each grid. Realm names arrive from clients,
//! so a per-grid array indexed by peer slot would let a peer mint an arbitrary number of full-size
//! arrays by teleporting to fresh names; here the cost is one pair of vectors regardless of realm
//! count, and a grid is dropped once its last occupant leaves.

use std::collections::HashMap;

use crate::decentraland::common::Vector3;
use crate::interest::SpatialGrid;

pub struct RealmSpatialGrids {
    cell_size: f32,
    inverse_cell_size: f32,
    grids_by_realm: HashMap<String, SpatialGrid>,
    peer_realms: Vec<Option<String>>,
}

impl RealmSpatialGrids {
    pub fn new(cell_size: f32, max_peers: usize) -> Self {
        Self {
            cell_size,
            inverse_cell_size: 1.0 / cell_size,
            grids_by_realm: HashMap::new(),
            peer_realms: vec![None; max_peers],
        }
    }

    /// Places a peer at a position within a realm, moving it out of its previous realm's grid if
    /// it had one. Creates the realm's grid on its first occupant.
    pub fn set(&mut self, peer: u32, realm: &str, position: Vector3) {
        let slot = peer as usize;
        if self.peer_realms[slot].as_deref() != Some(realm) {
            self.remove(peer);
            self.peer_realms[slot] = Some(realm.to_string());
        }
        self.grids_by_realm
            .entry(realm.to_string())
            .or_insert_with(|| SpatialGrid::new(self.cell_size))
            .set(peer, position);
    }

    /// Removes a peer from whichever realm and cell it occupies. Idempotent.
    pub fn remove(&mut self, peer: u32) {
        let slot = peer as usize;
        let Some(realm) = self.peer_realms[slot].take() else {
            return;
        };
        let Some(grid) = self.grids_by_realm.get_mut(&realm) else {
            return;
        };
        grid.remove(peer);
        if grid.is_empty() {
            self.grids_by_realm.remove(&realm);
        }
    }

    /// The grid holding `realm`'s peers, or `None` when no peer occupies it -- which is also the
    /// answer for a peer with no realm, so an unplaced peer and an empty realm resolve alike.
    pub fn grid(&self, realm: Option<&str>) -> Option<&SpatialGrid> {
        self.grids_by_realm.get(realm?)
    }

    /// The realms that currently hold peers, with their grids: the cluster pass's entry point.
    pub fn realm_grids(&self) -> impl Iterator<Item = (&str, &SpatialGrid)> {
        self.grids_by_realm.iter().map(|(r, g)| (r.as_str(), g))
    }

    pub fn realm_of(&self, peer: u32) -> Option<&str> {
        self.peer_realms[peer as usize].as_deref()
    }

    pub fn realm_count(&self) -> usize {
        self.grids_by_realm.len()
    }

    /// Realm-independent, because cell size is global: a scene listener's cover is one set of keys
    /// probed against each announced realm's own grid.
    pub fn inverse_cell_size(&self) -> f32 {
        self.inverse_cell_size
    }

    pub fn cell_coord(&self, v: f32) -> i32 {
        (v * self.inverse_cell_size).floor() as i32
    }

    pub fn cell_key(&self, x: f32, z: f32) -> i64 {
        SpatialGrid::pack_key(self.cell_coord(x), self.cell_coord(z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interest::SPATIAL_GRID_CELL_SIZE;

    fn v3(x: f32, z: f32) -> Vector3 {
        Vector3 { x, y: 0.0, z }
    }

    fn grids() -> RealmSpatialGrids {
        RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, 16)
    }

    #[test]
    fn peers_of_two_realms_never_share_a_grid() {
        let mut g = grids();
        g.set(1, "a", v3(0.0, 0.0));
        g.set(2, "b", v3(0.0, 0.0));
        let key = g.cell_key(0.0, 0.0);
        assert_eq!(g.grid(Some("a")).unwrap().peers_in_cell(key), &[1]);
        assert_eq!(g.grid(Some("b")).unwrap().peers_in_cell(key), &[2]);
    }

    #[test]
    fn an_empty_realm_and_an_unplaced_peer_resolve_alike() {
        let mut g = grids();
        g.set(1, "a", v3(0.0, 0.0));
        g.remove(1);
        assert!(g.grid(Some("a")).is_none());
        assert!(g.grid(None).is_none());
        assert_eq!(g.realm_count(), 0);
    }

    #[test]
    fn changing_realm_vacates_the_previous_grid() {
        let mut g = grids();
        g.set(1, "a", v3(0.0, 0.0));
        g.set(1, "b", v3(0.0, 0.0));
        assert!(g.grid(Some("a")).is_none());
        assert_eq!(
            g.grid(Some("b"))
                .unwrap()
                .peers_in_cell(g.cell_key(0.0, 0.0)),
            &[1]
        );
        assert_eq!(g.realm_of(1), Some("b"));
    }

    #[test]
    fn moving_within_a_realm_keeps_the_grid_alive() {
        let mut g = grids();
        g.set(1, "a", v3(0.0, 0.0));
        g.set(1, "a", v3(450.0, 450.0));
        assert_eq!(g.realm_count(), 1);
        assert!(g
            .grid(Some("a"))
            .unwrap()
            .peers_in_cell(g.cell_key(0.0, 0.0))
            .is_empty());
        assert_eq!(
            g.grid(Some("a"))
                .unwrap()
                .peers_in_cell(g.cell_key(450.0, 450.0)),
            &[1]
        );
    }

    #[test]
    fn remove_is_idempotent_and_drops_only_the_last_occupant_s_grid() {
        let mut g = grids();
        g.set(1, "a", v3(0.0, 0.0));
        g.set(2, "a", v3(10.0, 10.0));
        g.remove(1);
        g.remove(1);
        assert_eq!(g.realm_count(), 1);
        g.remove(2);
        assert_eq!(g.realm_count(), 0);
        assert_eq!(g.realm_of(2), None);
    }

    #[test]
    fn cell_math_is_realm_independent() {
        let mut g = grids();
        g.set(1, "a", v3(150.0, 250.0));
        assert_eq!(g.cell_coord(150.0), 1);
        assert_eq!(g.cell_key(150.0, 250.0), SpatialGrid::pack_key(1, 2));
        assert_eq!(
            g.grid(Some("a"))
                .unwrap()
                .peers_in_cell(SpatialGrid::pack_key(1, 2)),
            &[1]
        );
    }
}
