use serde::Serialize;

use crate::dcl_schemas::{ChainId, Network};
use crate::http::params::Params;

pub const SHOP_DEFAULT_PAGE_SIZE: i64 = 48;
pub const SHOP_MIN_PAGE_SIZE: i64 = 1;
pub const SHOP_MAX_PAGE_SIZE: i64 = 1000;

/// Look-back window and size for the shop's creator rail (`/v3/catalog/creators`,
/// marketplace-server #389). Both the row count and the window are clamped.
///
/// `TOP_CREATORS_MIN_ITEMS` is the smallest published catalogue a "top creator"
/// can have (#390). Ranking over a 30-day window means a month can be won on
/// ONE lucky item -- upstream saw a creator rank 3rd on 33 windowed sales whose
/// whole catalogue was four items. The rail exists to send a shopper off to
/// browse, and four items is not something to browse. The value comes off
/// upstream's production distribution (median candidate: 36 published items;
/// nothing sits near the line).
pub const TOP_CREATORS_MIN_ITEMS: i64 = 10;

/// Smallest number of windowed sales a "top creator" can be ranked on,
/// expressed as a RATE: this many per default-length window, scaled to whatever
/// window was asked for (#394).
///
/// Ordering by REVENUE is what makes a floor necessary. A count cannot be won
/// by a single event but a sum can: one expensive sale would outrank a month of
/// ordinary trading. A RATE rather than a fixed number because the window is a
/// caller-supplied 1-365 and a flat floor falls off a cliff at the short end --
/// five sales is an ordinary month but an exceptional week, and a flat five
/// emptied a 7-day ranking outright on upstream's production data (42 creators
/// qualified, none cleared it).
pub const TOP_CREATORS_MIN_SALES_PER_WINDOW: i64 = 5;

/// Floor beneath the scaled rate. Whatever the window, ranking a creator on ONE
/// sale is ranking them on the price of that sale, which is what the floor
/// exists to prevent.
pub const TOP_CREATORS_MIN_WINDOW_SALES_FLOOR: i64 = 2;

pub const TOP_CREATORS_DEFAULT_LIMIT: i64 = 30;
pub const TOP_CREATORS_MIN_LIMIT: i64 = 1;
pub const TOP_CREATORS_MAX_LIMIT: i64 = 60;
pub const TOP_CREATORS_DEFAULT_DAYS: i64 = 30;
pub const TOP_CREATORS_MIN_DAYS: i64 = 1;
pub const TOP_CREATORS_MAX_DAYS: i64 = 365;

/// Row count and look-back window for the trending rail (`/v3/catalog/trending`,
/// marketplace-server #384). One carousel, so the caps sit well below the browse
/// page size; the window is capped at a week because it is a full scan of `sale`
/// above a timestamp.
pub const TRENDING_DEFAULT_LIMIT: i64 = 12;
pub const TRENDING_MAX_LIMIT: i64 = 50;
pub const TRENDING_DEFAULT_DAYS: i64 = 1;
pub const TRENDING_MIN_DAYS: i64 = 1;
pub const TRENDING_MAX_DAYS: i64 = 7;

/// Share of the rail's slots that go to the highest sale COUNT; the remaining
/// 40% go to the biggest TRADED VOLUME among whatever the first pass left behind.
/// Both signals are kept because either alone misleads -- a 1-credit item that
/// sold 50 times would bury a 200-credit item that sold 10, and volume alone is
/// dominated by a single expensive sale.
pub const TRENDING_SALES_CUT: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSortBy {
    Newest,
    Cheapest,
    MostExpensive,
    Name,
}

pub const SHOP_SORT_VALUES: &[&str] = &["newest", "cheapest", "most_expensive", "name"];

impl ShopSortBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "newest" => Some(Self::Newest),
            "cheapest" => Some(Self::Cheapest),
            "most_expensive" => Some(Self::MostExpensive),
            "name" => Some(Self::Name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShopCatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub category: Option<String>,
    pub contract_address: Option<String>,
    pub item_id: Option<String>,
    pub creator: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_categories: Vec<String>,
    /// Restrict to smart wearables; gated on the query param's presence, not its value.
    pub is_smart: bool,
    /// `wearableGender` values (`male`/`female`/`unisex`) the item's body shapes
    /// must SATISFY: the row must declare ALL the shapes they map to, so
    /// `[male]` keeps male-exclusive AND unisex items while `[male, female]` --
    /// equivalently `[unisex]` -- keeps only what ships both, which is what the
    /// outgoing `gender` reports as `unisex`. Wearables-only, like the param it
    /// is named after: an emote declares no wearable body shapes. Only the
    /// unified feed populates it (`parse_unified_filters`); /v3/catalog/shop
    /// and the trending rail leave it empty, as upstream does.
    pub wearable_genders: Vec<String>,
    pub min_price_credits: Option<f64>,
    pub max_price_credits: Option<f64>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
    /// Whether SOCIAL emotes (emotes carrying an outcome type) may appear. `true`
    /// (included) is the default and matches /v1/items, /v2/catalog and
    /// /v1/trendings; only the shared unified feed (`append_unified_filters`,
    /// backing /v3/catalog/unified, /related and /trending) reads it -- the
    /// per-listing /v3/catalog/shop path leaves it untouched.
    pub include_social_emotes: bool,
}

