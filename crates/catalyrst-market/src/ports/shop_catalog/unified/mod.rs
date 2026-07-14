use serde::Serialize;

use super::component::{
    listing_type, network_and_chain, parse_available, top_level_category, ShopCatalogComponent,
};
use super::sql::{
    credits_to_wei, emit, escape_like, gender_expr, metadata_joins, received_asset_exists,
    shop_clamp_first, shop_clamp_skip, store_base_relation, Bind, ASSET_TYPE_ERC20,
    ASSET_TYPE_USD_PEGGED_MANA, MAX_USD_WEI, SHOP_NAME_EXPR, USD_WEI_PER_CREDIT,
};
use super::types::{
    trending_clamp_days, trending_clamp_first, ShopCatalogFilters, ShopSortBy, SHOP_MIN_PAGE_SIZE,
    TRENDING_SALES_CUT,
};
use crate::dcl_schemas::{ChainId, Network};
use crate::http::response::ApiError;
use crate::logic::sql_filters::where_from;
use crate::ports::mana_rate::rate_to_numeric_string;
use crate::ports::trendings::midnight_days_ago;

mod params;

pub use params::{
    body_shapes_for_genders, parse_trending_filters, parse_unified_filters, parse_unified_group_by,
    ShopListingType, TrendingRequest, UnifiedAcquisition, UnifiedCatalogFilters, UnifiedGroupBy,
    UnifiedSource, SHOP_LISTING_TYPE_VALUES, UNIFIED_GROUP_BY_VALUES,
};

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
    #[cfg_attr(feature = "ts", ts(type = "\"trade\" | \"store\""))]
    pub acquisition: String,
    /// `None` for a CollectionStore mint, which has no trade -- there is no
    /// order and nothing signed. Deliberately nullable rather than a synthetic
    /// id: this value is threaded into credit authorization and persisted on
    /// the purchase intent, so a fabricated id would put a reference to a
    /// nonexistent trade into the money ledger.
    pub trade_id: Option<String>,
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
    /// Reseller (current owner of the sent NFT); null for primary listings.
    pub seller: Option<String>,
    /// NFT mint index (issued id); null for primary listings.
    pub issued_id: Option<String>,
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

/// One row per item: the representative listing (primary if present, else cheapest credit-buyable secondary) plus that item's open-listing count.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct UnifiedItem {
    #[cfg_attr(feature = "ts", ts(type = "\"native\" | \"legacy\""))]
    pub source: String,
    #[cfg_attr(feature = "ts", ts(type = "\"trade\" | \"store\""))]
    pub acquisition: String,
    /// `None` when the representative row is a CollectionStore mint (no trade).
    pub trade_id: Option<String>,
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
    /// Representative listing's seller; null when the headline listing is a primary.
    pub seller: Option<String>,
    /// Representative listing's NFT mint index (issued id); null for a primary headline.
    pub issued_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub price_credits: i64,
    pub mana_wei: Option<String>,
    /// How many open credit-buyable listings this item has (primary + secondary, native + legacy).
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub listing_count: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: i64,
    pub network: Network,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

/// A trending rail entry: the item-unified shape the browse grid and the related
/// rail already render, plus the ranking signal that put it there. `trendingSales`
/// is the windowed sale COUNT (mints and resales alike), exposed so the ordering is
/// verifiable from outside -- the signal is broader than the row (see
/// `build_trending_items_sql`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct TrendingItem {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub item: UnifiedItem,
    #[serde(rename = "trendingSales")]
    #[cfg_attr(feature = "ts", ts(rename = "trendingSales", type = "number"))]
    pub trending_sales: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct UnifiedListingRow {
    source: String,
    acquisition: String,
    // Never NULL in SQL: the store branch projects the item id here purely as
    // the ORDER BY / DISTINCT ON tiebreaker. The row->model mapping is what
    // drops it, so nothing downstream can mistake it for a trade.
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
    seller: Option<String>,
    issued_id: Option<String>,
    price_credits: i64,
    mana_wei: Option<String>,
    available: Option<String>,
    network: Option<String>,
    created_at: i64,
    total: i64,
}

