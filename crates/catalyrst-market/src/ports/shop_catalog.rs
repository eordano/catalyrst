use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::{ethereum_chain_id, polygon_chain_id, ChainId, Network};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::sql_filters::where_from;
use crate::MARKETPLACE_SQUID_SCHEMA;

pub const SHOP_DEFAULT_PAGE_SIZE: i64 = 48;
pub const SHOP_MIN_PAGE_SIZE: i64 = 1;
pub const SHOP_MAX_PAGE_SIZE: i64 = 1000;

// The received-asset type that marks a credit-buyable (Shop) listing, as opposed to a classic
// ERC20-MANA one.
const ASSET_TYPE_USD_PEGGED_MANA: i64 = 2;
// A classic MANA-priced received asset: a listing that predates the Shop and can be imported.
const ASSET_TYPE_ERC20: i64 = 1;

// 1 credit = $0.10; $1 = 1e18 USD wei = 10 credits, so 1 credit = 1e17 USD wei.
const USD_WEI_PER_CREDIT: u128 = 100_000_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSortBy {
    Newest,
    Cheapest,
    MostExpensive,
    Name,
}

pub const SHOP_SORT_VALUES: &[&str] = &["newest", "cheapest", "most_expensive", "name"];

impl ShopSortBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "newest" => Some(Self::Newest),
            "cheapest" => Some(Self::Cheapest),
            "most_expensive" => Some(Self::MostExpensive),
            "name" => Some(Self::Name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShopCatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub category: Option<String>,
    pub contract_address: Option<String>,
    pub item_id: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_categories: Vec<String>,
    pub min_price_credits: Option<f64>,
    pub max_price_credits: Option<f64>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
}

#[derive(Debug, Clone, Default)]
pub struct LegacyCatalogFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub category: Option<String>,
    pub rarities: Vec<String>,
    pub wearable_categories: Vec<String>,
    pub search: Option<String>,
    pub sort_by: Option<ShopSortBy>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ShopListing {
    pub trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub token_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    pub creator: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub price_credits: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

// A seller's OLD classic (ERC20-MANA) listing that can be re-listed into the Shop as
// credit-buyable. Carries the raw MANA price (client converts to credits via the oracle).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct ImportableListing {
    pub old_trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub token_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    pub mana_wei: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
}

// A classic (ERC20-MANA) PRIMARY listing surfaced as a paginated browse feed so the Shop can
// offer the "old liquidity" for purchase with credits. Primaries only: secondary-with-credits is
// disabled upstream, so public_nft_order rows are excluded entirely.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct LegacyListing {
    pub trade_id: String,
    pub listing_type: String,
    pub contract_address: String,
    pub item_id: Option<String>,
    pub name: String,
    pub thumbnail: String,
    pub rarity: String,
    pub category: String,
    pub wearable_category: Option<String>,
    pub creator: String,
    pub mana_wei: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ShopListingRow {
    trade_id: String,
    trade_type: String,
    contract_address: Option<String>,
    item_id: Option<String>,
    token_id: Option<String>,
    name: Option<String>,
    image: Option<String>,
    rarity: Option<String>,
    item_type: Option<String>,
    wearable_category: Option<String>,
    creator: Option<String>,
    price: Option<String>,
    available: Option<String>,
    network: Option<String>,
    created_at: i64,
    total: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ImportableListingRow {
    old_trade_id: String,
    trade_type: String,
    contract_address: Option<String>,
    item_id: Option<String>,
    token_id: Option<String>,
    name: Option<String>,
    image: Option<String>,
    rarity: Option<String>,
    item_type: Option<String>,
    wearable_category: Option<String>,
    mana_wei: Option<String>,
    available: Option<String>,
    network: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyListingRow {
    trade_id: String,
    contract_address: Option<String>,
    item_id: Option<String>,
    name: Option<String>,
    image: Option<String>,
    rarity: Option<String>,
    item_type: Option<String>,
    wearable_category: Option<String>,
    creator: Option<String>,
    mana_wei: Option<String>,
    available: Option<String>,
    network: Option<String>,
    created_at: i64,
    total: i64,
}

// Shop listings are created at whole-credit prices, so amount_received is expected to be an exact
// multiple of USD_WEI_PER_CREDIT. Rounds UP (ceil) as a defensive measure: a non-conforming price
// can then never be advertised for less than it would settle at on-chain. A non-positive or
// unparseable amount yields None so the caller can drop the row instead of advertising a free item.
fn to_credits(usd_wei: &str) -> Option<u64> {
    let wei = usd_wei.parse::<u128>().ok()?;
    if wei == 0 {
        return None;
    }
    u64::try_from((wei + USD_WEI_PER_CREDIT - 1) / USD_WEI_PER_CREDIT).ok()
}

// A whole-credit price bound -> USD wei. Returns None for non-finite input (e.g.
// `?minPriceCredits=Infinity`) so the caller skips the filter instead of applying a bogus bound.
fn credits_to_wei(credits: f64) -> Option<u128> {
    if !credits.is_finite() {
        return None;
    }
    Some(credits.max(0.0).floor() as u128 * USD_WEI_PER_CREDIT)
}

// Escape LIKE/ILIKE metacharacters so user input is matched literally (Postgres default escape
// is `\`). The value is already bound as a parameter (no injection); this only stops `%`/`_`
// from turning a search into an unbounded wildcard scan.
fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn shop_clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(SHOP_DEFAULT_PAGE_SIZE)
        .clamp(SHOP_MIN_PAGE_SIZE, SHOP_MAX_PAGE_SIZE)
}

fn shop_clamp_skip(skip: Option<i64>) -> i64 {
    skip.unwrap_or(0).max(0)
}

fn top_level_category(item_type: Option<&str>) -> &'static str {
    if item_type
        .map(|t| t.to_lowercase().starts_with("emote"))
        .unwrap_or(false)
    {
        "emote"
    } else {
        "wearable"
    }
}

