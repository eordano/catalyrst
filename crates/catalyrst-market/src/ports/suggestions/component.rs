use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use sqlx::{PgPool, Row};

use super::candidates::{
    build_candidate_pools, collections_of, finish_candidates, pool_cut, CandidatePools,
    NeighborScore, OWNERSHIP_PROBE_MULTIPLIER,
};
use super::constants::{
    MAX_EQUIPPED, MAX_EXCLUDE, MAX_FAVORITES, MAX_PROFILE_ITEMS, MAX_SEEDS, MIN_PERSONAL_ROWS,
    NEIGHBORS_PER_ITEM, NEIGHBOR_SOURCE_CF, SHED_LOG_INTERVAL_MS, SUGGESTED_DEFAULT_LIMIT,
    SUGGESTED_MAX_LIMIT, SUGGESTIONS_CACHE_TTL_SECONDS, TASTE_CREATOR_COUNT,
};
use super::profile::{
    build_profile_aggregates, build_taste_profile, sort_by_weight, top_creators_of,
    OwnedAcquisition, ProfileEntry, ProfileInput, ProfileItemAttributes,
};
use super::queries::{
    select_creator_items, select_item_attributes, select_neighbors, select_owned_acquisitions,
    select_owned_among, select_popularity,
};
use super::scoring::{
    blend_candidates, rerank_for_diversity, BlendedCandidate, ScoredCandidate, SuggestionReason,
    SuggestionReasonKind,
};
use super::urn::{normalize_item_ids, to_item_ids};
use crate::http::response::ApiError;
use crate::ports::lists::{ListsComponent, DEFAULT_LIST_ID};
use crate::ports::shop_catalog::{
    ShopCatalogComponent, ShopCatalogFilters, UnifiedCatalogFilters, UnifiedItem,
    TRENDING_MAX_LIMIT,
};
use crate::ports::trendings::midnight_days_ago;

/// Popularity window. Long enough that a rail asked for at 3am is not ranked on an empty night,
/// short enough that "popular" still means now.
const POPULARITY_DAYS: i64 = 7;
/// Popular items pulled as the popularity component of the blend.
const POPULARITY_POOL: i64 = 200;
/// Responses held per process. The key carries the caller's whole request, so a busy rail with
/// many distinct seed sets must not be able to grow this without bound.
const SUGGESTIONS_CACHE_ENTRIES: usize = 2_000;

#[derive(Debug, Clone, Default)]
pub struct SuggestionRequest {
    pub wallet: Option<String>,
    pub first: Option<i64>,
    pub seeds: Vec<String>,
    pub equipped: Vec<String>,
    pub exclude: Vec<String>,
    pub category: Option<String>,
    pub body_shape: Option<String>,
}

/// One rail entry: the browse grid's item shape plus WHY it is here.
///
/// The reason is not decoration -- it is the only part of a personalised rail a person can check.
/// An item with no explanation is indistinguishable from an ad.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SuggestedItem {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub item: UnifiedItem,
    pub reason: SuggestionReason,
    /// The blended score this row was ranked on. Carried so a consumer can compare rows across
    /// algorithm versions rather than trusting the order alone; the unpersonalised fallback
    /// reports zero, because trending rows were never scored against a wallet.
    pub score: f64,
}

/// `personalized` is FALSE whenever the rail is the popularity floor rather than this wallet's
/// own signals, so the client can label it honestly instead of promising a personal rail to a
/// wallet we know nothing about.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SuggestionsResponse {
    pub data: Vec<SuggestedItem>,
    pub personalized: bool,
    pub algorithm: String,
}

pub struct SuggestionsComponent {
    pool: PgPool,
    /// Its own handles on the two components it reads through rather than references into
    /// `AppState`: the rail must hydrate through the SAME item-unified core the grid uses, and
    /// both components are a pool handle and nothing else.
    catalog: ShopCatalogComponent,
    lists: ListsComponent,
    /// Shedding, not queueing: see `SUGGESTIONS_MAX_CONCURRENT`.
    permits: Arc<tokio::sync::Semaphore>,
    /// Read BEFORE the shed gate, so a hit is served under exactly the load that would otherwise
    /// shed it. Keyed on everything the CALLER varies and on whether favourites were readable --
    /// without that second half a signed answer, which names the caller's saved items in its
    /// reasons, and an unsigned one for the same `?address=` collide on one entry.
    cache: SuggestionsCache,
    /// Cumulative rather than per-window: a window that ends before its line is printed would
    /// otherwise take its sheds with it, and a total that only ever grows is the shape a counter
    /// metric wants if this is ever promoted to one.
    shed_total: AtomicU64,
    /// Milliseconds since `started` at which the last shed line was printed, zero for none yet.
    shed_logged_at: AtomicU64,
    started: std::time::Instant,
    /// Global and day-windowed, so one query a minute serves every miss.
    popularity: PopularityCache,
}

