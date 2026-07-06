use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::{PgPool, Postgres, Row};

use crate::http::response::ApiError;

use super::queries::{
    build_collections_items_catalog_query, build_collections_items_catalog_query_with_trades,
    build_collections_items_count_query, build_search_query,
};
use super::rows::from_db_row_to_catalog_item;
use super::types::{CatalogFilters, CatalogItem, DbRow};

pub struct CatalogComponent {
    pool: PgPool,
    cache: std::sync::Arc<super::catalog_cache::CatalogCache>,
}

impl CatalogComponent {
    pub fn new(pool: PgPool) -> Self {
        let cache = std::sync::Arc::new(super::catalog_cache::CatalogCache::from_env());
        super::catalog_cache::spawn_invalidation_listener(pool.clone(), cache.clone());
        Self { pool, cache }
    }

    pub async fn fetch(
        &self,
        filters: CatalogFilters,
        search_id: &str,
        anon_id: &str,
        is_v2: bool,
    ) -> Result<(Vec<CatalogItem>, i64), ApiError> {
        let key = (is_v2, filters.clone());
        if let Some(page) = self.cache.lookup(&key) {
            return Ok((page.0.clone(), page.1));
        }
        let (items, total) = self
            .fetch_uncached(filters, search_id, anon_id, is_v2)
            .await?;
        self.cache
            .store(key, std::sync::Arc::new((items.clone(), total)));
        Ok((items, total))
    }

    async fn fetch_uncached(
        &self,
        mut filters: CatalogFilters,
        _search_id: &str,
        _anon_id: &str,
        is_v2: bool,
    ) -> Result<(Vec<CatalogItem>, i64), ApiError> {
        if let Some(ref search) = filters.search.clone() {
            if !search.trim().is_empty() {
                let (sql, args) = build_search_query(&filters);
                let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), args)
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

        let items_q: Query<'_, Postgres, PgArguments> =
            sqlx::query_with(sqlx::AssertSqlSafe(items_sql), items_args);
        let count_q: Query<'_, Postgres, PgArguments> =
            sqlx::query_with(sqlx::AssertSqlSafe(count_sql), count_args);

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
