use serde::Serialize;
use serde_json::Value as JsonValue;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::{Arguments, PgPool, Postgres, Row};

use crate::dcl_schemas::{ethereum_chain_id, polygon_chain_id, ChainId, Network, NftCategory};
use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip, MAX_PAGE_LIMIT};
use crate::{BUILDER_SERVER_TABLE_SCHEMA, MARKETPLACE_SQUID_SCHEMA};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSortBy {
    Newest,
    RecentlyListed,
    RecentlySold,
    Cheapest,
    MostExpensive,
}

impl CatalogSortBy {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "newest" => Self::Newest,
            "recently_listed" => Self::RecentlyListed,
            "recently_sold" => Self::RecentlySold,
            "cheapest" => Self::Cheapest,
            "most_expensive" => Self::MostExpensive,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default)]
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
}

#[derive(Debug, Serialize)]
pub struct WearableData {
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub rarity: String,
    #[serde(rename = "isSmart")]
    pub is_smart: bool,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ItemData {
    Wearable { wearable: WearableData },
    Emote { emote: serde_json::Value },
}

#[derive(Debug, Serialize)]
pub struct PickStats {
    pub count: i64,
    #[serde(rename = "itemId")]
    pub item_id: String,
    #[serde(rename = "pickedByUser", skip_serializing_if = "Option::is_none")]
    pub picked_by_user: Option<bool>,
}

#[derive(Debug, Serialize)]
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
    pub available: i64,
    #[serde(rename = "isOnSale")]
    pub is_on_sale: bool,
    pub creator: String,
    pub data: ItemData,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub price: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "reviewedAt")]
    pub reviewed_at: i64,
    #[serde(rename = "firstListedAt")]
    pub first_listed_at: Option<i64>,
    #[serde(rename = "soldAt")]
    pub sold_at: i64,
    #[serde(rename = "minPrice", skip_serializing_if = "Option::is_none")]
    pub min_price: Option<String>,
    #[serde(rename = "maxListingPrice")]
    pub max_listing_price: Option<String>,
    #[serde(rename = "minListingPrice")]
    pub min_listing_price: Option<String>,
    pub listings: Option<i64>,
    pub owners: Option<i64>,
    pub picks: Option<PickStats>,
}

#[derive(Debug)]
struct DbRow {
    id: String,
    blockchain_id: String,
    image: String,
    collection_id: String,
    rarity: String,
    item_type: String,
    price: String,
    available: String,
    search_is_store_minter: bool,
    search_is_marketplace_v3_minter: bool,
    creator: String,
    beneficiary: Option<String>,
    created_at: String,
    updated_at: String,
    reviewed_at: String,
    sold_at: String,
    first_listed_at: Option<String>,
    urn: String,
    network: String,
    metadata: Option<JsonValue>,
    min_listing_price: Option<String>,
    max_listing_price: Option<String>,
    open_item_trade_id: Option<String>,
    open_item_trade_price: Option<String>,
    listings_count: Option<i64>,
    owners_count: Option<i64>,
    min_price: Option<String>,
    #[allow(dead_code)]
    max_price: Option<String>,
}

fn fix_thumbnail(thumbnail: &str, blockchain_id: &str) -> String {
    if thumbnail.is_empty() {
        return String::new();
    }

    let mut t = if matches!(polygon_chain_id(), ChainId::MaticAmoy)
        || matches!(ethereum_chain_id(), ChainId::EthereumSepolia)
    {
        thumbnail.replace(".org", ".zone")
    } else {
        thumbnail.to_string()
    };

    t = t.replace("polygon", "matic").replace("mainnet", "ethereum");

    if t.contains("ethereum") {
        return t;
    }

    let mut parts: Vec<String> = t.split(':').map(String::from).collect();
    if parts.len() <= 5 {
        return t;
    }
    if !parts[5].starts_with("0x") {
        let with_prefix = format!("0x{}", parts[5]);
        parts[5] = with_prefix.replace("/thumbnail", &format!(":{}/thumbnail", blockchain_id));
    }
    parts.join(":")
}

fn from_db_row_to_catalog_item(row: DbRow, network_hint: Option<Network>) -> CatalogItem {
    let metadata = row.metadata.clone().unwrap_or(JsonValue::Null);
    let meta_obj = metadata.as_object();
    let get_str = |key: &str| -> Option<String> {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str().map(String::from))
    };
    let get_bool = |key: &str| -> bool {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let get_string_array = |key: &str| -> Vec<String> {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };

    let (name, category, data): (String, &'static str, ItemData) = match row.item_type.as_str() {
        FRAGMENT_WEARABLE_V1 | FRAGMENT_WEARABLE_V2 | FRAGMENT_SMART_WEARABLE_V1 => {
            let wearable = WearableData {
                description: get_str("description"),
                category: get_str("category"),
                body_shapes: get_string_array("body_shapes"),
                rarity: row.rarity.clone(),
                is_smart: row.item_type == FRAGMENT_SMART_WEARABLE_V1,
            };
            (
                get_str("name").unwrap_or_default(),
                "wearable",
                ItemData::Wearable { wearable },
            )
        }
        FRAGMENT_EMOTE_V1 => {
            let emote_category_lower = get_str("category").map(|s| s.to_lowercase());
            let emote_value = serde_json::json!({
                "description": get_str("description"),
                "category": emote_category_lower,
                "bodyShapes": get_string_array("body_shapes"),
                "rarity": row.rarity,
                "loop": get_bool("loop"),
                "hasGeometry": get_bool("has_geometry"),
                "hasSound": get_bool("has_sound"),
                "outcomeType": get_str("outcome_type"),
            });
            (
                get_str("name").unwrap_or_default(),
                "emote",
                ItemData::Emote { emote: emote_value },
            )
        }
        other => {
            tracing::warn!(item_type = %other, item_id = %row.id, "unknown item_type, defaulting to wearable");
            (
                String::new(),
                "wearable",
                ItemData::Wearable {
                    wearable: WearableData {
                        description: None,
                        category: None,
                        body_shapes: vec![],
                        rarity: row.rarity.clone(),
                        is_smart: false,
                    },
                },
            )
        }
    };

    let available_n = row.available.parse::<i64>().unwrap_or(0);
    let price = if available_n > 0 {
        if row.open_item_trade_id.is_some() && row.search_is_marketplace_v3_minter {
            row.open_item_trade_price
                .clone()
                .unwrap_or_else(|| "0".into())
        } else if row.search_is_store_minter {
            row.price.clone()
        } else {
            "0".into()
        }
    } else {
        "0".into()
    };

    let item_network_str = if !row.network.is_empty() {
        row.network.clone()
    } else {
        match network_hint {
            Some(Network::Ethereum) => "ETHEREUM".into(),
            _ => "POLYGON".into(),
        }
    };
    let (item_network, chain_id) = if item_network_str.eq_ignore_ascii_case("POLYGON")
        || item_network_str.eq_ignore_ascii_case("MATIC")
    {
        (Network::Matic, polygon_chain_id())
    } else {
        (Network::Ethereum, ethereum_chain_id())
    };

    let parse_i64_lossy = |s: &str| s.parse::<i64>().unwrap_or(0);

    CatalogItem {
        id: row.id.clone(),
        beneficiary: row.beneficiary.clone(),
        item_id: row.blockchain_id.clone(),
        name,
        thumbnail: fix_thumbnail(&row.image, &row.blockchain_id),
        url: format!(
            "/contracts/{}/items/{}",
            row.collection_id, row.blockchain_id
        ),
        urn: row.urn.clone(),
        category,
        contract_address: row.collection_id.clone(),
        rarity: row.rarity.clone(),
        available: available_n,
        is_on_sale: (row.search_is_store_minter
            || (row.open_item_trade_id.is_some() && row.search_is_marketplace_v3_minter))
            && available_n > 0,
        creator: row.creator.clone(),
        data,
        network: item_network,
        chain_id,
        price,
        created_at: parse_i64_lossy(&row.created_at),
        updated_at: parse_i64_lossy(&row.updated_at),
        reviewed_at: parse_i64_lossy(&row.reviewed_at),
        first_listed_at: row.first_listed_at.as_deref().map(parse_i64_lossy),
        sold_at: parse_i64_lossy(&row.sold_at),
        min_price: row.min_price.clone(),
        max_listing_price: row.max_listing_price.clone(),
        min_listing_price: row.min_listing_price.clone(),
        listings: row.listings_count,
        owners: row.owners_count,
        picks: None,
    }
}