type SuggestionsCache = catalyrst_commons::cache::TtlMap<String, Arc<SuggestionsResponse>>;
type PopularityCache = catalyrst_commons::cache::TtlMap<i64, Arc<HashMap<String, f64>>>;
const POPULARITY_CACHE_TTL_SECONDS: u64 = 60;

pub fn clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(SUGGESTED_DEFAULT_LIMIT)
        .clamp(1, SUGGESTED_MAX_LIMIT)
}

impl SuggestionsComponent {
    pub fn new(pool: PgPool, max_concurrent: usize) -> Self {
        Self {
            catalog: ShopCatalogComponent::new(pool.clone()),
            lists: ListsComponent::new(pool.clone()),
            pool,
            permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent.max(1))),
            cache: catalyrst_commons::cache::TtlMap::bounded(
                "market_suggestions",
                std::time::Duration::from_secs(SUGGESTIONS_CACHE_TTL_SECONDS),
                SUGGESTIONS_CACHE_ENTRIES,
            ),
            shed_total: AtomicU64::new(0),
            shed_logged_at: AtomicU64::new(0),
            started: std::time::Instant::now(),
            popularity: catalyrst_commons::cache::TtlMap::bounded(
                "market_suggestions_popularity",
                std::time::Duration::from_secs(POPULARITY_CACHE_TTL_SECONDS),
                2,
            ),
        }
    }

    /// Favourites are read ONLY for a caller whose signature we verified.
    ///
    /// A favourites list is private: the address alone is a public identifier anyone can type, so
    /// accepting one as proof would turn this rail into a lookup service for other people's
    /// saved items. The handler passes `Some` only after `optional_signer` resolved.
    async fn favorites_of(&self, signer: Option<&str>) -> Vec<String> {
        let Some(signer) = signer else {
            return Vec::new();
        };
        match self
            .lists
            .get_picks_by_list_id(DEFAULT_LIST_ID, Some(signer), MAX_FAVORITES as i64, 0)
            .await
        {
            Ok((picks, _)) => picks.into_iter().map(|p| p.item_id).collect(),
            Err(e) => {
                tracing::info!(error = ?e, "favourites unavailable: suggestions run without them");
                Vec::new()
            }
        }
    }

    async fn owned_of(&self, wallet: &str) -> Result<Vec<OwnedAcquisition>, ApiError> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_owned_acquisitions()))
            .bind(wallet.to_lowercase())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(OwnedAcquisition {
                    item_id: row.try_get::<String, _>("item_id").ok()?,
                    acquired_at: row.try_get::<i64, _>("acquired_at").unwrap_or(0),
                })
            })
            .collect())
    }

    /// The rate travels with the ids: the band these attributes build is compared against
    /// candidates the unified core already priced in credits, so the profile has to be priced the
    /// same way or the price term of the taste score compares MANA against credits.
    async fn attributes_of(
        &self,
        item_ids: &[String],
        mana_usd_rate: f64,
    ) -> Result<HashMap<String, ProfileItemAttributes>, ApiError> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_item_attributes()))
            .bind(item_ids.to_vec())
            .bind(mana_usd_rate)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let item_id: String = row.try_get("item_id").ok()?;
                Some((
                    item_id.clone(),
                    ProfileItemAttributes {
                        item_id,
                        creator: row.try_get("creator").unwrap_or_default(),
                        sub_category: row.try_get("sub_category").unwrap_or_default(),
                        rarity: row.try_get("rarity").unwrap_or_default(),
                        price_credits: row.try_get::<f64, _>("price_credits").unwrap_or(0.0),
                        is_wearable: row.try_get("is_wearable").unwrap_or(true),
                    },
                ))
            })
            .collect())
    }

    /// Which of these candidates the wallet already HOLDS.
    ///
    /// The profile cannot stand in for this. It is capped, paid-only and weight-sorted, so every
    /// airdropped or claimed holding and everything past the cap is absent from it and would be
    /// offered back to its own owner -- which is the single most damaging thing this rail can do.
    /// Asked about the candidates already on the table rather than about every holding, so the
    /// cost does not scale with the size of the wallet.
    async fn owned_among(
        &self,
        wallet: &str,
        item_ids: &[String],
    ) -> Result<HashSet<String>, ApiError> {
        if item_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_owned_among()))
            .bind(wallet.to_lowercase())
            .bind(item_ids.to_vec())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>("item_id").ok())
            .collect())
    }

    /// Precomputed neighbours of the profile's items, accumulated per candidate.
    ///
    /// A candidate reached from several profile items accumulates their contributions: an item
    /// that is a neighbour of three things the wallet owns is a better bet than one that neighbours
    /// a single thing very closely, and a MAX would say the opposite.
    async fn neighbors_of(
        &self,
        profile: &[ProfileEntry],
    ) -> Result<HashMap<String, NeighborScore>, ApiError> {
        let anchors: Vec<String> = profile.iter().map(|e| e.item_id.clone()).collect();
        if anchors.is_empty() {
            return Ok(HashMap::new());
        }
        let weights: HashMap<&str, &ProfileEntry> = profile
            .iter()
            .map(|entry| (entry.item_id.as_str(), entry))
            .collect();

        let rows = sqlx::query(sqlx::AssertSqlSafe(select_neighbors()))
            .bind(anchors)
            .bind(NEIGHBORS_PER_ITEM as i32)
            .fetch_all(&self.pool)
            .await?;

        let mut out: HashMap<String, NeighborScore> = HashMap::new();
        for row in rows {
            let (Ok(anchor), Ok(neighbor), Ok(source)) = (
                row.try_get::<String, _>("item_id"),
                row.try_get::<String, _>("neighbor_id"),
                row.try_get::<String, _>("source"),
            ) else {
                continue;
            };
            let Some(entry) = weights.get(anchor.as_str()) else {
                continue;
            };
            let sim = row.try_get::<f32, _>("sim").unwrap_or(0.0) as f64;
            let contribution = sim * entry.weight;
            let slot = out.entry(neighbor).or_default();
            if source == NEIGHBOR_SOURCE_CF {
                slot.cf += contribution;
            } else {
                slot.content += contribution;
            }
            if contribution > slot.best {
                slot.best = contribution;
                slot.trigger_item = Some(anchor);
                slot.trigger_source = Some(entry.source);
            }
        }
        Ok(out)
    }

    async fn creator_items_of(&self, creators: &[String]) -> Result<Vec<String>, ApiError> {
        if creators.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_creator_items()))
            .bind(creators.to_vec())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>("item_id").ok())
            .collect())
    }

    /// Sale counts over the window, rescaled to 0..1 by the busiest item.
    async fn popularity(&self) -> Result<Arc<HashMap<String, f64>>, ApiError> {
        let from = midnight_days_ago(POPULARITY_DAYS);
        self.popularity
            .get_or_fetch(from, || async move {
                let rows = sqlx::query(sqlx::AssertSqlSafe(select_popularity()))
                    .bind(from)
                    .bind(POPULARITY_POOL)
                    .fetch_all(&self.pool)
                    .await?;
                let counted: Vec<(String, f64)> = rows
                    .into_iter()
                    .filter_map(|row| {
                        Some((
                            row.try_get::<String, _>("item_id").ok()?,
                            row.try_get::<i64, _>("sales").unwrap_or(0) as f64,
                        ))
                    })
                    .collect();
                let max = counted.iter().map(|(_, n)| *n).fold(0.0f64, f64::max);
                if max <= 0.0 {
                    return Ok(Arc::new(HashMap::new()));
                }
                Ok(Arc::new(
                    counted.into_iter().map(|(id, n)| (id, n / max)).collect(),
                ))
            })
            .await
    }

    /// The rail.
    ///
    /// The cache is read FIRST, before the shed gate: a hit costs nothing, so refusing it under
    /// load would shed exactly the requests the cache exists to answer. Past it, shedding rather
    /// than queueing on saturation: a miss is seconds of database work across several queries, and
    /// enough concurrent misses would hold the pool while every other route waits behind a rail
    /// the shop treats as optional. A shed request answers with an EMPTY rail and runs no query at
    /// all, and is NOT cached -- it describes the server's load, not the caller's request.
    pub async fn get_suggestions(
        &self,
        request: &SuggestionRequest,
        signer: Option<&str>,
        mana_usd_rate: f64,
    ) -> Result<Arc<SuggestionsResponse>, ApiError> {
        let limit = clamp_first(request.first) as usize;
        let wallet = request.wallet.as_deref().map(str::to_lowercase);
        let favorites_for = signer.map(str::to_lowercase);
        let normalized = NormalizedLists {
            equipped: to_item_ids(&request.equipped, MAX_EQUIPPED),
            seeds: to_item_ids(&request.seeds, MAX_SEEDS),
            exclude: to_item_ids(&request.exclude, MAX_EXCLUDE),
        };

        let key = cache_key(
            request,
            wallet.as_deref(),
            favorites_for.as_deref(),
            &normalized,
            limit,
        );
        if let Some(hit) = self.cache.get_fresh(&key) {
            return Ok(hit);
        }

        let Ok(_permit) = self.permits.clone().try_acquire_owned() else {
            return Ok(self.shed());
        };

        let response = self
            .compute(
                request,
                &normalized,
                wallet.as_deref(),
                favorites_for.as_deref(),
                limit,
                mana_usd_rate,
            )
            .await?;
        store(&self.cache, key, response.clone());
        Ok(response)
    }

    /// The shed answer, and the counting that keeps a burst from becoming its own log flood.
    ///
    /// Saturation arrives as a burst by definition, so one line per shed request is a flood on top
    /// of a flood.
    fn shed(&self) -> Arc<SuggestionsResponse> {
        let total = self.shed_total.fetch_add(1, Ordering::Relaxed) + 1;
        let elapsed = self.started.elapsed().as_millis() as u64;
        let last = self.shed_logged_at.load(Ordering::Relaxed);
        if last == 0 || elapsed.saturating_sub(last) >= SHED_LOG_INTERVAL_MS {
            self.shed_logged_at.store(elapsed.max(1), Ordering::Relaxed);
            tracing::warn!(
                shed_total = total,
                "suggestions shed: computing at capacity"
            );
        }
        shed_response()
    }

    async fn compute(
        &self,
        request: &SuggestionRequest,
        normalized: &NormalizedLists,
        wallet: Option<&str>,
        favorites_for: Option<&str>,
        limit: usize,
        mana_usd_rate: f64,
    ) -> Result<Arc<SuggestionsResponse>, ApiError> {
        let (owned, favorites) = tokio::join!(
            async {
                match wallet {
                    Some(wallet) => self.owned_of(wallet).await,
                    None => Ok(Vec::new()),
                }
            },
            self.favorites_of(favorites_for)
        );
        let owned = owned?;
        let favorites = normalize_item_ids(&favorites, MAX_FAVORITES);

        let profile = build_taste_profile(ProfileInput {
            owned: &owned,
            favorites: &favorites,
            equipped: &normalized.equipped,
            seeds: &normalized.seeds,
            now: chrono::Utc::now().timestamp(),
            limit: Some(MAX_PROFILE_ITEMS),
        });
        let mut profile = profile;
        sort_by_weight(&mut profile);
        let profile = profile;

        if profile.is_empty() {
            return self
                .trending_fallback(request, normalized, wallet, limit, mana_usd_rate)
                .await;
        }

        let profile_ids: Vec<String> = profile.iter().map(|e| e.item_id.clone()).collect();
        let (attributes, neighbors, popularity) = tokio::try_join!(
            self.attributes_of(&profile_ids, mana_usd_rate),
            self.neighbors_of(&profile),
            self.popularity()
        )?;
        let aggregates = build_profile_aggregates(&profile, &attributes);

        let creator_items = self
            .creator_items_of(&top_creators_of(
                &aggregates.creator_affinity,
                TASTE_CREATOR_COUNT,
            ))
            .await?;

        let excluded: HashSet<String> = normalized
            .exclude
            .iter()
            .cloned()
            .chain(profile_ids.iter().cloned())
            .collect();

        let cut = pool_cut(limit);
        let CandidatePools {
            neighbours: mut candidate_ids,
            taste: mut taste_ids,
        } = build_candidate_pools(
            &neighbors,
            creator_items.into_iter().chain(popularity.keys().cloned()),
            &excluded,
            cut * OWNERSHIP_PROBE_MULTIPLIER,
        );

        // Asked once, and BEFORE the pool is cut: the profile is capped and paid-only, so it
        // cannot answer "does this wallet already hold it" for a gift or for anything past the
        // cap, and a cut taken first would hand the blend a pool of the wallet's own shelf.
        if let Some(wallet) = wallet {
            let probe: Vec<String> = candidate_ids
                .iter()
                .chain(taste_ids.iter())
                .cloned()
                .collect();
            let owned_now = self.owned_among(wallet, &probe).await?;
            candidate_ids.retain(|id| !owned_now.contains(id));
            taste_ids.retain(|id| !owned_now.contains(id));
        }
        let candidate_ids = finish_candidates(candidate_ids, taste_ids, cut);

        let hydrated = self
            .catalog
            .get_items_by_ids(
                &candidate_ids,
                Some(collections_of(&candidate_ids)),
                mana_usd_rate,
            )
            .await?;
        let hydrated = filter_by_request(hydrated, request);

        // Upstream's gate, and it precedes the blend: below this many rows there is nothing for
        // the diversity re-rank to choose between, and a three-row rail claiming to be personal
        // is worse than an honest trending one.
        if hydrated.len() < MIN_PERSONAL_ROWS {
            return self
                .trending_fallback(request, normalized, wallet, limit, mana_usd_rate)
                .await;
        }

        let profile_creator_counts = creator_counts(&profile, &attributes);
        let candidates: Vec<ScoredCandidate> = hydrated
            .iter()
            .map(|item| {
                let id = unified_item_id(item);
                let neighbor = neighbors.get(&id);
                ScoredCandidate {
                    item_id: id.clone(),
                    collection: item.contract_address.clone(),
                    creator: item.creator.clone(),
                    sub_category: sub_category_of(item),
                    rarity: item.rarity.clone(),
                    price_credits: item.price_credits as f64,
                    is_wearable: item.category != "emote",
                    cf: neighbor.map(|n| n.cf).unwrap_or(0.0),
                    content: neighbor.map(|n| n.content).unwrap_or(0.0),
                    popularity: popularity.get(&id).copied().unwrap_or(0.0),
                    top_trigger_item_id: neighbor.and_then(|n| n.trigger_item.clone()),
                    top_trigger_source: neighbor.and_then(|n| n.trigger_source),
                }
            })
            .collect();

        let blended = blend_candidates(candidates, &aggregates, &profile_creator_counts);
        let ranked = rerank_for_diversity(blended, limit, aggregates.wearable_ratio);

        let by_id: HashMap<String, UnifiedItem> = hydrated
            .into_iter()
            .map(|item| (unified_item_id(&item), item))
            .collect();

        Ok(Arc::new(build_response(ranked, by_id)))
    }

    /// The rail when there was nothing personal to say.
    ///
    /// It is the TRENDING rail, not a second opinion about what is popular: the two appear on the
    /// same page, and a rail built from a different query would disagree with the one beside it
    /// about the same word. The hard filters still apply, because "not this item", "not a shape
    /// this avatar can wear" and "not something they already own" are true whatever produced the
    /// rows -- and every one of them drops rows AFTER the query, so the query is asked for more
    /// than the rail shows, up to the trending rail's own ceiling.
    async fn trending_fallback(
        &self,
        request: &SuggestionRequest,
        normalized: &NormalizedLists,
        wallet: Option<&str>,
        limit: usize,
        mana_usd_rate: f64,
    ) -> Result<Arc<SuggestionsResponse>, ApiError> {
        let excluded: HashSet<String> = normalized.exclude.iter().cloned().collect();
        let first = fallback_first(
            limit,
            !excluded.is_empty() || request.body_shape.is_some() || wallet.is_some(),
        );

        let filters = UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                category: request.category.clone(),
                include_social_emotes: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let trending = self
            .catalog
            .get_trending_items(Some(first), None, &filters, mana_usd_rate)
            .await?;

        let items: Vec<UnifiedItem> = trending.into_iter().map(|row| row.item).collect();
        let mut items = filter_by_request(items, request);
        items.retain(|item| !excluded.contains(&unified_item_id(item)));

        // Asked last, and only about what survived the free filters: it is the one filter that
        // costs a round trip, and the rows it would have judged are already gone.
        if let Some(wallet) = wallet {
            let ids: Vec<String> = items.iter().map(unified_item_id).collect();
            let owned = self.owned_among(wallet, &ids).await?;
            items.retain(|item| !owned.contains(&unified_item_id(item)));
        }
        items.truncate(limit);

        Ok(Arc::new(SuggestionsResponse {
            data: items
                .into_iter()
                .map(|item| SuggestedItem {
                    item,
                    reason: SuggestionReason::trending(),
                    score: 0.0,
                })
                .collect(),
            personalized: false,
            algorithm: super::constants::ALGORITHM_VERSION.to_string(),
        }))
    }
}

