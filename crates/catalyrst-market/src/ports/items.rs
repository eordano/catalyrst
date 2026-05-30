use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::{
    ethereum_chain_id, get_db_networks, polygon_chain_id, ChainId, Network, NftCategory,
};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip, where_from};
use crate::MARKETPLACE_SQUID_SCHEMA;

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

#[derive(Debug, Clone, Default)]
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

    // ORDER BY expression over the outer projection. Every variant carries a
    // stable `item.id ASC` tie-breaker so LIMIT/OFFSET pages partition
    // deterministically. The outer `price` alias is item.price::text, so cheapest
    // MUST cast back to numeric (price::numeric) — a bare `price ASC` sorts
    // lexicographically ("9e18" after "1e19"). name collates as text (matching
    // upstream's createdAt/price/searchText sort fields).
    fn order_by(&self) -> &'static str {
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

#[derive(Debug, Serialize)]
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
    pub available: i64,
    #[serde(rename = "isOnSale")]
    pub is_on_sale: bool,
    pub creator: String,
    pub beneficiary: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "reviewedAt")]
    pub reviewed_at: i64,
    #[serde(rename = "soldAt")]
    pub sold_at: i64,
    pub data: ItemData,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub urn: String,
    #[serde(rename = "firstListedAt", skip_serializing_if = "Option::is_none")]
    pub first_listed_at: Option<i64>,
    pub picks: PicksCount,
    // Prod nft-server always emits these three (as explicit null when absent);
    // omitting them diverged on the wire, so serialize them unconditionally.
    pub utility: Option<String>,
    #[serde(rename = "tradeId")]
    pub trade_id: Option<String>,
    #[serde(rename = "tradeExpiresAt", skip_serializing_if = "Option::is_none")]
    pub trade_expires_at: Option<i64>,
    #[serde(rename = "tradeContractAddress")]
    pub trade_contract_address: Option<String>,
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
pub struct PicksCount {
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
        max_price: p
            .get_string("maxPrice", None)
            .filter(|s| !s.trim().is_empty()),
        min_price: p
            .get_string("minPrice", None)
            .filter(|s| !s.trim().is_empty()),
        urns: p.get_list("urn", &[]),
        ids: p.get_list("id", &[]),
        sort_by: p.get_string("sortBy", None).and_then(|s| ItemSortBy::from_str(&s)),
    })
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

    pub async fn get_items(&self, filters: &ItemFilters) -> Result<(Vec<Item>, i64), ApiError> {
        let (sql, binds) = build_items_query(filters);
        let sql = sql.replace("marketplace.mv_trades", MV_TRADES_STUB);
        let mut q = sqlx::query_as::<_, DbItem>(&sql);
        for b in &binds {
            q = match b {
                Bind::Text(s) => q.bind(s.clone()),
                Bind::TextArray(v) => q.bind(v.clone()),
                Bind::Int(i) => q.bind(*i),
            };
        }
        let rows: Vec<DbItem> = q.fetch_all(&self.pool).await?;
        let total = rows.first().map(|r| r.count).unwrap_or(0);
        let results = rows.iter().map(from_db_item_to_item).collect();
        Ok((results, total))
    }
}

const MV_TRADES_STUB: &str = "(SELECT \
    NULL::text       AS id, \
    NULL::timestamptz AS created_at, \
    NULL::text       AS type, \
    NULL::text       AS signer, \
    NULL::text       AS contract_address_sent, \
    NULL::numeric    AS amount_received, \
    NULL::numeric    AS available, \
    NULL::jsonb      AS assets, \
    NULL::text       AS sent_contract_address, \
    NULL::text       AS sent_token_id, \
    NULL::text       AS sent_nft_category, \
    NULL::text       AS sent_item_id, \
    NULL::text       AS sent_nft_id, \
    NULL::text       AS network, \
    NULL::timestamptz AS expires_at, \
    NULL::text       AS trade_contract, \
    NULL::text       AS status \
    WHERE FALSE)";

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