struct Builder {
    sql: String,
    args: PgArguments,
    next_index: usize,
}

impl Builder {
    fn new() -> Self {
        Self {
            sql: String::new(),
            args: PgArguments::default(),
            next_index: 1,
        }
    }

    fn push_sql(&mut self, s: &str) -> &mut Self {
        self.sql.push_str(s);
        self
    }

    fn bind_string(&mut self, v: String) -> usize {
        let idx = self.next_index;
        self.args.add(v).expect("add string arg");
        self.next_index += 1;
        idx
    }

    fn bind_string_slice(&mut self, vs: &[String]) -> usize {
        let idx = self.next_index;
        self.args.add(vs.to_vec()).expect("add string[] arg");
        self.next_index += 1;
        idx
    }

    fn bind_bool(&mut self, v: bool) -> usize {
        let idx = self.next_index;
        self.args.add(v).expect("add bool arg");
        self.next_index += 1;
        idx
    }

    fn bind_i64(&mut self, v: i64) -> usize {
        let idx = self.next_index;
        self.args.add(v).expect("add i64 arg");
        self.next_index += 1;
        idx
    }
}

fn build_category_where(b: &mut Builder, f: &CatalogFilters) {
    if let Some(cat) = f.category {
        match cat {
            NftCategory::Wearable => {
                if f.is_wearable_smart {
                    b.push_sql(&format!(
                        "items.item_type = '{}'",
                        FRAGMENT_SMART_WEARABLE_V1
                    ));
                } else {
                    let in_list = WEARABLE_ITEM_TYPES
                        .iter()
                        .map(|t| format!("'{}'", t))
                        .collect::<Vec<_>>()
                        .join(", ");
                    b.push_sql(&format!("items.item_type IN ({})", in_list));
                }
            }
            NftCategory::Emote => {
                b.push_sql(&format!("items.item_type = '{}'", FRAGMENT_EMOTE_V1));
            }
            _ => {
                b.push_sql("TRUE");
            }
        }
    }
}

fn build_wearable_category_where(b: &mut Builder, f: &CatalogFilters) {
    if let Some(c) = &f.wearable_category {
        // Bound param, not a hand-escaped literal — matches every neighboring
        // filter and is injection-safe regardless of standard_conforming_strings.
        let bi = b.bind_string(c.clone());
        b.push_sql(&format!("metadata_wearable.category = ${}", bi));
    }
}

fn build_emote_category_where(b: &mut Builder, f: &CatalogFilters) {
    if let Some(c) = &f.emote_category {
        let bi = b.bind_string(c.clone());
        b.push_sql(&format!("metadata_emote.category = ${}", bi));
    }
}

fn build_emote_play_mode_where(b: &mut Builder, f: &CatalogFilters) {
    if f.emote_play_mode.len() == 1 {
        let is_loop = f.emote_play_mode[0] == "loop";
        let bi = b.bind_bool(is_loop);
        b.push_sql(&format!("metadata_emote.loop = ${}", bi));
    }
}

fn build_is_sold_out_where(b: &mut Builder) {
    b.push_sql("items.available = 0");
}

fn build_is_on_sale_where(b: &mut Builder, f: &CatalogFilters) {
    if f.is_on_sale == Some(true) {
        b.push_sql(
            "((search_is_store_minter = true AND available > 0) OR listings_count IS NOT NULL)",
        );
    } else {
        b.push_sql(
            "((search_is_store_minter = false OR available = 0) AND listings_count IS NULL)",
        );
    }
}

fn build_is_on_sale_with_trades_where(b: &mut Builder, f: &CatalogFilters) {
    if f.only_minting && f.is_on_sale == Some(true) {
        b.push_sql(
            "((search_is_store_minter = true OR (search_is_marketplace_v3_minter = true AND offchain_orders.count IS NOT NULL)) AND available > 0)",
        );
        return;
    }
    if f.is_on_sale == Some(true) {
        b.push_sql(
            "(((search_is_store_minter = true OR (search_is_marketplace_v3_minter = true AND offchain_orders.count IS NOT NULL)) AND available > 0) OR (nfts_with_orders.orders_listings_count IS NOT NULL OR offchain_orders.nfts_listings_count IS NOT NULL))",
        );
    } else {
        b.push_sql(
            "(((search_is_store_minter = false AND search_is_marketplace_v3_minter = false) OR available = 0) OR (search_is_marketplace_v3_minter = true AND (nfts_with_orders.orders_listings_count IS NULL AND offchain_orders.count IS NULL)))",
        );
    }
}

fn build_is_wearable_head_where(b: &mut Builder) {
    b.push_sql("items.search_is_wearable_head = true");
}

fn build_wearable_accessory_where(b: &mut Builder) {
    b.push_sql("items.search_is_wearable_accessory = true");
}

