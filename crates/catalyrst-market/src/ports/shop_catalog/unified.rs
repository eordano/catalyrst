use serde::Serialize;

use super::component::{
    listing_type, network_and_chain, parse_available, top_level_category, ShopCatalogComponent,
};
use super::sql::{
    credits_to_wei, emit, escape_like, gender_expr, metadata_joins, received_asset_exists,
    shop_clamp_first, shop_clamp_skip, Bind, ASSET_TYPE_ERC20, ASSET_TYPE_USD_PEGGED_MANA,
    SHOP_NAME_EXPR, USD_WEI_PER_CREDIT,
};
use super::types::{parse_shop_filters, ShopCatalogFilters, ShopSortBy};
use crate::dcl_schemas::{ChainId, Network};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::sql_filters::where_from;
use crate::ports::mana_rate::rate_to_numeric_string;

// Which liquidity pool a unified item comes from: Native = credit-buyable (USD-pegged) Shop
// listing, Legacy = classic MANA-priced primary converted to credits server-side via the live
// MANA/USD rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedSource {
    Native,
    Legacy,
}

pub const UNIFIED_SOURCE_VALUES: &[&str] = &["native", "legacy"];

impl UnifiedSource {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "native" => Some(Self::Native),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Legacy => "legacy",
        }
    }
}

// Filters for the unified feed: the full shop filter set (price-range works across BOTH sources
// now that the server has a MANA/USD rate) plus an optional source filter to restrict to one pool.
#[derive(Debug, Clone, Default)]
pub struct UnifiedCatalogFilters {
    pub base: ShopCatalogFilters,
    pub source: Option<UnifiedSource>,
}

// A unified feed item: the same shape as a ShopListing (so the frontend consumes both uniformly)
// plus the source discriminator and, for legacy items only, the raw MANA price so the client can
// size the purchase at the LIVE rate at checkout.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct UnifiedListing {
    #[cfg_attr(feature = "ts", ts(type = "\"native\" | \"legacy\""))]
    pub source: String,
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
    #[cfg_attr(feature = "ts", ts(type = "\"male\" | \"female\" | \"unisex\" | null"))]
    pub gender: Option<String>,
    pub creator: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub price_credits: i64,
    pub mana_wei: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct UnifiedListingRow {
    source: String,
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
    gender: Option<String>,
    creator: Option<String>,
    price_credits: i64,
    mana_wei: Option<String>,
    available: Option<String>,
    network: Option<String>,
    created_at: i64,
    total: i64,
}

pub fn parse_unified_filters(pairs: &[(String, String)]) -> UnifiedCatalogFilters {
    let p = Params::new(pairs);
    UnifiedCatalogFilters {
        base: parse_shop_filters(pairs),
        source: p
            .get_value("source", UNIFIED_SOURCE_VALUES, None)
            .as_deref()
            .and_then(UnifiedSource::parse),
    }
}

// The shared browse filters applied identically to each branch of the unified feed. Mirrors the
// expressions used by build_shop_listings_sql.
fn append_unified_filters(
    wheres: &mut Vec<String>,
    filters: &ShopCatalogFilters,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) {
    if let Some(ca) = &filters.contract_address {
        if !ca.is_empty() {
            let p = emit(Bind::Text(ca.to_lowercase()), binds, next_idx);
            wheres.push(format!(" mv.sent_contract_address = {p} "));
        }
    }
    if let Some(iid) = &filters.item_id {
        let p = emit(Bind::Text(iid.clone()), binds, next_idx);
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
        let p = emit(Bind::TextArray(lowered), binds, next_idx);
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
        let p = emit(Bind::TextArray(lowered), binds, next_idx);
        wheres.push(format!(
            " lower(COALESCE(item_p.search_wearable_category, item_s.search_wearable_category, \
               item_p.search_emote_category, item_s.search_emote_category)) = ANY({p}) "
        ));
    }
    if let Some(search) = filters.search.as_deref().filter(|s| !s.is_empty()) {
        let p = emit(
            Bind::Text(format!("%{}%", escape_like(search))),
            binds,
            next_idx,
        );
        wheres.push(format!(" {SHOP_NAME_EXPR} ILIKE {p} "));
    }
}

// One branch of the unified UNION. usd_wei is the USD-wei-equivalent expression: native listings
// are already USD-pegged (amount_received IS USD wei); legacy listings are MANA wei, so
// usd_wei = amount * rate. Columns are identical across branches so the two can be UNIONed and
// sorted/paginated as one.
fn unified_branch(
    source: UnifiedSource,
    rate_placeholder: Option<&str>,
    filters: &ShopCatalogFilters,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) -> String {
    let (asset_type, primary_only) = match source {
        UnifiedSource::Native => (ASSET_TYPE_USD_PEGGED_MANA, false),
        UnifiedSource::Legacy => (ASSET_TYPE_ERC20, true),
    };
    let usd_wei = match rate_placeholder {
        Some(rate_p) => format!("(mv.amount_received::numeric * {rate_p}::numeric)"),
        None => "mv.amount_received::numeric".to_string(),
    };
    // Raw MANA price, exposed only for legacy (MANA-priced) items so the client can size the
    // purchase at the LIVE rate at checkout; native (USD-pegged) items carry no MANA price.
    let mana_wei = match source {
        UnifiedSource::Native => "NULL::text",
        UnifiedSource::Legacy => "mv.amount_received::text",
    };

    let mut wheres = vec![
        " mv.status = 'open' ".to_string(),
        " (mv.available IS NULL OR mv.available > 0) ".to_string(),
    ];
    if primary_only {
        wheres.push(" mv.type = 'public_item_order' ".to_string());
    }
    wheres.push(received_asset_exists(asset_type, binds, next_idx));
    append_unified_filters(&mut wheres, filters, binds, next_idx);

    format!(
        "SELECT\n\
           '{source}' AS source,\n\
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
           {usd_wei} AS usd_wei,\n\
           mv.available::text AS available,\n\
           mv.network AS network,\n\
           EXTRACT(EPOCH FROM mv.created_at)::bigint * 1000 AS created_at,\n\
           {mana_wei} AS mana_wei,\n\
           {gender}\n\
         {joins}\n\
         {where_clause}",
        source = source.as_str(),
        name_expr = SHOP_NAME_EXPR,
        gender = gender_expr(),
        joins = metadata_joins(),
        where_clause = where_from(&wheres),
    )
}

