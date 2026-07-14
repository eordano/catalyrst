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
/// ONE lucky item — upstream saw a creator rank 3rd on 33 windowed sales whose
/// whole catalogue was four items. The rail exists to send a shopper off to
/// browse, and four items is not something to browse. The value comes off
/// upstream's production distribution (median candidate: 36 published items;
/// nothing sits near the line).
pub const TOP_CREATORS_MIN_ITEMS: i64 = 10;
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
/// Both signals are kept because either alone misleads — a 1-credit item that
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
    pub min_price_credits: Option<f64>,
    pub max_price_credits: Option<f64>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
    /// Whether SOCIAL emotes (emotes carrying an outcome type) may appear. `true`
    /// (included) is the default and matches /v1/items, /v2/catalog and
    /// /v1/trendings; only the shared unified feed (`append_unified_filters`,
    /// backing /v3/catalog/unified, /related and /trending) reads it — the
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

/// A creator ranked by how much of THEIR catalogue sold in the window.
///
/// Deliberately not `/v1/rankings/{entity}/{timeframe}` (entity=creators), which
/// reads the squid's per-account day rollups and so counts only sales where the
/// creator's own address was the SELLER. A primary mint is executed by the buyer
/// against the store, so it never lands there — and for a shop whose creators
/// sell mostly primary, that undercounts them severalfold (upstream measured 14
/// vs 35 over the same 30 days). This attributes a sale to whoever CREATED the
/// item (`sale.item_id = item.id` join), counting mints and resales alike.
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
    /// Sales in the requested window — what the ranking is ORDERED by.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sales: i64,
    /// Sales over all time — what the row DISPLAYS: a creator's standing, not
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
    pub(super) sales: i64,
    pub(super) total_sales: i64,
    pub(super) collections: i64,
    pub(super) items: i64,
}

/// Clamp the row count to `[TOP_CREATORS_MIN_LIMIT, TOP_CREATORS_MAX_LIMIT]`,
/// defaulting when absent — mirrors upstream's `clampCount`.
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

/// Clamp the trending row count to `[SHOP_MIN_PAGE_SIZE, TRENDING_MAX_LIMIT]`,
/// defaulting when absent — mirrors upstream's `clampCount`.
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
