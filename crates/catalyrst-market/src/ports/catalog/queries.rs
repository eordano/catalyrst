use sqlx::postgres::PgArguments;

use crate::{BUILDER_SERVER_TABLE_SCHEMA, MARKETPLACE_SQUID_SCHEMA};

use super::sql::{
    build_collections_where, build_get_max_price_case, build_get_max_price_case_with_trades,
    build_get_min_price_case, build_get_min_price_case_with_trades, build_item_level_filters_where,
    build_limit_offset, build_metadata_joins, build_min_item_created_at_cte,
    build_nfts_with_orders_cte_v1, build_nfts_with_orders_cte_v2, build_order_by,
    build_owners_join, build_seen_join, build_top_n_items_cte, build_trades_cte, build_trades_join,
    push_nfts_with_orders_v1_body, Builder,
};
use super::types::{CatalogFilters, CatalogSortBy};
use super::MAX_NUMERIC_NUMBER;

pub fn build_collections_items_catalog_query(f: &CatalogFilters) -> (String, PgArguments) {
    let mut b = Builder::new();
    if two_pass_v1(f) {
        b.push_sql(" WITH nfts_with_orders AS MATERIALIZED ( ");
        push_nfts_with_orders_v1_body(&mut b, f);
        b.push_sql(" ) ");
        build_ranked_cte_v1(&mut b, f);
    }
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
    if two_pass_v1(f) {
        b.push_sql(" LEFT JOIN nfts_with_orders ON nfts_with_orders.item_id = items.id ");
        build_metadata_joins(&mut b);
        build_seen_join(&mut b, f);
        b.push_sql(" JOIN ranked ON ranked.ranked_id = items.id ");
        build_order_by(&mut b, f, false);
    } else {
        build_nfts_with_orders_cte_v1(&mut b, f);
        build_metadata_joins(&mut b);
        build_seen_join(&mut b, f);
        build_collections_where(&mut b, f, false);
        build_order_by(&mut b, f, false);
        build_limit_offset(&mut b, f);
    }

    (b.sql, b.args)
}

fn two_pass_v1(f: &CatalogFilters) -> bool {
    f.first.is_some() || f.skip.is_some()
}

fn ranked_needs_metadata(f: &CatalogFilters) -> bool {
    f.wearable_category.is_some() || f.emote_category.is_some() || f.emote_play_mode.len() == 1
}

fn build_ranked_cte_v1(b: &mut Builder, f: &CatalogFilters) {
    b.push_sql(", ranked AS ( SELECT items.id AS ranked_id, ");
    build_get_min_price_case(b, f);
    b.push_sql(", ");
    build_get_max_price_case(b, f);
    b.push_sql(&format!(
        " FROM {schema}.item AS items LEFT JOIN nfts_with_orders ON nfts_with_orders.item_id = items.id ",
        schema = MARKETPLACE_SQUID_SCHEMA
    ));
    if ranked_needs_metadata(f) {
        build_metadata_joins(b);
    }
    build_seen_join(b, f);
    build_collections_where(b, f, false);
    build_order_by(b, f, false);
    build_limit_offset(b, f);
    b.push_sql(" ) ");
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
    build_seen_join(&mut b, f);
    build_collections_where(&mut b, f, true);
    build_order_by(&mut b, f, true);
    build_limit_offset(&mut b, f);

    (b.sql, b.args)
}

pub fn build_collections_items_count_query(f: &CatalogFilters) -> (String, PgArguments) {
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

pub(super) fn build_search_query(f: &CatalogFilters) -> (String, PgArguments) {
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

#[allow(dead_code)]
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}