impl Default for ShopCatalogFilters {
    fn default() -> Self {
        Self {
            first: None,
            skip: None,
            category: None,
            contract_address: None,
            item_id: None,
            creator: None,
            rarities: Vec::new(),
            wearable_categories: Vec::new(),
            is_smart: false,
            wearable_genders: Vec::new(),
            min_price_credits: None,
            max_price_credits: None,
            search: None,
            sort_by: None,
            include_social_emotes: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LegacyCatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub category: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_categories: Vec<String>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ShopListing {
    pub trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub token_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "\"male\" | \"female\" | \"unisex\" | null"))]
    pub gender: Option<String>,
    pub creator: String,
    /// Reseller (current owner of the sent NFT); null for primary listings.
    pub seller: Option<String>,
    /// NFT mint index (issued id); null for primary listings.
    pub issued_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub price_credits: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ImportableListing {
    pub old_trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub token_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    pub mana_wei: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct LegacyListing {
    pub trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "\"male\" | \"female\" | \"unisex\" | null"))]
    pub gender: Option<String>,
    pub creator: String,
    pub mana_wei: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ShopListingRow {
    pub(super) trade_id: String,
    pub(super) trade_type: String,
    pub(super) contract_address: Option<String>,
    pub(super) item_id: Option<String>,
    pub(super) token_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) image: Option<String>,
    pub(super) rarity: Option<String>,
    pub(super) item_type: Option<String>,
    pub(super) wearable_category: Option<String>,
    pub(super) gender: Option<String>,
    pub(super) creator: Option<String>,
    pub(super) seller: Option<String>,
    pub(super) issued_id: Option<String>,
    pub(super) price: Option<String>,
    pub(super) available: Option<String>,
    pub(super) network: Option<String>,
    pub(super) created_at: i64,
    pub(super) total: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ImportableListingRow {
    pub(super) old_trade_id: String,
    pub(super) trade_type: String,
    pub(super) contract_address: Option<String>,
    pub(super) item_id: Option<String>,
    pub(super) token_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) image: Option<String>,
    pub(super) rarity: Option<String>,
    pub(super) item_type: Option<String>,
    pub(super) wearable_category: Option<String>,
    pub(super) mana_wei: Option<String>,
    pub(super) available: Option<String>,
    pub(super) network: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct LegacyListingRow {
    pub(super) trade_id: String,
    pub(super) contract_address: Option<String>,
    pub(super) item_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) image: Option<String>,
    pub(super) rarity: Option<String>,
    pub(super) item_type: Option<String>,
    pub(super) wearable_category: Option<String>,
    pub(super) gender: Option<String>,
    pub(super) creator: Option<String>,
    pub(super) mana_wei: Option<String>,
    pub(super) available: Option<String>,
    pub(super) network: Option<String>,
    pub(super) created_at: i64,
    pub(super) total: i64,
}

/// A creator ranked by how much of THEIR catalogue EARNED in the window.
///
/// Deliberately not `/v1/rankings/{entity}/{timeframe}` (entity=creators), which
/// reads the squid's per-account day rollups and so counts only sales where the
/// creator's own address was the SELLER. A primary mint is executed by the buyer
/// against the store, so it never lands there -- and for a shop whose creators
/// sell mostly primary, that undercounts them severalfold (upstream measured 14
/// vs 35 over the same 30 days). This attributes a sale to whoever CREATED the
/// item (`sale.item_id = item.id` join), counting mints and resales alike.
///
/// Ranked on REVENUE rather than on units (#394), because the two disagree
/// sharply and only one is the question the rail asks. Upstream measured the
/// month's second-highest EARNING creator placing twelfth on unit count, their
/// items selling at ~400 MANA against a ~100 MANA field: a row ordered by units
/// puts a month of cheap sales above four times the trade.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct TopCreator {
    /// Creator wallet address (lowercase).
    pub id: String,
    /// MANA taken in the window, in wei -- what the ranking is ORDERED by. A
    /// string because it is a sum of raw on-chain prices and overflows an f64,
    /// same reason and same shape as a listing's `manaWei`.
    pub volume_wei: String,
    /// Sales in the requested window. Not the sort key -- the floor the sort
    /// key has to clear.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sales: i64,
    /// Sales over all time -- what the row DISPLAYS: a creator's standing, not
    /// their last month.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_sales: i64,
    /// Approved collections they have published.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub collections: i64,
    /// Approved items across those collections.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub items: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TopCreatorRow {
    pub(super) creator: String,
    pub(super) volume: String,
    pub(super) sales: i64,
    pub(super) total_sales: i64,
    pub(super) collections: i64,
    pub(super) items: i64,
}

