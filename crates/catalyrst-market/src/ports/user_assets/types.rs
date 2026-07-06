use serde::{Deserialize, Serialize};

pub const FIRST_DEFAULT: i64 = 100;
pub const SKIP_DEFAULT: i64 = 0;

#[derive(Debug, Clone, Default)]
pub struct UserAssetsFilters {
    pub first: i64,
    pub skip: i64,
    pub category: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub order_by: Option<String>,
    pub direction: Option<String>,
    pub item_type: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ProfileWearable {
    pub urn: String,
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub category: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: Option<String>,
    pub name: String,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub price: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,

    #[serde(rename = "unlockAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub unlock_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ProfileEmote {
    pub urn: String,
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub category: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: Option<String>,
    pub name: String,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub price: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,

    #[serde(rename = "unlockAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub unlock_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ProfileName {
    pub name: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct UrnToken {
    pub urn: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct NameOnly {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct GroupedWearable {
    pub urn: String,
    pub amount: String,
    #[serde(rename = "individualData")]
    pub individual_data: Vec<IndividualData>,
    pub name: String,
    pub rarity: String,
    #[serde(rename = "minTransferredAt")]
    pub min_transferred_at: String,
    #[serde(rename = "maxTransferredAt")]
    pub max_transferred_at: String,
    pub category: String,
    #[serde(rename = "itemType")]
    pub item_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,

    #[serde(rename = "unlockAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub unlock_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct GroupedEmote {
    pub urn: String,
    pub amount: String,
    #[serde(rename = "individualData")]
    pub individual_data: Vec<IndividualData>,
    pub name: String,
    pub rarity: String,
    #[serde(rename = "minTransferredAt")]
    pub min_transferred_at: String,
    #[serde(rename = "maxTransferredAt")]
    pub max_transferred_at: String,
    pub category: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,

    #[serde(rename = "unlockAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub unlock_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct IndividualData {
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: String,
    pub price: String,
}

pub fn parse_user_assets_params(pairs: &[(String, String)]) -> UserAssetsFilters {
    use crate::http::params::Params;
    const MAX_LIMIT: i64 = 1000;
    const DEFAULT_LIMIT: i64 = 100;

    let p = Params::new(pairs);

    let limit = p.get_number("limit", None).map(|n| n as i64);
    let offset = p.get_number("offset", None).map(|n| n as i64);
    let first = p.get_number("first", None).map(|n| n as i64);
    let skip = p.get_number("skip", None).map(|n| n as i64);

    let requested_limit = limit.or(first).unwrap_or(DEFAULT_LIMIT);
    let requested_skip = offset.or(skip).unwrap_or(0).max(0);

    let capped_limit = requested_limit.clamp(0, MAX_LIMIT);

    let item_type_list = p.get_list("itemType", &[]);
    let item_type = if item_type_list.is_empty() {
        None
    } else {
        Some(item_type_list)
    };

    UserAssetsFilters {
        first: capped_limit,
        skip: requested_skip,
        category: p.get_string("category", None),
        rarity: p.get_string("rarity", None),
        name: p.get_string("name", None),
        order_by: p.get_string("orderBy", None),
        direction: p.get_string("direction", None),
        item_type,
    }
}