fn build_wearable_gender_where(b: &mut Builder, f: &CatalogFilters) {
    let mut parsed = Vec::new();
    for g in &f.wearable_genders {
        match g.as_str() {
            "female" => parsed.push("BaseFemale".to_string()),
            "male" => parsed.push("BaseMale".to_string()),
            _ => {}
        }
    }
    if parsed.is_empty() {
        b.push_sql("TRUE");
        return;
    }
    let bi = b.bind_string_slice(&parsed);
    b.push_sql(&format!("items.search_wearable_body_shapes @> (${})", bi));
}

fn build_creator_where(b: &mut Builder, f: &CatalogFilters) {
    if f.creator.is_empty() {
        b.push_sql("TRUE");
        return;
    }
    if f.creator.len() == 1 {
        let bi = b.bind_string(f.creator[0].clone());
        b.push_sql(&format!("items.creator = ${}", bi));
    } else {
        let bi = b.bind_string_slice(&f.creator);
        b.push_sql(&format!("items.creator = ANY(${})", bi));
    }
}

fn build_rarities_where(b: &mut Builder, f: &CatalogFilters) {
    let bi = b.bind_string_slice(&f.rarities);
    b.push_sql(&format!("items.rarity = ANY(${})", bi));
}

fn build_min_price_where(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
    let mp = f.min_price.clone().unwrap_or_default();
    if f.only_minting {
        let bi = b.bind_string(mp);

        b.push_sql(&format!(
            "(price >= ${} AND price IS DISTINCT FROM '{}')",
            bi, MAX_NUMERIC_NUMBER
        ));
        return;
    }
    if f.only_listing {
        let bi = b.bind_string(mp);
        b.push_sql(&format!("min_price >= ${}", bi));
        return;
    }
    let bi = b.bind_string(mp);
    let mut s = format!(
        "(min_price >= ${0} OR (price >= ${0} AND available > 0 AND (search_is_store_minter = true OR search_is_marketplace_v3_minter = true))",
        bi
    );
    if is_v2 {
        s.push_str(&format!(
            " OR offchain_orders.min_order_amount_received >= ${}",
            bi
        ));
    }
    s.push(')');
    b.push_sql(&s);
}

fn build_max_price_where(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
    let mp = f.max_price.clone().unwrap_or_default();
    if f.only_minting {
        let bi = b.bind_string(mp);
        b.push_sql(&format!("price <= ${}", bi));
        return;
    }
    if f.only_listing {
        let bi = b.bind_string(mp);
        b.push_sql(&format!("max_price <= ${}", bi));
        return;
    }
    let bi = b.bind_string(mp);
    let mut s = format!(
        "(max_price <= ${0} OR (price <= ${0} AND available > 0 AND (search_is_store_minter = true OR search_is_marketplace_v3_minter = true))",
        bi
    );
    if is_v2 {
        s.push_str(&format!(
            " OR offchain_orders.max_order_amount_received <= ${}",
            bi
        ));
    }
    s.push(')');
    b.push_sql(&s);
}

fn build_contract_address_where(b: &mut Builder, f: &CatalogFilters) {
    let bi = b.bind_string_slice(&f.contract_addresses);
    b.push_sql(&format!("items.collection_id = ANY(${})", bi));
}

fn build_only_listings_where(b: &mut Builder) {
    b.push_sql(
        "(items.search_is_store_minter = false OR (items.search_is_store_minter = true AND available = 0)) AND listings_count > 0",
    );
}

fn build_only_listings_with_trades_where(b: &mut Builder) {
    b.push_sql(
        "((items.search_is_store_minter = false AND items.search_is_marketplace_v3_minter = false) OR (items.search_is_store_minter = true AND available = 0) OR (items.search_is_marketplace_v3_minter = true AND COALESCE(offchain_orders.items_listings_count, 0) = 0)) AND (COALESCE(nfts_with_orders.orders_listings_count, 0) + COALESCE(offchain_orders.nfts_listings_count, 0)) > 0",
    );
}

fn build_only_minting_where(b: &mut Builder) {
    b.push_sql("items.search_is_store_minter = true AND available > 0");
}

fn build_only_minting_with_trades_where(b: &mut Builder) {
    b.push_sql("(((items.search_is_store_minter = true OR (items.search_is_marketplace_v3_minter = true AND offchain_orders.count IS NOT NULL))) AND available > 0)");
}

fn build_ids_where(b: &mut Builder, f: &CatalogFilters) {
    let bi = b.bind_string_slice(&f.ids);
    b.push_sql(&format!("items.id = ANY(${})", bi));
}

fn build_has_sound_where(b: &mut Builder) {
    b.push_sql("items.search_emote_has_sound = true");
}

fn build_has_geometry_where(b: &mut Builder) {
    b.push_sql("items.search_emote_has_geometry = true");
}

fn build_has_outcome_type_where(b: &mut Builder) {
    b.push_sql("items.search_emote_outcome_type IS NOT NULL");
}

fn build_urns_where(b: &mut Builder, f: &CatalogFilters) {
    // Match either ethereum network-token form; see
    // `items::expand_urn_network_forms` (additive — preserves upstream parity).
    let expanded = crate::ports::items::expand_urn_network_forms(&f.urns);
    let bi = b.bind_string_slice(&expanded);
    b.push_sql(&format!("items.urn = ANY(${})", bi));
}

fn build_network_where(b: &mut Builder, f: &CatalogFilters) {
    if let Some(net) = f.network {
        let label = match net {
            Network::Matic => "POLYGON",
            Network::Ethereum => "ETHEREUM",
        };
        let bi = b.bind_string(label.to_string());
        b.push_sql(&format!("items.network = ${}", bi));
    }
}

