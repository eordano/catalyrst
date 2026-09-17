use std::collections::{HashMap, HashSet};

use super::co_ownership::NeighborRow;
use super::constants::{CONTENT_WEIGHTS as W, MAX_TAG_DOCUMENT_FREQUENCY, NEIGHBORS_PER_ITEM};

#[derive(Debug, Clone)]
pub struct ContentItem {
    pub index: usize,
    pub creator: String,
    pub collection: String,
    /// Wearable or emote sub-category, prefixed with the kind so a wearable "hat" and an emote
    /// category can never collide. Empty when the item declares neither.
    pub sub_category: String,
    pub rarity_tier: i32,
    pub price_band: i32,
    pub is_candidate: bool,
    pub tags: Vec<u32>,
    /// IDF weights, already L2-normalised, aligned with `tags`.
    pub tag_weights: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct ContentOptions {
    pub neighbors_per_item: usize,
    pub max_tag_document_frequency: usize,
}

impl Default for ContentOptions {
    fn default() -> Self {
        Self {
            neighbors_per_item: NEIGHBORS_PER_ITEM,
            max_tag_document_frequency: MAX_TAG_DOCUMENT_FREQUENCY,
        }
    }
}

struct InvertedIndex {
    by_creator: HashMap<String, Vec<usize>>,
    by_collection: HashMap<String, Vec<usize>>,
    by_sub_category: HashMap<String, Vec<usize>>,
    /// tag -> (candidate index, that candidate's IDF weight for the tag). Carrying the weight in
    /// the posting is what keeps the cosine a single pass instead of a lookup per pair.
    postings: HashMap<u32, Vec<(usize, f64)>>,
}

fn build_index(items: &[ContentItem], max_tag_document_frequency: usize) -> InvertedIndex {
    let mut by_creator: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_collection: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_sub_category: HashMap<String, Vec<usize>> = HashMap::new();
    let mut postings: HashMap<u32, Vec<(usize, f64)>> = HashMap::new();

    for item in items {
        if !item.is_candidate {
            continue;
        }
        for (key, map) in [
            (&item.creator, &mut by_creator),
            (&item.collection, &mut by_collection),
            (&item.sub_category, &mut by_sub_category),
        ] {
            if !key.is_empty() {
                map.entry(key.clone()).or_default().push(item.index);
            }
        }
        for (i, tag) in item.tags.iter().enumerate() {
            postings
                .entry(*tag)
                .or_default()
                .push((item.index, item.tag_weights[i] as f64));
        }
    }

    // A tag carried by thousands of items says nothing about any of them, and its posting list is
    // what makes this pass expensive. Dropping it is the IDF decision made structural.
    postings.retain(|_, list| list.len() <= max_tag_document_frequency);

    InvertedIndex {
        by_creator,
        by_collection,
        by_sub_category,
        postings,
    }
}

/// Content neighbours for one anchor at a time, so the ~600k-row set never exists all at once.
///
/// The pool is the union of the creator, collection, sub-category and shared-tag buckets.
/// Rarity and price band are scored but do NOT admit anyone: a rarity tier holds tens of
/// thousands of items, so admitting on it would make every pool the whole catalogue and the
/// resulting top-50 arbitrary. They are tie-breakers between candidates something real already
/// connected to the anchor.
///
/// Non-candidate items still appear as ANCHORS -- the profile is full of things the wallet owns
/// and nobody is selling -- but never as neighbours, where a row that can never be recommended is
/// dead weight in the table.
pub fn content_neighbors_by_anchor<F>(items: &[ContentItem], options: ContentOptions, mut emit: F)
where
    F: FnMut(Vec<NeighborRow>),
{
    let k = options.neighbors_per_item.max(1);
    let index = build_index(items, options.max_tag_document_frequency);

    let mut tag_dot = vec![0.0f64; items.len()];
    let mut touched: Vec<usize> = Vec::new();
    let mut pool: HashSet<usize> = HashSet::new();

    for anchor in items {
        for (i, tag) in anchor.tags.iter().enumerate() {
            let Some(list) = index.postings.get(tag) else {
                continue;
            };
            let anchor_weight = anchor.tag_weights[i] as f64;
            for (candidate, candidate_weight) in list {
                if tag_dot[*candidate] == 0.0 {
                    touched.push(*candidate);
                }
                tag_dot[*candidate] += anchor_weight * candidate_weight;
            }
        }

        pool.clear();
        pool.extend(touched.iter().copied());
        for list in [
            index.by_creator.get(&anchor.creator),
            index.by_collection.get(&anchor.collection),
            index.by_sub_category.get(&anchor.sub_category),
        ]
        .into_iter()
        .flatten()
        {
            pool.extend(list.iter().copied());
        }
        pool.remove(&anchor.index);

        let mut scored: Vec<NeighborRow> = Vec::new();
        for candidate_index in pool.iter().copied() {
            let candidate = &items[candidate_index];
            if !candidate.is_candidate {
                continue;
            }
            let mut sim = 0.0;
            if !anchor.creator.is_empty() && anchor.creator == candidate.creator {
                sim += W.creator;
            }
            if !anchor.collection.is_empty() && anchor.collection == candidate.collection {
                sim += W.collection;
            }
            if !anchor.sub_category.is_empty() && anchor.sub_category == candidate.sub_category {
                sim += W.sub_category;
            }
            if anchor.rarity_tier >= 0
                && candidate.rarity_tier >= 0
                && (anchor.rarity_tier - candidate.rarity_tier).abs() <= 1
            {
                sim += W.rarity;
            }
            let dot = tag_dot[candidate_index];
            if dot > 0.0 {
                sim += W.tags * dot.min(1.0);
            }
            if anchor.price_band >= 0 && anchor.price_band == candidate.price_band {
                sim += W.price_band;
            }
            if sim > 0.0 {
                scored.push(NeighborRow {
                    item: anchor.index,
                    neighbor: candidate_index,
                    sim,
                    support: 0,
                });
            }
        }

        for candidate in touched.drain(..) {
            tag_dot[candidate] = 0.0;
        }

        scored.sort_by(|a, b| {
            b.sim
                .partial_cmp(&a.sim)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.neighbor.cmp(&b.neighbor))
        });
        scored.truncate(k);
        if !scored.is_empty() {
            emit(scored);
        }
    }
}

