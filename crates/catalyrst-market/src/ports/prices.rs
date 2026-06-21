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

// Keyed by NumericKey so the serialized JSON object preserves NUMERIC price
// order (BTreeMap iterates in Ord order). Upstream marketplace-server sorts the
// price histogram by bignum (`new BN(a).gt(new BN(b))`); a plain
// BTreeMap<String, _> would re-sort lexicographically ("1000…0" before
// "100…311…"), diverging from upstream.
pub type PricesResponse = BTreeMap<NumericKey, i64>;

pub struct PricesComponent {
    pool: PgPool,
}

fn is_fetching_land(f: &PriceFilters) -> bool {
    f.adjacent_to_road
        || f.min_distance_to_plaza.is_some()
        || f.max_distance_to_plaza.is_some()
        || f.min_estate_size.is_some()
        || f.max_estate_size.is_some()
        || matches!(
            f.category.as_deref(),
            Some("land") | Some("parcel") | Some("estate")
        )
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
        if is_fetching_land(f) || matches!(f.category.as_deref(), Some("ens")) {
            return Ok(BTreeMap::new());
        }

        let catalog_filters = to_catalog_filters(f);
        let (inner_sql, inner_args) =
            build_collections_items_catalog_query_with_trades(&catalog_filters);

        let sql = format!(
            "SELECT COALESCE(catalog.price, catalog.min_price)::text AS price FROM ({}) as catalog",
            inner_sql
        );

        let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), inner_args)
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
    let mut tally: PricesResponse = BTreeMap::new();
    for p in prices {
        *tally.entry(NumericKey(p)).or_insert(0) += 1;
    }
    tally
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericKey(pub String);

impl serde::Serialize for NumericKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consolidate_prices_tallies_and_serializes_in_numeric_order() {
        // mixed lengths: lexicographic order would put "1000" before "999"
        let input = vec![
            "999".into(),
            "1000".into(),
            "999".into(),
            "0".into(),
            "100".into(),
        ];
        let out = consolidate_prices(input);
        // counts correct
        assert_eq!(out.get(&NumericKey("999".into())), Some(&2));
        assert_eq!(out.get(&NumericKey("1000".into())), Some(&1));
        // BTreeMap<NumericKey,_> iterates in NUMERIC order — this is the order
        // axum's Json (direct serde_json::to_vec) emits, NOT lexicographic.
        let keys: Vec<&str> = out.keys().map(|k| k.0.as_str()).collect();
        assert_eq!(
            keys,
            vec!["0", "100", "999", "1000"],
            "must be numeric, not lexicographic"
        );
        // and the directly-serialized JSON string has them in that order too
        let s = serde_json::to_string(&out).unwrap();
        assert!(
            s.find("\"999\"").unwrap() < s.find("\"1000\"").unwrap(),
            "serialized order must be numeric: {s}"
        );
    }

    #[test]
    fn numeric_key_orders_by_value_for_huge_wei_strings() {
        // real wei-scale values of differing length
        let a = NumericKey("999999999999999999000000000000000000".into()); // 36 digits
        let b = NumericKey("1000000000000000000000000000000000000000".into()); // 40 digits
        assert!(a < b, "shorter (smaller) wei value must sort first");
    }
}