/// The item-grouped row minus the paginated feed's `total`. The related-items
/// rail is unpaginated, so it selects exactly these columns and maps them
/// through the same `map_unified_item`; the browse grid layers `total` on top
/// via `UnifiedItemRow`.
#[derive(Debug, sqlx::FromRow)]
struct RelatedItemRow {
    source: String,
    acquisition: String,
    // See UnifiedListingRow::trade_id -- tiebreaker only for store rows.
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
    seller: Option<String>,
    issued_id: Option<String>,
    price_credits: i64,
    mana_wei: Option<String>,
    listing_count: i64,
    available: Option<String>,
    network: Option<String>,
    created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct UnifiedItemRow {
    #[sqlx(flatten)]
    core: RelatedItemRow,
    total: i64,
}

/// The item-unified row plus the windowed sale COUNT that ranked it. The volume
/// SUM the ranking also orders by feeds only the SQL ORDER BY and is not read back
/// into a field here.
#[derive(Debug, sqlx::FromRow)]
struct TrendingItemRow {
    #[sqlx(flatten)]
    core: RelatedItemRow,
    sales: i64,
}

fn append_unified_filters(
    wheres: &mut Vec<String>,
    filters: &ShopCatalogFilters,
    listing_type: Option<ShopListingType>,
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
    if let Some(creator) = filters.creator.as_deref().filter(|c| !c.is_empty()) {
        let p = emit(Bind::Text(creator.to_lowercase()), binds, next_idx);
        wheres.push(format!(
            " lower(COALESCE(item_p.creator, item_s.creator, '')) = {p} "
        ));
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
    if filters.is_smart {
        wheres.push(
            " COALESCE(item_p.item_type, item_s.item_type, nft.item_type) = 'smart_wearable_v1' "
                .to_string(),
        );
    }
    // Same column, COALESCE and ::text[] cast as `gender_expr`, so what this
    // selects and what the row reports as `gender` cannot disagree. `@>` is
    // "declares all of these", which makes `wearableGender=male` mean "wearable
    // BY a male avatar" -- male-exclusive plus unisex -- rather than
    // male-exclusive only. An emote declares no wearable body shapes, so it
    // matches nothing here, the same wearables-only scope the param has on
    // /v1/items.
    if let Some(body_shapes) = body_shapes_for_genders(&filters.wearable_genders) {
        let p = emit(Bind::TextArray(body_shapes), binds, next_idx);
        wheres.push(format!(
            " COALESCE(item_p.search_wearable_body_shapes, item_s.search_wearable_body_shapes)::text[] @> {p}::text[] "
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
    // Social emotes are INCLUDED by default and excluded only on an explicit
    // `includeSocialEmotes=false`, matching /v1/items, /v2/catalog and /v1/trendings.
    // COALESCE over both item joins so it lands on primary (item_p) and secondary
    // (item_s) rows alike; the store branch already excludes them at its base
    // relation (see `store_base_relation`), so this is a no-op there.
    if !filters.include_social_emotes {
        wheres.push(
            " COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL "
                .to_string(),
        );
    }
    // Same expression the row mapper reads back as `listingType`, so the filter and
    // the reported value can never disagree.
    match listing_type {
        Some(ShopListingType::Primary) => {
            wheres.push(" mv.type = 'public_item_order' ".to_string())
        }
        Some(ShopListingType::Secondary) => {
            wheres.push(" mv.type <> 'public_item_order' ".to_string())
        }
        None => {}
    }
}

fn unified_branch(
    source: UnifiedSource,
    acquisition: UnifiedAcquisition,
    rate_placeholder: Option<&str>,
    filters: &ShopCatalogFilters,
    listing_type: Option<ShopListingType>,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) -> String {
    let is_store = acquisition == UnifiedAcquisition::Store;
    let (asset_type, primary_only) = match source {
        UnifiedSource::Native => (ASSET_TYPE_USD_PEGGED_MANA, false),
        UnifiedSource::Legacy => (ASSET_TYPE_ERC20, true),
    };
    let usd_wei = match rate_placeholder {
        Some(rate_p) => format!("(mv.amount_received::numeric * {rate_p}::numeric)"),
        None => "mv.amount_received::numeric".to_string(),
    };
    let mana_wei = match source {
        UnifiedSource::Native => "NULL::text",
        UnifiedSource::Legacy => "mv.amount_received::text",
    };

    let mut wheres = if is_store {
        // The store relation has already filtered itself (minter / approved /
        // available / price) and has no `status` column, no per-trade asset
        // rows and nothing to restrict to primary -- it is primary by
        // construction. So none of the trade-shaped predicates apply. The
        // leading TRUE keeps the WHERE present even with no browse filters, so
        // `append_unified_filters` always has a clause list to extend.
        vec![" TRUE ".to_string()]
    } else {
        let mut w = vec![
            " mv.status = 'open' ".to_string(),
            " (mv.available IS NULL OR mv.available > 0) ".to_string(),
        ];
        if primary_only {
            w.push(" mv.type = 'public_item_order' ".to_string());
        }
        w.push(received_asset_exists(asset_type, binds, next_idx));
        w
    };
    append_unified_filters(&mut wheres, filters, listing_type, binds, next_idx);

    // ::text casts on the id/enum columns because the branches are UNIONed and
    // their underlying types differ: mv_trades.id is uuid while the store
    // relation carries item.id (varchar). Postgres refuses to merge uuid with
    // varchar across a UNION; text is invisible downstream and keeps the ORDER
    // BY tiebreaker deterministic.
    format!(
        "SELECT\n\
           '{source}' AS source,\n\
           '{acquisition}' AS acquisition,\n\
           mv.id::text AS trade_id,\n\
           mv.type::text AS trade_type,\n\
           mv.sent_contract_address::text AS contract_address,\n\
           mv.sent_item_id::text AS item_id,\n\
           mv.sent_token_id::text AS token_id,\n\
           {name_expr} AS name,\n\
           COALESCE(nft.image, item_p.image, item_s.image) AS image,\n\
           COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity) AS rarity,\n\
           COALESCE(item_p.item_type, item_s.item_type, nft.item_type) AS item_type,\n\
           COALESCE(\n\
             item_p.search_wearable_category, item_p.search_emote_category,\n\
             item_s.search_wearable_category, item_s.search_emote_category\n\
           ) AS wearable_category,\n\
           COALESCE(item_p.creator, item_s.creator, '') AS creator,\n\
           mv.assets->'sent'->>'owner' AS seller,\n\
           mv.assets->'sent'->>'issued_id' AS issued_id,\n\
           {usd_wei} AS usd_wei,\n\
           mv.available::text AS available,\n\
           mv.network::text AS network,\n\
           EXTRACT(EPOCH FROM mv.created_at)::bigint * 1000 AS created_at,\n\
           {mana_wei} AS mana_wei,\n\
           {gender}\n\
         {joins}\n\
         {where_clause}",
        source = source.as_str(),
        acquisition = acquisition.as_str(),
        name_expr = SHOP_NAME_EXPR,
        gender = gender_expr(),
        joins = if is_store {
            store_base_relation()
        } else {
            metadata_joins()
        },
        where_clause = where_from(&wheres),
    )
}

pub(super) fn unified_min_price_bound_wei(min_credits: f64) -> Option<u128> {
    let min_wei = credits_to_wei(min_credits)?;
    if min_wei == 0 {
        return None;
    }
    Some(min_wei - USD_WEI_PER_CREDIT)
}

fn unified_inner(
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) -> String {
    let rate_p = if filters.source != Some(UnifiedSource::Native) {
        Some(emit(
            Bind::Text(rate_to_numeric_string(mana_usd_rate)),
            binds,
            next_idx,
        ))
    } else {
        None
    };

    let mut parts: Vec<String> = Vec::new();
    if filters.source != Some(UnifiedSource::Legacy) {
        parts.push(unified_branch(
            UnifiedSource::Native,
            UnifiedAcquisition::Trade,
            None,
            &filters.base,
            filters.listing_type,
            binds,
            next_idx,
        ));
    }
    if filters.source != Some(UnifiedSource::Native) {
        parts.push(unified_branch(
            UnifiedSource::Legacy,
            UnifiedAcquisition::Trade,
            rate_p.as_deref(),
            &filters.base,
            filters.listing_type,
            binds,
            next_idx,
        ));
        // CollectionStore mints. `source: legacy` because they are MANA-priced
        // and must inherit the legacy price treatment exactly --
        // server-converted at the live rate, re-priced client-side at
        // checkout, and hidden when no rate is available. What differs is only
        // HOW you buy it, which is `acquisition`. Folding these two orthogonal
        // facts into one `source` enum would force every existing
        // `source == "legacy"` branch to be re-audited.
        parts.push(unified_branch(
            UnifiedSource::Legacy,
            UnifiedAcquisition::Store,
            rate_p.as_deref(),
            &filters.base,
            filters.listing_type,
            binds,
            next_idx,
        ));
    }
    parts.join("\n UNION ALL \n")
}

pub fn build_unified_listings_sql(
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let inner = unified_inner(filters, mana_usd_rate, &mut binds, &mut next_idx);

    let mut outer_wheres = vec![
        " sub.usd_wei > 0 ".to_string(),
        format!(" sub.usd_wei <= {MAX_USD_WEI}::numeric "),
    ];
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

/// The item-unified core: the UNION ALL of the source branches collapsed to ONE
/// representative row per (contract, item), with the per-item `listing_count`
/// window and the `price_credits` CEIL of the survivor. Callers wrap this as `d`
/// and add their own outer filtering/ordering/pagination.
///
/// Shared by the browse item feed (`build_unified_items_sql`) and the
/// related-items rail (`build_related_items_sql`) so the rail is drawn from
/// exactly the same universe, grouping and headline-price rules as the grid it
/// mirrors -- a divergence here would show the same item at two prices on two
/// screens.
fn build_item_unified_core(
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) -> String {
    let inner = unified_inner(filters, mana_usd_rate, binds, next_idx);

    format!(
        "SELECT DISTINCT ON (f.contract_address, f.item_id)\n\
           f.*,\n\
           CEIL(f.usd_wei / {credit_wei}::numeric)::bigint AS price_credits\n\
         FROM (\n\
           SELECT\n\
             u.*,\n\
             COUNT(*) OVER (PARTITION BY u.contract_address, u.item_id) AS listing_count\n\
           FROM (\n{inner}\n) u\n\
           WHERE u.usd_wei > 0 AND u.usd_wei <= {max_usd_wei}::numeric\n\
         ) f\n\
         ORDER BY\n\
           f.contract_address,\n\
           f.item_id,\n\
           (CASE WHEN f.trade_type = 'public_item_order' THEN 0 ELSE 1 END),\n\
           (CASE WHEN f.source = 'native' THEN 0 ELSE 1 END),\n\
           f.usd_wei ASC,\n\
           (CASE WHEN f.acquisition = 'trade' THEN 0 ELSE 1 END),\n\
           f.trade_id",
        credit_wei = USD_WEI_PER_CREDIT,
        max_usd_wei = MAX_USD_WEI,
    )
}

pub fn build_unified_items_sql(
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let core = build_item_unified_core(filters, mana_usd_rate, &mut binds, &mut next_idx);

    let mut outer_wheres = vec![" d.usd_wei > 0 ".to_string()];
    if let Some(bound) = filters
        .base
        .min_price_credits
        .and_then(unified_min_price_bound_wei)
    {
        let p = emit(Bind::Text(bound.to_string()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" d.usd_wei > {p}::numeric "));
    }
    if let Some(max_wei) = filters.base.max_price_credits.and_then(credits_to_wei) {
        let p = emit(Bind::Text(max_wei.to_string()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" d.usd_wei <= {p}::numeric "));
    }

    let order = match filters.base.sort_by {
        Some(ShopSortBy::Cheapest) => "ORDER BY d.usd_wei ASC, d.trade_id",
        Some(ShopSortBy::MostExpensive) => "ORDER BY d.usd_wei DESC, d.trade_id",
        Some(ShopSortBy::Name) => "ORDER BY d.name ASC, d.trade_id",
        Some(ShopSortBy::Newest) | None => "ORDER BY d.created_at DESC, d.trade_id",
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
           d.*,\n\
           COUNT(*) OVER() AS total\n\
         FROM (\n{core}\n) d\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        where_clause = where_from(&outer_wheres),
    );

    (sql, binds)
}

/// Bounds for the related-items rail: a single carousel, so deliberately far
/// below the browse page size (a caller asking for hundreds would only widen a
/// scan nothing can render). Mirrors upstream RELATED_DEFAULT_LIMIT/MAX_LIMIT.
pub const RELATED_DEFAULT_LIMIT: i64 = 10;
pub const RELATED_MAX_LIMIT: i64 = 50;

/// Rarity tiers scarcest-first, mirroring @dcl/schemas' `Rarity` enum order so
/// the related rail's rarity-distance ordering can never drift from the enum.
const RARITY_ORDER: &[&str] = &[
    "unique",
    "mythic",
    "exotic",
    "legendary",
    "epic",
    "rare",
    "uncommon",
    "common",
];

/// One past the widest real gap (unique..common = 7): sorts a row whose rarity
/// is missing or unrecognised behind every known tier.
const UNKNOWN_RARITY_DISTANCE: usize = RARITY_ORDER.len();

fn rarity_rank(rarity: &str) -> Option<usize> {
    let lower = rarity.to_ascii_lowercase();
    RARITY_ORDER.iter().position(|tier| *tier == lower)
}

fn related_clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(RELATED_DEFAULT_LIMIT)
        .clamp(SHOP_MIN_PAGE_SIZE, RELATED_MAX_LIMIT)
}

/// "Prioritise rarity" as a sortable distance from the anchor's tier, closest
/// first. Returns `None` when the anchor's own rarity is missing or
/// unrecognised -- the caller then omits the rarity sort key entirely, leaving
/// the ordering to `created_at`/`trade_id`. (Upstream emits a bare `0` there,
/// but a lone integer literal in Postgres ORDER BY is read as ordinal column
/// position 0 and errors; omitting the key gives the same "no rarity
/// preference" ordering without 500-ing the rail this endpoint promises never
/// to error.) The distances are fixed constants from `RARITY_ORDER`, so the
/// CASE carries no interpolated input.
fn related_rarity_sort_key(reference_rarity: Option<&str>) -> Option<String> {
    let anchor = reference_rarity.and_then(rarity_rank)?;
    let mut expr = String::from("CASE lower(d.rarity)");
    for (rank, tier) in RARITY_ORDER.iter().enumerate() {
        expr.push_str(&format!(" WHEN '{tier}' THEN {}", anchor.abs_diff(rank)));
    }
    expr.push_str(&format!(" ELSE {UNKNOWN_RARITY_DISTANCE} END"));
    Some(expr)
}

/// The anchor item's similarity attributes, resolved from the squid `item` row
/// rather than passed by the caller: the PDP knows the item's identity from its
/// URL long before it has hydrated rarity/category, so resolving here lets the
/// rail be requested on the first render.
pub(super) struct ReferenceItem {
    pub(super) category: &'static str,
    pub(super) wearable_category: Option<String>,
    pub(super) rarity: Option<String>,
}

impl ReferenceItem {
    fn from_row(row: ReferenceItemRow) -> Self {
        Self {
            category: top_level_category(row.item_type.as_deref()),
            wearable_category: row.wearable_category,
            rarity: row.rarity,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ReferenceItemRow {
    rarity: Option<String>,
    item_type: Option<String>,
    wearable_category: Option<String>,
}

/// Look up the anchor item's similarity attributes by collection + blockchain
/// id. `item_id` is bound as text and cast `::numeric` in SQL so an unbounded
/// blockchain id survives; the handler has already guaranteed it is all digits.
pub(super) fn build_reference_item_sql(
    contract_address: &str,
    item_id: &str,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let addr_p = emit(
        Bind::Text(contract_address.to_lowercase()),
        &mut binds,
        &mut next_idx,
    );
    let item_p = emit(Bind::Text(item_id.to_string()), &mut binds, &mut next_idx);

    let sql = format!(
        "SELECT\n\
           item.rarity AS rarity,\n\
           item.item_type AS item_type,\n\
           COALESCE(item.search_wearable_category, item.search_emote_category) AS wearable_category\n\
         FROM {schema}.item item\n\
         WHERE item.collection_id = {addr_p}\n\
           AND item.blockchain_id = {item_p}::numeric\n\
         LIMIT 1",
        schema = crate::MARKETPLACE_SQUID_SCHEMA,
    );

    (sql, binds)
}

/// The related-items feed: the same item-unified core as the browse grid, hard
/// filtered to the anchor's top-level + on-chain sub-category, with the anchor
/// itself excluded and rows ordered by rarity distance from the anchor. Kept
/// separate from the anchor lookup (two statements) so the shared browse-filter
/// SQL is reused rather than re-expressed as a self-join on the anchor row.
pub(super) fn build_related_items_sql(
    contract_address: &str,
    item_id: &str,
    reference: &ReferenceItem,
    first: Option<i64>,
    mana_usd_rate: f64,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            category: Some(reference.category.to_string()),
            wearable_categories: reference
                .wearable_category
                .clone()
                .map(|c| vec![c])
                .unwrap_or_default(),
            ..Default::default()
        },
        source: None,
        listing_type: None,
    };

    let core = build_item_unified_core(&filters, mana_usd_rate, &mut binds, &mut next_idx);

    let addr_p = emit(
        Bind::Text(contract_address.to_lowercase()),
        &mut binds,
        &mut next_idx,
    );
    let item_p = emit(Bind::Text(item_id.to_string()), &mut binds, &mut next_idx);
    let limit_p = emit(
        Bind::Int(related_clamp_first(first)),
        &mut binds,
        &mut next_idx,
    );

    let order = match related_rarity_sort_key(reference.rarity.as_deref()) {
        Some(rarity_expr) => format!("ORDER BY {rarity_expr}, d.created_at DESC, d.trade_id"),
        None => "ORDER BY d.created_at DESC, d.trade_id".to_string(),
    };

    // Drop the anchor itself with a NULL-safe disjunction, not `NOT (a AND b)`:
    // item_id is nullable, and the negated form would evaluate to NULL and
    // silently discard every secondary-only row.
    let sql = format!(
        "SELECT d.*\n\
         FROM (\n{core}\n) d\n\
         WHERE d.usd_wei > 0\n\
           AND (d.contract_address <> {addr_p} OR COALESCE(d.item_id, '') <> {item_p})\n\
         {order}\n\
         LIMIT {limit_p}",
    );

    (sql, binds)
}

/// The TRENDING rail: what is selling right now, restricted to the same
/// credit-buyable, item-unified universe as the browse grid so every card is one
/// the Shop can actually sell.
///
/// Ranking, made deterministic (upstream shuffles its equivalent, discarding the
/// order):
///   - `sales_window` counts every windowed sale of each item and sums what those
///     sales were actually paid at. Both signals matter -- see `TRENDING_SALES_CUT`.
///   - the first 60% of the slots go to the highest sale COUNT; the remaining 40%
///     to the highest VOLUME among the items the sales pass did not already take.
///
/// Two asymmetries mirror upstream. The SIGNAL is broader than the ROW: `sale`
/// counts mints and resales, primary and secondary, MANA- and credit-priced --
/// demand for an item is demand however it was met -- while what the row may
/// DISPLAY is narrowed by the shared core plus the caller's filters, so an item
/// trending purely on resales with no credit-buyable listing is simply absent.
/// And `volume` sums the PRICES THOSE SALES SETTLED AT, not the current price
/// times the count, so a later price cut does not rewrite past volume.
///
/// Ordered by a TOTAL order -- `(contract_address, item_id)` is unique out of the
/// core's DISTINCT ON, so the LIMIT can neither drop nor duplicate a row.
pub(super) fn build_trending_items_sql(
    first: Option<i64>,
    days: Option<i64>,
    filters: &UnifiedCatalogFilters,
    mana_usd_rate: f64,
) -> (String, Vec<Bind>) {
    let first = trending_clamp_first(first);
    let days = trending_clamp_days(days);

    // ceil + remainder rather than two independent fractional slices, which would
    // truncate to fewer than `first` rows even with plenty of supply.
    let sales_slots = (first as f64 * TRENDING_SALES_CUT).ceil() as i64;
    let volume_slots = first - sales_slots;

    // Same window anchor the marketplace's /v1/trendings row uses (both resolve it
    // through `midnight_days_ago`), so the two rows span the same slice of history.
    // `sale.timestamp` is stored in unix SECONDS.
    let from_seconds = midnight_days_ago(days);

    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    let from_p = emit(Bind::Int(from_seconds), &mut binds, &mut next_idx);
    let core = build_item_unified_core(filters, mana_usd_rate, &mut binds, &mut next_idx);
    let sales_slots_p = emit(Bind::Int(sales_slots), &mut binds, &mut next_idx);
    let volume_slots_p = emit(Bind::Int(volume_slots), &mut binds, &mut next_idx);
    let limit_p = emit(Bind::Int(first), &mut binds, &mut next_idx);

    let sql = format!(
        "WITH sales_window AS (\n\
           SELECT\n\
             sale.search_contract_address AS contract_address,\n\
             sale.search_item_id::text AS item_id,\n\
             COUNT(*)::int8 AS sales,\n\
             SUM(sale.price::numeric) AS volume\n\
           FROM {schema}.sale sale\n\
           WHERE sale.timestamp > {from_p}\n\
             AND sale.search_item_id IS NOT NULL\n\
           GROUP BY 1, 2\n\
         ),\n\
         listed AS (\n\
           SELECT d.*, w.sales, w.volume\n\
           FROM (\n{core}\n) d\n\
           JOIN sales_window w\n\
             ON w.contract_address = d.contract_address AND w.item_id = d.item_id\n\
           WHERE d.usd_wei > 0\n\
         ),\n\
         ranked AS (\n\
           SELECT listed.*,\n\
             ROW_NUMBER() OVER (\n\
               ORDER BY listed.sales DESC, listed.volume DESC, listed.contract_address, listed.item_id\n\
             ) AS sales_rank\n\
           FROM listed\n\
         ),\n\
         composed AS (\n\
           SELECT ranked.*,\n\
             (ranked.sales_rank <= {sales_slots_p}) AS by_sales,\n\
             ROW_NUMBER() OVER (\n\
               PARTITION BY (ranked.sales_rank <= {sales_slots_p})\n\
               ORDER BY ranked.volume DESC, ranked.sales DESC, ranked.contract_address, ranked.item_id\n\
             ) AS volume_rank\n\
           FROM ranked\n\
         )\n\
         SELECT *\n\
         FROM composed\n\
         WHERE by_sales OR volume_rank <= {volume_slots_p}\n\
         ORDER BY by_sales DESC,\n\
           (CASE WHEN by_sales THEN sales_rank ELSE volume_rank END),\n\
           contract_address, item_id\n\
         LIMIT {limit_p}",
        schema = crate::MARKETPLACE_SQUID_SCHEMA,
    );

    (sql, binds)
}

/// Row -> model. Store rows drop the trade id here (there is no trade; the SQL
/// keeps the item id in that column only as a DISTINCT ON tiebreaker), which is
/// what stops a nonexistent trade reference reaching credit authorization.
fn map_unified_listing(r: UnifiedListingRow) -> UnifiedListing {
    let (network, chain_id) = network_and_chain(r.network.as_deref());
    UnifiedListing {
        acquisition: r.acquisition.clone(),
        trade_id: if r.acquisition == "store" {
            None
        } else {
            Some(r.trade_id)
        },
        source: r.source,
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
        // Seller + issued id come from `mv.assets`, which the store relation
        // supplies as NULL::jsonb -- both land as None with no special case.
        // That is the right answer for a mint: nobody is reselling it and no
        // token has been issued yet.
        seller: r.seller,
        issued_id: r.issued_id,
        price_credits: r.price_credits,
        mana_wei: r.mana_wei,
        available: parse_available(r.available.as_deref()),
        network,
        chain_id,
        created_at: r.created_at,
    }
}

/// Same mapping for the grouped feed; only `listing_count` is added on top.
/// NOTE it counts store mints alongside trades, so it is "credit-buyable
/// offers" rather than strictly "listings". Takes the `total`-free row so both
/// the paginated browse grid and the unpaginated related-items rail map through
/// this one function and stay byte-for-byte the same JSON shape.
fn map_unified_item(r: RelatedItemRow) -> UnifiedItem {
    let (network, chain_id) = network_and_chain(r.network.as_deref());
    UnifiedItem {
        acquisition: r.acquisition.clone(),
        trade_id: if r.acquisition == "store" {
            None
        } else {
            Some(r.trade_id)
        },
        source: r.source,
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
        seller: r.seller,
        issued_id: r.issued_id,
        price_credits: r.price_credits,
        mana_wei: r.mana_wei,
        listing_count: r.listing_count,
        available: parse_available(r.available.as_deref()),
        network,
        chain_id,
        created_at: r.created_at,
    }
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

        let data = rows.into_iter().map(map_unified_listing).collect();
        Ok((data, total))
    }

    /// One row per (contract, item), priced primary-if-present else cheapest credit-buyable secondary, with a per-item `listingCount` (the shop browse feed).
    pub async fn get_shop_items(
        &self,
        filters: &UnifiedCatalogFilters,
        mana_usd_rate: f64,
    ) -> Result<(Vec<UnifiedItem>, i64), ApiError> {
        let (sql, binds) = build_unified_items_sql(filters, mana_usd_rate);
        let rows: Vec<UnifiedItemRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let data = rows.into_iter().map(|r| map_unified_item(r.core)).collect();
        Ok((data, total))
    }

    /// Items SIMILAR to one item -- the PDP's fallback rail. Two statements: the
    /// anchor lookup, then the item-unified feed built from the same shared core
    /// as the browse grid. An unknown/missing anchor yields an empty rail (no
    /// second query) rather than an error.
    pub async fn get_related_items(
        &self,
        contract_address: &str,
        item_id: &str,
        first: Option<i64>,
        mana_usd_rate: f64,
    ) -> Result<Vec<UnifiedItem>, ApiError> {
        let (anchor_sql, anchor_binds) = build_reference_item_sql(contract_address, item_id);
        let anchor: Vec<ReferenceItemRow> = self.fetch(anchor_sql, anchor_binds).await?;
        let Some(reference) = anchor.into_iter().next().map(ReferenceItem::from_row) else {
            return Ok(Vec::new());
        };

        let (sql, binds) =
            build_related_items_sql(contract_address, item_id, &reference, first, mana_usd_rate);
        let rows: Vec<RelatedItemRow> = self.fetch(sql, binds).await?;
        Ok(rows.into_iter().map(map_unified_item).collect())
    }

    /// The trending rail (`/v3/catalog/trending`): items ranked by how much they
    /// sold in the look-back window, drawn from the same credit-buyable universe as
    /// the browse grid so every card is one the Shop can sell. Unpaginated and
    /// ordered BY the ranking -- no total, no caller sort. `first`/`days` are
    /// clamped inside the query builder.
    pub async fn get_trending_items(
        &self,
        first: Option<i64>,
        days: Option<i64>,
        filters: &UnifiedCatalogFilters,
        mana_usd_rate: f64,
    ) -> Result<Vec<TrendingItem>, ApiError> {
        let (sql, binds) = build_trending_items_sql(first, days, filters, mana_usd_rate);
        let rows: Vec<TrendingItemRow> = self.fetch(sql, binds).await?;
        Ok(rows
            .into_iter()
            .map(|r| TrendingItem {
                item: map_unified_item(r.core),
                trending_sales: r.sales,
            })
            .collect())
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    fn listing_row(acquisition: &str) -> UnifiedListingRow {
        UnifiedListingRow {
            source: "legacy".to_string(),
            acquisition: acquisition.to_string(),
            trade_id: "row-id".to_string(),
            trade_type: "public_item_order".to_string(),
            contract_address: Some("0xc".to_string()),
            item_id: Some("7".to_string()),
            token_id: None,
            name: Some("hat".to_string()),
            image: None,
            rarity: Some("Rare".to_string()),
            item_type: Some("wearable_v2".to_string()),
            wearable_category: Some("hat".to_string()),
            gender: None,
            creator: Some("0xdead".to_string()),
            seller: None,
            issued_id: None,
            price_credits: 5,
            mana_wei: Some("1000".to_string()),
            available: Some("3".to_string()),
            network: Some("MATIC".to_string()),
            created_at: 1_000,
            total: 1,
        }
    }

    fn item_row(acquisition: &str) -> RelatedItemRow {
        let l = listing_row(acquisition);
        RelatedItemRow {
            source: l.source,
            acquisition: l.acquisition,
            trade_id: l.trade_id,
            trade_type: l.trade_type,
            contract_address: l.contract_address,
            item_id: l.item_id,
            token_id: l.token_id,
            name: l.name,
            image: l.image,
            rarity: l.rarity,
            item_type: l.item_type,
            wearable_category: l.wearable_category,
            gender: l.gender,
            creator: l.creator,
            seller: l.seller,
            issued_id: l.issued_id,
            price_credits: l.price_credits,
            mana_wei: l.mana_wei,
            listing_count: 2,
            available: l.available,
            network: l.network,
            created_at: l.created_at,
        }
    }

    #[test]
    fn store_rows_carry_acquisition_and_drop_the_tiebreak_trade_id() {
        let m = map_unified_listing(listing_row("store"));
        assert_eq!(m.acquisition, "store");
        assert_eq!(m.trade_id, None, "a mint has no trade");
        assert_eq!(m.listing_type, "primary");
        assert_eq!(m.mana_wei.as_deref(), Some("1000"));
    }

    #[test]
    fn trade_rows_keep_their_trade_id() {
        let m = map_unified_listing(listing_row("trade"));
        assert_eq!(m.acquisition, "trade");
        assert_eq!(m.trade_id.as_deref(), Some("row-id"));
    }

    #[test]
    fn item_feed_maps_store_rows_the_same_way_plus_listing_count() {
        let m = map_unified_item(item_row("store"));
        assert_eq!(m.acquisition, "store");
        assert_eq!(m.trade_id, None);
        assert_eq!(m.listing_count, 2);

        let t = map_unified_item(item_row("trade"));
        assert_eq!(t.trade_id.as_deref(), Some("row-id"));
    }

    #[test]
    fn unified_wire_shape_serializes_acquisition_and_null_trade_id() {
        let v = serde_json::to_value(map_unified_listing(listing_row("store"))).unwrap();
        assert_eq!(v["acquisition"], "store");
        assert!(v["tradeId"].is_null(), "{v}");
        assert_eq!(v["source"], "legacy");
    }
}
