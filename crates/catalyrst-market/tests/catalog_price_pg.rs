use catalyrst_market::ports::catalog::{
    build_collections_items_catalog_query, build_collections_items_catalog_query_with_trades,
    build_collections_items_count_query, CatalogFilters, CatalogSortBy,
};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn price_bounds_prepare_against_numeric_catalog_columns() {
    let Some(url) = catalyrst_testgate::require_pg("CATALYRST_MARKET_TEST_PG") else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only = on")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("catalog PostgreSQL connection");
    for (min_price, max_price) in [
        (Some("1"), None),
        (None, Some("1000000000000000000")),
        (Some("1"), Some("1000000000000000000")),
    ] {
        for (only_minting, only_listing) in [(false, false), (true, false), (false, true)] {
            let filters = CatalogFilters {
                first: Some(12),
                is_on_sale: Some(true),
                sort_by: Some(CatalogSortBy::RecentlyListed),
                min_price: min_price.map(str::to_owned),
                max_price: max_price.map(str::to_owned),
                only_minting,
                only_listing,
                ..Default::default()
            };
            for (sql, args) in [
                build_collections_items_catalog_query(&filters),
                build_collections_items_catalog_query_with_trades(&filters),
                build_collections_items_count_query(&filters),
            ] {
                sqlx::query_with(sqlx::AssertSqlSafe(format!("EXPLAIN {sql}")), args)
                    .fetch_all(&pool)
                    .await
                    .expect("price-filter SQL must type-check with string-bound amounts");
            }
        }
    }
    pool.close().await;
}