/// The caller's three id lists, parsed and capped once.
///
/// Normalised BEFORE anything reads them, so the cache key and the queries can never disagree
/// about what was asked: an unparseable entry that reached the key but not the query would give
/// two identical requests two entries.
#[derive(Debug, Default, Clone)]
pub struct NormalizedLists {
    pub seeds: Vec<String>,
    pub equipped: Vec<String>,
    pub exclude: Vec<String>,
}

/// Everything the CALLER varies, and nothing else.
///
/// `favorites_for` is part of it and is load-bearing: without it a signed answer -- which carries
/// `favorite_similar` reasons naming what the caller saved -- and an unsigned one for the same
/// `?address=` collide on one entry, and whoever asks second reads the first one's favourites out
/// of the cache. That is the leak the signature exists to close, arriving by another door.
///
/// The MANA/USD rate is deliberately absent: it moves continuously, so including it would miss
/// almost every time, and a rate change inside the window only shifts credit prices by rounding.
fn cache_key(
    request: &SuggestionRequest,
    wallet: Option<&str>,
    favorites_for: Option<&str>,
    normalized: &NormalizedLists,
    limit: usize,
) -> String {
    format!(
        "suggestions:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        super::constants::ALGORITHM_VERSION,
        wallet.unwrap_or("anon"),
        favorites_for.unwrap_or("unsigned"),
        hash_list(&normalized.seeds),
        hash_list(&normalized.equipped),
        hash_list(&normalized.exclude),
        request.body_shape.as_deref().unwrap_or(""),
        request.category.as_deref().unwrap_or(""),
        limit,
    )
}