fn network_and_chain(raw: Option<&str>) -> (Network, ChainId) {
    if raw.unwrap_or("MATIC").eq_ignore_ascii_case("ETHEREUM") {
        (Network::Ethereum, ethereum_chain_id())
    } else {
        (Network::Matic, polygon_chain_id())
    }
}

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

fn emit(b: Bind, bs: &mut Vec<Bind>, idx: &mut usize) -> String {
    bs.push(b);
    let s = format!("${}", *idx);
    *idx += 1;
    s
}

// The shared FROM + metadata joins used by the shop, legacy and import feeds. Resolves item
// metadata for primary (item_p -> wearable/emote) and secondary (nft + item_s) listings.
fn metadata_joins() -> String {
    format!(
        "FROM marketplace.mv_trades mv\n\
         LEFT JOIN {schema}.item item_p ON mv.type = 'public_item_order'\n\
            AND item_p.collection_id = mv.sent_contract_address\n\
            AND item_p.blockchain_id = mv.sent_item_id::numeric\n\
         LEFT JOIN {schema}.metadata meta_p ON meta_p.id = item_p.metadata_id\n\
         LEFT JOIN {schema}.wearable w_p ON w_p.id = meta_p.wearable_id\n\
         LEFT JOIN {schema}.emote e_p ON e_p.id = meta_p.emote_id\n\
         LEFT JOIN {schema}.nft nft ON mv.type = 'public_nft_order' AND nft.id = mv.sent_nft_id\n\
         LEFT JOIN {schema}.item item_s ON mv.type = 'public_nft_order' AND item_s.id = nft.item_id",
        schema = MARKETPLACE_SQUID_SCHEMA,
    )
}

const SHOP_NAME_EXPR: &str = "COALESCE(nft.name, w_p.name, e_p.name)";
const LEGACY_NAME_EXPR: &str = "COALESCE(w_p.name, e_p.name)";

// Sort uses fixed expressions only — never interpolate user input into ORDER BY.
fn order_by(sort_by: Option<ShopSortBy>, name_expr: &str) -> String {
    match sort_by {
        Some(ShopSortBy::Cheapest) => "ORDER BY mv.amount_received ASC".to_string(),
        Some(ShopSortBy::MostExpensive) => "ORDER BY mv.amount_received DESC".to_string(),
        Some(ShopSortBy::Name) => format!("ORDER BY {name_expr} ASC"),
        Some(ShopSortBy::Newest) | None => "ORDER BY mv.created_at DESC".to_string(),
    }
}

fn received_asset_exists(asset_type: i64, binds: &mut Vec<Bind>, next_idx: &mut usize) -> String {
    let p = emit(Bind::Int(asset_type), binds, next_idx);
    format!(
        " EXISTS (SELECT 1 FROM marketplace.trade_assets ta \
           WHERE ta.trade_id = mv.id AND ta.direction = 'received' AND ta.asset_type = {p}) "
    )
}

