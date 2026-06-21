mod queries;

pub use queries::{build_catalog_items_query, build_items_query, Bind};

use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::{
    ethereum_chain_id, polygon_chain_id, repoint_content_url, ChainId, Network, NftCategory,
};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::mana_rate::rate_to_numeric_string;

pub const DEFAULT_LIMIT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    EmoteV1,
    WearableV1,
    WearableV2,
    SmartWearableV1,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::EmoteV1 => "emote_v1",
            ItemType::WearableV1 => "wearable_v1",
            ItemType::WearableV2 => "wearable_v2",
            ItemType::SmartWearableV1 => "smart_wearable_v1",
        }
    }
}

pub fn get_item_types_from_nft_category(category: NftCategory) -> Vec<&'static str> {
    match category {
        NftCategory::Wearable => vec![
            ItemType::WearableV1.as_str(),
            ItemType::WearableV2.as_str(),
            ItemType::SmartWearableV1.as_str(),
        ],
        NftCategory::Emote => vec![ItemType::EmoteV1.as_str()],
        _ => vec![],
    }
}

#[derive(Debug, Clone)]
pub struct ItemFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
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
    pub sort_by: Option<ItemSortBy>,

    pub include_social_emotes: bool,
}

impl Default for ItemFilters {
    fn default() -> Self {
        Self {
            first: None,
            skip: None,
            category: None,
            creator: Vec::new(),
            is_sold_out: false,
            is_on_sale: None,
            search: None,
            is_wearable_head: false,
            is_wearable_accessory: false,
            is_wearable_smart: false,
            wearable_category: None,
            rarities: Vec::new(),
            wearable_genders: Vec::new(),
            emote_category: None,
            emote_genders: Vec::new(),
            emote_play_mode: Vec::new(),
            emote_has_geometry: false,
            emote_has_sound: false,
            emote_outcome_type: None,
            contract_addresses: Vec::new(),
            item_id: None,
            network: None,
            max_price: None,
            min_price: None,
            urns: Vec::new(),
            ids: Vec::new(),
            sort_by: None,

            include_social_emotes: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSortBy {
    Newest,
    RecentlyReviewed,
    RecentlySold,
    Name,
    Cheapest,
    RecentlyListed,
}

impl ItemSortBy {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "newest" => Some(Self::Newest),
            "recently_reviewed" => Some(Self::RecentlyReviewed),
            "recently_sold" => Some(Self::RecentlySold),
            "name" => Some(Self::Name),
            "cheapest" => Some(Self::Cheapest),
            "recently_listed" => Some(Self::RecentlyListed),
            _ => None,
        }
    }

    pub(super) fn order_by(&self) -> &'static str {
        match self {
            Self::Newest => " ORDER BY created_at DESC, item.id ASC ",
            Self::RecentlyReviewed => " ORDER BY reviewed_at DESC, item.id ASC ",
            Self::RecentlySold => " ORDER BY sold_at DESC, item.id ASC ",
            Self::Name => " ORDER BY name ASC, item.id ASC ",
            Self::Cheapest => " ORDER BY price::numeric ASC, item.id ASC ",
            Self::RecentlyListed => " ORDER BY first_listed_at DESC NULLS LAST, item.id ASC ",
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbItem {
    pub count: i64,
    pub id: String,
    pub image: Option<String>,
    pub uri: Option<String>,
    pub item_id: Option<String>,
    pub contract_address: Option<String>,
    pub rarity: Option<String>,
    pub price: Option<String>,
    pub available: Option<i64>,
    pub creator: Option<String>,
    pub beneficiary: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub reviewed_at: Option<i64>,
    pub sold_at: Option<i64>,
    pub urn: Option<String>,
    pub network: Option<String>,
    pub search_is_store_minter: Option<bool>,
    pub search_is_marketplace_v3_minter: Option<bool>,
    pub trade_id: Option<String>,
    pub name: Option<String>,
    pub wearable_body_shapes: Option<Vec<String>>,
    pub emote_body_shapes: Option<Vec<String>>,
    pub wearable_category: Option<String>,
    pub emote_category: Option<String>,
    pub item_type: Option<String>,
    #[sqlx(rename = "loop")]
    pub r#loop: Option<bool>,
    pub has_sound: Option<bool>,
    pub has_geometry: Option<bool>,
    pub emote_outcome_type: Option<String>,
    pub description: Option<String>,
    pub first_listed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub trade_beneficiary: Option<String>,
    pub trade_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub trade_contract: Option<String>,
    pub trade_price: Option<String>,
    pub utility: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CatalogDbItem {
    #[sqlx(flatten)]
    pub base: DbItem,
    pub price_credits: Option<i64>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub thumbnail: String,
    pub url: String,
    pub category: NftCategory,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    pub rarity: String,
    pub price: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    #[serde(rename = "isOnSale")]
    pub is_on_sale: bool,
    pub creator: String,
    pub beneficiary: Option<String>,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[serde(rename = "reviewedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub reviewed_at: i64,
    #[serde(rename = "soldAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sold_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub data: ItemData,
    pub network: Network,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    pub urn: String,
    #[serde(rename = "firstListedAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub first_listed_at: Option<i64>,
    pub picks: PicksCount,

    pub utility: Option<String>,
    #[serde(rename = "tradeId")]
    pub trade_id: Option<String>,
    #[serde(rename = "tradeExpiresAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub trade_expires_at: Option<i64>,
    #[serde(rename = "tradeContractAddress")]
    pub trade_contract_address: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct CreditCatalogItem {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub item: Item,
    #[serde(rename = "priceCredits")]
    #[cfg_attr(feature = "ts", ts(rename = "priceCredits", type = "number"))]
    pub price_credits: i64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ItemData {
    Wearable { wearable: WearableData },
    Emote { emote: EmoteData },
}

#[derive(Debug, Serialize)]
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
pub struct EmoteData {
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub category: String,
    pub description: String,
    pub rarity: String,
    #[serde(rename = "loop")]
    pub r#loop: bool,
    #[serde(rename = "hasSound")]
    pub has_sound: bool,
    #[serde(rename = "hasGeometry")]
    pub has_geometry: bool,
    #[serde(rename = "outcomeType")]
    pub outcome_type: Option<String>,
}

#[derive(Debug, Serialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PicksCount {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub count: i64,
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<ItemFilters, ApiError> {
    let p = Params::new(pairs);

    let nft_categories = &["parcel", "estate", "wearable", "ens", "emote"];
    let category = p
        .get_value("category", nft_categories, None)
        .map(|s| match s.as_str() {
            "parcel" => NftCategory::Parcel,
            "estate" => NftCategory::Estate,
            "wearable" => NftCategory::Wearable,
            "ens" => NftCategory::Ens,
            "emote" => NftCategory::Emote,
            _ => unreachable!(),
        });

    let networks = &["ETHEREUM", "MATIC"];
    let network = p
        .get_value("network", networks, None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            "MATIC" => Network::Matic,
            _ => unreachable!(),
        });

    Ok(ItemFilters {
        first: p.get_number("first", None).map(|f| f as i64),
        skip: p.get_number("skip", None).map(|f| f as i64),
        category,
        creator: p.get_list("creator", &[]),
        is_sold_out: p.get_boolean("isSoldOut"),
        is_on_sale: parse_optional_bool(&p, "isOnSale"),
        search: p.get_string("search", None),
        is_wearable_head: p.get_boolean("isWearableHead"),
        is_wearable_accessory: p.get_boolean("isWearableAccessory"),
        is_wearable_smart: p.get_boolean("isWearableSmart"),
        wearable_category: p.get_string("wearableCategory", None),
        rarities: p.get_list("rarity", &[]),
        wearable_genders: p.get_list("wearableGender", &[]),
        emote_category: p.get_string("emoteCategory", None),
        emote_genders: p.get_list("emoteGender", &[]),
        emote_play_mode: p.get_list("emotePlayMode", &[]),
        emote_has_geometry: p.get_boolean("emoteHasGeometry"),
        emote_has_sound: p.get_boolean("emoteHasSound"),
        emote_outcome_type: p.get_string("emoteOutcomeType", None),
        contract_addresses: p.get_address_list("contractAddress", false),
        item_id: p.get_string("itemId", None),
        network,
        max_price: parse_price_param(&p, "maxPrice")?,
        min_price: parse_price_param(&p, "minPrice")?,
        urns: p.get_list("urn", &[]),
        ids: p.get_list("id", &[]),
        sort_by: p
            .get_string("sortBy", None)
            .and_then(|s| ItemSortBy::from_str(&s)),

        include_social_emotes: p.get_string("includeSocialEmotes", None).as_deref()
            != Some("false"),
    })
}

fn parse_price_param(p: &Params, key: &str) -> Result<Option<String>, ApiError> {
    match p.get_string(key, None) {
        Some(raw) if !raw.trim().is_empty() => parse_ether(&raw)
            .map(Some)
            .ok_or_else(|| ApiError::bad_request(format!("Invalid {} value: {}", key, raw))),
        _ => Ok(None),
    }
}

pub fn parse_ether(value: &str) -> Option<String> {
    const DECIMALS: usize = 18;
    let (neg, body) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let mut parts = body.splitn(2, '.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }

    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
        || frac_part.len() > DECIMALS
    {
        return None;
    }
    let mut digits = String::with_capacity(int_part.len() + DECIMALS);
    digits.push_str(int_part);
    digits.push_str(frac_part);
    for _ in 0..(DECIMALS - frac_part.len()) {
        digits.push('0');
    }
    let trimmed = digits.trim_start_matches('0');
    let out = if trimmed.is_empty() { "0" } else { trimmed };

    if neg && out != "0" {
        Some(format!("-{}", out))
    } else {
        Some(out.to_string())
    }
}

fn parse_optional_bool(p: &Params, key: &str) -> Option<bool> {
    if p.get_boolean(key) {
        p.get_string(key, None).map(|s| s == "true")
    } else {
        None
    }
}

pub struct ItemsComponent {
    pool: PgPool,
}

impl ItemsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn fetch<R>(&self, sql: String, binds: Vec<Bind>) -> Result<Vec<R>, ApiError>
    where
        R: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let mut q = sqlx::query_as::<_, R>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = match b {
                Bind::Text(s) => q.bind(s.clone()),
                Bind::TextArray(v) => q.bind(v.clone()),
                Bind::Int(i) => q.bind(*i),
            };
        }
        Ok(q.fetch_all(&self.pool).await?)
    }

    pub async fn get_items(&self, filters: &ItemFilters) -> Result<(Vec<Item>, i64), ApiError> {
        let (sql, binds) = build_items_query(filters);
        let rows: Vec<DbItem> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.count).unwrap_or(0);
        let results = rows.iter().map(from_db_item_to_item).collect();
        Ok((results, total))
    }

    pub async fn get_catalog_items(
        &self,
        filters: &ItemFilters,
        mana_usd_rate: f64,
    ) -> Result<(Vec<CreditCatalogItem>, i64), ApiError> {
        let (sql, binds) =
            build_catalog_items_query(filters, &rate_to_numeric_string(mana_usd_rate));
        let rows: Vec<CatalogDbItem> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.base.count).unwrap_or(0);
        let results = rows
            .iter()
            .map(|r| CreditCatalogItem {
                item: from_db_item_to_item(&r.base),
                price_credits: r.price_credits.unwrap_or(0),
            })
            .collect();
        Ok((results, total))
    }
}

pub fn from_db_item_to_item(d: &DbItem) -> Item {
    let item_type = d.item_type.as_deref().unwrap_or("");
    let is_wearable = matches!(
        item_type,
        "wearable_v1" | "wearable_v2" | "smart_wearable_v1"
    );
    let category = if is_wearable {
        NftCategory::Wearable
    } else {
        NftCategory::Emote
    };

    let available = d.available.unwrap_or(0);

    let store_minter = d.search_is_store_minter.unwrap_or(false);
    let v3_minter = d.search_is_marketplace_v3_minter.unwrap_or(false);
    let has_trade = d.trade_id.is_some();

    let mut price = "0".to_string();
    if available > 0 {
        if has_trade && v3_minter {
            price = d.trade_price.clone().unwrap_or_else(|| "0".to_string());
        } else if store_minter {
            price = d.price.clone().unwrap_or_else(|| "0".to_string());
        }
    }

    let beneficiary = d
        .trade_beneficiary
        .clone()
        .or_else(|| d.beneficiary.clone())
        .unwrap_or_default();
    let beneficiary_out = if is_address_zero(&beneficiary) {
        None
    } else {
        Some(beneficiary)
    };

    let is_on_sale = (store_minter || (has_trade && v3_minter)) && available > 0;

    let rarity = d.rarity.clone().unwrap_or_default();
    let urn = fix_urn(&d.urn.clone().unwrap_or_default());
    let image = repoint_content_url(&fix_urn(&d.image.clone().unwrap_or_default()));
    let name = d.name.clone().unwrap_or_default();

    let chain_id = network_chain_id(d.network.as_deref());
    let network = network_to_canonical(d.network.as_deref());

    let data = if is_wearable {
        ItemData::Wearable {
            wearable: WearableData {
                body_shapes: d.wearable_body_shapes.clone().unwrap_or_default(),
                category: d.wearable_category.clone().unwrap_or_default(),
                description: d.description.clone().unwrap_or_default(),
                rarity: rarity.clone(),
                is_smart: item_type == "smart_wearable_v1",
            },
        }
    } else {
        ItemData::Emote {
            emote: EmoteData {
                body_shapes: d.emote_body_shapes.clone().unwrap_or_default(),
                category: d.emote_category.clone().unwrap_or_default(),
                description: d.description.clone().unwrap_or_default(),
                rarity: rarity.clone(),
                r#loop: d.r#loop.unwrap_or(false),
                has_sound: d.has_sound.unwrap_or(false),
                has_geometry: d.has_geometry.unwrap_or(false),
                outcome_type: d.emote_outcome_type.clone(),
            },
        }
    };

    let contract_address = d.contract_address.clone().unwrap_or_default();
    let item_id_str = d.item_id.clone().unwrap_or_default();

    Item {
        id: format!("{}-{}", contract_address, item_id_str),
        name,
        thumbnail: image,
        url: format!("/contracts/{}/items/{}", contract_address, item_id_str),
        category,
        contract_address,
        item_id: item_id_str,
        rarity,
        price,
        available,
        is_on_sale,
        creator: d.creator.clone().unwrap_or_default(),
        beneficiary: beneficiary_out,
        created_at: d.created_at.unwrap_or(0),
        updated_at: d.updated_at.unwrap_or(0),
        reviewed_at: d.reviewed_at.unwrap_or(0),
        sold_at: d.sold_at.unwrap_or(0),
        data,
        network,
        chain_id,
        urn,
        first_listed_at: d.first_listed_at.map(|t| t.timestamp_millis()),
        picks: PicksCount { count: 0 },
        utility: d.utility.clone(),
        trade_id: d.trade_id.clone(),
        trade_expires_at: d.trade_expires_at.map(|t| t.timestamp_millis()),
        trade_contract_address: d.trade_contract.clone(),
    }
}

pub fn fix_urn(urn: &str) -> String {
    urn.replace("mainnet", "ethereum")
}

pub fn expand_urn_network_forms(urns: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(urns.len());
    let mut seen = std::collections::HashSet::new();
    for u in urns {
        for v in [
            u.clone(),
            u.replace(":mainnet:", ":ethereum:"),
            u.replace(":ethereum:", ":mainnet:"),
        ] {
            if seen.insert(v.clone()) {
                out.push(v);
            }
        }
    }
    out
}

pub fn is_address_zero(addr: &str) -> bool {
    addr.is_empty() || addr.eq_ignore_ascii_case("0x0000000000000000000000000000000000000000")
}

fn network_to_canonical(network: Option<&str>) -> Network {
    match network {
        Some("MATIC") | Some("POLYGON") => Network::Matic,
        _ => Network::Ethereum,
    }
}

fn network_chain_id(network: Option<&str>) -> ChainId {
    match network {
        Some("MATIC") | Some("POLYGON") => polygon_chain_id(),
        _ => ethereum_chain_id(),
    }
}

#[cfg(test)]
mod canonical_id_tests {
    use super::{from_db_item_to_item, DbItem};

    fn v1_db_item() -> DbItem {
        DbItem {
            count: 1,
            id: "0xf64dc33a192e056bb5f0e5049356a0498b502d50-mch_nightingale_upper_body".into(),
            image: None,
            uri: None,
            item_id: Some("13".into()),
            contract_address: Some("0xf64dc33a192e056bb5f0e5049356a0498b502d50".into()),
            rarity: Some("epic".into()),
            price: Some("0".into()),
            available: Some(0),
            creator: None,
            beneficiary: None,
            created_at: Some(0),
            updated_at: Some(0),
            reviewed_at: Some(0),
            sold_at: Some(0),
            urn: None,
            network: Some("ETHEREUM".into()),
            search_is_store_minter: Some(false),
            search_is_marketplace_v3_minter: Some(false),
            trade_id: None,
            name: Some("MCH Nightingale".into()),
            wearable_body_shapes: None,
            emote_body_shapes: None,
            wearable_category: None,
            emote_category: None,
            item_type: Some("wearable_v1".into()),
            r#loop: None,
            has_sound: None,
            has_geometry: None,
            emote_outcome_type: None,
            description: None,
            first_listed_at: None,
            trade_beneficiary: None,
            trade_expires_at: None,
            trade_contract: None,
            trade_price: None,
            utility: None,
        }
    }

    #[test]
    fn v1_string_squid_id_becomes_detail_resolvable() {
        let item = from_db_item_to_item(&v1_db_item());
        assert_eq!(item.id, "0xf64dc33a192e056bb5f0e5049356a0498b502d50-13");
        assert_eq!(item.item_id, "13");
    }
}

#[cfg(test)]
mod urn_form_tests {
    use super::expand_urn_network_forms;

    #[test]
    fn expands_both_ethereum_token_forms() {
        let eth = "urn:decentraland:ethereum:collections-v1:0xabc:0".to_string();
        let out = expand_urn_network_forms(std::slice::from_ref(&eth));

        assert!(out.contains(&eth));
        assert!(out.contains(&"urn:decentraland:mainnet:collections-v1:0xabc:0".to_string()));
    }

    #[test]
    fn mainnet_input_still_present_parity_preserved() {
        let main = "urn:decentraland:mainnet:collections-v1:0xabc:0".to_string();
        let out = expand_urn_network_forms(std::slice::from_ref(&main));
        assert!(out.contains(&main));
    }

    #[test]
    fn matic_urn_unchanged_no_dupes() {
        let matic = "urn:decentraland:matic:collections-v2:0xdef:1".to_string();
        let out = expand_urn_network_forms(std::slice::from_ref(&matic));
        assert_eq!(out, vec![matic]);
    }

    #[test]
    fn dedups_when_both_forms_supplied() {
        let a = "urn:decentraland:ethereum:x:0xa:0".to_string();
        let b = "urn:decentraland:mainnet:x:0xa:0".to_string();
        let out = expand_urn_network_forms(&[a.clone(), b.clone()]);

        assert_eq!(out.len(), 2);
        assert!(out.contains(&a) && out.contains(&b));
    }
}

#[cfg(test)]
mod parse_ether_tests {
    use super::parse_ether;

    #[test]
    fn whole_numbers_scale_by_1e18() {
        assert_eq!(parse_ether("1").unwrap(), "1000000000000000000");
        assert_eq!(parse_ether("100").unwrap(), "100000000000000000000");
        assert_eq!(parse_ether("0").unwrap(), "0");
    }

    #[test]
    fn fractional_values() {
        assert_eq!(parse_ether("1.5").unwrap(), "1500000000000000000");

        assert_eq!(parse_ether("0.000000000000000001").unwrap(), "1");
        assert_eq!(parse_ether("0.5").unwrap(), "500000000000000000");

        assert_eq!(parse_ether(".5").unwrap(), "500000000000000000");
        assert_eq!(parse_ether("2.").unwrap(), "2000000000000000000");
    }

    #[test]
    fn max_18_decimals_enforced() {
        assert!(parse_ether("1.123456789012345678").is_some());

        assert!(parse_ether("1.1234567890123456789").is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_ether("").is_none());
        assert!(parse_ether("abc").is_none());
        assert!(parse_ether("1.2.3").is_none());
        assert!(parse_ether("1,5").is_none());
        assert!(parse_ether("0x10").is_none());
    }

    #[test]
    fn negative_sign_preserved_for_nonzero_only() {
        assert_eq!(parse_ether("-1").unwrap(), "-1000000000000000000");

        assert_eq!(parse_ether("-0").unwrap(), "0");
    }
}
