use super::{
    expand_urn_network_forms, get_item_types_from_nft_category, ItemFilters, ItemSortBy, ItemType,
    DEFAULT_LIMIT,
};
use crate::dcl_schemas::{get_db_networks, NftCategory};
use crate::logic::sql_filters::{clamp_first, clamp_skip, where_from};
use crate::MARKETPLACE_SQUID_SCHEMA;

// The received-asset type that marks a USD-pegged (credit-buyable) trade; anything else priced
// through a v3 trade is classic MANA and needs the MANA/USD rate to reach credits.
const ASSET_TYPE_USD_PEGGED_MANA: i64 = 2;

// 1 credit = $0.10; $1 = 1e18 USD wei = 10 credits, so 1 credit = 1e17 USD wei. Kept as a literal
// in the SQL so the numeric math stays exact.
const USD_WEI_PER_CREDIT_SQL: &str = "100000000000000000::numeric";

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

fn emit(bind: Bind, binds: &mut Vec<Bind>, idx: &mut usize) -> String {
    binds.push(bind);
    let s = format!("${}", *idx);
    *idx += 1;
    s
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

// The asset-type-aware price_credits column for the catalog-items feed. Unlike the mixed-unit
// /v1/items price, this normalizes every item to whole credits, CEIL-consistent with the native
// Shop path ("Model B"):
//   - a v3 trade priced in USD-pegged MANA is already USD wei -> no rate;
//   - a v3 trade priced in classic MANA, or a classic store-minter item.price (MANA wei), is
//     multiplied by the MANA/USD rate to reach USD wei;
//   - anything not currently for sale (available = 0, no open minter) -> 0.
// The trade branch mirrors from_db_item_to_item's precedence (open v3 trade wins over the store
// minter).
fn price_credits_select(rate_p: &str) -> String {
    format!(
        "CASE\n\
           WHEN item.available > 0 AND unified_trades.id IS NOT NULL AND item.search_is_marketplace_v3_minter = true THEN\n\
             CASE\n\
               WHEN EXISTS (\n\
                 SELECT 1 FROM marketplace.trade_assets ta\n\
                 WHERE ta.trade_id = unified_trades.id\n\
                   AND ta.direction = 'received'\n\
                   AND ta.asset_type = {asset_type}\n\
               )\n\
               THEN CEIL((unified_trades.assets -> 'received' ->> 'amount')::numeric / {credit_wei})\n\
               ELSE CEIL((unified_trades.assets -> 'received' ->> 'amount')::numeric * {rate_p}::numeric / {credit_wei})\n\
             END\n\
           WHEN item.available > 0 AND item.search_is_store_minter = true THEN\n\
             CEIL(item.price::numeric * {rate_p}::numeric / {credit_wei})\n\
           ELSE 0\n\
         END::bigint as price_credits",
        asset_type = ASSET_TYPE_USD_PEGGED_MANA,
        credit_wei = USD_WEI_PER_CREDIT_SQL,
    )
}

pub fn build_items_query(filters: &ItemFilters) -> (String, Vec<Bind>) {
    build_items_query_with(filters, None)
}

// The credit-aware catalog-items feed backing GET /v3/catalog/items: same rows, filters and
// ordering as build_items_query plus the SQL-computed price_credits column. `rate_numeric` is
// the MANA/USD rate as a fixed-precision numeric literal ('0' for a broken rate).
pub fn build_catalog_items_query(filters: &ItemFilters, rate_numeric: &str) -> (String, Vec<Bind>) {
    build_items_query_with(filters, Some(rate_numeric))
}

fn build_items_query_with(
    filters: &ItemFilters,
    rate_numeric: Option<&str>,
) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

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
        let p = emit(Bind::Text(it.clone()), &mut binds, &mut next_idx);
        wheres.push(format!(" item.blockchain_id::text = {} ", p));
    }

    if !filters.ids.is_empty() {
        let p = emit(
            Bind::TextArray(filters.ids.clone()),
            &mut binds,
            &mut next_idx,
        );
        wheres.push(format!(
            " (item.id = ANY ({p}) OR (item.collection_id || '-' || item.blockchain_id::text) = ANY ({p})) ",
            p = p
        ));
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

    if !filters.include_social_emotes {
        wheres.push(" emote.outcome_type IS NULL ".to_string());
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

    let order_by = filters.sort_by.unwrap_or(ItemSortBy::Newest).order_by();

    let price_credits_column = match rate_numeric {
        Some(rate) => {
            let rate_p = emit(Bind::Text(rate.to_string()), &mut binds, &mut next_idx);
            format!(",\n           {}", price_credits_select(&rate_p))
        }
        None => String::new(),
    };

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
           NULL::text as utility{price_credits_column}\n\
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

    #[test]
    fn items_query_carries_no_price_credits_column() {
        let (sql, _) = build_items_query(&ItemFilters::default());
        assert!(!sql.contains("price_credits"), "{sql}");
    }

    #[test]
    fn catalog_items_query_adds_the_asset_type_aware_credit_price() {
        let (sql, binds) =
            build_catalog_items_query(&ItemFilters::default(), "0.020000000000000000");
        assert!(
            sql.contains("ta.asset_type = 2"),
            "USD-pegged trades bypass the rate: {sql}"
        );
        assert!(
            sql.contains(
                "THEN CEIL((unified_trades.assets -> 'received' ->> 'amount')::numeric / 100000000000000000::numeric)"
            ),
            "{sql}"
        );
        assert!(
            sql.contains(
                "ELSE CEIL((unified_trades.assets -> 'received' ->> 'amount')::numeric * $1::numeric / 100000000000000000::numeric)"
            ),
            "{sql}"
        );
        assert!(
            sql.contains("CEIL(item.price::numeric * $1::numeric / 100000000000000000::numeric)"),
            "{sql}"
        );
        assert!(sql.contains("ELSE 0"), "{sql}");
        assert!(sql.contains("END::bigint as price_credits"), "{sql}");
        assert_eq!(bind_texts(&binds), vec!["0.020000000000000000".to_string()]);
    }

    #[test]
    fn catalog_items_query_keeps_the_v1_filters_and_pagination() {
        let filters = ItemFilters {
            first: Some(24),
            creator: vec!["0xCreator".to_string()],
            ..Default::default()
        };
        let (sql, _) = build_catalog_items_query(&filters, "0");
        assert!(sql.contains("LOWER(item.creator) = ANY($"), "{sql}");
        assert!(sql.contains("LIMIT $"), "{sql}");
        assert!(sql.contains("COUNT(*) OVER() as count"), "{sql}");
        assert!(sql.contains("WITH unified_trades AS"), "{sql}");
    }

    #[test]
    fn catalog_items_rate_placeholder_lands_after_filter_binds() {
        let filters = ItemFilters {
            item_id: Some("7".to_string()),
            ..Default::default()
        };
        let (sql, binds) = build_catalog_items_query(&filters, "0.5");
        assert!(sql.contains("item.blockchain_id::text = $1"), "{sql}");
        assert!(sql.contains("* $2::numeric"), "{sql}");
        assert_eq!(bind_texts(&binds), vec!["7".to_string(), "0.5".to_string()]);
    }
}