pub fn build_shop_listings_sql(filters: &ShopCatalogFilters) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let mut wheres = vec![
        " mv.status = 'open' ".to_string(),
        " (mv.available IS NULL OR mv.available > 0) ".to_string(),
        received_asset_exists(ASSET_TYPE_USD_PEGGED_MANA, &mut binds, &mut next_idx),
    ];

    if let Some(ca) = &filters.contract_address {
        if !ca.is_empty() {
            let p = emit(Bind::Text(ca.to_lowercase()), &mut binds, &mut next_idx);
            wheres.push(format!(" mv.sent_contract_address = {p} "));
        }
    }
    if let Some(iid) = &filters.item_id {
        let p = emit(Bind::Text(iid.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" mv.sent_item_id = {p} "));
    }
    match filters.category.as_deref() {
        Some("emote") => wheres.push(
            " COALESCE(item_p.item_type, item_s.item_type, nft.item_type) ILIKE 'emote%' "
                .to_string(),
        ),
        Some("wearable") => wheres.push(
            " COALESCE(item_p.item_type, item_s.item_type, nft.item_type) NOT ILIKE 'emote%' "
                .to_string(),
        ),
        _ => {}
    }
    if !filters.rarities.is_empty() {
        let lowered = filters.rarities.iter().map(|r| r.to_lowercase()).collect();
        let p = emit(Bind::TextArray(lowered), &mut binds, &mut next_idx);
        wheres.push(format!(
            " lower(COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity)) = ANY({p}) "
        ));
    }
    if !filters.wearable_categories.is_empty() {
        let lowered = filters
            .wearable_categories
            .iter()
            .map(|c| c.to_lowercase())
            .collect();
        let p = emit(Bind::TextArray(lowered), &mut binds, &mut next_idx);
        wheres.push(format!(
            " lower(COALESCE(item_p.search_wearable_category, item_s.search_wearable_category, \
               item_p.search_emote_category, item_s.search_emote_category)) = ANY({p}) "
        ));
    }
    if let Some(min_wei) = filters.min_price_credits.and_then(credits_to_wei) {
        let p = emit(Bind::Text(min_wei.to_string()), &mut binds, &mut next_idx);
        wheres.push(format!(" mv.amount_received >= {p}::numeric "));
    }
    if let Some(max_wei) = filters.max_price_credits.and_then(credits_to_wei) {
        let p = emit(Bind::Text(max_wei.to_string()), &mut binds, &mut next_idx);
        wheres.push(format!(" mv.amount_received <= {p}::numeric "));
    }
    if let Some(search) = filters.search.as_deref().filter(|s| !s.is_empty()) {
        let p = emit(
            Bind::Text(format!("%{}%", escape_like(search))),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" {SHOP_NAME_EXPR} ILIKE {p} "));
    }

    let limit_p = emit(
        Bind::Int(shop_clamp_first(filters.first)),
        &mut binds,
        &mut next_idx,
    );
    let offset_p = emit(
        Bind::Int(shop_clamp_skip(filters.skip)),
        &mut binds,
        &mut next_idx,
    );

    let sql = format!(
        "SELECT\n\
           mv.id::text AS trade_id,\n\
           mv.type AS trade_type,\n\
           mv.sent_contract_address AS contract_address,\n\
           mv.sent_item_id AS item_id,\n\
           mv.sent_token_id AS token_id,\n\
           {name_expr} AS name,\n\
           COALESCE(nft.image, item_p.image, item_s.image) AS image,\n\
           COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity) AS rarity,\n\
           COALESCE(item_p.item_type, item_s.item_type, nft.item_type) AS item_type,\n\
           COALESCE(\n\
             item_p.search_wearable_category, item_p.search_emote_category,\n\
             item_s.search_wearable_category, item_s.search_emote_category\n\
           ) AS wearable_category,\n\
           COALESCE(item_p.creator, item_s.creator, '') AS creator,\n\
           mv.amount_received::text AS price,\n\
           mv.available::text AS available,\n\
           mv.network AS network,\n\
           EXTRACT(EPOCH FROM mv.created_at)::bigint * 1000 AS created_at,\n\
           COUNT(*) OVER() AS total\n\
         {joins}\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        name_expr = SHOP_NAME_EXPR,
        joins = metadata_joins(),
        where_clause = where_from(&wheres),
        order = order_by(filters.sort_by, SHOP_NAME_EXPR),
    );

    (sql, binds)
}