// minPriceCredits is a floor on the DISPLAYED price, which is CEIL(usd_wei / USD_WEI_PER_CREDIT).
// CEIL(x / C) >= m  <=>  x > (m - 1) * C, so the correct bound on usd_wei is
// (minWei - USD_WEI_PER_CREDIT). A plain `usd_wei >= minWei` (minWei = m * C) would wrongly drop
// fractional-priced legacy items whose CEIL equals m but whose usd_wei sits just below m * C.
// None when the bound would go negative (m <= 0), where every priced item (usd_wei > 0) already
// qualifies, or when the input is non-finite.
pub(super) fn unified_min_price_bound_wei(min_credits: f64) -> Option<u128> {
    let min_wei = credits_to_wei(min_credits)?;
    if min_wei == 0 {
        return None;
    }
    Some(min_wei - USD_WEI_PER_CREDIT)
}

// The UNIFIED feed: native (USD-pegged) + legacy (classic MANA) primaries in ONE credit-priced
// set. Legacy MANA prices are converted to a USD-wei-equivalent (amount * rate) so price_credits,
// the price-range filter and the sort are all computed uniformly across both sources.
// price_credits is CEIL(usd_wei / USD_WEI_PER_CREDIT) — whole credits rounded UP, same "Model B"
// as the native path.
pub fn build_unified_listings_sql(
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let rate_p = if filters.source != Some(UnifiedSource::Native) {
        Some(emit(
            Bind::Text(rate_to_numeric_string(mana_usd_rate)),
            &mut binds,
            &mut next_idx,
        ))
    } else {
        None
    };

    let mut parts: Vec<String> = Vec::new();
    if filters.source != Some(UnifiedSource::Legacy) {
        parts.push(unified_branch(
            UnifiedSource::Native,
            None,
            &filters.base,
            &mut binds,
            &mut next_idx,
        ));
    }
    if filters.source != Some(UnifiedSource::Native) {
        parts.push(unified_branch(
            UnifiedSource::Legacy,
            rate_p.as_deref(),
            &filters.base,
            &mut binds,
            &mut next_idx,
        ));
    }
    let inner = parts.join("\n UNION ALL \n");

    // usd_wei > 0 also guards a broken rate: rate_to_numeric_string maps a non-positive or
    // non-finite rate to '0', which zeroes every legacy usd_wei and drops those rows rather than
    // advertising a free item.
    let mut outer_wheres = vec![" sub.usd_wei > 0 ".to_string()];
    if let Some(bound) = filters
        .base
        .min_price_credits
        .and_then(unified_min_price_bound_wei)
    {
        let p = emit(Bind::Text(bound.to_string()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" sub.usd_wei > {p}::numeric "));
    }
    if let Some(max_wei) = filters.base.max_price_credits.and_then(credits_to_wei) {
        let p = emit(Bind::Text(max_wei.to_string()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" sub.usd_wei <= {p}::numeric "));
    }

    // Sort uses fixed expressions only. A sub.trade_id tiebreaker makes the order total so
    // pagination is stable when many rows share a usd_wei/name.
    let order = match filters.base.sort_by {
        Some(ShopSortBy::Cheapest) => "ORDER BY sub.usd_wei ASC, sub.trade_id",
        Some(ShopSortBy::MostExpensive) => "ORDER BY sub.usd_wei DESC, sub.trade_id",
        Some(ShopSortBy::Name) => "ORDER BY sub.name ASC, sub.trade_id",
        Some(ShopSortBy::Newest) | None => "ORDER BY sub.created_at DESC, sub.trade_id",
    };

    let limit_p = emit(
        Bind::Int(shop_clamp_first(filters.base.first)),
        &mut binds,
        &mut next_idx,
    );
    let offset_p = emit(
        Bind::Int(shop_clamp_skip(filters.base.skip)),
        &mut binds,
        &mut next_idx,
    );

    let sql = format!(
        "SELECT\n\
           sub.*,\n\
           CEIL(sub.usd_wei / {credit_wei}::numeric)::bigint AS price_credits,\n\
           COUNT(*) OVER() AS total\n\
         FROM (\n{inner}\n) sub\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        credit_wei = USD_WEI_PER_CREDIT,
        where_clause = where_from(&outer_wheres),
    );

    (sql, binds)
}

impl ShopCatalogComponent {
    pub async fn get_unified_listings(
        &self,
        filters: &UnifiedCatalogFilters,
        mana_usd_rate: f64,
    ) -> Result<(Vec<UnifiedListing>, i64), ApiError> {
        let (sql, binds) = build_unified_listings_sql(filters, mana_usd_rate);
        let rows: Vec<UnifiedListingRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|r| {
                let (network, chain_id) = network_and_chain(r.network.as_deref());
                UnifiedListing {
                    source: r.source,
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
                    gender: r.gender,
                    creator: r.creator.unwrap_or_default(),
                    price_credits: r.price_credits,
                    mana_wei: r.mana_wei,
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
