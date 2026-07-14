use super::types::{
    top_creators_clamp_days, top_creators_clamp_first, top_creators_min_sales,
    LegacyCatalogFilters, ShopCatalogFilters, ShopSortBy, SHOP_DEFAULT_PAGE_SIZE,
    SHOP_MAX_PAGE_SIZE, SHOP_MIN_PAGE_SIZE, TOP_CREATORS_MIN_ITEMS,
};
use crate::logic::sql_filters::where_from;
use crate::ports::trendings::midnight_days_ago;
use crate::MARKETPLACE_SQUID_SCHEMA;

pub(super) const ASSET_TYPE_USD_PEGGED_MANA: i64 = 2;
pub(super) const ASSET_TYPE_ERC20: i64 = 1;

pub(super) const USD_WEI_PER_CREDIT: u128 = 100_000_000_000_000_000;

/// uint256 max, which the squid writes into `item.price` to mean "no price set"
/// rather than leaving it NULL. A `price > 0` guard does NOT exclude it, so a
/// store item carrying the sentinel would be advertised at ~1.16e42 credits.
/// ports/catalog guards the same value the same way (`MAX_NUMERIC_NUMBER`);
/// duplicated here as upstream does rather than coupling the two ports.
pub(super) const NO_PRICE_SENTINEL: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// Upper bound on a row's USD-wei price, applied before `price_credits` is cast
/// to bigint. Without it an absurd price does not merely render badly --
/// `CEIL(usd_wei / C)::bigint` raises `bigint out of range` and the ENTIRE
/// query aborts, so one bad item 500s the catalogue for every user. The
/// sentinel guard above does not cover this: `sentinel - 1` clears it and still
/// overflows. 1e30 USD wei is $1e12 -- orders of magnitude above any real item
/// and ~1e6 below the bigint ceiling, so the cast has room. Rows above it are
/// dropped rather than fatal.
pub(super) const MAX_USD_WEI: &str = "1000000000000000000000000000000";

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

#[derive(Debug)]
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

/// The shop's creator rail (`/v3/catalog/creators`, marketplace-server
/// #389/#390/#394): creators ranked by how much MANA THEIR items took in the
/// window, attributing each sale to `item.creator` via the `sale.item_id = item.id`
/// join rather than to the seller (which a primary mint never reaches -- see
/// [`super::types::TopCreator`]). `sale.timestamp` is unix SECONDS, so the
/// window anchor is bound as seconds.
///
/// The window is a FILTER on the ranking count, not part of the scan's WHERE:
/// in the WHERE it would bound the all-time `total_sales` too, silently making
/// it a second copy of the windowed figure. Catalogue counts come from their
/// own CTE over `item` alone -- joined to `sale`, one item is one row PER SALE,
/// so a creator's item count would come back multiplied by how well it sold.
/// The join is LEFT so a creator whose sales are visible but whose catalogue is
/// not still ranks; the [`top_creators_min_sales`] floor keeps a thin window out
/// of a REVENUE ranking, and the [`TOP_CREATORS_MIN_ITEMS`] floor keeps out a
/// one-hit month with nothing to browse.
///
/// The windowed revenue stays NUMERIC through the ORDER BY and is cast to text
/// only on the way out: sorting the wei as text would put '900...' above
/// '1000...' and rank a creator by the first digit of their revenue.
pub(super) fn build_top_creators_sql(first: Option<i64>, days: Option<i64>) -> (String, Vec<Bind>) {
    let first = top_creators_clamp_first(first);
    let days = top_creators_clamp_days(days);
    let from_seconds = midnight_days_ago(days);

    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;
    let from_p = emit(Bind::Int(from_seconds), &mut binds, &mut next_idx);
    let min_sales_p = emit(
        Bind::Int(top_creators_min_sales(days)),
        &mut binds,
        &mut next_idx,
    );
    let min_items_p = emit(Bind::Int(TOP_CREATORS_MIN_ITEMS), &mut binds, &mut next_idx);
    let limit_p = emit(Bind::Int(first), &mut binds, &mut next_idx);

    let sql = format!(
        "WITH ranked AS (\n\
           SELECT item.creator AS creator,\n\
                  COALESCE(SUM(sale.price::numeric) FILTER (WHERE sale.timestamp > {from_p}), 0) AS volume,\n\
                  COUNT(*) FILTER (WHERE sale.timestamp > {from_p})::int8 AS sales,\n\
                  COUNT(*)::int8 AS total_sales\n\
           FROM {schema}.sale sale\n\
           JOIN {schema}.item item ON item.id = sale.item_id\n\
           WHERE sale.item_id IS NOT NULL\n\
             AND item.search_is_collection_approved = true\n\
           GROUP BY item.creator\n\
         ), catalogue AS (\n\
           SELECT creator, COUNT(*)::int8 AS items,\n\
                  COUNT(DISTINCT collection_id)::int8 AS collections\n\
           FROM {schema}.item\n\
           WHERE search_is_collection_approved = true\n\
           GROUP BY creator\n\
         )\n\
         SELECT r.creator, r.volume::text AS volume, r.sales, r.total_sales,\n\
                COALESCE(c.collections, 0) AS collections,\n\
                COALESCE(c.items, 0) AS items\n\
         FROM ranked r\n\
         LEFT JOIN catalogue c ON c.creator = r.creator\n\
         WHERE r.sales >= {min_sales_p}\n\
           AND COALESCE(c.items, 0) >= {min_items_p}\n\
         ORDER BY r.volume DESC, r.creator ASC\n\
         LIMIT {limit_p}",
        schema = MARKETPLACE_SQUID_SCHEMA,
    );

    (sql, binds)
}