pub fn build_importable_listings_sql(seller: &str) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let mut wheres = vec![
        " mv.status = 'open' ".to_string(),
        " (mv.available IS NULL OR mv.available > 0) ".to_string(),
    ];
    let p = emit(Bind::Text(seller.to_lowercase()), &mut binds, &mut next_idx);
    wheres.push(format!(" lower(mv.signer) = {p} "));
    wheres.push(received_asset_exists(
        ASSET_TYPE_ERC20,
        &mut binds,
        &mut next_idx,
    ));

    let limit_p = emit(Bind::Int(SHOP_MAX_PAGE_SIZE), &mut binds, &mut next_idx);

    let sql = format!(
        "SELECT\n\
           mv.id::text AS old_trade_id,\n\
           mv.type AS trade_type,\n\
           mv.sent_contract_address AS contract_address,\n\
           mv.sent_item_id AS item_id,\n\
           mv.sent_token_id AS token_id,\n\
           {name_expr} AS name,\n\
           COALESCE(nft.image, item_p.image, item_s.image) AS image,\n\
           COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity) AS rarity,\n\
           COALESCE(item_p.item_type, item_s.item_type, nft.item_type) AS item_type,\n\
           COALESCE(\n\
             item_p.search_wearable_category, item_p.search_emote_category,\n\
             item_s.search_wearable_category, item_s.search_emote_category\n\
           ) AS wearable_category,\n\
           mv.amount_received::text AS mana_wei,\n\
           mv.available::text AS available,\n\
           mv.network AS network\n\
         {joins}\n\
         {where_clause}\n\
         ORDER BY mv.created_at DESC\n\
         LIMIT {limit_p}",
        name_expr = SHOP_NAME_EXPR,
        joins = metadata_joins(),
        where_clause = where_from(&wheres),
    );

    (sql, binds)
}

pub fn build_legacy_listings_sql(filters: &LegacyCatalogFilters) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let mut wheres = vec![
        " mv.status = 'open' ".to_string(),
        " mv.type = 'public_item_order' ".to_string(),
        " (mv.available IS NULL OR mv.available > 0) ".to_string(),
        received_asset_exists(ASSET_TYPE_ERC20, &mut binds, &mut next_idx),
    ];

    match filters.category.as_deref() {
        Some("emote") => wheres.push(" item_p.item_type ILIKE 'emote%' ".to_string()),
        Some("wearable") => wheres.push(" item_p.item_type NOT ILIKE 'emote%' ".to_string()),
        _ => {}
    }
    if !filters.rarities.is_empty() {
        let lowered = filters.rarities.iter().map(|r| r.to_lowercase()).collect();
        let p = emit(Bind::TextArray(lowered), &mut binds, &mut next_idx);
        wheres.push(format!(" lower(item_p.rarity) = ANY({p}) "));
    }
    if !filters.wearable_categories.is_empty() {
        let lowered = filters
            .wearable_categories
            .iter()
            .map(|c| c.to_lowercase())
            .collect();
        let p = emit(Bind::TextArray(lowered), &mut binds, &mut next_idx);
        wheres.push(format!(
            " lower(COALESCE(item_p.search_wearable_category, item_p.search_emote_category)) = ANY({p}) "
        ));
    }
    if let Some(search) = filters.search.as_deref().filter(|s| !s.is_empty()) {
        let p = emit(
            Bind::Text(format!("%{}%", escape_like(search))),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(" {LEGACY_NAME_EXPR} ILIKE {p} "));
    }

    let limit_p = emit(
        Bind::Int(shop_clamp_first(filters.first)),
        &mut binds,
        &mut next_idx,
    );
    let offset_p = emit(
        Bind::Int(shop_clamp_skip(filters.skip)),
        &mut binds,
        &mut next_idx,
    );

    let sql = format!(
        "SELECT\n\
           mv.id::text AS trade_id,\n\
           mv.sent_contract_address AS contract_address,\n\
           mv.sent_item_id AS item_id,\n\
           {name_expr} AS name,\n\
           item_p.image AS image,\n\
           item_p.rarity AS rarity,\n\
           item_p.item_type AS item_type,\n\
           COALESCE(item_p.search_wearable_category, item_p.search_emote_category) AS wearable_category,\n\
           COALESCE(item_p.creator, '') AS creator,\n\
           mv.amount_received::text AS mana_wei,\n\
           mv.available::text AS available,\n\
           mv.network AS network,\n\
           EXTRACT(EPOCH FROM mv.created_at)::bigint * 1000 AS created_at,\n\
           COUNT(*) OVER() AS total\n\
         {joins}\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        name_expr = LEGACY_NAME_EXPR,
        joins = metadata_joins(),
        where_clause = where_from(&wheres),
        order = order_by(filters.sort_by, LEGACY_NAME_EXPR),
    );

    (sql, binds)
}