fn build_collections_where(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
    b.push_sql(" WHERE items.search_is_collection_approved = true ");

    let mut first = true;
    let and_sep = |b: &mut Builder, first: &mut bool| {
        if !*first {
            b.push_sql(" AND ");
        } else {
            b.push_sql(" AND ");
            *first = false;
        }
    };

    if f.category.is_some() {
        and_sep(b, &mut first);
        build_category_where(b, f);
    }
    if !f.rarities.is_empty() {
        and_sep(b, &mut first);
        build_rarities_where(b, f);
    }
    if !f.creator.is_empty() {
        and_sep(b, &mut first);
        build_creator_where(b, f);
    }
    if f.is_sold_out {
        and_sep(b, &mut first);
        build_is_sold_out_where(b);
    }
    if f.is_on_sale.is_some() {
        and_sep(b, &mut first);
        if is_v2 {
            build_is_on_sale_with_trades_where(b, f);
        } else {
            build_is_on_sale_where(b, f);
        }
    }
    if f.is_wearable_head {
        and_sep(b, &mut first);
        build_is_wearable_head_where(b);
    }
    if f.is_wearable_accessory {
        and_sep(b, &mut first);
        build_wearable_accessory_where(b);
    }
    if f.wearable_category.is_some() {
        and_sep(b, &mut first);
        build_wearable_category_where(b, f);
    }
    if !f.wearable_genders.is_empty() {
        and_sep(b, &mut first);
        build_wearable_gender_where(b, f);
    }
    if f.emote_category.is_some() {
        and_sep(b, &mut first);
        build_emote_category_where(b, f);
    }
    if !f.emote_play_mode.is_empty() && f.emote_play_mode.len() < 2 {
        and_sep(b, &mut first);
        build_emote_play_mode_where(b, f);
    }
    if !f.contract_addresses.is_empty() {
        and_sep(b, &mut first);
        build_contract_address_where(b, f);
    }
    if f.min_price.is_some() {
        and_sep(b, &mut first);
        build_min_price_where(b, f, is_v2);
    }
    if f.max_price.is_some() {
        and_sep(b, &mut first);
        build_max_price_where(b, f, is_v2);
    }
    if f.only_listing {
        and_sep(b, &mut first);
        if is_v2 {
            build_only_listings_with_trades_where(b);
        } else {
            build_only_listings_where(b);
        }
    }
    if f.only_minting {
        and_sep(b, &mut first);
        if is_v2 {
            build_only_minting_with_trades_where(b);
        } else {
            build_only_minting_where(b);
        }
    }
    if !f.ids.is_empty() {
        and_sep(b, &mut first);
        build_ids_where(b, f);
    }
    if f.emote_has_sound {
        and_sep(b, &mut first);
        build_has_sound_where(b);
    }
    if f.emote_has_geometry {
        and_sep(b, &mut first);
        build_has_geometry_where(b);
    }
    if f.emote_outcome_type.is_some() {
        and_sep(b, &mut first);
        build_has_outcome_type_where(b);
    }
    if !f.urns.is_empty() {
        and_sep(b, &mut first);
        build_urns_where(b, f);
    }
    if f.network.is_some() {
        and_sep(b, &mut first);
        build_network_where(b, f);
    }
    b.push_sql(" ");
}

fn build_order_by(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
    let sort_by = f.sort_by.unwrap_or(CatalogSortBy::Newest);
    let sort_direction = f.sort_direction.unwrap_or(CatalogSortDirection::Desc);

    if f.is_on_sale == Some(false) && sort_by != CatalogSortBy::Newest {
        return;
    }

    b.push_sql("ORDER BY ");

    if f.search.is_some() && !f.ids.is_empty() {
        let bi = b.bind_string_slice(&f.ids);
        b.push_sql(&format!("array_position(${}::text[], id), ", bi));
    }

    match sort_by {
        CatalogSortBy::Newest => b.push_sql(
            "GREATEST(COALESCE(ROUND(EXTRACT(EPOCH FROM offchain_orders.item_first_listed_at)), 0), first_listed_at) desc nulls LAST",
        ),
        CatalogSortBy::MostExpensive => b.push_sql("max_price desc"),
        CatalogSortBy::RecentlyListed => {
            if is_v2 {
                if f.only_minting {
                    b.push_sql(
                        "GREATEST(GREATEST(COALESCE(ROUND(EXTRACT(EPOCH FROM offchain_orders.max_created_at)), 0)), first_listed_at) desc",
                    )
                } else {
                    b.push_sql(
                        "GREATEST(GREATEST(COALESCE(ROUND(EXTRACT(EPOCH FROM offchain_orders.max_created_at)), 0), COALESCE(nfts_with_orders.max_order_created_at, 0)), first_listed_at) desc",
                    )
                }
            } else {
                b.push_sql("GREATEST(max_order_created_at, first_listed_at) desc")
            }
        }
        CatalogSortBy::RecentlySold => b.push_sql("sold_at desc"),
        CatalogSortBy::Cheapest => b.push_sql("min_price asc, first_listed_at desc"),
    };

    let _ = sort_direction;
    b.push_sql(" ");
}

fn build_limit_offset(b: &mut Builder, f: &CatalogFilters) {
    if f.first.is_some() || f.skip.is_some() {
        let li = b.bind_i64(clamp_first(f.first, MAX_PAGE_LIMIT));
        let oi = b.bind_i64(clamp_skip(f.skip));
        b.push_sql(&format!("LIMIT ${} OFFSET ${}", li, oi));
    }
}

fn build_metadata_joins(b: &mut Builder) {
    b.push_sql(&format!(
        " LEFT JOIN (
            SELECT metadata.id as metadata_id, wearable.description, wearable.category, wearable.body_shapes, wearable.rarity, wearable.name
            FROM {schema}.wearable AS wearable
            JOIN {schema}.metadata AS metadata ON metadata.wearable_id = wearable.id
        ) AS metadata_wearable ON metadata_wearable.metadata_id = items.metadata_id AND (items.item_type = 'wearable_v1' OR items.item_type = 'wearable_v2' OR items.item_type = 'smart_wearable_v1')
        LEFT JOIN (
            SELECT metadata.id as metadata_id, emote.description, emote.category, emote.body_shapes, emote.rarity, emote.name, emote.loop, emote.has_sound, emote.has_geometry, emote.outcome_type
            FROM {schema}.emote AS emote
            JOIN {schema}.metadata AS metadata ON metadata.emote_id = emote.id
        ) AS metadata_emote ON metadata_emote.metadata_id = items.metadata_id AND items.item_type = 'emote_v1' ",
        schema = MARKETPLACE_SQUID_SCHEMA,
    ));
}

fn build_owners_join(b: &mut Builder) {
    b.push_sql(&format!(
        " LEFT JOIN LATERAL (SELECT count(DISTINCT owner_id) AS owners_count FROM {schema}.nft WHERE nft.item_id = items.id) AS nfts ON true ",
        schema = MARKETPLACE_SQUID_SCHEMA,
    ));
}

fn build_order_range_price_where(b: &mut Builder, f: &CatalogFilters) {
    match (f.min_price.as_deref(), f.max_price.as_deref()) {
        (Some(mn), None) => {
            let bi = b.bind_string(mn.to_string());
            b.push_sql(&format!(" AND orders.price >= ${}", bi));
        }
        (None, Some(mx)) => {
            let bi = b.bind_string(mx.to_string());
            b.push_sql(&format!(" AND orders.price <= ${}", bi));
        }
        (Some(mn), Some(mx)) => {
            let bin = b.bind_string(mn.to_string());
            let bix = b.bind_string(mx.to_string());
            b.push_sql(&format!(
                " AND orders.price >= ${} AND orders.price <= ${}",
                bin, bix
            ));
        }
        (None, None) => {}
    }
}

