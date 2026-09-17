use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::constants::{
    DEFAULT_WEARABLE_RATIO, MAX_PER_COLLECTION, MAX_PER_CREATOR, RARITY_TIERS, SCORE_WEIGHTS,
};
use super::profile::{ProfileAggregates, ProfileSource};

/// One item by a creator is a purchase; two is a pattern worth naming.
const MIN_ITEMS_TO_COLLECT_CREATOR: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "snake_case")
)]
pub enum SuggestionReasonKind {
    CoOwned,
    CreatorAffinity,
    FavoriteSimilar,
    EquippedSimilar,
    SeedSimilar,
    Trending,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SuggestionReason {
    pub kind: SuggestionReasonKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub creator: Option<String>,
}

impl SuggestionReason {
    pub fn trending() -> Self {
        Self {
            kind: SuggestionReasonKind::Trending,
            item_id: None,
            creator: None,
        }
    }

    fn of_item(kind: SuggestionReasonKind, item_id: &str) -> Self {
        Self {
            kind,
            item_id: Some(item_id.to_string()),
            creator: None,
        }
    }
}

/// One candidate with its raw, un-normalised component scores.
#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    pub item_id: String,
    pub collection: String,
    pub creator: String,
    pub sub_category: String,
    pub rarity: String,
    pub price_credits: f64,
    pub is_wearable: bool,
    pub cf: f64,
    pub content: f64,
    pub popularity: f64,
    /// The profile item that contributed most of `cf` + `content`, for the explanation.
    pub top_trigger_item_id: Option<String>,
    pub top_trigger_source: Option<ProfileSource>,
}

#[derive(Debug, Clone)]
pub struct BlendedCandidate {
    pub candidate: ScoredCandidate,
    pub taste: f64,
    pub score: f64,
    pub reason: SuggestionReason,
}

/// How well a candidate matches the wallet's aggregate taste, independent of any specific item
/// it owns. This is the component that produces "more from a creator you collect": it scores a
/// brand-new drop from a familiar creator that no co-ownership or content neighbour could reach.
pub fn taste_score(candidate: &ScoredCandidate, aggregates: &ProfileAggregates) -> f64 {
    let creator = affinity(&aggregates.creator_affinity, &candidate.creator);
    let sub_category = affinity(&aggregates.sub_category_affinity, &candidate.sub_category);

    let mut rarity = 0.0;
    if let Some(tier) = rarity_tier(&candidate.rarity) {
        let from = tier.saturating_sub(1);
        let to = (tier + 1).min(RARITY_TIERS.len() - 1);
        for tier in &RARITY_TIERS[from..=to] {
            rarity += affinity(&aggregates.rarity_affinity, tier);
        }
    }

    // Neutral rather than zero when the wallet has never paid for anything: an unknown price
    // preference should not push every priced candidate down.
    let mut price = 0.5;
    if aggregates.price_high > 0.0 && candidate.price_credits > 0.0 {
        if candidate.price_credits >= aggregates.price_low
            && candidate.price_credits <= aggregates.price_high
        {
            price = 1.0;
        } else {
            let bound = if candidate.price_credits < aggregates.price_low {
                aggregates.price_low.max(1e-9)
            } else {
                aggregates.price_high
            };
            price = (-(candidate.price_credits / bound).ln().abs()).exp();
        }
    }

    0.5 * creator + 0.3 * sub_category + 0.2 * (0.5 * rarity + 0.5 * price)
}

fn affinity(map: &HashMap<String, f64>, key: &str) -> f64 {
    if key.is_empty() {
        return 0.0;
    }
    map.get(key).copied().unwrap_or(0.0)
}

fn rarity_tier(rarity: &str) -> Option<usize> {
    let lower = rarity.to_ascii_lowercase();
    RARITY_TIERS.iter().position(|tier| *tier == lower)
}

struct Contributions {
    cf: f64,
    content: f64,
    taste: f64,
    popularity: f64,
}