fn listing_type(trade_type: &str) -> &'static str {
    if trade_type == "public_item_order" {
        "primary"
    } else {
        "secondary"
    }
}

fn parse_available(available: Option<&str>) -> i64 {
    available.and_then(|s| s.parse::<i64>().ok()).unwrap_or(1)
}

pub struct ShopCatalogComponent {
    pool: PgPool,
}

impl ShopCatalogComponent {
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

    pub async fn get_shop_listings(
        &self,
        filters: &ShopCatalogFilters,
    ) -> Result<(Vec<ShopListing>, i64), ApiError> {
        let (sql, binds) = build_shop_listings_sql(filters);
        let rows: Vec<ShopListingRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let mut data = Vec::with_capacity(rows.len());
        for r in rows {
            let Some(price_credits) = r.price.as_deref().and_then(to_credits) else {
                tracing::warn!(
                    trade_id = %r.trade_id,
                    price = r.price.as_deref().unwrap_or(""),
                    "dropping shop listing with non-positive or unparseable price"
                );
                continue;
            };
            let (network, chain_id) = network_and_chain(r.network.as_deref());
            data.push(ShopListing {
                trade_id: r.trade_id,
                listing_type: listing_type(&r.trade_type).to_string(),
                contract_address: r.contract_address.unwrap_or_default(),
                item_id: r.item_id,
                token_id: r.token_id,
                name: r.name.unwrap_or_default(),
                thumbnail: r.image.unwrap_or_default(),
                rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                category: top_level_category(r.item_type.as_deref()).to_string(),
                wearable_category: r.wearable_category,
                creator: r.creator.unwrap_or_default(),
                price_credits,
                available: parse_available(r.available.as_deref()),
                network,
                chain_id,
                created_at: r.created_at,
            });
        }
        Ok((data, total))
    }

    // A seller's OPEN classic (ERC20-MANA) listings — the "old liquidity" they can import into
    // the Shop. Price is returned raw (MANA wei); the client converts to credits via the oracle.
    pub async fn get_importable_listings(
        &self,
        seller: &str,
    ) -> Result<Vec<ImportableListing>, ApiError> {
        let (sql, binds) = build_importable_listings_sql(seller);
        let rows: Vec<ImportableListingRow> = self.fetch(sql, binds).await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let (network, chain_id) = network_and_chain(r.network.as_deref());
                ImportableListing {
                    old_trade_id: r.old_trade_id,
                    listing_type: listing_type(&r.trade_type).to_string(),
                    contract_address: r.contract_address.unwrap_or_default(),
                    item_id: r.item_id,
                    token_id: r.token_id,
                    name: r.name.unwrap_or_default(),
                    thumbnail: r.image.unwrap_or_default(),
                    rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                    category: top_level_category(r.item_type.as_deref()).to_string(),
                    wearable_category: r.wearable_category,
                    mana_wei: r.mana_wei.unwrap_or_default(),
                    available: parse_available(r.available.as_deref()),
                    network,
                    chain_id,
                }
            })
            .collect())
    }

    pub async fn get_legacy_listings(
        &self,
        filters: &LegacyCatalogFilters,
    ) -> Result<(Vec<LegacyListing>, i64), ApiError> {
        let (sql, binds) = build_legacy_listings_sql(filters);
        let rows: Vec<LegacyListingRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|r| {
                let (network, chain_id) = network_and_chain(r.network.as_deref());
                LegacyListing {
                    trade_id: r.trade_id,
                    listing_type: "primary".to_string(),
                    contract_address: r.contract_address.unwrap_or_default(),
                    item_id: r.item_id,
                    name: r.name.unwrap_or_default(),
                    thumbnail: r.image.unwrap_or_default(),
                    rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                    category: top_level_category(r.item_type.as_deref()).to_string(),
                    wearable_category: r.wearable_category,
                    creator: r.creator.unwrap_or_default(),
                    mana_wei: r.mana_wei.unwrap_or_default(),
                    available: parse_available(r.available.as_deref()),
                    network,
                    chain_id,
                    created_at: r.created_at,
                }
            })
            .collect();
        Ok((data, total))
    }
}

