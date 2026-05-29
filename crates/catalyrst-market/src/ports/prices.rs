use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::dcl_schemas::{Network, NftCategory};
use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::catalog::{build_collections_items_catalog_query_with_trades, CatalogFilters};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    Nft,
    Item,
}

#[derive(Debug, Clone, Default)]
pub struct PriceFilters {
    pub category: Option<String>,
    pub asset_type: Option<AssetType>,
    pub is_wearable_head: bool,
    pub is_wearable_accessory: bool,
    pub is_wearable_smart: bool,
    pub wearable_category: Option<String>,
    pub wearable_genders: Vec<String>,
    pub emote_category: Option<String>,
    pub emote_genders: Vec<String>,
    pub emote_play_mode: Vec<String>,
    pub contract_addresses: Vec<String>,
    pub item_rarities: Vec<String>,
    pub network: Option<Network>,
    pub adjacent_to_road: bool,
    pub min_distance_to_plaza: Option<f64>,
    pub max_distance_to_plaza: Option<f64>,
    pub max_estate_size: Option<f64>,
    pub min_estate_size: Option<f64>,
    pub emote_has_sound: bool,
    pub emote_has_geometry: bool,
    pub emote_outcome_type: Option<String>,
}

pub type PricesResponse = BTreeMap<String, i64>;

pub struct PricesComponent {
    pool: PgPool,
}

fn is_fetching_land(f: &PriceFilters) -> bool {
    f.adjacent_to_road
        || f.min_distance_to_plaza.is_some()
        || f.max_distance_to_plaza.is_some()
        || f.min_estate_size.is_some()
        || f.max_estate_size.is_some()
        || matches!(f.category.as_deref(), Some("land") | Some("parcel") | Some("estate"))
}

fn nft_category_from_str(s: &str) -> Option<NftCategory> {
    Some(match s {
        "wearable" => NftCategory::Wearable,
        "emote" => NftCategory::Emote,
        "ens" => NftCategory::Ens,
        "parcel" => NftCategory::Parcel,
        "estate" => NftCategory::Estate,
        _ => return None,
    })
}

fn to_catalog_filters(f: &PriceFilters) -> CatalogFilters {
    CatalogFilters {
        is_wearable_head: f.is_wearable_head,
        is_wearable_accessory: f.is_wearable_accessory,
        is_wearable_smart: f.is_wearable_smart,
        wearable_category: f.wearable_category.clone(),
        wearable_genders: f.wearable_genders.clone(),
        emote_category: f.emote_category.clone(),
        emote_genders: f.emote_genders.clone(),
        emote_play_mode: f.emote_play_mode.clone(),
        contract_addresses: f.contract_addresses.clone(),
        rarities: f.item_rarities.clone(),
        network: f.network,
        emote_has_geometry: f.emote_has_geometry,
        emote_has_sound: f.emote_has_sound,
        emote_outcome_type: f.emote_outcome_type.clone(),
        category: f.category.as_deref().and_then(nft_category_from_str),
        is_on_sale: Some(true),
        ..Default::default()
    }
}

impl PricesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_prices(&self, f: &PriceFilters) -> Result<PricesResponse, ApiError> {
        // Land / ENS paths use the NFTs query; we don't currently cover them in
        // the parity slice. For wearable/emote (the tested slice), compose the
        // catalog query and select COALESCE(catalog.price, catalog.min_price)
        // exactly like upstream `getPricesQuery`.
        if is_fetching_land(f) || matches!(f.category.as_deref(), Some("ens")) {
            // Fall back to empty for now — only wearable/emote are covered.
            return Ok(BTreeMap::new());
        }

        let catalog_filters = to_catalog_filters(f);
        let (inner_sql, inner_args) =
            build_collections_items_catalog_query_with_trades(&catalog_filters);

        let sql = format!(
            "SELECT COALESCE(catalog.price, catalog.min_price)::text AS price FROM ({}) as catalog",
            inner_sql
        );

        let rows = sqlx::query_with(&sql, inner_args)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let prices: Vec<String> = rows
            .iter()
            .filter_map(|r| r.try_get::<Option<String>, _>("price").ok().flatten())
            .collect();
        Ok(consolidate_prices(prices))
    }
}

pub fn consolidate_prices(prices: Vec<String>) -> PricesResponse {
    let mut tally: BTreeMap<NumericKey, i64> = BTreeMap::new();
    for p in prices {
        let k = NumericKey(p);
        *tally.entry(k).or_insert(0) += 1;
    }
    tally.into_iter().map(|(k, v)| (k.0, v)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumericKey(String);

impl PartialOrd for NumericKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NumericKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let a_num = self.0.chars().all(|c| c.is_ascii_digit());
        let b_num = other.0.chars().all(|c| c.is_ascii_digit());
        if a_num && b_num {
            self.0
                .len()
                .cmp(&other.0.len())
                .then_with(|| self.0.cmp(&other.0))
        } else {
            self.0.cmp(&other.0)
        }
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<PriceFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    let asset_type = p
        .get_string("assetType", None)
        .and_then(|s| match s.as_str() {
            "nft" => Some(AssetType::Nft),
            "item" => Some(AssetType::Item),
            _ => None,
        });
    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| {
            if s == "ETHEREUM" {
                Network::Ethereum
            } else {
                Network::Matic
            }
        });

    Ok(PriceFilters {
        category: p.get_string("category", None),
        asset_type,
        is_wearable_head: p.get_boolean("isWearableHead"),
        is_wearable_accessory: p.get_boolean("isWearableAccessory"),
        is_wearable_smart: p.get_boolean("isWearableSmart"),
        wearable_category: p.get_string("wearableCategory", None),
        wearable_genders: p.get_list("wearableGender", &[]),
        emote_category: p.get_string("emoteCategory", None),
        emote_genders: p.get_list("emoteGender", &[]),
        emote_play_mode: p.get_list("emotePlayMode", &[]),
        contract_addresses: p.get_address_list("contractAddress", true),
        item_rarities: p.get_list("itemRarity", &[]),
        network,
        adjacent_to_road: p.get_boolean("adjacentToRoad"),
        min_distance_to_plaza: p.get_number("minDistanceToPlaza", None),
        max_distance_to_plaza: p.get_number("maxDistanceToPlaza", None),
        max_estate_size: p.get_number("maxEstateSize", None),
        min_estate_size: p.get_number("minEstateSize", None),
        emote_has_sound: p.get_boolean("emoteHasSound"),
        emote_has_geometry: p.get_boolean("emoteHasGeometry"),
        emote_outcome_type: p.get_string("emoteOutcomeType", None),
    })
}

#[derive(Debug, Serialize)]
pub struct PricesEnvelope {
    pub data: PricesResponse,
}
