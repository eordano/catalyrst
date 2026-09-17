use std::collections::HashMap;

use super::constants::{PROFILE_WEIGHTS, RECENCY_DECAY_DAYS};

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Where a profile entry came from, which is what the row's `reason` is derived from later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    Owned,
    Favorite,
    Equipped,
    Seed,
}

impl ProfileSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Favorite => "favorite",
            Self::Equipped => "equipped",
            Self::Seed => "seed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owned" => Some(Self::Owned),
            "favorite" => Some(Self::Favorite),
            "equipped" => Some(Self::Equipped),
            "seed" => Some(Self::Seed),
            _ => None,
        }
    }

    /// Signals the user expressed deliberately, as opposed to everything the wallet happens to hold.
    fn is_explicit(self) -> bool {
        !matches!(self, Self::Owned)
    }
}

#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub item_id: String,
    pub weight: f64,
    pub source: ProfileSource,
}

/// A purchase the wallet still holds. Unpaid acquisitions never reach the profile (see
/// `FREE_ACQUISITION_WEIGHT`), so there is no flag to carry. `acquired_at` is unix seconds.
#[derive(Debug, Clone)]
pub struct OwnedAcquisition {
    pub item_id: String,
    pub acquired_at: i64,
}

pub struct ProfileInput<'a> {
    pub owned: &'a [OwnedAcquisition],
    pub favorites: &'a [String],
    pub equipped: &'a [String],
    pub seeds: &'a [String],
    /// Unix seconds; injectable so the decay is testable.
    pub now: i64,
    /// Most entries to keep. `None` = no cap.
    pub limit: Option<usize>,
}

/// The wallet's taste profile: every signal we have about it, as one weighted set of item ids.
///
/// `owned` carries only what the wallet BOUGHT and still holds; an item it was given never gets
/// here. Purchases decay with age. Favourites and equipped items outrank even a recent purchase:
/// they are statements about what the wallet likes NOW, not a year ago, so they carry no decay.
///
/// When the same item arrives from several signals it keeps the STRONGEST one rather than their
/// sum, so an item the wallet owns, favourited and is wearing cannot crowd out everything else.
pub fn build_taste_profile(input: ProfileInput<'_>) -> Vec<ProfileEntry> {
    let mut best: HashMap<String, ProfileEntry> = HashMap::new();

    let mut offer = |item_id: &str, weight: f64, source: ProfileSource| {
        if item_id.is_empty() || weight <= 0.0 {
            return;
        }
        let replace = best
            .get(item_id)
            .is_none_or(|current| weight > current.weight);
        if replace {
            best.insert(
                item_id.to_string(),
                ProfileEntry {
                    item_id: item_id.to_string(),
                    weight,
                    source,
                },
            );
        }
    };

    for acquisition in input.owned {
        let age_days = ((input.now - acquisition.acquired_at) as f64 / SECONDS_PER_DAY).max(0.0);
        offer(
            &acquisition.item_id,
            PROFILE_WEIGHTS.paid * (-age_days / RECENCY_DECAY_DAYS).exp(),
            ProfileSource::Owned,
        );
    }
    for item_id in input.favorites {
        offer(item_id, PROFILE_WEIGHTS.favorite, ProfileSource::Favorite);
    }
    for item_id in input.equipped {
        offer(item_id, PROFILE_WEIGHTS.equipped, ProfileSource::Equipped);
    }
    for item_id in input.seeds {
        offer(item_id, PROFILE_WEIGHTS.seed, ProfileSource::Seed);
    }

    let mut entries: Vec<ProfileEntry> = best.into_values().collect();
    sort_by_weight(&mut entries);

    let Some(limit) = input.limit else {
        return entries;
    };
    if entries.len() <= limit {
        return entries;
    }

    // The cap exists because a whale's holdings would overflow Postgres' bind-parameter limit,
    // but trimming by weight alone would spend the whole budget on purchases: a wallet with 900
    // recent paid items has 900 entries above the 0.8 a seed carries. Seeds, favourites and what
    // the avatar is wearing are the deliberate, current signals, so they are reserved first and
    // the remaining slots go to holdings by weight.
    let (explicit, owned): (Vec<ProfileEntry>, Vec<ProfileEntry>) =
        entries.into_iter().partition(|e| e.source.is_explicit());
    let mut kept: Vec<ProfileEntry> = explicit.into_iter().take(limit).collect();
    let room = limit.saturating_sub(kept.len());
    kept.extend(owned.into_iter().take(room));
    sort_by_weight(&mut kept);
    kept
}