/// FNV-1a over the sorted list. Not cryptographic -- this only has to be stable and short, and
/// fifty ids spelled out is a key that is its own problem.
fn hash_list(values: &[String]) -> String {
    if values.is_empty() {
        return "0".to_string();
    }
    let mut sorted: Vec<&String> = values.iter().collect();
    sorted.sort();
    let mut hash: u32 = 0x811c_9dc5;
    for value in sorted {
        for byte in value.bytes() {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash ^= 0x2c;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:x}")
}

/// What a shed request is answered with: an EMPTY rail, and no database work at all.
///
/// The gate exists to keep a burst off the pool, so answering it with a query would spend exactly
/// what it was raised to protect -- the harder the burst, the more scans it would start. The shop
/// hides an empty rail, so a saturated process degrades to "no suggestions" rather than to a failed
/// request on the storefront.
fn shed_response() -> Arc<SuggestionsResponse> {
    Arc::new(SuggestionsResponse {
        data: Vec::new(),
        personalized: false,
        algorithm: super::constants::ALGORITHM_VERSION.to_string(),
    })
}

/// Bounded on the write, because nothing else bounds it.
///
/// `TtlMap` enforces its cap inside `get_or_fetch` alone, and this path cannot use it: the read
/// happens before the shed gate and the write after it. Without this the map holds one entry per
/// distinct wallet and per distinct seed set for the life of the process.
fn store(cache: &SuggestionsCache, key: String, response: Arc<SuggestionsResponse>) {
    if cache.len() >= SUGGESTIONS_CACHE_ENTRIES {
        cache.retain_fresh();
        if cache.len() >= SUGGESTIONS_CACHE_ENTRIES {
            cache.clear();
        }
    }
    cache.insert(key, response);
}

/// How many trending rows to ask for so the fallback can still fill after its own filters.
///
/// The ceiling is the trending rail's own maximum -- asking past it is clamped there anyway, so
/// naming it keeps this from claiming headroom it never gets.
fn fallback_first(limit: usize, filters_rows: bool) -> i64 {
    if filters_rows {
        ((limit as i64) * 2).min(TRENDING_MAX_LIMIT)
    } else {
        limit as i64
    }
}

/// The ranked head, paired back with the rows it was ranked from.
///
/// `personalized` is an absolute count, not a share of what came back: a rail of three rows with
/// three reasons is not a personal rail, it is a thin one, and the shop hides the rail outright
/// on `false` rather than showing something generic under a personal heading.
fn build_response(
    ranked: Vec<BlendedCandidate>,
    mut by_id: HashMap<String, UnifiedItem>,
) -> SuggestionsResponse {
    let mut personal_rows = 0usize;
    let mut data: Vec<SuggestedItem> = Vec::with_capacity(ranked.len());
    for entry in ranked {
        let Some(item) = by_id.remove(&entry.candidate.item_id) else {
            continue;
        };
        if entry.reason.kind != SuggestionReasonKind::Trending {
            personal_rows += 1;
        }
        data.push(SuggestedItem {
            item,
            reason: entry.reason,
            score: entry.score,
        });
    }

    SuggestionsResponse {
        personalized: personal_rows >= MIN_PERSONAL_ROWS,
        algorithm: super::constants::ALGORITHM_VERSION.to_string(),
        data,
    }
}

pub fn unified_item_id(item: &UnifiedItem) -> String {
    match item.item_id.as_deref() {
        Some(id) => format!("{}-{}", item.contract_address, id),
        None => item.contract_address.clone(),
    }
}

fn sub_category_of(item: &UnifiedItem) -> String {
    format!(
        "{}:{}",
        item.category,
        item.wearable_category.clone().unwrap_or_default()
    )
}

/// The caller's category and body-shape narrowing, applied AFTER hydration.
///
/// Both are properties of the hydrated row, and pushing them into the candidate query would mean
/// a second definition of "is this a wearable" living next to the grid's.
fn filter_by_request(items: Vec<UnifiedItem>, request: &SuggestionRequest) -> Vec<UnifiedItem> {
    let category = request.category.as_deref();
    let body_shape = request.body_shape.as_deref();
    items
        .into_iter()
        .filter(|item| match category {
            Some("emote") => item.category == "emote",
            Some("wearable") => item.category != "emote",
            _ => true,
        })
        .filter(|item| {
            !matches!(
                (body_shape, item.gender.as_deref()),
                (Some("BaseMale"), Some("female")) | (Some("BaseFemale"), Some("male"))
            )
        })
        .collect()
}

fn creator_counts(
    profile: &[ProfileEntry],
    attributes: &HashMap<String, ProfileItemAttributes>,
) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in profile {
        if let Some(attribute) = attributes.get(&entry.item_id) {
            if !attribute.creator.is_empty() {
                *counts.entry(attribute.creator.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_size_is_clamped_to_the_rails_range() {
        assert_eq!(clamp_first(None), SUGGESTED_DEFAULT_LIMIT);
        assert_eq!(clamp_first(Some(0)), 1);
        assert_eq!(clamp_first(Some(-5)), 1);
        assert_eq!(clamp_first(Some(9999)), SUGGESTED_MAX_LIMIT);
        assert_eq!(clamp_first(Some(7)), 7);
    }

    fn item(category: &str, gender: Option<&str>) -> UnifiedItem {
        use crate::dcl_schemas::{ChainId, Network};
        UnifiedItem {
            source: "native".into(),
            acquisition: "trade".into(),
            trade_id: None,
            listing_type: "public_item_order".into(),
            contract_address: "0xc".into(),
            item_id: Some("1".into()),
            token_id: None,
            name: String::new(),
            thumbnail: String::new(),
            rarity: "rare".into(),
            category: category.into(),
            wearable_category: None,
            emote_loop: None,
            gender: gender.map(str::to_string),
            creator: "0xcreator".into(),
            seller: None,
            issued_id: None,
            price_credits: 10,
            sale: Default::default(),
            mana_wei: None,
            listing_count: 1,
            available: 1,
            network: Network::Matic,
            chain_id: ChainId::MaticMainnet,
            created_at: 0,
        }
    }

    /// A caller asking for emotes must not be handed wearables, and the split is read off the
    /// hydrated row rather than re-derived.
    #[test]
    fn the_category_narrowing_is_applied_to_hydrated_rows() {
        let items = vec![item("emote", None), item("wearable", None)];
        let request = SuggestionRequest {
            category: Some("emote".into()),
            ..Default::default()
        };
        let kept = filter_by_request(items, &request);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].category, "emote");
    }

    /// A unisex item fits either body shape; only the opposite-exclusive one is dropped.
    #[test]
    fn a_body_shape_drops_only_the_opposite_exclusive_wearable() {
        let items = vec![
            item("wearable", Some("male")),
            item("wearable", Some("female")),
            item("wearable", Some("unisex")),
            item("wearable", None),
        ];
        let request = SuggestionRequest {
            body_shape: Some("BaseFemale".into()),
            ..Default::default()
        };
        let kept = filter_by_request(items, &request);
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|i| i.gender.as_deref() != Some("male")));
    }

    #[test]
    fn a_unified_item_id_is_the_contract_and_item_id() {
        assert_eq!(unified_item_id(&item("wearable", None)), "0xc-1");
    }

    fn row(contract: &str, item_id: &str) -> UnifiedItem {
        let mut row = item("wearable", None);
        row.contract_address = contract.into();
        row.item_id = Some(item_id.into());
        row
    }

    fn blended(item_id: &str, kind: SuggestionReasonKind, score: f64) -> BlendedCandidate {
        BlendedCandidate {
            candidate: ScoredCandidate {
                item_id: item_id.to_string(),
                collection: "0xc".into(),
                creator: "0xcreator".into(),
                sub_category: "wearable:hat".into(),
                rarity: "rare".into(),
                price_credits: 10.0,
                is_wearable: true,
                cf: 0.0,
                content: 0.0,
                popularity: 0.0,
                top_trigger_item_id: None,
                top_trigger_source: None,
            },
            taste: 0.0,
            score,
            reason: SuggestionReason {
                kind,
                item_id: None,
                creator: None,
            },
        }
    }

    fn response_of(kinds: &[SuggestionReasonKind]) -> SuggestionsResponse {
        let ranked: Vec<BlendedCandidate> = kinds
            .iter()
            .enumerate()
            .map(|(i, kind)| blended(&format!("0xc-{i}"), *kind, i as f64 + 1.0))
            .collect();
        let by_id: HashMap<String, UnifiedItem> = (0..kinds.len())
            .map(|i| (format!("0xc-{i}"), row("0xc", &i.to_string())))
            .collect();
        build_response(ranked, by_id)
    }

    /// The threshold is an absolute count. A three-row rail with three reasons is a thin rail,
    /// not a personal one, and the shop hides the rail outright on `false`.
    #[test]
    fn three_personal_rows_are_not_a_personalised_rail() {
        let response = response_of(&[SuggestionReasonKind::CoOwned; 3]);
        assert_eq!(response.data.len(), 3);
        assert!(!response.personalized);
    }

    #[test]
    fn four_personal_rows_are() {
        let response = response_of(&[SuggestionReasonKind::CoOwned; 4]);
        assert!(response.personalized);
    }

    /// A trending row inside a personal rail does not count towards the threshold.
    #[test]
    fn a_trending_row_does_not_count_as_personal() {
        let response = response_of(&[
            SuggestionReasonKind::CoOwned,
            SuggestionReasonKind::CoOwned,
            SuggestionReasonKind::CoOwned,
            SuggestionReasonKind::Trending,
        ]);
        assert!(!response.personalized);
    }

    /// The shop declares `score` required and falls back to zero only for an older server, so a
    /// row that omits it pins every consumer to the degraded branch.
    #[test]
    fn every_row_carries_its_score_on_the_wire() {
        let response = response_of(&[SuggestionReasonKind::CoOwned]);
        let json = serde_json::to_value(&response).expect("serialises");
        let first = &json["data"][0];
        assert!(first["score"].is_number(), "{first}");
        assert_eq!(first["score"].as_f64(), Some(1.0));
        assert!(first["reason"]["itemId"].is_null(), "{first}");
    }

    #[test]
    fn the_shed_answer_is_an_empty_unpersonalised_rail() {
        let response = shed_response();
        assert!(response.data.is_empty());
        assert!(!response.personalized);
        assert_eq!(
            response.algorithm,
            crate::ports::suggestions::constants::ALGORITHM_VERSION
        );
    }

    /// The key varies on caller-supplied data on a public route, so the map has to be bounded on
    /// the write: `TtlMap` bounds itself only inside `get_or_fetch`, which this path cannot use.
    #[test]
    fn the_response_cache_cannot_grow_past_its_cap() {
        let cache: SuggestionsCache = catalyrst_commons::cache::TtlMap::bounded(
            "market_suggestions_test",
            std::time::Duration::from_secs(SUGGESTIONS_CACHE_TTL_SECONDS),
            SUGGESTIONS_CACHE_ENTRIES,
        );
        for i in 0..(SUGGESTIONS_CACHE_ENTRIES + 64) {
            store(&cache, format!("suggestions:v1:{i}"), shed_response());
        }
        assert!(
            cache.len() <= SUGGESTIONS_CACHE_ENTRIES + 1,
            "cache grew to {}",
            cache.len()
        );
    }

    /// Every post-query filter drops rows, so the query is asked for more than the rail shows --
    /// but never past the trending rail's own ceiling.
    #[test]
    fn the_fallback_overfetches_only_when_something_will_drop_rows() {
        assert_eq!(fallback_first(12, false), 12);
        assert_eq!(fallback_first(12, true), 24);
        assert_eq!(fallback_first(40, true), TRENDING_MAX_LIMIT);
    }

    /// A signed and an unsigned answer for the same `?address=` must never share an entry: the
    /// signed one names the caller's saved items in its reasons.
    #[test]
    fn the_cache_key_separates_a_signed_answer_from_an_unsigned_one() {
        let request = SuggestionRequest {
            wallet: Some("0xa".into()),
            ..Default::default()
        };
        let lists = NormalizedLists::default();
        let unsigned = cache_key(&request, Some("0xa"), None, &lists, 12);
        let signed = cache_key(&request, Some("0xa"), Some("0xa"), &lists, 12);
        assert_ne!(unsigned, signed);
        assert!(unsigned.contains(":unsigned:"), "{unsigned}");
        assert!(signed.starts_with("suggestions:v1:0xa:0xa:"), "{signed}");
    }

    /// Everything the caller varies has to reach the key, or two different requests share one
    /// expensive answer.
    #[test]
    fn the_cache_key_covers_every_list_and_narrowing_the_caller_sends() {
        let base = SuggestionRequest::default();
        let lists = NormalizedLists::default();
        let key = cache_key(&base, None, None, &lists, 12);

        let seeded = NormalizedLists {
            seeds: vec!["0xc-1".into()],
            ..Default::default()
        };
        assert_ne!(key, cache_key(&base, None, None, &seeded, 12));

        let equipped = NormalizedLists {
            equipped: vec!["0xc-1".into()],
            ..Default::default()
        };
        assert_ne!(key, cache_key(&base, None, None, &equipped, 12));
        assert_ne!(
            cache_key(&base, None, None, &seeded, 12),
            cache_key(&base, None, None, &equipped, 12),
            "the same id in a different list is a different request"
        );

        let excluded = NormalizedLists {
            exclude: vec!["0xc-1".into()],
            ..Default::default()
        };
        assert_ne!(key, cache_key(&base, None, None, &excluded, 12));

        let narrowed = SuggestionRequest {
            category: Some("emote".into()),
            body_shape: Some("BaseMale".into()),
            ..Default::default()
        };
        assert_ne!(key, cache_key(&narrowed, None, None, &lists, 12));
        assert_ne!(key, cache_key(&base, None, None, &lists, 13));
    }

    /// Order is not a request: the same ids sent in a different order are one cached answer.
    #[test]
    fn the_list_hash_does_not_depend_on_order() {
        assert_eq!(
            hash_list(&["0xc-1".to_string(), "0xc-2".to_string()]),
            hash_list(&["0xc-2".to_string(), "0xc-1".to_string()])
        );
        assert_ne!(hash_list(&[]), hash_list(&["0xc-1".to_string()]));
    }
}