fn csv(value: Option<String>) -> Vec<String> {
    value
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn finite_i64(v: Option<f64>) -> Option<i64> {
    v.filter(|n| n.is_finite()).map(|n| n as i64)
}

pub fn parse_shop_filters(pairs: &[(String, String)]) -> ShopCatalogFilters {
    let p = Params::new(pairs);
    ShopCatalogFilters {
        first: finite_i64(p.get_number("first", None)),
        skip: finite_i64(p.get_number("skip", None)),
        category: p.get_string("category", None),
        contract_address: p.get_string("contractAddress", None),
        item_id: p.get_string("itemId", None),
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        min_price_credits: p.get_number("minPriceCredits", None),
        max_price_credits: p.get_number("maxPriceCredits", None),
        search: p.get_string("search", None),
        sort_by: p
            .get_value("sortBy", SHOP_SORT_VALUES, None)
            .as_deref()
            .and_then(ShopSortBy::parse),
    }
}

pub fn parse_legacy_filters(pairs: &[(String, String)]) -> LegacyCatalogFilters {
    let p = Params::new(pairs);
    LegacyCatalogFilters {
        first: finite_i64(p.get_number("first", None)),
        skip: finite_i64(p.get_number("skip", None)),
        category: p.get_string("category", None),
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        search: p.get_string("search", None),
        sort_by: p
            .get_value("sortBy", SHOP_SORT_VALUES, None)
            .as_deref()
            .and_then(ShopSortBy::parse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bind_texts(binds: &[Bind]) -> Vec<String> {
        binds
            .iter()
            .filter_map(|b| match b {
                Bind::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    fn bind_ints(binds: &[Bind]) -> Vec<i64> {
        binds
            .iter()
            .filter_map(|b| match b {
                Bind::Int(i) => Some(*i),
                _ => None,
            })
            .collect()
    }

    fn bind_arrays(binds: &[Bind]) -> Vec<Vec<String>> {
        binds
            .iter()
            .filter_map(|b| match b {
                Bind::TextArray(v) => Some(v.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shop_sql_targets_open_credit_buyable_listings() {
        let (sql, binds) = build_shop_listings_sql(&ShopCatalogFilters::default());
        assert!(sql.contains("mv.status = 'open'"), "{sql}");
        assert!(
            sql.contains("mv.available IS NULL OR mv.available > 0"),
            "{sql}"
        );
        assert!(
            sql.contains("ta.direction = 'received' AND ta.asset_type = $1"),
            "{sql}"
        );
        assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
        assert!(sql.contains("marketplace.mv_trades mv"), "{sql}");
        assert!(
            sql.contains("item_p.blockchain_id = mv.sent_item_id::numeric"),
            "{sql}"
        );
        assert!(
            sql.contains("nft ON mv.type = 'public_nft_order' AND nft.id = mv.sent_nft_id"),
            "{sql}"
        );
        assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_USD_PEGGED_MANA, 48, 0]);
    }

    #[test]
    fn shop_price_bounds_bind_whole_credit_wei() {
        let filters = ShopCatalogFilters {
            min_price_credits: Some(3.0),
            max_price_credits: Some(10.0),
            ..Default::default()
        };
        let (sql, binds) = build_shop_listings_sql(&filters);
        assert!(sql.contains("mv.amount_received >= $"), "{sql}");
        assert!(sql.contains("mv.amount_received <= $"), "{sql}");
        let texts = bind_texts(&binds);
        assert!(texts.contains(&(3 * USD_WEI_PER_CREDIT).to_string()));
        assert!(texts.contains(&(10 * USD_WEI_PER_CREDIT).to_string()));
    }

    #[test]
    fn shop_non_finite_price_bounds_are_skipped() {
        let filters = ShopCatalogFilters {
            min_price_credits: Some(f64::INFINITY),
            max_price_credits: Some(f64::NAN),
            ..Default::default()
        };
        let (sql, _) = build_shop_listings_sql(&filters);
        assert!(!sql.contains("mv.amount_received >="), "{sql}");
        assert!(!sql.contains("mv.amount_received <="), "{sql}");
    }

    #[test]
    fn shop_search_escapes_ilike_wildcards() {
        let filters = ShopCatalogFilters {
            search: Some("50%_off".to_string()),
            ..Default::default()
        };
        let (sql, binds) = build_shop_listings_sql(&filters);
        assert!(
            sql.contains("COALESCE(nft.name, w_p.name, e_p.name) ILIKE $"),
            "{sql}"
        );
        assert!(bind_texts(&binds).contains(&"%50\\%\\_off%".to_string()));
    }

    #[test]
    fn shop_sort_uses_fixed_expressions_only() {
        for (sort, expected) in [
            (
                Some(ShopSortBy::Cheapest),
                "ORDER BY mv.amount_received ASC",
            ),
            (
                Some(ShopSortBy::MostExpensive),
                "ORDER BY mv.amount_received DESC",
            ),
            (
                Some(ShopSortBy::Name),
                "ORDER BY COALESCE(nft.name, w_p.name, e_p.name) ASC",
            ),
            (Some(ShopSortBy::Newest), "ORDER BY mv.created_at DESC"),
            (None, "ORDER BY mv.created_at DESC"),
        ] {
            let filters = ShopCatalogFilters {
                sort_by: sort,
                ..Default::default()
            };
            let (sql, _) = build_shop_listings_sql(&filters);
            assert!(sql.contains(expected), "{sort:?}: {sql}");
        }
    }

    #[test]
    fn shop_pagination_is_clamped() {
        let filters = ShopCatalogFilters {
            first: Some(99_999),
            skip: Some(-5),
            ..Default::default()
        };
        let (sql, binds) = build_shop_listings_sql(&filters);
        assert!(sql.contains("LIMIT $"), "{sql}");
        assert!(sql.contains("OFFSET $"), "{sql}");
        let ints = bind_ints(&binds);
        assert!(ints.contains(&SHOP_MAX_PAGE_SIZE));
        assert!(ints.contains(&0));

        let (_, binds) = build_shop_listings_sql(&ShopCatalogFilters {
            first: Some(0),
            ..Default::default()
        });
        assert!(bind_ints(&binds).contains(&SHOP_MIN_PAGE_SIZE));
    }

    #[test]
    fn shop_rarities_and_categories_are_lowercased_array_binds() {
        let filters = ShopCatalogFilters {
            rarities: vec!["Rare".to_string(), "EPIC".to_string()],
            wearable_categories: vec!["Upper_Body".to_string(), "HAT".to_string()],
            ..Default::default()
        };
        let (sql, binds) = build_shop_listings_sql(&filters);
        assert!(
            sql.contains(
                "lower(COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity)) = ANY($"
            ),
            "{sql}"
        );
        assert!(
            sql.contains(
                "lower(COALESCE(item_p.search_wearable_category, item_s.search_wearable_category"
            ),
            "{sql}"
        );
        let arrays = bind_arrays(&binds);
        assert!(arrays.contains(&vec!["rare".to_string(), "epic".to_string()]));
        assert!(arrays.contains(&vec!["upper_body".to_string(), "hat".to_string()]));
    }

    #[test]
    fn shop_contract_address_is_lowercased() {
        let filters = ShopCatalogFilters {
            contract_address: Some("0xABCdef".to_string()),
            item_id: Some("3".to_string()),
            ..Default::default()
        };
        let (sql, binds) = build_shop_listings_sql(&filters);
        assert!(sql.contains("mv.sent_contract_address = $"), "{sql}");
        assert!(sql.contains("mv.sent_item_id = $"), "{sql}");
        let texts = bind_texts(&binds);
        assert!(texts.contains(&"0xabcdef".to_string()));
        assert!(texts.contains(&"3".to_string()));
    }

    #[test]
    fn importable_sql_is_seller_scoped_classic_mana_and_capped() {
        let (sql, binds) = build_importable_listings_sql("0xABCdef");
        assert!(sql.contains("lower(mv.signer) = $1"), "{sql}");
        assert!(
            sql.contains("ta.direction = 'received' AND ta.asset_type = $2"),
            "{sql}"
        );
        assert!(sql.contains("ORDER BY mv.created_at DESC"), "{sql}");
        assert!(sql.contains("LIMIT $3"), "{sql}");
        assert!(
            sql.contains("mv.amount_received::text AS mana_wei"),
            "{sql}"
        );
        assert!(bind_texts(&binds).contains(&"0xabcdef".to_string()));
        assert_eq!(
            bind_ints(&binds),
            vec![ASSET_TYPE_ERC20, SHOP_MAX_PAGE_SIZE]
        );
    }

    #[test]
    fn legacy_sql_is_primary_only_classic_mana() {
        let (sql, binds) = build_legacy_listings_sql(&LegacyCatalogFilters::default());
        assert!(sql.contains("mv.status = 'open'"), "{sql}");
        assert!(sql.contains("mv.type = 'public_item_order'"), "{sql}");
        assert!(
            sql.contains("mv.available IS NULL OR mv.available > 0"),
            "{sql}"
        );
        assert!(
            sql.contains("ta.direction = 'received' AND ta.asset_type = $1"),
            "{sql}"
        );
        assert!(
            sql.contains("mv.amount_received::text AS mana_wei"),
            "{sql}"
        );
        assert!(!sql.contains("mv.amount_received >="), "{sql}");
        assert!(!sql.contains("mv.amount_received <="), "{sql}");
        assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
        assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_ERC20, 48, 0]);
    }

    #[test]
    fn legacy_filters_use_primary_columns_only() {
        let filters = LegacyCatalogFilters {
            rarities: vec!["Rare".to_string()],
            wearable_categories: vec!["HAT".to_string()],
            search: Some("50%_off".to_string()),
            sort_by: Some(ShopSortBy::Name),
            ..Default::default()
        };
        let (sql, binds) = build_legacy_listings_sql(&filters);
        assert!(sql.contains("lower(item_p.rarity) = ANY($"), "{sql}");
        assert!(
            sql.contains(
                "lower(COALESCE(item_p.search_wearable_category, item_p.search_emote_category)) = ANY($"
            ),
            "{sql}"
        );
        assert!(
            sql.contains("COALESCE(w_p.name, e_p.name) ILIKE $"),
            "{sql}"
        );
        assert!(
            sql.contains("ORDER BY COALESCE(w_p.name, e_p.name) ASC"),
            "{sql}"
        );
        assert!(bind_texts(&binds).contains(&"%50\\%\\_off%".to_string()));
        let arrays = bind_arrays(&binds);
        assert!(arrays.contains(&vec!["rare".to_string()]));
        assert!(arrays.contains(&vec!["hat".to_string()]));
    }

    #[test]
    fn to_credits_ceils_and_drops_bad_amounts() {
        assert_eq!(to_credits(&USD_WEI_PER_CREDIT.to_string()), Some(1));
        assert_eq!(to_credits(&(5 * USD_WEI_PER_CREDIT).to_string()), Some(5));
        assert_eq!(
            to_credits(&(USD_WEI_PER_CREDIT + 1).to_string()),
            Some(2),
            "non-conforming price rounds up, never advertised below settlement"
        );
        assert_eq!(to_credits("1"), Some(1));
        assert_eq!(to_credits("0"), None);
        assert_eq!(to_credits("-5"), None);
        assert_eq!(to_credits("not-a-number"), None);
        assert_eq!(to_credits(""), None);
    }

    #[test]
    fn credits_to_wei_floors_and_clamps() {
        assert_eq!(credits_to_wei(3.7), Some(3 * USD_WEI_PER_CREDIT));
        assert_eq!(credits_to_wei(-5.0), Some(0));
        assert_eq!(credits_to_wei(f64::INFINITY), None);
        assert_eq!(credits_to_wei(f64::NAN), None);
    }

    #[test]
    fn escape_like_neutralizes_metacharacters() {
        assert_eq!(escape_like("50%_off"), "50\\%\\_off");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn top_level_category_splits_on_emote_prefix() {
        assert_eq!(top_level_category(Some("emote_v1")), "emote");
        assert_eq!(top_level_category(Some("EMOTE_V1")), "emote");
        assert_eq!(top_level_category(Some("wearable_v2")), "wearable");
        assert_eq!(top_level_category(None), "wearable");
    }

    #[test]
    fn network_defaults_to_matic() {
        assert_eq!(network_and_chain(None).0, Network::Matic);
        assert_eq!(network_and_chain(Some("POLYGON")).0, Network::Matic);
        assert_eq!(network_and_chain(Some("ETHEREUM")).0, Network::Ethereum);
        assert_eq!(network_and_chain(Some("ethereum")).0, Network::Ethereum);
    }

    #[test]
    fn parse_shop_filters_validates_sort_and_splits_csv() {
        let pairs = vec![
            ("first".to_string(), "10".to_string()),
            ("skip".to_string(), "Infinity".to_string()),
            ("rarity".to_string(), "rare, epic,".to_string()),
            ("wearableCategory".to_string(), "hat".to_string()),
            ("sortBy".to_string(), "cheapest".to_string()),
        ];
        let f = parse_shop_filters(&pairs);
        assert_eq!(f.first, Some(10));
        assert_eq!(f.skip, None);
        assert_eq!(f.rarities, vec!["rare".to_string(), "epic".to_string()]);
        assert_eq!(f.wearable_categories, vec!["hat".to_string()]);
        assert_eq!(f.sort_by, Some(ShopSortBy::Cheapest));

        let bad = vec![("sortBy".to_string(), "1; DROP TABLE".to_string())];
        assert_eq!(parse_shop_filters(&bad).sort_by, None);
    }
}
