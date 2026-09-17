use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_market::ports::sales::{SalesComponent, SalesSummaryFilters};
use sqlx::PgPool;

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const SELLER: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const COLLECTION_A: &str = "0xaaa0000000000000000000000000000000000001";
const COLLECTION_B: &str = "0xbbb0000000000000000000000000000000000002";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

async fn build_scratch() -> Option<ScratchDb> {
    let scratch = ScratchDb::builder(PG_VAR, "cg_mkt_sales_summary")
        .schemas(["squid_marketplace"])
        .build()
        .await?;
    sqlx::raw_sql(
        "CREATE TABLE squid_marketplace.sale (
            id text PRIMARY KEY,
            type text NOT NULL,
            buyer text,
            seller text,
            item_id text,
            price numeric,
            timestamp numeric,
            search_contract_address text,
            search_item_id numeric
        );
        CREATE TABLE squid_marketplace.item (id text PRIMARY KEY, creator text)",
    )
    .execute(&scratch.pool)
    .await
    .expect("the squid sale and item tables");
    Some(scratch)
}

#[allow(clippy::too_many_arguments)]
async fn seed_sale(
    pool: &PgPool,
    id: &str,
    kind: &str,
    seller: &str,
    price: &str,
    ts_ms: i64,
    contract: &str,
    item_id: Option<i64>,
    item: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO squid_marketplace.sale
         (id, type, buyer, seller, item_id, price, timestamp, search_contract_address, search_item_id)
         VALUES ($1, $2, $3, $4, $5, $6::numeric, $7::numeric / 1000, $8, $9)",
    )
    .bind(id)
    .bind(kind)
    .bind(OTHER)
    .bind(seller)
    .bind(item)
    .bind(price)
    .bind(ts_ms)
    .bind(contract)
    .bind(item_id)
    .execute(pool)
    .await
    .expect("a sale row");
}

async fn seed_item(pool: &PgPool, id: &str, creator: &str) {
    sqlx::query("INSERT INTO squid_marketplace.item (id, creator) VALUES ($1, $2)")
        .bind(id)
        .bind(creator)
        .execute(pool)
        .await
        .expect("an item row");
}

/// Two mints and a resale inside the window, one mint outside it, plus a resale of an item this
/// seller created but did not sell: the royalties leg and the sales leg count different rows.
async fn seed_world(pool: &PgPool) {
    seed_item(pool, "item-a-1", &SELLER.to_uppercase()).await;
    seed_item(pool, "item-b-1", OTHER).await;
    seed_sale(
        pool,
        "s1",
        "mint",
        SELLER,
        "100",
        5 * DAY_MS,
        COLLECTION_A,
        Some(1),
        Some("item-a-1"),
    )
    .await;
    seed_sale(
        pool,
        "s2",
        "mint",
        SELLER,
        "200",
        6 * DAY_MS,
        COLLECTION_A,
        Some(1),
        Some("item-a-1"),
    )
    .await;
    seed_sale(
        pool,
        "s3",
        "order",
        SELLER,
        "50",
        7 * DAY_MS,
        COLLECTION_B,
        Some(2),
        Some("item-b-1"),
    )
    .await;
    seed_sale(
        pool,
        "s4",
        "mint",
        SELLER,
        "900",
        40 * DAY_MS,
        COLLECTION_B,
        Some(3),
        Some("item-b-1"),
    )
    .await;
    seed_sale(
        pool,
        "s5",
        "bid",
        OTHER,
        "70",
        6 * DAY_MS,
        COLLECTION_A,
        Some(1),
        Some("item-a-1"),
    )
    .await;
}

#[tokio::test]
async fn a_windowed_summary_aggregates_the_seller_in_one_request() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    seed_world(&scratch.pool).await;
    let sales = SalesComponent::new(scratch.pool.clone());

    let summary = sales
        .get_summary(&SalesSummaryFilters {
            seller: SELLER.to_string(),
            from: Some(DAY_MS),
            to: Some(10 * DAY_MS),
        })
        .await
        .expect("the summary answers");

    assert_eq!((summary.total, summary.mints, summary.resales), (3, 2, 1));
    assert_eq!(summary.earned_wei, "350");
    assert_eq!(
        summary
            .by_collection
            .iter()
            .map(|c| (c.contract_address.as_str(), c.sold, c.earned_wei.as_str()))
            .collect::<Vec<_>>(),
        vec![(COLLECTION_A, 2, "300"), (COLLECTION_B, 1, "50")]
    );
}

