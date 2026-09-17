//! Every constant here was fixed by upstream's phase 0 offline evaluation (temporal holdout
//! over dated acquisitions). Do not re-tune them without re-running that evaluation: the
//! weights are only meaningful together.

/// Bumped whenever the maths change, so a stale table is recognisable and an A/B can split on it.
pub const ALGORITHM_VERSION: &str = "v1";

pub const NEIGHBORS_TABLE: &str = "marketplace.item_neighbors";
pub const NEIGHBORS_TABLE_NAME: &str = "item_neighbors";
pub const NEIGHBORS_META_TABLE: &str = "marketplace.item_neighbors_meta";
pub const NEIGHBORS_ITEM_INDEX: &str = "idx_item_neighbors_item_id";

pub const NEIGHBOR_SOURCE_CF: &str = "cf";
pub const NEIGHBOR_SOURCE_CONTENT: &str = "content";

/// Neighbours kept per anchor item, per source.
pub const NEIGHBORS_PER_ITEM: usize = 50;
/// A co-ownership pair needs this many co-owners before it is trusted at all.
pub const MIN_CO_OWNERS: u32 = 3;
/// Shrinks the cosine towards zero for thin pairs: sim * co / (co + this).
pub const CO_OWNERSHIP_SHRINKAGE: f64 = 10.0;
/// Owners outside this band contribute nothing to co-ownership: a wallet holding one item has
/// no pair to offer, and past the ceiling a wallet correlates everything with everything. The
/// ceiling counts PURCHASES, not holdings, so it is far higher than it looks.
pub const MIN_WALLET_ITEMS: usize = 2;
pub const MAX_WALLET_ITEMS: usize = 500;
/// Taste weights decay with e^(-age_days / this).
pub const RECENCY_DECAY_DAYS: f64 = 365.0;
/// Tags this common carry no IDF signal; including them only inflates the content pass.
pub const MAX_TAG_DOCUMENT_FREQUENCY: usize = 2000;

pub struct ContentWeights {
    pub creator: f64,
    pub collection: f64,
    pub sub_category: f64,
    pub rarity: f64,
    pub tags: f64,
    pub price_band: f64,
}

pub const CONTENT_WEIGHTS: ContentWeights = ContentWeights {
    creator: 0.35,
    collection: 0.25,
    sub_category: 0.15,
    rarity: 0.1,
    tags: 0.1,
    price_band: 0.05,
};

pub struct ScoreWeights {
    pub cf: f64,
    pub content: f64,
    pub taste: f64,
    pub popularity: f64,
}

/// Final blend over the per-wallet max-normalised components.
pub const SCORE_WEIGHTS: ScoreWeights = ScoreWeights {
    cf: 0.45,
    content: 0.25,
    taste: 0.2,
    popularity: 0.1,
};

/// How much each signal about a wallet is worth, relative to a purchase.
///
/// What the avatar is wearing and what it has favourited outrank a purchase, and carry no age
/// decay: they are statements about what the wallet likes NOW, while a purchase is a statement
/// about what it liked on the day it was made. A seed -- something viewed or put in the cart
/// this session -- is real but weaker intent.
pub struct ProfileWeights {
    pub paid: f64,
    pub favorite: f64,
    pub equipped: f64,
    pub seed: f64,
}

pub const PROFILE_WEIGHTS: ProfileWeights = ProfileWeights {
    paid: 1.0,
    favorite: 1.2,
    equipped: 1.5,
    seed: 0.8,
};

/// What an unpaid acquisition -- an airdrop, a free claim, a gift -- is worth as evidence of
/// taste: zero, measured. Both pipelines therefore drop unpaid acquisitions in SQL rather than
/// carrying them at zero weight. The constant stays as the single place the decision is recorded.
pub const FREE_ACQUISITION_WEIGHT: f64 = 0.0;

/// Scarcest-first, mirroring @dcl/schemas' `Rarity` enum order. Only ADJACENCY is read, which
/// is symmetric, so the direction is immaterial -- but the set has to be the enum's.
pub const RARITY_TIERS: &[&str] = &[
    "unique",
    "mythic",
    "exotic",
    "legendary",
    "epic",
    "rare",
    "uncommon",
    "common",
];

