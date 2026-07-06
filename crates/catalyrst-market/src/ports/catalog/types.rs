use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::dcl_schemas::{ChainId, Network, NftCategory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogSortBy {
    Newest,
    RecentlyListed,
    RecentlySold,
    Cheapest,
    MostExpensive,
    Suggested,
}

impl CatalogSortBy {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "newest" => Self::Newest,
            "recently_listed" => Self::RecentlyListed,
            "recently_sold" => Self::RecentlySold,
            "cheapest" => Self::Cheapest,
            "most_expensive" => Self::MostExpensive,
            "suggested" => Self::Suggested,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogSortDirection {
    Asc,
    Desc,
}

impl CatalogSortDirection {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "asc" => Self::Asc,
            "desc" => Self::Desc,
            _ => return None,
        })
    }

    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct CatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<CatalogSortBy>,
    pub sort_direction: Option<CatalogSortDirection>,
    pub only_listing: bool,
    pub only_minting: bool,
    pub category: Option<NftCategory>,
    pub creator: Vec<String>,
    pub is_sold_out: bool,
    pub is_on_sale: Option<bool>,
    pub search: Option<String>,
    pub is_wearable_head: bool,
    pub is_wearable_accessory: bool,
    pub is_wearable_smart: bool,
    pub wearable_category: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_genders: Vec<String>,
    pub emote_category: Option<String>,
    pub emote_genders: Vec<String>,
    pub emote_play_mode: Vec<String>,
    pub emote_has_geometry: bool,
    pub emote_has_sound: bool,
    pub emote_outcome_type: Option<String>,
    pub contract_addresses: Vec<String>,
    pub item_id: Option<String>,
    pub network: Option<Network>,
    pub max_price: Option<String>,
    pub min_price: Option<String>,
    pub urns: Vec<String>,
    pub ids: Vec<String>,
    pub picked_by: Option<String>,
    pub include_social_emotes: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WearableData {
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub rarity: String,
    #[serde(rename = "isSmart")]
    pub is_smart: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmoteData {
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub rarity: String,
    pub loop_: bool,
    #[serde(rename = "hasGeometry")]
    pub has_geometry: bool,
    #[serde(rename = "hasSound")]
    pub has_sound: bool,
    #[serde(rename = "outcomeType", skip_serializing_if = "Option::is_none")]
    pub outcome_type: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ItemData {
    Wearable { wearable: WearableData },
    Emote { emote: serde_json::Value },
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct PickStats {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub count: i64,
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "pickedByUser", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub picked_by_user: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct CatalogItem {
    pub id: String,
    pub beneficiary: Option<String>,
    #[serde(rename = "itemId")]
    pub item_id: String,
    pub name: String,
    pub thumbnail: String,
    pub url: String,
    pub urn: String,
    pub category: &'static str,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub rarity: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    #[serde(rename = "isOnSale")]
    pub is_on_sale: bool,
    pub creator: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub data: ItemData,
    pub network: Network,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    pub price: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[serde(rename = "reviewedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub reviewed_at: i64,
    #[serde(rename = "firstListedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub first_listed_at: Option<i64>,
    #[serde(rename = "soldAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sold_at: i64,
    #[serde(rename = "minPrice", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub min_price: Option<String>,
    #[serde(rename = "maxListingPrice")]
    pub max_listing_price: Option<String>,
    #[serde(rename = "minListingPrice")]
    pub min_listing_price: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub listings: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub owners: Option<i64>,
    pub picks: Option<PickStats>,
}

#[derive(Debug)]
pub(super) struct DbRow {
    pub(super) id: String,
    pub(super) blockchain_id: String,
    pub(super) image: String,
    pub(super) collection_id: String,
    pub(super) rarity: String,
    pub(super) item_type: String,
    pub(super) price: String,
    pub(super) available: String,
    pub(super) search_is_store_minter: bool,
    pub(super) search_is_marketplace_v3_minter: bool,
    pub(super) creator: String,
    pub(super) beneficiary: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) reviewed_at: String,
    pub(super) sold_at: String,
    pub(super) first_listed_at: Option<String>,
    pub(super) urn: String,
    pub(super) network: String,
    pub(super) metadata: Option<JsonValue>,
    pub(super) min_listing_price: Option<String>,
    pub(super) max_listing_price: Option<String>,
    pub(super) open_item_trade_id: Option<String>,
    pub(super) open_item_trade_price: Option<String>,
    pub(super) listings_count: Option<i64>,
    pub(super) owners_count: Option<i64>,
    pub(super) min_price: Option<String>,
    #[allow(dead_code)]
    pub(super) max_price: Option<String>,
}