/// Blends the four components into one score.
///
/// Each wallet-dependent component is divided by its own largest value across this wallet's
/// candidates first: raw co-ownership sums run an order of magnitude above taste affinities,
/// which are shares of one, so without the rescaling the published weights describe a blend that
/// never happens. Popularity arrives already normalised to 0..1 across the catalogue and is left
/// alone -- it is the one component that is deliberately NOT relative to the wallet.
pub fn blend_candidates(
    candidates: Vec<ScoredCandidate>,
    aggregates: &ProfileAggregates,
    profile_creator_counts: &HashMap<String, usize>,
) -> Vec<BlendedCandidate> {
    let tastes: Vec<f64> = candidates
        .iter()
        .map(|candidate| taste_score(candidate, aggregates))
        .collect();

    let max_cf = max_of(candidates.iter().map(|c| c.cf));
    let max_content = max_of(candidates.iter().map(|c| c.content));
    let max_taste = max_of(tastes.iter().copied());

    candidates
        .into_iter()
        .zip(tastes)
        .map(|(candidate, taste_raw)| {
            let contributions = Contributions {
                cf: SCORE_WEIGHTS.cf * relative(candidate.cf, max_cf),
                content: SCORE_WEIGHTS.content * relative(candidate.content, max_content),
                taste: SCORE_WEIGHTS.taste * relative(taste_raw, max_taste),
                popularity: SCORE_WEIGHTS.popularity * candidate.popularity,
            };
            let score = contributions.cf
                + contributions.content
                + contributions.taste
                + contributions.popularity;
            let collects = collects_creator(&candidate.creator, profile_creator_counts);
            let reason = pick_reason(&candidate, &contributions, collects);
            BlendedCandidate {
                candidate,
                taste: taste_raw,
                score,
                reason,
            }
        })
        .collect()
}

fn max_of(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0f64, |acc, v| if v > acc { v } else { acc })
}

fn relative(value: f64, max: f64) -> f64 {
    if max > 0.0 {
        value / max
    } else {
        0.0
    }
}

/// The single signal that contributed most of the row's score, mapped to the copy the shop shows.
///
/// Taste winning means the wallet's affinity for the creator carried the row, which is exactly
/// "more from a creator you collect". Popularity winning means nothing personal did.
fn pick_reason(
    candidate: &ScoredCandidate,
    contributions: &Contributions,
    collects_creator: bool,
) -> SuggestionReason {
    let ranked = [
        ("cf", contributions.cf),
        ("content", contributions.content),
        ("taste", contributions.taste),
        ("popularity", contributions.popularity),
    ];
    let (winner, value) = ranked
        .iter()
        .copied()
        .fold(("cf", f64::NEG_INFINITY), |best, entry| {
            if entry.1 > best.1 {
                entry
            } else {
                best
            }
        });

    if value <= 0.0 {
        return SuggestionReason::trending();
    }

    // Taste is three things at once -- creator, sub-category, rarity/price -- so winning on
    // taste is not by itself evidence that the wallet collects this creator. Claiming "more from
    // a creator you collect" about a creator the wallet has never bought from is the one
    // explanation here that would read as an outright lie, so it has to be earned separately.
    if winner == "taste" {
        if collects_creator && !candidate.creator.is_empty() {
            return SuggestionReason {
                kind: SuggestionReasonKind::CreatorAffinity,
                item_id: None,
                creator: Some(candidate.creator.clone()),
            };
        }
        return reason_from_trigger(candidate);
    }
    if winner == "popularity" {
        return SuggestionReason::trending();
    }

    reason_from_trigger(candidate)
}

/// cf and content are both driven by a specific profile item, so the explanation names it -- and
/// names it for what it was: something worn, favourited, browsed, or owned.
fn reason_from_trigger(candidate: &ScoredCandidate) -> SuggestionReason {
    let Some(trigger) = candidate.top_trigger_item_id.as_deref() else {
        return SuggestionReason::trending();
    };
    match candidate.top_trigger_source {
        Some(ProfileSource::Favorite) => {
            SuggestionReason::of_item(SuggestionReasonKind::FavoriteSimilar, trigger)
        }
        Some(ProfileSource::Equipped) => {
            SuggestionReason::of_item(SuggestionReasonKind::EquippedSimilar, trigger)
        }
        // A seed is something the visitor looked at or put in the cart, never something they
        // hold, so "because you have X" would be wrong about an item they do not own.
        Some(ProfileSource::Seed) => {
            SuggestionReason::of_item(SuggestionReasonKind::SeedSimilar, trigger)
        }
        _ => SuggestionReason::of_item(SuggestionReasonKind::CoOwned, trigger),
    }
}

/// Whether the profile holds enough of this creator for "a creator you collect" to be true.
pub fn collects_creator(creator: &str, profile_creator_counts: &HashMap<String, usize>) -> bool {
    !creator.is_empty()
        && profile_creator_counts.get(creator).copied().unwrap_or(0) >= MIN_ITEMS_TO_COLLECT_CREATOR
}