/// Clamp the row count to `[TOP_CREATORS_MIN_LIMIT, TOP_CREATORS_MAX_LIMIT]`,
/// defaulting when absent -- mirrors upstream's `clampCount`.
pub(super) fn top_creators_clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(TOP_CREATORS_DEFAULT_LIMIT)
        .clamp(TOP_CREATORS_MIN_LIMIT, TOP_CREATORS_MAX_LIMIT)
}

/// Clamp the look-back window to `[TOP_CREATORS_MIN_DAYS, TOP_CREATORS_MAX_DAYS]`,
/// defaulting when absent.
pub(super) fn top_creators_clamp_days(days: Option<i64>) -> i64 {
    days.unwrap_or(TOP_CREATORS_DEFAULT_DAYS)
        .clamp(TOP_CREATORS_MIN_DAYS, TOP_CREATORS_MAX_DAYS)
}

/// Scale [`TOP_CREATORS_MIN_SALES_PER_WINDOW`] to an ALREADY-CLAMPED window,
/// held above [`TOP_CREATORS_MIN_WINDOW_SALES_FLOOR`] so a one-day window never
/// rounds the bar down to where a single sale wins the revenue ranking.
pub(super) fn top_creators_min_sales(days: i64) -> i64 {
    let scaled =
        (days as f64 / TOP_CREATORS_DEFAULT_DAYS as f64) * TOP_CREATORS_MIN_SALES_PER_WINDOW as f64;
    (scaled.round() as i64).max(TOP_CREATORS_MIN_WINDOW_SALES_FLOOR)
}

/// Clamp the trending row count to `[SHOP_MIN_PAGE_SIZE, TRENDING_MAX_LIMIT]`,
/// defaulting when absent -- mirrors upstream's `clampCount`.
pub(super) fn trending_clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(TRENDING_DEFAULT_LIMIT)
        .clamp(SHOP_MIN_PAGE_SIZE, TRENDING_MAX_LIMIT)
}

/// Clamp the trending look-back window to `[TRENDING_MIN_DAYS, TRENDING_MAX_DAYS]`,
/// defaulting when absent.
pub(super) fn trending_clamp_days(days: Option<i64>) -> i64 {
    days.unwrap_or(TRENDING_DEFAULT_DAYS)
        .clamp(TRENDING_MIN_DAYS, TRENDING_MAX_DAYS)
}

pub(super) fn csv(value: Option<String>) -> Vec<String> {
    value
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn finite_i64(v: Option<f64>) -> Option<i64> {
    v.filter(|n| n.is_finite()).map(|n| n as i64)
}

pub fn parse_shop_filters(pairs: &[(String, String)]) -> ShopCatalogFilters {
    let p = Params::new(pairs);
    ShopCatalogFilters {
        first: finite_i64(p.get_number("first", None)),
        skip: finite_i64(p.get_number("skip", None)),
        category: p.get_string("category", None),
        contract_address: p.get_string("contractAddress", None),
        item_id: p.get_string("itemId", None),
        creator: p.get_string("creator", None),
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        is_smart: p.get_boolean("isSmart"),
        wearable_genders: Vec::new(),
        min_price_credits: p.get_number("minPriceCredits", None),
        max_price_credits: p.get_number("maxPriceCredits", None),
        search: p.get_string("search", None),
        sort_by: p
            .get_value("sortBy", SHOP_SORT_VALUES, None)
            .as_deref()
            .and_then(ShopSortBy::parse),
        include_social_emotes: p.get_string("includeSocialEmotes", None).as_deref()
            != Some("false"),
    }
}

pub fn parse_legacy_filters(pairs: &[(String, String)]) -> LegacyCatalogFilters {
    let p = Params::new(pairs);
    LegacyCatalogFilters {
        first: finite_i64(p.get_number("first", None)),
        skip: finite_i64(p.get_number("skip", None)),
        category: p.get_string("category", None),
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        search: p.get_string("search", None),
        sort_by: p
            .get_value("sortBy", SHOP_SORT_VALUES, None)
            .as_deref()
            .and_then(ShopSortBy::parse),
    }
}