fn build_nfts_with_orders_cte_v1(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(&format!(
        " LEFT JOIN (
            SELECT
                orders.item_id,
                COUNT(orders.id) AS listings_count,
                MIN(orders.price) AS min_price,
                MAX(orders.price) AS max_price,
                MAX(orders.created_at) AS max_order_created_at
            FROM {schema}.\"order\" AS orders
            WHERE orders.status = 'open' AND orders.expires_at < {ts}
                AND ((LENGTH(orders.expires_at::text) = 13 AND TO_TIMESTAMP(orders.expires_at / 1000.0) > NOW())
                  OR (LENGTH(orders.expires_at::text) = 10 AND TO_TIMESTAMP(orders.expires_at) > NOW()))",
        schema = MARKETPLACE_SQUID_SCHEMA,
        ts = MAX_ORDER_TIMESTAMP,
    ));
    build_order_range_price_where(b, f);
    b.push_sql(
        " GROUP BY orders.item_id ) AS nfts_with_orders ON nfts_with_orders.item_id = items.id ",
    );
}

fn build_nfts_with_orders_cte_v2(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(&format!(
        ", nfts_with_orders AS (SELECT orders.item_id, COUNT(orders.id) AS orders_listings_count, MIN(orders.price) AS min_price, MAX(orders.price) AS max_price, MAX(orders.created_at) AS max_order_created_at FROM {schema}.\"order\" AS orders WHERE orders.status = 'open' AND orders.expires_at_normalized > NOW()",
        schema = MARKETPLACE_SQUID_SCHEMA,
    ));

    if f.is_on_sale == Some(false)
        && matches!(
            f.sort_by,
            Some(CatalogSortBy::Newest) | Some(CatalogSortBy::RecentlySold)
        )
    {
        b.push_sql(" AND orders.item_id IN (SELECT id::text FROM top_n_items)");
    }
    build_order_range_price_where(b, f);
    b.push_sql(" GROUP BY orders.item_id )");
}

fn build_trades_cte(b: &mut Builder) {
    b.push_sql(" WITH unified_trades AS ( SELECT * FROM marketplace.mv_trades ) ");
}

fn build_top_n_items_cte(b: &mut Builder, f: &CatalogFilters) {
    if f.is_on_sale == Some(false)
        && matches!(
            f.sort_by,
            Some(CatalogSortBy::Newest) | Some(CatalogSortBy::RecentlySold)
        )
    {
        let limit = clamp_first(f.first, 10);
        let offset = clamp_skip(f.skip);
        b.push_sql(&format!(
            ", top_n_items AS ( SELECT * FROM {schema}.item AS items ",
            schema = MARKETPLACE_SQUID_SCHEMA,
        ));
        build_item_level_filters_where(b, f);
        let order_col = if matches!(f.sort_by, Some(CatalogSortBy::Newest)) {
            "first_listed_at"
        } else {
            "sold_at"
        };
        let li = b.bind_i64(limit);
        let oi = b.bind_i64(offset);
        b.push_sql(&format!(
            " ORDER BY items.{} DESC LIMIT ${} OFFSET ${} )",
            order_col, li, oi
        ));
    }
}

fn build_min_item_created_at_cte(b: &mut Builder) {
    b.push_sql(
        ", ut_min_item AS (SELECT contract_address_sent, (assets -> 'sent' ->> 'item_id') AS item_id, MIN(created_at) AS min_item_created_at FROM unified_trades WHERE type = 'public_item_order' GROUP BY contract_address_sent, (assets -> 'sent' ->> 'item_id'))",
    );
}

fn build_trades_join(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(
        " LEFT JOIN (
            SELECT
                COUNT(id),
                COUNT(id) FILTER (WHERE status = 'open' and type = 'public_nft_order') AS nfts_listings_count,
                COUNT(id) FILTER (WHERE status = 'open' and type = 'public_item_order') AS items_listings_count,
                contract_address_sent,
                MIN(amount_received) FILTER (WHERE status = 'open' and type = 'public_nft_order') AS min_order_amount_received,
                MAX(amount_received) FILTER (WHERE status = 'open' and type = 'public_nft_order') AS max_order_amount_received,
                assets -> 'sent' ->> 'item_id' AS item_id,
                MAX(created_at) AS max_created_at,
                MAX(id::text) FILTER (WHERE status = 'open' and type = 'public_item_order') AS open_item_trade_id,
                MAX(amount_received) FILTER (WHERE status = 'open' and type = 'public_item_order') AS open_item_trade_price,
                MIN(created_at) FILTER (WHERE type = 'public_item_order') AS item_first_listed_at
            FROM unified_trades
            WHERE status = 'open' AND (available IS NULL OR available > 0)",
    );
    if f.only_minting {
        b.push_sql(" AND type = 'public_item_order'");
    }
    if let Some(mn) = &f.min_price {
        let bi = b.bind_string(mn.clone());
        b.push_sql(&format!(" AND amount_received >= ${}", bi));
    }
    if let Some(mx) = &f.max_price {
        let bi = b.bind_string(mx.clone());
        b.push_sql(&format!(" AND amount_received <= ${}", bi));
    }
    b.push_sql(
        " GROUP BY contract_address_sent, assets -> 'sent' ->> 'item_id') AS offchain_orders ON offchain_orders.contract_address_sent = items.collection_id AND offchain_orders.item_id::numeric = items.blockchain_id LEFT JOIN ut_min_item ON offchain_orders.contract_address_sent = ut_min_item.contract_address_sent AND offchain_orders.item_id = ut_min_item.item_id ",
    );
}

fn build_item_level_filters_where(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(" WHERE items.search_is_collection_approved = true ");
    let mut first = true;
    let and_sep = |b: &mut Builder, first: &mut bool| {
        if !*first {
            b.push_sql(" AND ");
        } else {
            b.push_sql(" AND ");
            *first = false;
        }
    };

    if f.category.is_some() {
        and_sep(b, &mut first);
        build_category_where(b, f);
    }
    if !f.rarities.is_empty() {
        and_sep(b, &mut first);
        build_rarities_where(b, f);
    }
    if !f.creator.is_empty() {
        and_sep(b, &mut first);
        build_creator_where(b, f);
    }
    if f.is_sold_out {
        and_sep(b, &mut first);
        build_is_sold_out_where(b);
    }
    if f.is_wearable_head {
        and_sep(b, &mut first);
        build_is_wearable_head_where(b);
    }
    if f.is_wearable_accessory {
        and_sep(b, &mut first);
        build_wearable_accessory_where(b);
    }
    if f.wearable_category.is_some() {
        and_sep(b, &mut first);
        build_wearable_category_where(b, f);
    }
    if !f.wearable_genders.is_empty() {
        and_sep(b, &mut first);
        build_wearable_gender_where(b, f);
    }
    if f.emote_category.is_some() {
        and_sep(b, &mut first);
        build_emote_category_where(b, f);
    }
    if !f.emote_play_mode.is_empty() && f.emote_play_mode.len() < 2 {
        and_sep(b, &mut first);
        build_emote_play_mode_where(b, f);
    }
    if !f.contract_addresses.is_empty() {
        and_sep(b, &mut first);
        build_contract_address_where(b, f);
    }
    if !f.ids.is_empty() {
        and_sep(b, &mut first);
        build_ids_where(b, f);
    }
    if f.emote_has_sound {
        and_sep(b, &mut first);
        build_has_sound_where(b);
    }
    if f.emote_has_geometry {
        and_sep(b, &mut first);
        build_has_geometry_where(b);
    }
    if f.emote_outcome_type.is_some() {
        and_sep(b, &mut first);
        build_has_outcome_type_where(b);
    }
    if !f.urns.is_empty() {
        and_sep(b, &mut first);
        build_urns_where(b, f);
    }
    if f.network.is_some() {
        and_sep(b, &mut first);
        build_network_where(b, f);
    }
}

