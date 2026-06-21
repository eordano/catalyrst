use super::types::{
    LegacyCatalogFilters, ShopCatalogFilters, ShopSortBy, SHOP_DEFAULT_PAGE_SIZE,
    SHOP_MAX_PAGE_SIZE, SHOP_MIN_PAGE_SIZE,
};
use crate::logic::sql_filters::where_from;
use crate::MARKETPLACE_SQUID_SCHEMA;

pub(super) const ASSET_TYPE_USD_PEGGED_MANA: i64 = 2;
pub(super) const ASSET_TYPE_ERC20: i64 = 1;

pub(super) const USD_WEI_PER_CREDIT: u128 = 100_000_000_000_000_000;

pub(super) fn to_credits(usd_wei: &str) -> Option<u64> {
    let wei = usd_wei.parse::<u128>().ok()?;
    if wei == 0 {
        return None;
    }
    u64::try_from(wei.div_ceil(USD_WEI_PER_CREDIT)).ok()
}

pub(super) fn credits_to_wei(credits: f64) -> Option<u128> {
    if !credits.is_finite() {
        return None;
    }
    Some(credits.max(0.0).floor() as u128 * USD_WEI_PER_CREDIT)
}

pub(super) fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub(super) fn shop_clamp_first(first: Option<i64>) -> i64 {
    first
        .unwrap_or(SHOP_DEFAULT_PAGE_SIZE)
        .clamp(SHOP_MIN_PAGE_SIZE, SHOP_MAX_PAGE_SIZE)
}

pub(super) fn shop_clamp_skip(skip: Option<i64>) -> i64 {
    skip.unwrap_or(0).max(0)
}

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

pub(super) fn emit(b: Bind, bs: &mut Vec<Bind>, idx: &mut usize) -> String {
    bs.push(b);
    let s = format!("${}", *idx);
    *idx += 1;
    s
}

pub(super) fn metadata_joins() -> String {
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

pub(super) fn gender_expr() -> &'static str {
    "CASE\n\
       WHEN COALESCE(item_p.search_wearable_body_shapes, item_s.search_wearable_body_shapes)::text[] @> ARRAY['BaseMale','BaseFemale']::text[] THEN 'unisex'\n\
       WHEN COALESCE(item_p.search_wearable_body_shapes, item_s.search_wearable_body_shapes)::text[] @> ARRAY['BaseMale']::text[] THEN 'male'\n\
       WHEN COALESCE(item_p.search_wearable_body_shapes, item_s.search_wearable_body_shapes)::text[] @> ARRAY['BaseFemale']::text[] THEN 'female'\n\
       ELSE NULL\n\
     END AS gender"
}

pub(super) const SHOP_NAME_EXPR: &str = "COALESCE(nft.name, w_p.name, e_p.name)";
pub(super) const LEGACY_NAME_EXPR: &str = "COALESCE(w_p.name, e_p.name)";

pub(super) fn order_by(sort_by: Option<ShopSortBy>, name_expr: &str) -> String {
    match sort_by {
        Some(ShopSortBy::Cheapest) => "ORDER BY mv.amount_received ASC".to_string(),
        Some(ShopSortBy::MostExpensive) => "ORDER BY mv.amount_received DESC".to_string(),
        Some(ShopSortBy::Name) => format!("ORDER BY {name_expr} ASC"),
        Some(ShopSortBy::Newest) | None => "ORDER BY mv.created_at DESC".to_string(),
    }
}

pub(super) fn received_asset_exists(
    asset_type: i64,
    binds: &mut Vec<Bind>,
    next_idx: &mut usize,
) -> String {
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
           COUNT(*) OVER() AS total,\n\
           {gender}\n\
         {joins}\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        name_expr = SHOP_NAME_EXPR,
        gender = gender_expr(),
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
           COUNT(*) OVER() AS total,\n\
           {gender}\n\
         {joins}\n\
         {where_clause}\n\
         {order}\n\
         LIMIT {limit_p} OFFSET {offset_p}",
        name_expr = LEGACY_NAME_EXPR,
        gender = gender_expr(),
        joins = metadata_joins(),
        where_clause = where_from(&wheres),
        order = order_by(filters.sort_by, LEGACY_NAME_EXPR),
    );

    (sql, binds)
}
