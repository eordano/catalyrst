use catalyrst_market::ports::catalog::{build_collections_items_catalog_query, CatalogFilters};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Duration;

#[tokio::test]
async fn legacy_expiry_filter_matches_indexed_normalized_filter() {
    let Some(url) = catalyrst_testgate::require_pg("CATALYRST_MARKET_TEST_PG") else {
        return;
    };
    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(err) => {
            let _: Option<()> =
                catalyrst_testgate::pg_unusable("CATALYRST_MARKET_TEST_PG", &err.to_string());
            return;
        }
    };
    let (generated, _) = build_collections_items_catalog_query(&CatalogFilters::default());
    let start = generated
        .find("WHERE orders.status")
        .expect("v1 order filter")
        + "WHERE ".len();
    let end = generated[start..]
        .find("GROUP BY orders.item_id")
        .expect("v1 order aggregation")
        + start;
    let predicate = &generated[start..end];
    let sql = format!(
        r#"WITH vals(expires_at, expires_at_normalized) AS (
             VALUES
               (NULL::numeric, NULL::timestamptz),
               (0, to_timestamp(0)),
               (-1, to_timestamp(-1)),
               (1700000000, now() - interval '1 day'),
               (1700000000000, now() - interval '1 day'),
               (10000000000, now() + interval '1 day'),
               (100000000000, now() + interval '1 day'),
               (10000000000000, now() + interval '1 day'),
               (253378408746999, now() + interval '1 day'),
               (253378408747000, now() + interval '1 day'),
               (9999999999, now() + interval '1 day'),
               (9999999999999, now() + interval '1 day')
           ), orders AS (SELECT vals.*, 'open'::text AS status, 'fixture'::text AS item_id FROM vals), checks AS (
             SELECT
               ((expires_at < 253378408747000
                 AND ((length(expires_at::text) = 13 AND to_timestamp(expires_at / 1000.0) > now())
                   OR (length(expires_at::text) = 10 AND to_timestamp(expires_at) > now())))) AS legacy,
               ({predicate}) AS indexed
             FROM orders
           )
           SELECT bool_and(legacy IS NOT DISTINCT FROM indexed) AS equivalent FROM checks"#,
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .fetch_one(&pool)
        .await
        .expect("expiry compatibility query");
    assert!(row.try_get::<bool, _>("equivalent").unwrap());
    pool.close().await;
}