fn build_get_min_price_case(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(
        " (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ",
    );
    if let Some(mn) = &f.min_price {
        let bi = b.bind_string(mn.clone());
        b.push_sql(&format!(" AND items.price >= ${}", bi));
    }
    b.push_sql(
        " THEN LEAST(items.price, nfts_with_orders.min_price) ELSE nfts_with_orders.min_price END)::text AS min_price ",
    );
}

fn build_get_max_price_case(b: &mut Builder, f: &CatalogFilters) {
    if f.only_minting {
        b.push_sql(" (CASE WHEN items.available > 0 AND items.search_is_store_minter = true ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            b.push_sql(&format!(" AND items.price <= ${}", bi));
        }
        b.push_sql(" THEN items.price ELSE NULL END)::text AS max_price ");
    } else {
        b.push_sql(" (CASE WHEN items.available > 0 AND items.search_is_store_minter = true ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            b.push_sql(&format!(" AND items.price <= ${}", bi));
        }
        b.push_sql(" THEN GREATEST(items.price, nfts_with_orders.max_price) ELSE nfts_with_orders.max_price END)::text AS max_price ");
    }
}

fn build_get_min_price_case_with_trades(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(
        " (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ",
    );
    if let Some(mn) = &f.min_price {
        let bi = b.bind_string(mn.clone());
        b.push_sql(&format!(" AND items.price >= ${}", bi));
    }
    if f.only_minting {
        b.push_sql(&format!(
            " THEN LEAST(COALESCE(items.price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) ELSE LEAST(COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) END)::text AS min_price ",
            n = MAX_NUMERIC_NUMBER,
        ));
    } else {
        b.push_sql(&format!(
            " THEN LEAST(COALESCE(items.price, '{n}'), COALESCE(nfts_with_orders.min_price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) ELSE LEAST(COALESCE(nfts_with_orders.min_price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) END)::text AS min_price ",
            n = MAX_NUMERIC_NUMBER,
        ));
    }
}

fn build_get_max_price_case_with_trades(b: &mut Builder, f: &CatalogFilters) {
    if f.only_minting {
        b.push_sql(" (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            b.push_sql(&format!(" AND items.price <= ${}", bi));
        }
        b.push_sql(" THEN GREATEST(items.price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) ELSE GREATEST(offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) END)::text AS max_price ");
    } else {
        b.push_sql(" (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            b.push_sql(&format!(" AND items.price <= ${}", bi));
        }
        b.push_sql(" THEN GREATEST(items.price, nfts_with_orders.max_price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) ELSE GREATEST(nfts_with_orders.max_price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) END)::text AS max_price ");
    }
}

fn build_collections_items_catalog_query(f: &CatalogFilters) -> (String, PgArguments) {
    let mut b = Builder::new();
    b.push_sql(
        " SELECT
            items.id,
            items.blockchain_id::text AS blockchain_id,
            items.search_is_collection_approved,
            to_json(
                CASE WHEN (items.item_type = 'wearable_v1' OR items.item_type = 'wearable_v2' OR items.item_type = 'smart_wearable_v1') THEN metadata_wearable
                ELSE metadata_emote END
            ) as metadata,
            items.image,
            items.collection_id,
            items.rarity,
            items.item_type::text,
            items.price::text AS price,
            items.available::text AS available,
            items.search_is_store_minter,
            items.search_is_marketplace_v3_minter,
            items.creator,
            items.beneficiary,
            items.created_at::text AS created_at,
            items.updated_at::text AS updated_at,
            items.reviewed_at::text AS reviewed_at,
            items.sold_at::text AS sold_at,
            items.network,
            items.first_listed_at::text AS first_listed_at,
            items.urn,
            NULL::text AS open_item_trade_id,
            NULL::text AS open_item_trade_price,
        ",
    );

    if f.only_minting {
        b.push_sql(" NULL::text AS min_listing_price, NULL::text AS max_listing_price, 0::int8 AS listings_count, ");
    } else {
        b.push_sql(" nfts_with_orders.min_price::text AS min_listing_price, nfts_with_orders.max_price::text AS max_listing_price, COALESCE(nfts_with_orders.listings_count, 0)::int8 AS listings_count, ");
    }
    if f.is_on_sale == Some(false) {
        b.push_sql(" nfts.owners_count::int8 AS owners_count, ");
    } else {
        b.push_sql(" NULL::int8 AS owners_count, ");
    }
    if f.only_minting {
    } else {
        b.push_sql(" nfts_with_orders.max_order_created_at::text as max_order_created_at, ");
    }

    build_get_min_price_case(&mut b, f);
    b.push_sql(", ");
    build_get_max_price_case(&mut b, f);
    b.push_sql(&format!(
        " FROM {schema}.item AS items ",
        schema = MARKETPLACE_SQUID_SCHEMA
    ));
    if f.is_on_sale == Some(false) {
        build_owners_join(&mut b);
    }
    build_nfts_with_orders_cte_v1(&mut b, f);
    build_metadata_joins(&mut b);
    build_collections_where(&mut b, f, false);
    build_order_by(&mut b, f, false);
    build_limit_offset(&mut b, f);

    (b.sql, b.args)
}

