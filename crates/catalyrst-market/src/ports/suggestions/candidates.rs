//! Which ids the blend is allowed to choose between, and in what order the two arms claim them.

use std::collections::{HashMap, HashSet};

use super::constants::CANDIDATE_MULTIPLIER;
use super::profile::ProfileSource;

/// How much wider than the scored head the neighbour pool is allowed to be.
///
/// The neighbour table can reach twenty thousand ids from a two-hundred-item profile, and every
/// one of them is an element of a bound text array against the item-unified core. The blend only
/// ever keeps `first * CANDIDATE_MULTIPLIER`, so the pool exists to give the re-rank something to
/// choose between -- not to hand the core the transitive closure of the wallet's taste.
pub(super) const CANDIDATE_POOL_MULTIPLIER: usize = 8;
/// How far past the pool cut the ownership probe is allowed to reach.
///
/// The exclusions have to run BEFORE the cut or a wallet whose strongest neighbours are mostly its
/// own holdings reaches the blend with a short pool -- but the probe binds its ids as one array,
/// and the neighbour table can reach twenty thousand of them from a two-hundred-item profile, so
/// the set asked about cannot be all of them. Four times the cut is headroom enough for the pool to
/// fill past a head of holdings, and a bound the query can carry.
pub(super) const OWNERSHIP_PROBE_MULTIPLIER: usize = 4;

#[derive(Debug, Default, Clone)]
pub struct NeighborScore {
    pub cf: f64,
    pub content: f64,
    pub best: f64,
    pub trigger_item: Option<String>,
    pub trigger_source: Option<ProfileSource>,
}

/// The two arms the blend draws from, each claiming its ids on its own terms.
pub(super) struct CandidatePools {
    pub neighbours: Vec<String>,
    pub taste: Vec<String>,
}

pub(super) fn pool_cut(limit: usize) -> usize {
    limit * CANDIDATE_MULTIPLIER as usize * CANDIDATE_POOL_MULTIPLIER
}

/// The candidate arms, built in the order upstream's UNION builds them.
///
/// Upstream is two independently limited branches: the neighbour branch is cut at the blend's head,
/// the creator-affinity branch is bounded only by its per-creator rank. A creator drop nobody owns
/// yet carries no neighbour score at all, so the taste arm is the only way it can reach the blend
/// -- which means the taste arm has to claim an id BEFORE the neighbour walk does. The walk reaches
/// four times past the cut for the ownership probe and everything it claims out there is thrown
/// away by the truncate, so an id the walk took at rank 400 would reach the blend through neither.
pub(super) fn build_candidate_pools(
    neighbors: &HashMap<String, NeighborScore>,
    taste_sources: impl Iterator<Item = String>,
    excluded: &HashSet<String>,
    probe_limit: usize,
) -> CandidatePools {
    let mut seen: HashSet<String> = HashSet::new();
    let mut taste: Vec<String> = Vec::new();
    for id in taste_sources {
        if excluded.contains(&id) || !seen.insert(id.clone()) {
            continue;
        }
        taste.push(id);
    }
    let neighbours = neighbour_pool(neighbors, excluded, &mut seen, probe_limit);
    CandidatePools { neighbours, taste }
}

/// The pool hydration is asked for: the neighbour arm cut to the head the re-rank can use, then the
/// taste arm appended whole.
pub(super) fn finish_candidates(
    mut neighbours: Vec<String>,
    taste: Vec<String>,
    cut: usize,
) -> Vec<String> {
    neighbours.truncate(cut);
    neighbours.extend(taste);
    neighbours
}

/// The neighbours in order, strongest pull first. The cut belongs to `neighbour_pool`, which takes
/// it after the exclusions rather than before them.
fn ranked_neighbor_ids(neighbors: &HashMap<String, NeighborScore>) -> Vec<String> {
    let mut ordered: Vec<(&String, f64)> = neighbors
        .iter()
        .map(|(id, score)| (id, score.cf + score.content))
        .collect();
    ordered.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    ordered.into_iter().map(|(id, _)| id.clone()).collect()
}

/// The neighbour candidates the ownership probe is asked about: strongest pull first, the caller's
/// exclusions and the profile's own ids dropped BEFORE the cut.
///
/// Upstream applies both predicates inside the scored query and limits after them, so its head is
/// always full. Cutting first would shorten the pool by exactly the rows a wallet's own shelf fills
/// it with, and a short pool is what trips the personal-row floor and serves trending instead.
fn neighbour_pool(
    neighbors: &HashMap<String, NeighborScore>,
    excluded: &HashSet<String>,
    seen: &mut HashSet<String>,
    probe_limit: usize,
) -> Vec<String> {
    let mut pool: Vec<String> = Vec::new();
    for id in ranked_neighbor_ids(neighbors) {
        if excluded.contains(&id) || !seen.insert(id.clone()) {
            continue;
        }
        pool.push(id);
        if pool.len() >= probe_limit {
            break;
        }
    }
    pool
}

