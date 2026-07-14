use serde::Serialize;

use crate::dcl_schemas::{ChainId, Network};
use crate::http::params::Params;

pub const SHOP_DEFAULT_PAGE_SIZE: i64 = 48;
pub const SHOP_MIN_PAGE_SIZE: i64 = 1;
pub const SHOP_MAX_PAGE_SIZE: i64 = 1000;

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

#[derive(Debug, Clone, Default)]
pub struct ShopCatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub category: Option<String>,
    pub contract_address: Option<String>,
    pub item_id: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_categories: Vec<String>,
    pub min_price_credits: Option<f64>,
    pub max_price_credits: Option<f64>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
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

// A seller's OLD classic (ERC20-MANA) listing that can be re-listed into the Shop as
// credit-buyable. Carries the raw MANA price (client converts to credits via the oracle).
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

// A classic (ERC20-MANA) PRIMARY listing surfaced as a paginated browse feed so the Shop can
// offer the "old liquidity" for purchase with credits. Primaries only: secondary-with-credits is
// disabled upstream, so public_nft_order rows are excluded entirely.
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
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        min_price_credits: p.get_number("minPriceCredits", None),
        max_price_credits: p.get_number("maxPriceCredits", None),
        search: p.get_string("search", None),
        sort_by: p
            .get_value("sortBy", SHOP_SORT_VALUES, None)
            .as_deref()
            .and_then(ShopSortBy::parse),
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