pub fn build_collections_items_catalog_query_with_trades(
    f: &CatalogFilters,
) -> (String, PgArguments) {
    let mut b = Builder::new();
    build_trades_cte(&mut b);
    build_top_n_items_cte(&mut b, f);
    if !f.only_minting {
        build_nfts_with_orders_cte_v2(&mut b, f);
    }
    build_min_item_created_at_cte(&mut b);

    b.push_sql(
        " SELECT
            items.id,
            items.blockchain_id::text AS blockchain_id,
            items.search_is_collection_approved,
            to_json(
                CASE WHEN (items.item_type = 'wearable_v1' OR items.item_type = 'wearable_v2' OR items.item_type = 'smart_wearable_v1') THEN metadata_wearable
                ELSE metadata_emote END
            ) as metadata,
            items.image,
            items.collection_id,
            items.rarity,
            items.item_type::text,
            items.price::text AS price,
            items.available::text AS available,
            items.search_is_store_minter,
            items.search_is_marketplace_v3_minter,
            items.creator,
            items.beneficiary,
            items.created_at::text AS created_at,
            items.updated_at::text AS updated_at,
            items.reviewed_at::text AS reviewed_at,
            items.sold_at::text AS sold_at,
            items.network,
            offchain_orders.open_item_trade_id::text AS open_item_trade_id,
            offchain_orders.open_item_trade_price::text AS open_item_trade_price,
        ",
    );
    if f.is_on_sale == Some(true) {
        b.push_sql(" LEAST(items.first_listed_at, ROUND(EXTRACT(EPOCH FROM ut_min_item.min_item_created_at)))::text as first_listed_at, ");
    } else {
        b.push_sql(" items.first_listed_at::text as first_listed_at, ");
    }

    if f.only_minting {
        b.push_sql(&format!(
            " items.urn,
              (CASE WHEN offchain_orders.min_order_amount_received IS NULL THEN NULL
                   ELSE LEAST(COALESCE(offchain_orders.min_order_amount_received, '{n}')) END)::text AS min_listing_price,
              0::int8 AS min_onchain_price,
              offchain_orders.max_order_amount_received::text AS max_listing_price,
              NULL::text AS max_onchain_price,
              COALESCE(offchain_orders.nfts_listings_count, 0)::int8 AS listings_count,
              COALESCE(offchain_orders.count, 0)::int8 AS offchain_listings_count,
              0::int8 as onchain_listings_count,
              EXTRACT(EPOCH FROM offchain_orders.max_created_at)::text AS max_order_created_at,
            ",
            n = MAX_NUMERIC_NUMBER
        ));
    } else {
        b.push_sql(" items.urn,
              (CASE WHEN offchain_orders.min_order_amount_received IS NULL AND nfts_with_orders.min_price IS NULL THEN NULL
                   ELSE LEAST(COALESCE(offchain_orders.min_order_amount_received, nfts_with_orders.min_price), COALESCE(nfts_with_orders.min_price, offchain_orders.min_order_amount_received)) END)::text AS min_listing_price,
              nfts_with_orders.min_price::text AS min_onchain_price,
              GREATEST(offchain_orders.max_order_amount_received, nfts_with_orders.max_price)::text AS max_listing_price,
              nfts_with_orders.max_price::text AS max_onchain_price,
              (COALESCE(nfts_with_orders.orders_listings_count, 0) + COALESCE(offchain_orders.nfts_listings_count, 0))::int8 AS listings_count,
              COALESCE(offchain_orders.count, 0)::int8 AS offchain_listings_count,
              COALESCE(nfts_with_orders.orders_listings_count, 0)::int8 as onchain_listings_count,
              GREATEST(ROUND(EXTRACT(EPOCH FROM offchain_orders.max_created_at)), nfts_with_orders.max_order_created_at)::text AS max_order_created_at,
            ");
    }
    if f.is_on_sale == Some(false) {
        b.push_sql(" nfts.owners_count::int8 AS owners_count, ");
    } else {
        b.push_sql(" NULL::int8 AS owners_count, ");
    }
    build_get_min_price_case_with_trades(&mut b, f);
    b.push_sql(", ");
    build_get_max_price_case_with_trades(&mut b, f);

    if f.is_on_sale == Some(false)
        && matches!(
            f.sort_by,
            Some(CatalogSortBy::Newest) | Some(CatalogSortBy::RecentlySold)
        )
    {
        b.push_sql(" FROM top_n_items as items ");
    } else {
        b.push_sql(&format!(
            " FROM {schema}.item AS items ",
            schema = MARKETPLACE_SQUID_SCHEMA
        ));
    }
    if f.is_on_sale == Some(false) {
        build_owners_join(&mut b);
    }
    if !f.only_minting {
        b.push_sql(" LEFT JOIN nfts_with_orders ON nfts_with_orders.item_id = items.id ");
    }
    build_metadata_joins(&mut b);
    build_trades_join(&mut b, f);
    build_collections_where(&mut b, f, true);
    build_order_by(&mut b, f, true);
    build_limit_offset(&mut b, f);

    (b.sql, b.args)
}

fn build_collections_items_count_query(f: &CatalogFilters) -> (String, PgArguments) {
    let mut b = Builder::new();
    b.push_sql(&format!(
        " SELECT COUNT(*) AS total FROM {schema}.item AS items ",
        schema = MARKETPLACE_SQUID_SCHEMA,
    ));

    let needs_metadata_joins = f.wearable_category.is_some()
        || f.emote_category.is_some()
        || !f.emote_play_mode.is_empty();
    if needs_metadata_joins {
        build_metadata_joins(&mut b);
    }

    build_item_level_filters_where(&mut b, f);

    if f.is_on_sale == Some(false) {
        b.push_sql(" AND (items.search_is_store_minter = false OR items.available = 0)");
        b.push_sql(&format!(
            " AND NOT EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id)",
            schema = MARKETPLACE_SQUID_SCHEMA,
        ));
        b.push_sql(" AND NOT EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id)");
    } else if f.is_on_sale == Some(true) {
        b.push_sql(&format!(
            " AND ((items.search_is_store_minter = true AND items.available > 0) OR EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id))",
            schema = MARKETPLACE_SQUID_SCHEMA,
        ));
    }

    if f.only_minting {
        b.push_sql(" AND ((items.search_is_store_minter = true AND items.available > 0) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND t.type = 'public_item_order' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id");
        if let Some(mn) = &f.min_price {
            let bi = b.bind_string(mn.clone());
            b.push_sql(&format!(" AND t.amount_received >= ${}", bi));
        }
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            b.push_sql(&format!(" AND t.amount_received <= ${}", bi));
        }
        b.push_sql("))");
    }

    if f.only_listing {
        b.push_sql(" AND ((items.search_is_store_minter = false AND items.search_is_marketplace_v3_minter = false) OR (items.search_is_store_minter = true AND items.available = 0) OR (items.search_is_marketplace_v3_minter = true AND NOT EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND t.type = 'public_item_order' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id)))");
        b.push_sql(&format!(
            " AND (EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND t.type = 'public_nft_order' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id))",
            schema = MARKETPLACE_SQUID_SCHEMA,
        ));
    }

    if let Some(mn) = f.min_price.clone() {
        if f.only_minting {
            let bi = b.bind_string(mn);
            b.push_sql(&format!(
                " AND items.price >= ${} AND items.price IS DISTINCT FROM '{}'",
                bi, MAX_NUMERIC_NUMBER
            ));
        } else if f.only_listing {
            let bi = b.bind_string(mn);
            b.push_sql(&format!(
                " AND (EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id AND o.price >= ${0}) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND t.type = 'public_nft_order' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id AND t.amount_received >= ${0}))",
                bi, schema = MARKETPLACE_SQUID_SCHEMA,
            ));
        } else {
            let bi = b.bind_string(mn);
            b.push_sql(&format!(
                " AND ((items.price >= ${0} AND items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true)) OR EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id AND o.price >= ${0}) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id AND t.amount_received >= ${0}))",
                bi, schema = MARKETPLACE_SQUID_SCHEMA,
            ));
        }
    }

    if let Some(mx) = f.max_price.clone() {
        if f.only_minting {
            let bi = b.bind_string(mx);
            b.push_sql(&format!(" AND items.price <= ${}", bi));
        } else if f.only_listing {
            let bi = b.bind_string(mx);
            b.push_sql(&format!(
                " AND (EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id AND o.price <= ${0}) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND t.type = 'public_nft_order' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id AND t.amount_received <= ${0}))",
                bi, schema = MARKETPLACE_SQUID_SCHEMA,
            ));
        } else {
            let bi = b.bind_string(mx);
            b.push_sql(&format!(
                " AND ((items.price <= ${0} AND items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true)) OR EXISTS (SELECT 1 FROM {schema}.\"order\" AS o WHERE o.status = 'open' AND o.expires_at_normalized > NOW() AND o.item_id = items.id AND o.price <= ${0}) OR EXISTS (SELECT 1 FROM marketplace.mv_trades AS t WHERE t.status = 'open' AND (t.available IS NULL OR t.available > 0) AND t.contract_address_sent = items.collection_id AND (t.assets->'sent'->>'item_id')::numeric = items.blockchain_id AND t.amount_received <= ${0}))",
                bi, schema = MARKETPLACE_SQUID_SCHEMA,
            ));
        }
    }

    (b.sql, b.args)
}