/// The collections the candidates live in, so the item-unified core is built narrow rather than
/// built over the whole catalogue and then filtered by the id array.
pub(super) fn collections_of(candidate_ids: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for id in candidate_ids {
        if let Some(contract) = id.split('-').next() {
            if !contract.is_empty() && seen.insert(contract) {
                out.push(contract.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neighbors_of(count: i64) -> HashMap<String, NeighborScore> {
        (0..count)
            .map(|i| {
                (
                    format!("0xc-{i}"),
                    NeighborScore {
                        cf: i as f64,
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    const POOL_CUT: usize = 12 * CANDIDATE_MULTIPLIER as usize * CANDIDATE_POOL_MULTIPLIER;

    /// The neighbour table can reach twenty thousand ids from one profile; the core is handed a
    /// pool the re-rank can actually use, strongest pull first.
    #[test]
    fn the_neighbour_pool_is_cut_to_what_the_rerank_can_use() {
        let neighbors = neighbors_of(5_000);
        assert_eq!(ranked_neighbor_ids(&neighbors).len(), 5_000);
        let mut seen: HashSet<String> = HashSet::new();
        let pool = neighbour_pool(&neighbors, &HashSet::new(), &mut seen, POOL_CUT);
        assert_eq!(pool.len(), POOL_CUT);
        assert_eq!(pool[0], "0xc-4999");
    }

    /// The exclusions run BEFORE the cut. A wallet whose strongest neighbours are mostly things it
    /// already holds still reaches the blend with a full pool, rather than with the handful the cut
    /// left once the holdings were dropped out of it.
    #[test]
    fn an_excluded_head_does_not_shrink_the_neighbour_pool() {
        let neighbors = neighbors_of(5_000);
        let excluded: HashSet<String> = (1_000..5_000).map(|i| format!("0xc-{i}")).collect();
        let mut seen: HashSet<String> = HashSet::new();
        let pool = neighbour_pool(&neighbors, &excluded, &mut seen, POOL_CUT);
        assert_eq!(pool.len(), POOL_CUT);
        assert_eq!(pool[0], "0xc-999");
        assert!(pool.iter().all(|id| !excluded.contains(id)));
    }

    /// Upstream's two branches are limited independently and ours must be too. A creator-affinity
    /// id sitting at neighbour rank 338 is inside the probe's reach but outside the cut, so if the
    /// walk claims it first it reaches the blend through neither arm.
    #[test]
    fn a_taste_id_deep_in_the_neighbour_walk_still_reaches_the_blend() {
        let neighbors = neighbors_of(5_000);
        let deep = format!("0xc-{}", 5_000 - (POOL_CUT + 50));
        let pools = build_candidate_pools(
            &neighbors,
            std::iter::once(deep.clone()),
            &HashSet::new(),
            POOL_CUT * OWNERSHIP_PROBE_MULTIPLIER,
        );
        assert!(
            !pools.neighbours.contains(&deep),
            "the taste arm claims the id first"
        );
        assert_eq!(pools.taste, vec![deep.clone()]);
        let finished = finish_candidates(pools.neighbours, pools.taste, POOL_CUT);
        assert!(finished.contains(&deep), "and the truncate cannot drop it");
        assert_eq!(finished.len(), POOL_CUT + 1);
    }

    /// The taste arm claims first, so an id that is also a strong neighbour is not hydrated twice.
    #[test]
    fn a_taste_id_at_the_head_of_the_walk_is_not_claimed_twice() {
        let neighbors = neighbors_of(5_000);
        let head = "0xc-4999".to_string();
        let pools = build_candidate_pools(
            &neighbors,
            std::iter::once(head.clone()),
            &HashSet::new(),
            POOL_CUT * OWNERSHIP_PROBE_MULTIPLIER,
        );
        let finished = finish_candidates(pools.neighbours, pools.taste, POOL_CUT);
        assert_eq!(finished.iter().filter(|id| **id == head).count(), 1);
    }

    /// An excluded id is claimed by neither arm, whichever one reaches it first.
    #[test]
    fn an_excluded_taste_id_is_dropped_by_the_taste_arm_too() {
        let neighbors = neighbors_of(64);
        let excluded: HashSet<String> = ["0xc-63".to_string()].into_iter().collect();
        let pools = build_candidate_pools(
            &neighbors,
            std::iter::once("0xc-63".to_string()),
            &excluded,
            POOL_CUT,
        );
        assert!(pools.taste.is_empty());
        assert!(!pools.neighbours.contains(&"0xc-63".to_string()));
    }

    #[test]
    fn the_candidate_collections_are_the_contracts_deduped() {
        let ids = vec![
            "0xaaa-1".to_string(),
            "0xaaa-2".to_string(),
            "0xbbb-7".to_string(),
        ];
        assert_eq!(
            collections_of(&ids),
            vec!["0xaaa".to_string(), "0xbbb".to_string()]
        );
    }
}
