mod component;
mod queries;
mod rows;
mod sql;
mod types;

#[cfg(test)]
mod tests;

pub use component::CatalogComponent;
pub use queries::{
    build_collections_items_catalog_query, build_collections_items_catalog_query_with_trades,
    build_collections_items_count_query,
};
pub use types::{
    CatalogFilters, CatalogItem, CatalogSortBy, CatalogSortDirection, EmoteData, ItemData,
    PickStats, WearableData,
};

use super::catalog_cache;

#[cfg(test)]
use rows::from_db_row_to_catalog_item;
#[cfg(test)]
use sql::{build_collections_where, build_item_level_filters_where, Builder};
#[cfg(test)]
use types::DbRow;

pub const FRAGMENT_WEARABLE_V1: &str = "wearable_v1";
pub const FRAGMENT_WEARABLE_V2: &str = "wearable_v2";
pub const FRAGMENT_SMART_WEARABLE_V1: &str = "smart_wearable_v1";
pub const FRAGMENT_EMOTE_V1: &str = "emote_v1";

const WEARABLE_ITEM_TYPES: [&str; 3] = [
    FRAGMENT_WEARABLE_V1,
    FRAGMENT_WEARABLE_V2,
    FRAGMENT_SMART_WEARABLE_V1,
];

const MAX_NUMERIC_NUMBER: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

const MAX_ORDER_TIMESTAMP: i64 = 253_378_408_747_000;