/// The metadata joins, keyed off whatever relation is aliased `mv`. Split out
/// from `metadata_joins` so the CollectionStore branch can reuse them verbatim
/// over its own base relation (see `store_base_relation`): every shared
/// expression -- `append_unified_filters`, `gender_expr` -- reads these aliases,
/// so reusing the join chain is what makes the filters provably identical
/// across branches rather than identical by inspection.
pub(super) fn metadata_joins_on() -> String {
    format!(
        "LEFT JOIN {schema}.item item_p ON mv.type = 'public_item_order'\n\
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

pub(super) fn metadata_joins() -> String {
    format!(
        "FROM marketplace.mv_trades mv\n{joins}",
        joins = metadata_joins_on()
    )
}

/// The CollectionStore branch's base relation, shaped like `mv_trades` and
/// aliased `mv`.
///
/// A store item is NOT a trade: primary minting has no order and no signed
/// listing. It is a property of the item -- the CollectionStore is a minter for
/// its collection, and the buyer calls `CollectionStore.buy` at `item.price`.
/// So it cannot be recovered by filtering `mv_trades`; it needs its own source
/// relation, which is why the Shop's feed was missing it entirely.
///
/// Projected into mv_trades' column names rather than given its own branch
/// shape, so the metadata joins, the gender expression and every browse filter
/// apply UNCHANGED.
///
/// The predicate mirrors the store-minter half of ports/catalog's on-sale
/// logic (`only_minting` in catalog/queries.rs), minus its V3-minter half --
/// that half is the offchain primary trade the existing branch already covers,
/// and including it here would double-count every item. Predicates, and why:
/// - `search_is_collection_approved` mirrors the base WHERE /v2/catalog
///   applies; it constrains only THIS branch (the trade branches carry no
///   approval check -- narrowing that is deliberately out of scope).
/// - `available > 0` drops sold-out mints: store supply is finite.
/// - `price > 0` drops free claims.
/// - `search_emote_outcome_type IS NULL` excludes SOCIAL emotes, which the
///   marketplace deliberately hides.
/// - `network <> 'ETHEREUM'` is insurance: this row tells the client to call
///   CollectionStore.buy, which exists only on Polygon, so an L1 row would
///   offer a purchase that cannot settle.
pub(super) fn store_base_relation() -> String {
    format!(
        "FROM (\n\
           SELECT\n\
             i.id AS id,\n\
             'public_item_order'::text AS type,\n\
             i.collection_id AS sent_contract_address,\n\
             i.blockchain_id::text AS sent_item_id,\n\
             NULL::text AS sent_token_id,\n\
             NULL::text AS sent_nft_id,\n\
             i.price AS amount_received,\n\
             i.available AS available,\n\
             i.network AS network,\n\
             to_timestamp(i.created_at) AS created_at,\n\
             NULL::jsonb AS assets\n\
           FROM {schema}.item i\n\
           WHERE i.search_is_store_minter = true\n\
             AND i.search_is_collection_approved = true\n\
             AND i.available > 0\n\
             AND i.price > 0\n\
             AND i.price IS DISTINCT FROM '{sentinel}'::numeric\n\
             AND i.search_emote_outcome_type IS NULL\n\
             AND i.network <> 'ETHEREUM'\n\
         ) mv\n\
         {joins}",
        schema = MARKETPLACE_SQUID_SCHEMA,
        sentinel = NO_PRICE_SENTINEL,
        joins = metadata_joins_on(),
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
    if let Some(creator) = filters.creator.as_deref().filter(|c| !c.is_empty()) {
        let p = emit(
            Bind::Text(creator.to_lowercase()),
            &mut binds,
            &mut next_idx,
        );
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
    if filters.is_smart {
        wheres.push(
            " COALESCE(item_p.item_type, item_s.item_type, nft.item_type) = 'smart_wearable_v1' "
                .to_string(),
        );
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
           mv.assets->'sent'->>'owner' AS seller,\n\
           mv.assets->'sent'->>'issued_id' AS issued_id,\n\
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
