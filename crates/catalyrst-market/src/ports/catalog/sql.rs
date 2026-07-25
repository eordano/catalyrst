use sqlx::postgres::PgArguments;
use sqlx::Arguments;

use crate::dcl_schemas::{Network, NftCategory};
use crate::logic::sql_filters::{clamp_first, clamp_skip, MAX_PAGE_LIMIT};
use crate::MARKETPLACE_SQUID_SCHEMA;

use super::types::{CatalogFilters, CatalogSortBy, CatalogSortDirection};
use super::{
    FRAGMENT_EMOTE_V1, FRAGMENT_SMART_WEARABLE_V1, MAX_NUMERIC_NUMBER, MAX_ORDER_TIMESTAMP,
    WEARABLE_ITEM_TYPES,
};

pub(super) struct Builder {
    pub(super) sql: String,
    pub(super) args: PgArguments,
    next_index: usize,
}

impl Builder {
    pub(super) fn new() -> Self {
        Self {
            sql: String::new(),
            args: PgArguments::default(),
            next_index: 1,
        }
    }

    pub(super) fn push_sql(&mut self, s: &str) -> &mut Self {
        self.sql.push_str(s);
        self
    }

    pub(super) fn bind_string(&mut self, v: String) -> usize {
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

fn build_item_id_where(b: &mut Builder, f: &CatalogFilters) {
    let bi = b.bind_string(f.item_id.clone().unwrap_or_default());
    b.push_sql(&format!("items.blockchain_id::text = ${}", bi));
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
    b.push_sql(&format!(
        "(items.id = ANY(${bi}) OR (items.collection_id || '-' || items.blockchain_id::text) = ANY(${bi}))"
    ));
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

pub(super) fn build_collections_where(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
    b.push_sql(" WHERE items.search_is_collection_approved = true ");
    if f.include_social_emotes == Some(false) {
        b.push_sql(" AND items.search_emote_outcome_type IS NULL ");
    }

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
    if f.item_id.as_deref().is_some_and(|s| !s.is_empty()) {
        and_sep(b, &mut first);
        build_item_id_where(b, f);
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

pub(super) fn build_order_by(b: &mut Builder, f: &CatalogFilters, is_v2: bool) {
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
        CatalogSortBy::Newest => {
            if is_v2 {
                b.push_sql(
                    "GREATEST(COALESCE(ROUND(EXTRACT(EPOCH FROM offchain_orders.item_first_listed_at)), 0), first_listed_at) desc nulls LAST",
                )
            } else {
                b.push_sql("items.first_listed_at desc nulls LAST")
            }
        }
        CatalogSortBy::MostExpensive => b.push_sql("max_price_num desc"),
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
        CatalogSortBy::RecentlySold => b.push_sql("items.sold_at desc"),
        CatalogSortBy::Cheapest => b.push_sql("min_price_num asc, items.first_listed_at desc"),
        CatalogSortBy::Suggested => b.push_sql(
            "COALESCE(EXTRACT(EPOCH FROM seen.last_seen), 0) desc, items.first_listed_at desc nulls LAST",
        ),
    };

    let _ = sort_direction;
    b.push_sql(" ");
}

pub(super) fn build_limit_offset(b: &mut Builder, f: &CatalogFilters) {
    if f.first.is_some() || f.skip.is_some() {
        let li = b.bind_i64(clamp_first(f.first, MAX_PAGE_LIMIT));
        let oi = b.bind_i64(clamp_skip(f.skip));
        b.push_sql(&format!("LIMIT ${} OFFSET ${}", li, oi));
    }
}

pub(super) fn build_seen_join(b: &mut Builder, f: &CatalogFilters) {
    if matches!(f.sort_by, Some(CatalogSortBy::Suggested)) {
        b.push_sql(" LEFT JOIN marketplace.wearable_last_seen AS seen ON seen.urn = items.urn ");
    }
}

pub(super) fn build_metadata_joins(b: &mut Builder) {
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

pub(super) fn build_owners_join(b: &mut Builder) {
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

pub(super) fn push_nfts_with_orders_v1_body(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(&format!(
        " SELECT
                orders.item_id,
                COUNT(orders.id) AS listings_count,
                MIN(orders.price) AS min_price,
                MAX(orders.price) AS max_price,
                MAX(orders.created_at) AS max_order_created_at
            FROM {schema}.\"order\" AS orders
            WHERE orders.status = 'open' AND orders.item_id IS NOT NULL AND orders.expires_at < {ts}
                AND ((LENGTH(orders.expires_at::text) = 13 AND TO_TIMESTAMP(orders.expires_at / 1000.0) > NOW())
                  OR (LENGTH(orders.expires_at::text) = 10 AND TO_TIMESTAMP(orders.expires_at) > NOW()))",
        schema = MARKETPLACE_SQUID_SCHEMA,
        ts = MAX_ORDER_TIMESTAMP,
    ));
    build_order_range_price_where(b, f);
    b.push_sql(" GROUP BY orders.item_id ");
}

pub(super) fn build_nfts_with_orders_cte_v1(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(" LEFT JOIN ( ");
    push_nfts_with_orders_v1_body(b, f);
    b.push_sql(" ) AS nfts_with_orders ON nfts_with_orders.item_id = items.id ");
}

pub(super) fn build_nfts_with_orders_cte_v2(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(&format!(
        ", nfts_with_orders AS (SELECT orders.item_id, COUNT(orders.id) AS orders_listings_count, MIN(orders.price) AS min_price, MAX(orders.price) AS max_price, MAX(orders.created_at) AS max_order_created_at FROM {schema}.\"order\" AS orders WHERE orders.status = 'open' AND orders.item_id IS NOT NULL AND orders.expires_at_normalized > NOW()",
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

pub(super) fn build_trades_cte(b: &mut Builder) {
    b.push_sql(" WITH unified_trades AS ( SELECT * FROM marketplace.mv_trades ) ");
}

pub(super) fn build_top_n_items_cte(b: &mut Builder, f: &CatalogFilters) {
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

pub(super) fn build_min_item_created_at_cte(b: &mut Builder) {
    b.push_sql(
        ", ut_min_item AS (SELECT contract_address_sent, (assets -> 'sent' ->> 'item_id') AS item_id, MIN(created_at) AS min_item_created_at FROM unified_trades WHERE type = 'public_item_order' GROUP BY contract_address_sent, (assets -> 'sent' ->> 'item_id'))",
    );
}

pub(super) fn build_trades_join(b: &mut Builder, f: &CatalogFilters) {
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

pub(super) fn build_item_level_filters_where(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(" WHERE items.search_is_collection_approved = true ");
    if f.include_social_emotes == Some(false) {
        b.push_sql(" AND items.search_emote_outcome_type IS NULL ");
    }
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
    if f.item_id.as_deref().is_some_and(|s| !s.is_empty()) {
        and_sep(b, &mut first);
        build_item_id_where(b, f);
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

pub(super) fn build_get_min_price_case(b: &mut Builder, f: &CatalogFilters) {
    let mut expr = String::from(
        " (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ",
    );
    if let Some(mn) = &f.min_price {
        let bi = b.bind_string(mn.clone());
        expr.push_str(&format!(" AND items.price >= ${}", bi));
    }
    expr.push_str(
        " THEN LEAST(items.price, nfts_with_orders.min_price) ELSE nfts_with_orders.min_price END)",
    );
    b.push_sql(&format!(
        "{expr} AS min_price_num, {expr}::text AS min_price "
    ));
}

pub(super) fn build_get_max_price_case(b: &mut Builder, f: &CatalogFilters) {
    if f.only_minting {
        let mut expr = String::from(
            " (CASE WHEN items.available > 0 AND items.search_is_store_minter = true ",
        );
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            expr.push_str(&format!(" AND items.price <= ${}", bi));
        }
        expr.push_str(" THEN items.price ELSE NULL END)");
        b.push_sql(&format!(
            "{expr} AS max_price_num, {expr}::text AS max_price "
        ));
    } else {
        let mut expr = String::from(
            " (CASE WHEN items.available > 0 AND items.search_is_store_minter = true ",
        );
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            expr.push_str(&format!(" AND items.price <= ${}", bi));
        }
        expr.push_str(
            " THEN GREATEST(items.price, nfts_with_orders.max_price) ELSE nfts_with_orders.max_price END)",
        );
        b.push_sql(&format!(
            "{expr} AS max_price_num, {expr}::text AS max_price "
        ));
    }
}

pub(super) fn build_get_min_price_case_with_trades(b: &mut Builder, f: &CatalogFilters) {
    let mut expr = String::from(
        " (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ",
    );
    if let Some(mn) = &f.min_price {
        let bi = b.bind_string(mn.clone());
        expr.push_str(&format!(" AND items.price >= ${}", bi));
    }
    if f.only_minting {
        expr.push_str(&format!(
            " THEN LEAST(COALESCE(items.price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) ELSE LEAST(COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) END)",
            n = MAX_NUMERIC_NUMBER,
        ));
    } else {
        expr.push_str(&format!(
            " THEN LEAST(COALESCE(items.price, '{n}'), COALESCE(nfts_with_orders.min_price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) ELSE LEAST(COALESCE(nfts_with_orders.min_price, '{n}'), COALESCE(offchain_orders.min_order_amount_received, '{n}'), COALESCE(offchain_orders.open_item_trade_price, '{n}')) END)",
            n = MAX_NUMERIC_NUMBER,
        ));
    }
    b.push_sql(&format!(
        "{expr} AS min_price_num, {expr}::text AS min_price "
    ));
}

pub(super) fn build_get_max_price_case_with_trades(b: &mut Builder, f: &CatalogFilters) {
    if f.only_minting {
        let mut expr = String::from(" (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            expr.push_str(&format!(" AND items.price <= ${}", bi));
        }
        expr.push_str(" THEN GREATEST(items.price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) ELSE GREATEST(offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) END)");
        b.push_sql(&format!(
            "{expr} AS max_price_num, {expr}::text AS max_price "
        ));
    } else {
        let mut expr = String::from(" (CASE WHEN items.available > 0 AND (items.search_is_store_minter = true OR items.search_is_marketplace_v3_minter = true) ");
        if let Some(mx) = &f.max_price {
            let bi = b.bind_string(mx.clone());
            expr.push_str(&format!(" AND items.price <= ${}", bi));
        }
        expr.push_str(" THEN GREATEST(items.price, nfts_with_orders.max_price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) ELSE GREATEST(nfts_with_orders.max_price, offchain_orders.max_order_amount_received, offchain_orders.open_item_trade_price) END)");
        b.push_sql(&format!(
            "{expr} AS max_price_num, {expr}::text AS max_price "
        ));
    }
}