/// Price quartile within the item's OWN sub-category, so "expensive" means expensive for a hat.
pub fn assign_price_bands(entries: &[(usize, String, f64)]) -> HashMap<usize, i32> {
    let mut bands: HashMap<usize, i32> = HashMap::new();
    let mut buckets: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
    for (index, sub_category, price) in entries {
        if *price <= 0.0 {
            continue;
        }
        buckets
            .entry(sub_category.clone())
            .or_default()
            .push((*index, *price));
    }
    for bucket in buckets.values() {
        let mut sorted = bucket.clone();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let cuts: Vec<f64> = [0.25, 0.5, 0.75]
            .iter()
            .map(|q| sorted[(q * (sorted.len() - 1) as f64).floor() as usize].1)
            .collect();
        for (index, price) in bucket {
            let band = if *price <= cuts[0] {
                0
            } else if *price <= cuts[1] {
                1
            } else if *price <= cuts[2] {
                2
            } else {
                3
            };
            bands.insert(*index, band);
        }
    }
    bands
}

pub struct TagVector {
    pub tags: Vec<u32>,
    pub weights: Vec<f32>,
}

/// IDF over the corpus, L2-normalised per item so the dot product above is a cosine.
pub fn build_tag_vectors(
    tags_by_item: &HashMap<usize, Vec<u32>>,
    document_frequency: &[usize],
    corpus_size: usize,
) -> HashMap<usize, TagVector> {
    let mut vectors: HashMap<usize, TagVector> = HashMap::new();
    for (item, tags) in tags_by_item {
        let weights: Vec<f64> = tags
            .iter()
            .map(|tag| {
                let df = document_frequency.get(*tag as usize).copied().unwrap_or(0);
                (1.0 + corpus_size as f64 / (1.0 + df as f64)).ln()
            })
            .collect();
        let sum_of_squares: f64 = weights.iter().map(|w| w * w).sum();
        let norm = {
            let n = sum_of_squares.sqrt();
            if n == 0.0 {
                1.0
            } else {
                n
            }
        };
        vectors.insert(
            *item,
            TagVector {
                tags: tags.clone(),
                weights: weights.iter().map(|w| (w / norm) as f32).collect(),
            },
        );
    }
    vectors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: usize, creator: &str, collection: &str, sub_category: &str) -> ContentItem {
        ContentItem {
            index,
            creator: creator.to_string(),
            collection: collection.to_string(),
            sub_category: sub_category.to_string(),
            rarity_tier: -1,
            price_band: -1,
            is_candidate: true,
            tags: Vec::new(),
            tag_weights: Vec::new(),
        }
    }

    fn rows_for(items: &[ContentItem], options: ContentOptions) -> Vec<NeighborRow> {
        let mut out = Vec::new();
        content_neighbors_by_anchor(items, options, |rows| out.extend(rows));
        out
    }

    #[test]
    fn a_shared_creator_outweighs_a_shared_collection() {
        let items = vec![
            item(0, "0xa", "0xc1", "wearable:hat"),
            item(1, "0xa", "0xc2", "emote:"),
            item(2, "0xb", "0xc1", "emote:"),
        ];
        let rows = rows_for(&items, ContentOptions::default());

        let creator_match = rows
            .iter()
            .find(|r| r.item == 0 && r.neighbor == 1)
            .expect("creator neighbour");
        let collection_match = rows
            .iter()
            .find(|r| r.item == 0 && r.neighbor == 2)
            .expect("collection neighbour");
        assert!(creator_match.sim > collection_match.sim);
        assert!((creator_match.sim - W.creator).abs() < 1e-12);
    }

    /// A row whose NEIGHBOUR can never be recommended is dead weight in the table. The same item
    /// is still a valid ANCHOR: the wallet's profile is mostly things nobody is selling, and
    /// dropping those anchors would leave exactly those wallets with no rail.
    #[test]
    fn a_non_candidate_is_never_a_neighbour_but_is_still_an_anchor() {
        let mut items = vec![item(0, "0xa", "0xc1", ""), item(1, "0xa", "0xc1", "")];
        items[1].is_candidate = false;
        let rows = rows_for(&items, ContentOptions::default());

        assert!(rows.iter().all(|r| r.neighbor != 1), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.item == 1 && r.neighbor == 0),
            "{rows:?}"
        );
    }

    /// Rarity is a TIE-BREAKER inside the pool, never a reason to enter it: a tier holds tens of
    /// thousands of items, so admitting them on rarity alone would make the pool the catalogue
    /// and the top-50 arbitrary. Both candidates here are already pooled by creator.
    #[test]
    fn adjacent_rarity_tiers_score_and_distant_ones_do_not() {
        let mut items = vec![
            item(0, "0xa", "", ""),
            item(1, "0xa", "", ""),
            item(2, "0xa", "", ""),
        ];
        items[0].rarity_tier = 4;
        items[1].rarity_tier = 5;
        items[2].rarity_tier = 7;
        let rows = rows_for(&items, ContentOptions::default());

        let adjacent = rows
            .iter()
            .find(|r| r.item == 0 && r.neighbor == 1)
            .expect("an adjacent-tier neighbour");
        let distant = rows
            .iter()
            .find(|r| r.item == 0 && r.neighbor == 2)
            .expect("a distant-tier neighbour");

        assert!((adjacent.sim - (W.creator + W.rarity)).abs() < 1e-12);
        assert!((distant.sim - W.creator).abs() < 1e-12);
    }

    /// An item nobody shares an attribute with never enters any anchor's pool, so it costs
    /// nothing -- and yields nothing.
    #[test]
    fn rarity_alone_does_not_make_a_neighbour() {
        let mut items = vec![item(0, "", "", ""), item(1, "", "", "")];
        items[0].rarity_tier = 4;
        items[1].rarity_tier = 5;
        assert!(rows_for(&items, ContentOptions::default()).is_empty());
    }

    /// A tag carried by everything says nothing about anything.
    #[test]
    fn an_overly_common_tag_is_dropped_from_the_index() {
        let mut items: Vec<ContentItem> = (0..4).map(|i| item(i, "", "", "")).collect();
        for entry in items.iter_mut() {
            entry.tags = vec![7];
            entry.tag_weights = vec![1.0];
        }
        assert!(!rows_for(&items, ContentOptions::default()).is_empty());
        assert!(rows_for(
            &items,
            ContentOptions {
                max_tag_document_frequency: 3,
                ..Default::default()
            }
        )
        .is_empty());
    }

    #[test]
    fn each_anchor_keeps_at_most_k_neighbours() {
        let items: Vec<ContentItem> = (0..10).map(|i| item(i, "0xa", "0xc1", "")).collect();
        let rows = rows_for(
            &items,
            ContentOptions {
                neighbors_per_item: 3,
                ..Default::default()
            },
        );
        assert_eq!(rows.iter().filter(|r| r.item == 0).count(), 3);
    }

    /// Expensive for a hat, not expensive for the catalogue.
    #[test]
    fn price_bands_are_quartiles_within_a_sub_category() {
        let entries = vec![
            (0, "wearable:hat".to_string(), 1.0),
            (1, "wearable:hat".to_string(), 2.0),
            (2, "wearable:hat".to_string(), 3.0),
            (3, "wearable:hat".to_string(), 400.0),
            (4, "emote:".to_string(), 1000.0),
            (5, "emote:".to_string(), 0.0),
        ];
        let bands = assign_price_bands(&entries);
        assert_eq!(bands.get(&0), Some(&0));
        assert_eq!(bands.get(&3), Some(&3));
        assert_eq!(bands.get(&4), Some(&0), "cheapest emote of its own bucket");
        assert_eq!(bands.get(&5), None, "an unpriced item has no band");
    }

    #[test]
    fn tag_vectors_are_idf_weighted_and_unit_length() {
        let mut tags_by_item = HashMap::new();
        tags_by_item.insert(0usize, vec![0u32, 1u32]);
        let vectors = build_tag_vectors(&tags_by_item, &[1, 100], 1000);
        let vector = &vectors[&0];

        let norm: f64 = vector.weights.iter().map(|w| (*w as f64).powi(2)).sum();
        assert!((norm - 1.0).abs() < 1e-6, "{norm}");
        assert!(
            vector.weights[0] > vector.weights[1],
            "the rarer tag carries more"
        );
    }
}
