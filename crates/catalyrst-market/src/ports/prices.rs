use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::Network;
use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;
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

impl PricesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_prices(&self, f: &PriceFilters) -> Result<PricesResponse, ApiError> {
        let mut where_parts: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        let mut idx = 0;
        let mut next = || {
            idx += 1;
            format!("${}", idx)
        };
        if let Some(ref c) = f.category {
            where_parts.push(format!("ord.category = {}", next()));
            binds.push(c.clone());
        }
        if !f.contract_addresses.is_empty() {
            let mut placeholders = Vec::new();
            for a in &f.contract_addresses {
                placeholders.push(next());
                binds.push(a.to_lowercase());
            }
            where_parts.push(format!(
                "LOWER(ord.nft_address) IN ({})",
                placeholders.join(",")
            ));
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_parts.join(" AND "))
        };

        let sql = format!(
            r#"
SELECT ord.price::text AS price
FROM {schema}."order" ord
WHERE ord.status = 'open'
  AND ord.expires_at_normalized > NOW()
  {where_sql}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_sql = where_sql,
        );

        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let prices: Vec<String> = q.fetch_all(&self.pool).await.unwrap_or_default();
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