/// Re-ranks the scored head into the rail the user sees.
///
/// A pure score ordering collapses: co-ownership is strongest inside a collection, so the top 12
/// is routinely eight items from one drop. The caps spend the rail on variety instead, and the
/// wearable/emote mix follows what the wallet actually collects so an emote collector does not
/// get a wall of hats.
///
/// The constraints are relaxed in TIERS rather than all at once, because they are not equally
/// important. Never showing the same sub-category twice in a row is a presentation nicety;
/// showing eight items from one collection defeats the point of the rail. So a rail short on
/// supply first gives up the run-breaking, then the caps, and only ever falls back to pure score
/// order. A short rail is the worst outcome of the three, so the last tier accepts anything.
pub fn rerank_for_diversity(
    candidates: Vec<BlendedCandidate>,
    limit: usize,
    wearable_ratio: f64,
) -> Vec<BlendedCandidate> {
    let mut ordered = candidates;
    ordered.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.candidate.item_id.cmp(&b.candidate.item_id))
    });

    let ratio = if wearable_ratio.is_finite() && wearable_ratio > 0.0 {
        wearable_ratio
    } else {
        DEFAULT_WEARABLE_RATIO
    };
    let wearable_target = (limit as f64 * ratio).round() as usize;

    let mut picked: Vec<usize> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut per_collection: HashMap<String, usize> = HashMap::new();
    let mut per_creator: HashMap<String, usize> = HashMap::new();
    let mut wearables = 0usize;
    let mut emotes = 0usize;
    let mut last_sub_category: Option<String> = None;

    for tier in ["all", "without-run-breaking", "score-only"] {
        for (index, entry) in ordered.iter().enumerate() {
            if picked.len() >= limit {
                break;
            }
            let candidate = &entry.candidate;
            if taken.contains(&candidate.item_id) {
                continue;
            }
            if tier != "score-only" {
                let collection_count = per_collection
                    .get(&candidate.collection)
                    .copied()
                    .unwrap_or(0);
                let creator_count = per_creator.get(&candidate.creator).copied().unwrap_or(0);
                let emote_target = limit.saturating_sub(wearable_target);
                let would_exceed_mix = if candidate.is_wearable {
                    wearables >= wearable_target && emotes < emote_target
                } else {
                    emotes >= emote_target && wearables < wearable_target
                };

                if !candidate.collection.is_empty() && collection_count >= MAX_PER_COLLECTION {
                    continue;
                }
                if !candidate.creator.is_empty() && creator_count >= MAX_PER_CREATOR {
                    continue;
                }
                if would_exceed_mix {
                    continue;
                }
                if tier == "all"
                    && last_sub_category.is_some()
                    && !candidate.sub_category.is_empty()
                    && last_sub_category.as_deref() == Some(candidate.sub_category.as_str())
                {
                    continue;
                }
            }

            picked.push(index);
            taken.insert(candidate.item_id.clone());
            *per_collection
                .entry(candidate.collection.clone())
                .or_insert(0) += 1;
            *per_creator.entry(candidate.creator.clone()).or_insert(0) += 1;
            if candidate.is_wearable {
                wearables += 1;
            } else {
                emotes += 1;
            }
            last_sub_category =
                (!candidate.sub_category.is_empty()).then(|| candidate.sub_category.clone());
        }
        if picked.len() >= limit {
            break;
        }
    }

    let mut by_index: HashMap<usize, BlendedCandidate> = ordered.into_iter().enumerate().collect();
    picked
        .into_iter()
        .filter_map(|index| by_index.remove(&index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(rarity: &str) -> ScoredCandidate {
        ScoredCandidate {
            item_id: "0xc-1".into(),
            collection: "0xc".into(),
            creator: String::new(),
            sub_category: String::new(),
            rarity: rarity.into(),
            price_credits: 0.0,
            is_wearable: true,
            cf: 0.0,
            content: 0.0,
            popularity: 0.0,
            top_trigger_item_id: None,
            top_trigger_source: None,
        }
    }

    fn aggregates(rarity_key: &str) -> ProfileAggregates {
        ProfileAggregates {
            rarity_affinity: HashMap::from([(rarity_key.to_string(), 1.0)]),
            ..Default::default()
        }
    }

    /// The tiers this resolves through are the lowercase enum spellings, so an affinity keyed on
    /// the squid column's own casing is a term that silently scores zero. That is why the
    /// attributes query lowercases the column rather than leaving it to the scorer.
    #[test]
    fn the_rarity_term_is_reachable_only_through_lowercase_keys() {
        let lowered = taste_score(&candidate("rare"), &aggregates("rare"));
        let mixed = taste_score(&candidate("rare"), &aggregates("Rare"));
        assert!(lowered > 0.0);
        assert_eq!(mixed, taste_score(&candidate("rare"), &aggregates("")));
        assert!(mixed < lowered);
    }

    /// A candidate's own rarity is matched case-insensitively, so the squid row's casing on THAT
    /// side is already harmless.
    #[test]
    fn a_candidates_rarity_is_matched_case_insensitively() {
        assert_eq!(
            taste_score(&candidate("Rare"), &aggregates("rare")),
            taste_score(&candidate("rare"), &aggregates("rare"))
        );
    }
}