pub fn build_items_query(filters: &ItemFilters) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;
    let emit = |bind: Bind, binds: &mut Vec<Bind>, idx: &mut usize| -> String {
        binds.push(bind);
        let s = format!("${}", *idx);
        *idx += 1;
        s
    };

    let trades_category_clause = if let Some(c) = filters.category {
        let placeholder = emit(
            Bind::Text(nft_category_db_str(c).to_string()),
            &mut binds,
            &mut next_idx,
        );
        format!("WHERE sent_nft_category = {}", placeholder)
    } else {
        String::new()
    };

    let mut wheres: Vec<String> = Vec::new();

    if let Some(c) = filters.category {
        let types = get_item_types_from_nft_category(c);
        if !types.is_empty() {
            let p = emit(
                Bind::TextArray(types.into_iter().map(String::from).collect()),
                &mut binds,
                &mut next_idx,
            );
            wheres.push(format!(" LOWER(item.item_type) = ANY ({}) ", p));
        }
    }

    if !filters.creator.is_empty() {
        let lower: Vec<String> = filters.creator.iter().map(|c| c.to_lowercase()).collect();
        let p = emit(Bind::TextArray(lower), &mut binds, &mut next_idx);
        wheres.push(format!(" LOWER(item.creator) = ANY({}) ", p));
    }

    if !filters.rarities.is_empty() {
        let p = emit(
            Bind::TextArray(filters.rarities.clone()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.rarity = ANY ({}) ", p));
    }

    if filters.is_sold_out {
        wheres.push(" item.available = 0 ".to_string());
    }

    if filters.is_on_sale == Some(true) {
        wheres.push(
            " (((unified_trades.id IS NOT NULL AND item.search_is_marketplace_v3_minter = true) \
                OR item.search_is_store_minter = true) AND item.available > 0) "
                .to_string(),
        );
    }

    if let Some(ref s) = filters.search {
        let p = emit(Bind::Text(s.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" item.search_text % {} ", p));
    }

    if filters.is_wearable_head {
        wheres.push(" item.search_is_wearable_head = true ".to_string());
    }
    if filters.is_wearable_accessory {
        wheres.push(" item.search_is_wearable_accessory = true ".to_string());
    }
    if filters.is_wearable_smart {
        let p = emit(
            Bind::Text(ItemType::SmartWearableV1.as_str().to_string()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.item_type = {} ", p));
    }

    if let Some(ref wc) = filters.wearable_category {
        let p = emit(Bind::Text(wc.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" wearable.category = {} ", p));
    }

    if !filters.wearable_genders.is_empty() {
        if let Some(arr) = body_shapes_for_genders(&filters.wearable_genders) {
            let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
            wheres.push(format!(" item.search_wearable_body_shapes @> {} ", p));
        }
    }

    if let Some(ref ec) = filters.emote_category {
        let p = emit(Bind::Text(ec.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" emote.category = {} ", p));
    }

    if !filters.emote_genders.is_empty() {
        if let Some(arr) = body_shapes_for_genders(&filters.emote_genders) {
            let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
            wheres.push(format!(" item.search_emote_body_shapes @> {} ", p));
        }
    }

    if let Some(mode) = emote_play_mode_clause(&filters.emote_play_mode) {
        wheres.push(format!(" item.search_emote_loop = {} ", mode));
    }

    if !filters.contract_addresses.is_empty() {
        let p = emit(
            Bind::TextArray(filters.contract_addresses.clone()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.collection_id = ANY ({}) ", p));
    }

    if let Some(ref it) = filters.item_id {
        // blockchain_id is NUMERIC; the filter value is bound as text, so compare
        // on the text form (a bare `numeric = text` errors: "operator does not
        // exist"). Exercised by ?itemId= and the /v1/trendings item lookup.
        let p = emit(Bind::Text(it.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" item.blockchain_id::text = {} ", p));
    }

    if !filters.ids.is_empty() {
        let p = emit(
            Bind::TextArray(filters.ids.clone()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.id = ANY ({}) ", p));
    }

    if let Some(n) = filters.network {
        let p = emit(
            Bind::TextArray(get_db_networks(n).into_iter().map(String::from).collect()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.network = ANY ({}) ", p));
    }

    if let Some(ref mn) = filters.min_price {
        let p = emit(Bind::Text(mn.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(
            " ((item.search_is_store_minter = true AND item.price >= {p}) \
              OR (item.search_is_marketplace_v3_minter = true \
                AND (unified_trades.assets -> 'received' ->> 'amount')::numeric(78) >= {p})) ",
            p = p
        ));
    }
    if let Some(ref mx) = filters.max_price {
        let p = emit(Bind::Text(mx.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(
            " ((item.search_is_store_minter = true AND item.price <= {p}) \
              OR (item.search_is_marketplace_v3_minter = true \
                AND (unified_trades.assets -> 'received' ->> 'amount')::numeric(78) <= {p})) ",
            p = p
        ));
    }

    if filters.emote_has_sound {
        wheres.push(" emote.has_sound = true ".to_string());
    }
    if filters.emote_has_geometry {
        wheres.push(" emote.has_geometry = true ".to_string());
    }
    if filters.emote_outcome_type.is_some() {
        wheres.push(" emote.outcome_type IS NOT NULL ".to_string());
    }

    if !filters.urns.is_empty() {
        let p = emit(
            Bind::TextArray(expand_urn_network_forms(&filters.urns)),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" item.urn = ANY ({}) ", p));
    }

    let where_clause = where_from(&wheres);

    // Default to NEWEST (upstream ITEM_DEFAULT_SORT_BY); always include the
    // id tie-breaker so pagination is deterministic.
    let order_by = filters
        .sort_by
        .unwrap_or(ItemSortBy::Newest)
        .order_by();

    let limit = clamp_first(filters.first, DEFAULT_LIMIT);
    let offset = clamp_skip(filters.skip);
    let limit_p = emit(Bind::Int(limit), &mut binds, &mut next_idx);
    let offset_p = emit(Bind::Int(offset), &mut binds, &mut next_idx);

    let sql = format!(
        "WITH unified_trades AS (\
            SELECT * FROM marketplace.mv_trades {trades_cat}\
         )\n\
         SELECT\n\
           COUNT(*) OVER() as count,\n\
           item.id,\n\
           item.image,\n\
           item.uri,\n\
           item.blockchain_id::text as item_id,\n\
           item.collection_id as contract_address,\n\
           coalesce(wearable.rarity, emote.rarity) as rarity,\n\
           item.price::text as price,\n\
           item.available::int8 as available,\n\
           item.creator,\n\
           item.beneficiary,\n\
           item.created_at::int8 as created_at,\n\
           item.updated_at::int8 as updated_at,\n\
           item.reviewed_at::int8 as reviewed_at,\n\
           item.sold_at::int8 as sold_at,\n\
           item.urn,\n\
           item.network,\n\
           item.search_is_store_minter,\n\
           item.search_is_marketplace_v3_minter,\n\
           unified_trades.id::text as trade_id,\n\
           coalesce(wearable.name, emote.name) as name,\n\
           wearable.body_shapes as wearable_body_shapes,\n\
           emote.body_shapes as emote_body_shapes,\n\
           wearable.category as wearable_category,\n\
           emote.category as emote_category,\n\
           item.item_type,\n\
           emote.loop,\n\
           emote.has_sound,\n\
           emote.has_geometry,\n\
           emote.outcome_type as emote_outcome_type,\n\
           coalesce(wearable.description, emote.description) as description,\n\
           coalesce(to_timestamp(item.first_listed_at) AT TIME ZONE 'UTC', unified_trades.created_at) as first_listed_at,\n\
           unified_trades.assets -> 'received' ->> 'beneficiary' as trade_beneficiary,\n\
           unified_trades.expires_at as trade_expires_at,\n\
           unified_trades.trade_contract as trade_contract,\n\
           (unified_trades.assets -> 'received' ->> 'amount')::text as trade_price,\n\
           NULL::text as utility\n\
         FROM {schema}.item item\n\
         LEFT JOIN {schema}.metadata metadata ON item.metadata_id = metadata.id\n\
         LEFT JOIN {schema}.wearable wearable ON metadata.wearable_id = wearable.id\n\
         LEFT JOIN {schema}.emote emote ON metadata.emote_id = emote.id\n\
         LEFT JOIN unified_trades ON sent_item_id = item.blockchain_id::text \
            AND sent_contract_address = item.collection_id \
            AND type = 'public_item_order' AND status = 'open'\n\
         {where_clause}\n\
         {order_by}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        trades_cat = trades_category_clause,
        schema = MARKETPLACE_SQUID_SCHEMA,
        where_clause = where_clause,
        order_by = order_by,
        limit_p = limit_p,
        offset_p = offset_p,
    );

    (sql, binds)
}

fn nft_category_db_str(c: NftCategory) -> &'static str {
    match c {
        NftCategory::Parcel => "parcel",
        NftCategory::Estate => "estate",
        NftCategory::Wearable => "wearable",
        NftCategory::Ens => "ens",
        NftCategory::Emote => "emote",
    }
}

fn body_shapes_for_genders(genders: &[String]) -> Option<Vec<String>> {
    let has_unisex = genders.iter().any(|g| g == "unisex");
    let has_male = has_unisex || genders.iter().any(|g| g == "male");
    let has_female = has_unisex || genders.iter().any(|g| g == "female");
    let mut out = Vec::new();
    if has_male {
        out.push("BaseMale".to_string());
    }
    if has_female {
        out.push("BaseFemale".to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn emote_play_mode_clause(modes: &[String]) -> Option<bool> {
    if modes.is_empty() || modes.len() == 2 {
        return None;
    }
    if modes.iter().any(|m| m == "loop") {
        Some(true)
    } else {
        Some(false)
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
    let image = fix_urn(&d.image.clone().unwrap_or_default());
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
        id: d.id.clone(),
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

/// Expand a list of `urn` filter values so it matches the squid DB regardless
/// of the `:ethereum:` vs `:mainnet:` ethereum network-token form.
///
/// The squid mirror stores ethereum collection/item urns under the
/// `:mainnet:` token, but every urn the API *returns* is rewritten to
/// `:ethereum:` by [`fix_urn`]. A client that round-trips a returned ethereum
/// urn back into a `urn=` filter therefore queries with `:ethereum:` and
/// silently matches nothing (the same latent asymmetry exists upstream in
/// `marketplace-server`). This helper makes the filter token-form-agnostic by
/// including BOTH forms of every value. It is strictly additive — for any urn
/// already in `:mainnet:` form (the upstream parity case) the original value is
/// always present, so existing matches are never lost; it only *adds* the
/// rewritten counterpart, closing the round-trip miss.
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
mod urn_form_tests {
    use super::expand_urn_network_forms;

    #[test]
    fn expands_both_ethereum_token_forms() {
        let eth = "urn:decentraland:ethereum:collections-v1:0xabc:0".to_string();
        let out = expand_urn_network_forms(&[eth.clone()]);
        // additive: original kept, plus the :mainnet: counterpart for the DB.
        assert!(out.contains(&eth));
        assert!(out.contains(&"urn:decentraland:mainnet:collections-v1:0xabc:0".to_string()));
    }

    #[test]
    fn mainnet_input_still_present_parity_preserved() {
        // The upstream parity case (clients pass :mainnet:) must never lose its
        // original value — only gain the :ethereum: counterpart.
        let main = "urn:decentraland:mainnet:collections-v1:0xabc:0".to_string();
        let out = expand_urn_network_forms(&[main.clone()]);
        assert!(out.contains(&main));
    }

    #[test]
    fn matic_urn_unchanged_no_dupes() {
        let matic = "urn:decentraland:matic:collections-v2:0xdef:1".to_string();
        let out = expand_urn_network_forms(&[matic.clone()]);
        assert_eq!(out, vec![matic]); // no ethereum/mainnet token -> single value
    }

    #[test]
    fn dedups_when_both_forms_supplied() {
        let a = "urn:decentraland:ethereum:x:0xa:0".to_string();
        let b = "urn:decentraland:mainnet:x:0xa:0".to_string();
        let out = expand_urn_network_forms(&[a.clone(), b.clone()]);
        // both inputs expand to the same {ethereum, mainnet} set -> exactly 2 unique.
        assert_eq!(out.len(), 2);
        assert!(out.contains(&a) && out.contains(&b));
    }
}