#[tokio::test]
async fn by_item_counts_mints_over_the_seller_lifetime_not_the_window() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    seed_world(&scratch.pool).await;
    let sales = SalesComponent::new(scratch.pool.clone());

    let summary = sales
        .get_summary(&SalesSummaryFilters {
            seller: SELLER.to_string(),
            from: Some(DAY_MS),
            to: Some(10 * DAY_MS),
        })
        .await
        .expect("the summary answers");

    assert_eq!(
        summary
            .by_item
            .iter()
            .map(|i| (
                i.contract_address.as_str(),
                i.item_id.as_str(),
                i.sold_lifetime
            ))
            .collect::<Vec<_>>(),
        vec![(COLLECTION_A, "1", 2), (COLLECTION_B, "3", 1)],
        "the mint outside the window still counts toward the item's lifetime"
    );
}

#[tokio::test]
async fn royalties_follow_the_item_creator_whatever_case_the_indexer_stored() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    seed_world(&scratch.pool).await;
    let sales = SalesComponent::new(scratch.pool.clone());

    let summary = sales
        .get_summary(&SalesSummaryFilters {
            seller: SELLER.to_string(),
            from: Some(DAY_MS),
            to: Some(10 * DAY_MS),
        })
        .await
        .expect("the summary answers");

    assert_eq!(
        (
            summary.royalties.resales,
            summary.royalties.volume_wei.as_str()
        ),
        (1, "70"),
        "the checksummed creator on item-a-1 must still match the lowercased seller"
    );
}

#[tokio::test]
async fn an_unbounded_summary_covers_every_sale_and_an_unknown_seller_is_all_zeroes() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    seed_world(&scratch.pool).await;
    let sales = SalesComponent::new(scratch.pool.clone());

    let all = sales
        .get_summary(&SalesSummaryFilters {
            seller: SELLER.to_uppercase(),
            ..Default::default()
        })
        .await
        .expect("the summary answers");
    assert_eq!((all.total, all.mints, all.resales), (4, 3, 1));
    assert_eq!(all.earned_wei, "1250");

    let none = sales
        .get_summary(&SalesSummaryFilters {
            seller: format!("0x{}", "99".repeat(20)),
            ..Default::default()
        })
        .await
        .expect("an unknown seller still answers");
    assert_eq!((none.total, none.mints, none.resales), (0, 0, 0));
    assert_eq!(none.earned_wei, "0");
    assert!(none.by_collection.is_empty() && none.by_item.is_empty());
    assert_eq!(none.royalties.volume_wei, "0");
}

/// Upstream's summary is a full-window aggregate with no LIMIT of its own. A page's worth of
/// rows would still add up, so the count has to cross any default page size before a stray
/// LIMIT or a `COUNT(*) OVER()` in the CTE could show itself.
async fn seed_bulk(pool: &PgPool, rows: i64) {
    sqlx::query(
        "INSERT INTO squid_marketplace.sale
         (id, type, buyer, seller, item_id, price, timestamp, search_contract_address, search_item_id)
         SELECT 'bulk-' || g, 'mint', $1, $2, NULL, g::numeric, 1000000 + g,
                CASE WHEN g % 2 = 0 THEN $3 ELSE $4 END,
                CASE WHEN g % 2 = 0 THEN 1 ELSE 2 END
         FROM generate_series(1, $5::bigint) g",
    )
    .bind(OTHER)
    .bind(SELLER)
    .bind(COLLECTION_A)
    .bind(COLLECTION_B)
    .bind(rows)
    .execute(pool)
    .await
    .expect("the bulk sale rows");
}

#[tokio::test]
async fn a_summary_aggregates_past_any_page_size() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    const ROWS: i64 = 1200;
    seed_bulk(&scratch.pool, ROWS).await;
    let sales = SalesComponent::new(scratch.pool.clone());

    let summary = sales
        .get_summary(&SalesSummaryFilters {
            seller: SELLER.to_string(),
            ..Default::default()
        })
        .await
        .expect("the summary answers");

    let earned = ROWS * (ROWS + 1) / 2;
    assert_eq!(
        (summary.total, summary.mints, summary.resales),
        (ROWS, ROWS, 0),
        "every row the seller has must be counted, not the first page of them"
    );
    assert_eq!(summary.earned_wei, earned.to_string());
    assert_eq!(
        summary
            .by_collection
            .iter()
            .map(|c| (c.contract_address.as_str(), c.sold, c.earned_wei.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (COLLECTION_A, ROWS / 2, "360600"),
            (COLLECTION_B, ROWS / 2, "360000")
        ]
    );
    assert_eq!(
        summary
            .by_item
            .iter()
            .map(|i| (
                i.contract_address.as_str(),
                i.item_id.as_str(),
                i.sold_lifetime
            ))
            .collect::<Vec<_>>(),
        vec![(COLLECTION_A, "1", ROWS / 2), (COLLECTION_B, "2", ROWS / 2)]
    );
}
