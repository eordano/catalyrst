use serde::Serialize;

use crate::dcl_schemas::{ChainId, Network, NftCategory};

#[derive(Debug, Clone, Default)]
pub struct NftFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<NftSortBy>,
    pub category: Option<NftCategory>,
    pub owner: Option<String>,
    pub tenant: Option<String>,
    pub is_on_sale: Option<bool>,
    pub is_on_rent: bool,
    pub search: Option<String>,
    pub is_land: bool,
    pub is_wearable_head: bool,
    pub is_wearable_accessory: bool,
    pub is_wearable_smart: bool,
    pub wearable_category: Option<String>,
    pub wearable_genders: Vec<String>,
    pub emote_category: Option<String>,
    pub emote_genders: Vec<String>,
    pub emote_play_mode: Vec<String>,
    pub contract_addresses: Vec<String>,
    pub creator: Vec<String>,
    pub token_id: Option<String>,
    pub item_rarities: Vec<String>,
    pub item_id: Option<String>,
    pub network: Option<Network>,
    pub rental_status: Vec<String>,
    pub adjacent_to_road: bool,
    pub min_distance_to_plaza: Option<f64>,
    pub max_distance_to_plaza: Option<f64>,
    pub min_estate_size: Option<f64>,
    pub max_estate_size: Option<f64>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
    pub emote_has_geometry: bool,
    pub emote_has_sound: bool,
    pub emote_outcome_type: Option<String>,
    pub rental_days: Vec<i64>,
    pub ids: Vec<String>,
    pub banned_names: Vec<String>,
    pub include_social_emotes: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSortBy {
    Name,
    Newest,
    RecentlyListed,
    RecentlySold,
    CheapestParcel,
}

impl NftSortBy {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "name" => Some(Self::Name),
            "newest" => Some(Self::Newest),
            "recently_listed" => Some(Self::RecentlyListed),
            "recently_sold" => Some(Self::RecentlySold),
            "cheapest_parcel" => Some(Self::CheapestParcel),
            _ => None,
        }
    }
}

pub struct NftErrors;
impl NftErrors {
    pub const INVALID_SEARCH_BY_TENANT_AND_OWNER: &'static str =
        "Owner or tenant can be set, but not both.";
    pub const INVALID_TOKEN_ID: &'static str = "Invalid token id, token ids must be numbers";
    pub const MISSING_CONTRACT_ADDRESS: &'static str =
        "NFTs can't be queried by token id if no contract address is provided";
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbNft {
    pub id: String,
    pub count: i64,
    pub contract_address: Option<String>,
    pub token_id: Option<String>,
    pub network: Option<String>,
    pub created_at: Option<i64>,
    pub url: Option<String>,
    pub updated_at: Option<i64>,
    pub sold_at: Option<i64>,
    pub urn: Option<String>,
    pub owner: Option<String>,
    pub image: Option<String>,
    pub issued_id: Option<String>,
    pub item_id: Option<String>,
    pub item_type: Option<String>,
    pub rarity: Option<String>,
    pub category: Option<String>,
    pub name: Option<String>,
    pub body_shapes: Option<Vec<String>>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub wearable_category: Option<String>,
    pub emote_category: Option<String>,
    pub description: Option<String>,
    pub size: Option<i32>,
    pub subdomain: Option<String>,
    #[sqlx(rename = "loop")]
    pub r#loop: Option<bool>,
    pub has_sound: Option<bool>,
    pub has_geometry: Option<bool>,
    pub emote_outcome_type: Option<String>,
    pub estate_parcels: Option<sqlx::types::Json<Vec<EstateParcel>>>,
    pub parcel_estate_token_id: Option<String>,
    pub parcel_estate_name: Option<String>,
    pub parcel_estate_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct EstateParcel {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Nft {
    #[serde(rename = "activeOrderId")]
    pub active_order_id: Option<String>,
    pub category: String,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    pub data: NftData,
    pub id: String,
    pub image: String,
    #[serde(rename = "issuedId")]
    pub issued_id: Option<String>,
    #[serde(rename = "itemId")]
    pub item_id: Option<String>,
    pub name: String,
    pub network: Network,
    #[serde(rename = "openRentalId")]
    pub open_rental_id: Option<String>,
    pub owner: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "soldAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sold_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub urn: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub enum NftData {
    Wearable { wearable: WearableData },
    Emote { emote: EmoteData },
    Parcel { parcel: ParcelData },
    Estate { estate: EstateData },
    Ens { ens: EnsData },
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct WearableData {
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub category: String,
    pub description: String,
    pub rarity: String,
    #[serde(rename = "isSmart")]
    pub is_smart: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct EmoteData {
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub category: String,
    pub description: String,
    pub rarity: String,
    #[serde(rename = "loop")]
    #[cfg_attr(feature = "ts", ts(rename = "loop"))]
    pub r#loop: bool,
    #[serde(rename = "hasSound")]
    pub has_sound: bool,
    #[serde(rename = "hasGeometry")]
    pub has_geometry: bool,
    #[serde(rename = "outcomeType")]
    pub outcome_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ParcelData {
    pub x: String,
    pub y: String,
    pub description: Option<String>,
    pub estate: Option<ParcelEstate>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ParcelEstate {
    pub name: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct EstateData {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: i64,
    pub description: Option<String>,
    pub parcels: Vec<EstateParcel>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct EnsData {
    pub subdomain: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct NftResult {
    pub nft: Nft,
    #[cfg_attr(feature = "ts", ts(as = "Option<crate::ports::orders::Order>"))]
    pub order: Option<serde_json::Value>,
    #[cfg_attr(
        feature = "ts",
        ts(as = "Option<crate::ports::rentals::RentalListing>")
    )]
    pub rental: Option<serde_json::Value>,
}