/// Diversity caps applied to the re-ranked head.
pub const MAX_PER_COLLECTION: usize = 2;
pub const MAX_PER_CREATOR: usize = 3;
/// Wearable share used when the profile does not imply one.
pub const DEFAULT_WEARABLE_RATIO: f64 = 0.7;
/// Below this many personally-sourced rows the response is not worth calling personalised.
pub const MIN_PERSONAL_ROWS: usize = 4;

pub const SUGGESTED_DEFAULT_LIMIT: i64 = 12;
pub const SUGGESTED_MAX_LIMIT: i64 = 40;
pub const MAX_SEEDS: usize = 20;
/// Favourites read per request, on their own budget rather than sharing the seeds': they come
/// from a list the caller SAVED deliberately, and sharing one cap with a browsing session would
/// let that session push the deliberate signal out of the profile entirely.
pub const MAX_FAVORITES: usize = 20;
pub const MAX_EQUIPPED: usize = 30;
pub const MAX_EXCLUDE: usize = 20;
/// Strongest profile entries carried into the scoring query. Every entry becomes three bind
/// parameters in a VALUES list, so an uncapped profile blows Postgres' 65535-parameter limit and
/// the request fails outright.
pub const MAX_PROFILE_ITEMS: usize = 200;
/// Holdings fetched before the profile is assembled. The SQL orders by the same weight formula
/// the profile uses, so what is cut is what the profile would have cut anyway.
pub const PROFILE_SQL_LIMIT: i64 = 400;

/// Creators whose recent catalogue is pulled in alongside the neighbour-driven candidates, and
/// how many items each contributes. Without this branch a brand-new drop from a creator the
/// wallet collects can only surface if some neighbour happens to point at it.
pub const TASTE_CREATOR_COUNT: usize = 3;
pub const TASTE_ITEMS_PER_CREATOR: i64 = 30;

/// Candidates pulled from SQL before the diversity re-rank trims to `first`.
pub const CANDIDATE_MULTIPLIER: i64 = 3;

pub const SUGGESTIONS_CACHE_TTL_SECONDS: u64 = 600;

/// Suggestion computations allowed in flight at once, past which the rail sheds instead of
/// queueing. A cache MISS here is seconds of database work across three queries; without a
/// ceiling, enough concurrent misses hold every connection in the pool and every OTHER route
/// waits behind a rail the shop treats as optional. Shedding returns an empty, unpersonalised
/// rail rather than an error, which the shop already hides.
pub const SUGGESTIONS_MAX_CONCURRENT: usize = 2;

/// Quietest a saturated process stays between two shed-warning lines.
pub const SHED_LOG_INTERVAL_MS: u64 = 60_000;

/// How often the neighbours job rebuilds the table.
pub const NEIGHBORS_REBUILD_INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;
/// Let a freshly started process finish warming up before a multi-minute scan starts.
pub const NEIGHBORS_REBUILD_STARTUP_DELAY_MS: u64 = 5 * 60 * 1000;
/// The job's own connections need far longer than a request does, but not unbounded.
pub const NEIGHBORS_JOB_STATEMENT_TIMEOUT_MS: u64 = 300_000;
/// Past this the acquisition scan is abandoned and the previous table keeps serving.
pub const ACQUISITION_SCAN_DEADLINE_MS: u64 = 240_000;
/// Rows per INSERT into the staging table.
pub const NEIGHBORS_INSERT_BATCH_SIZE: usize = 5000;

/// The only body shapes an avatar has, and so the only values worth carrying into a query or a
/// cache key. Anything else is discarded rather than passed through: an unrecognised shape
/// filters nothing, so letting it vary the key would let a caller ask for the same expensive
/// answer under endless different names.
pub const BODY_SHAPES: &[&str] = &["BaseMale", "BaseFemale"];

/// Same reasoning for the category split the rail supports.
pub const SUGGESTION_CATEGORIES: &[&str] = &["wearable", "emote"];

pub fn normalize_body_shape(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    BODY_SHAPES
        .iter()
        .find(|shape| **shape == value)
        .map(|shape| (*shape).to_string())
}

pub fn normalize_category(value: Option<&str>) -> Option<String> {
    let lowered = value?.trim().to_lowercase();
    SUGGESTION_CATEGORIES
        .iter()
        .find(|category| **category == lowered)
        .map(|category| (*category).to_string())
}