fn build_search_query(f: &CatalogFilters) -> (String, PgArguments) {
    let mut b = Builder::new();
    let search = f.search.clone().unwrap_or_default();
    let bi = b.bind_string(format!("%{}%", search.to_lowercase()));
    b.push_sql(&format!(
        " SELECT DISTINCT items.id::text AS id, 'name'::text AS match_type, COALESCE(wearable.name, emote.name, '') AS word, 0.5::real AS word_similarity FROM {schema}.item AS items LEFT JOIN {schema}.wearable AS wearable ON wearable.id = items.metadata_id LEFT JOIN {schema}.emote AS emote ON emote.id = items.metadata_id WHERE lower(COALESCE(wearable.name, emote.name, '')) LIKE ${}",
        bi,
        schema = MARKETPLACE_SQUID_SCHEMA,
    ));
    let _ = BUILDER_SERVER_TABLE_SCHEMA;
    (b.sql, b.args)
}

pub struct CatalogComponent {
    pool: PgPool,
}

impl CatalogComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch(
        &self,
        mut filters: CatalogFilters,
        _search_id: &str,
        _anon_id: &str,
        is_v2: bool,
    ) -> Result<(Vec<CatalogItem>, i64), ApiError> {
        if let Some(ref search) = filters.search.clone() {
            if !search.trim().is_empty() {
                let (sql, args) = build_search_query(&filters);
                let rows = sqlx::query_with(&sql, args)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|_e| {
                        ApiError::bad_request(
                            "Couldn't fetch the catalog with the filters provided",
                        )
                    })?;
                let mut matched: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.try_get::<String, _>("id").ok())
                    .collect();
                if matched.is_empty() {
                    return Ok((Vec::new(), 0));
                }
                filters.ids.append(&mut matched);
            }
        }

        let network_hint = filters.network;
        let (items_sql, items_args) = if is_v2 {
            build_collections_items_catalog_query_with_trades(&filters)
        } else {
            build_collections_items_catalog_query(&filters)
        };
        let (count_sql, count_args) = build_collections_items_count_query(&filters);

        let items_q: Query<'_, Postgres, PgArguments> = sqlx::query_with(&items_sql, items_args);
        let count_q: Query<'_, Postgres, PgArguments> = sqlx::query_with(&count_sql, count_args);

        let items_fut = items_q.fetch_all(&self.pool);
        let count_fut = count_q.fetch_one(&self.pool);
        let (items_rows, count_row) = tokio::try_join!(items_fut, count_fut).map_err(|e| {
            tracing::error!(error = ?e, "catalog query failed");
            ApiError::bad_request("Couldn't fetch the catalog with the filters provided")
        })?;

        let total: i64 = count_row.try_get::<i64, _>("total").unwrap_or(0);

        let mut items: Vec<CatalogItem> = Vec::with_capacity(items_rows.len());
        for r in &items_rows {
            let row = DbRow {
                id: r.try_get("id").unwrap_or_default(),
                blockchain_id: r.try_get("blockchain_id").unwrap_or_default(),
                image: r.try_get("image").unwrap_or_default(),
                collection_id: r.try_get("collection_id").unwrap_or_default(),
                rarity: r.try_get("rarity").unwrap_or_default(),
                item_type: r.try_get("item_type").unwrap_or_default(),
                price: r.try_get("price").unwrap_or_default(),
                available: r.try_get("available").unwrap_or_default(),
                search_is_store_minter: r.try_get("search_is_store_minter").unwrap_or(false),
                search_is_marketplace_v3_minter: r
                    .try_get("search_is_marketplace_v3_minter")
                    .unwrap_or(false),
                creator: r.try_get("creator").unwrap_or_default(),
                beneficiary: r.try_get("beneficiary").ok(),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
                reviewed_at: r.try_get("reviewed_at").unwrap_or_default(),
                sold_at: r.try_get("sold_at").unwrap_or_default(),
                first_listed_at: r.try_get("first_listed_at").ok(),
                urn: r.try_get("urn").unwrap_or_default(),
                network: r.try_get("network").unwrap_or_default(),
                metadata: r.try_get("metadata").ok(),
                min_listing_price: r.try_get("min_listing_price").ok(),
                max_listing_price: r.try_get("max_listing_price").ok(),
                open_item_trade_id: r.try_get("open_item_trade_id").ok(),
                open_item_trade_price: r.try_get("open_item_trade_price").ok(),
                listings_count: r.try_get("listings_count").ok(),
                owners_count: r.try_get("owners_count").ok(),
                min_price: r.try_get("min_price").ok(),
                max_price: r.try_get("max_price").ok(),
            };
            items.push(from_db_row_to_catalog_item(row, network_hint));
        }

        Ok((items, total))
    }
}

// Retained for reference; category filters now use bound params (no callers).
#[allow(dead_code)]
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}