pub fn sort_by_weight(entries: &mut [ProfileEntry]) {
    entries.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.item_id.cmp(&b.item_id))
    });
}

#[derive(Debug, Clone, Default)]
pub struct ProfileAggregates {
    pub creator_affinity: HashMap<String, f64>,
    pub sub_category_affinity: HashMap<String, f64>,
    pub rarity_affinity: HashMap<String, f64>,
    /// Interquartile range of the prices the wallet actually paid, in credits.
    pub price_low: f64,
    pub price_high: f64,
    /// Share of the profile that is wearables; drives the rail's wearable/emote mix.
    pub wearable_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct ProfileItemAttributes {
    pub item_id: String,
    pub creator: String,
    pub sub_category: String,
    pub rarity: String,
    pub price_credits: f64,
    pub is_wearable: bool,
}

/// Shares of the profile's total weight per attribute -- the inputs to the taste scorer.
pub fn build_profile_aggregates(
    profile: &[ProfileEntry],
    attributes: &HashMap<String, ProfileItemAttributes>,
) -> ProfileAggregates {
    let mut creator_affinity: HashMap<String, f64> = HashMap::new();
    let mut sub_category_affinity: HashMap<String, f64> = HashMap::new();
    let mut rarity_affinity: HashMap<String, f64> = HashMap::new();
    let mut prices: Vec<f64> = Vec::new();
    let mut total = 0.0;
    let mut wearable_weight = 0.0;

    for entry in profile {
        let Some(item) = attributes.get(&entry.item_id) else {
            continue;
        };
        total += entry.weight;
        if item.is_wearable {
            wearable_weight += entry.weight;
        }
        for (key, map) in [
            (&item.creator, &mut creator_affinity),
            (&item.sub_category, &mut sub_category_affinity),
            (&item.rarity, &mut rarity_affinity),
        ] {
            if !key.is_empty() {
                *map.entry(key.clone()).or_insert(0.0) += entry.weight;
            }
        }
        if item.price_credits > 0.0 {
            prices.push(item.price_credits);
        }
    }

    if total > 0.0 {
        for map in [
            &mut creator_affinity,
            &mut sub_category_affinity,
            &mut rarity_affinity,
        ] {
            for value in map.values_mut() {
                *value /= total;
            }
        }
    }

    prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Nearest-rank, not linear interpolation on `len - 1`: a two-item profile is the common
    // case, and the interpolating form collapses both quartiles onto the cheaper item, which
    // would tell the price scorer the wallet has no range at all.
    let quantile = |q: f64| -> f64 {
        if prices.is_empty() {
            return 0.0;
        }
        let rank = ((q * prices.len() as f64).ceil() as usize).max(1) - 1;
        prices[rank.min(prices.len() - 1)]
    };

    ProfileAggregates {
        creator_affinity,
        sub_category_affinity,
        rarity_affinity,
        price_low: quantile(0.25),
        price_high: quantile(0.75),
        wearable_ratio: if total > 0.0 {
            wearable_weight / total
        } else {
            0.0
        },
    }
}

/// The creators the profile leans on hardest, which is what the taste branch of the query pulls from.
pub fn top_creators_of(creator_affinity: &HashMap<String, f64>, count: usize) -> Vec<String> {
    let mut ranked: Vec<(&String, &f64)> = creator_affinity.iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    ranked
        .into_iter()
        .take(count)
        .map(|(creator, _)| creator.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(item_id: &str, acquired_at: i64) -> OwnedAcquisition {
        OwnedAcquisition {
            item_id: item_id.to_string(),
            acquired_at,
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn profile_of(input: ProfileInput<'_>) -> Vec<ProfileEntry> {
        build_taste_profile(input)
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn what_the_avatar_wears_and_what_it_saved_outrank_even_a_fresh_purchase() {
        let entries = profile_of(ProfileInput {
            owned: &[owned("0xa-1", NOW)],
            favorites: &ids(&["0xa-2"]),
            equipped: &ids(&["0xa-3"]),
            seeds: &ids(&["0xa-4"]),
            now: NOW,
            limit: None,
        });
        let order: Vec<&str> = entries.iter().map(|e| e.item_id.as_str()).collect();
        assert_eq!(order, vec!["0xa-3", "0xa-2", "0xa-1", "0xa-4"]);
    }

    #[test]
    fn a_purchase_decays_with_age_while_a_deliberate_signal_does_not() {
        let year = 365 * 86_400;
        let entries = profile_of(ProfileInput {
            owned: &[owned("0xa-1", NOW - year)],
            favorites: &[],
            equipped: &[],
            seeds: &[],
            now: NOW,
            limit: None,
        });
        let decayed = entries[0].weight;
        assert!(decayed < PROFILE_WEIGHTS.paid, "{decayed}");
        assert!((decayed - PROFILE_WEIGHTS.paid * (-1.0f64).exp()).abs() < 1e-9);

        let fresh = profile_of(ProfileInput {
            owned: &[],
            favorites: &ids(&["0xa-1"]),
            equipped: &[],
            seeds: &[],
            now: NOW + 10 * year,
            limit: None,
        });
        assert_eq!(fresh[0].weight, PROFILE_WEIGHTS.favorite);
    }

    /// The strongest signal, not their sum: otherwise an item the wallet owns, favourited and is
    /// wearing crowds out everything else.
    #[test]
    fn one_item_reached_by_several_signals_keeps_the_strongest() {
        let entries = profile_of(ProfileInput {
            owned: &[owned("0xa-1", NOW)],
            favorites: &ids(&["0xa-1"]),
            equipped: &ids(&["0xa-1"]),
            seeds: &ids(&["0xa-1"]),
            now: NOW,
            limit: None,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].weight, PROFILE_WEIGHTS.equipped);
        assert_eq!(entries[0].source, ProfileSource::Equipped);
    }

    /// Trimming by weight alone would spend the whole budget on purchases, and the deliberate
    /// signals are the ones most worth keeping.
    #[test]
    fn the_cap_reserves_the_deliberate_signals_before_the_holdings() {
        let held: Vec<OwnedAcquisition> =
            (0..50).map(|i| owned(&format!("0xa-{i}"), NOW)).collect();
        let entries = profile_of(ProfileInput {
            owned: &held,
            favorites: &ids(&["0xb-1"]),
            equipped: &ids(&["0xb-2"]),
            seeds: &ids(&["0xb-3"]),
            now: NOW,
            limit: 5.into(),
        });
        assert_eq!(entries.len(), 5);
        let kept: Vec<&str> = entries.iter().map(|e| e.item_id.as_str()).collect();
        for deliberate in ["0xb-1", "0xb-2", "0xb-3"] {
            assert!(kept.contains(&deliberate), "{kept:?}");
        }
    }

    #[test]
    fn aggregates_are_shares_of_the_profile_weight() {
        let profile = vec![
            ProfileEntry {
                item_id: "0xa-1".into(),
                weight: 3.0,
                source: ProfileSource::Owned,
            },
            ProfileEntry {
                item_id: "0xa-2".into(),
                weight: 1.0,
                source: ProfileSource::Owned,
            },
        ];
        let mut attributes = HashMap::new();
        attributes.insert(
            "0xa-1".to_string(),
            ProfileItemAttributes {
                item_id: "0xa-1".into(),
                creator: "0xcreator".into(),
                sub_category: "wearable:hat".into(),
                rarity: "rare".into(),
                price_credits: 10.0,
                is_wearable: true,
            },
        );
        attributes.insert(
            "0xa-2".to_string(),
            ProfileItemAttributes {
                item_id: "0xa-2".into(),
                creator: "0xother".into(),
                sub_category: "emote:".into(),
                rarity: "common".into(),
                price_credits: 30.0,
                is_wearable: false,
            },
        );

        let aggregates = build_profile_aggregates(&profile, &attributes);
        assert!((aggregates.creator_affinity["0xcreator"] - 0.75).abs() < 1e-9);
        assert!((aggregates.wearable_ratio - 0.75).abs() < 1e-9);
        assert_eq!(aggregates.price_low, 10.0);
        assert_eq!(aggregates.price_high, 30.0);
        assert_eq!(
            top_creators_of(&aggregates.creator_affinity, 1),
            vec!["0xcreator".to_string()]
        );
    }
}
