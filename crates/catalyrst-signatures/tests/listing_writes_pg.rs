//! Create and refresh each run as one statement; these pin the row shape the
//! multi-statement version produced. DB-gated via `CATALYRST_TEST_PG`.

use std::str::FromStr;

use catalyrst_signatures::db::Database;
use catalyrst_signatures::types::{RentalListingCreation, RentalListingPeriodInput};
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;

const LESSOR: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";

static SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn scratch(name: &str) -> Option<(PgPool, String)> {
    let Ok(url) = std::env::var("CATALYRST_TEST_PG") else {
        eprintln!("CATALYRST_TEST_PG unset; skipping");
        return None;
    };
    let _serialized = SETUP.lock().await;
    let schema = format!("signatures_rt_{name}_{}", std::process::id());
    let admin = PgPool::connect(&url).await.expect("connect scratch pg");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\" WITH SCHEMA public")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let opts = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", format!("{schema},public").as_str())]);
    let pool = PgPool::connect_with(opts).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    Some((pool, schema))
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(pool)
        .await
        .unwrap();
}

fn creation(token_id: &str) -> RentalListingCreation {
    RentalListingCreation {
        network: "ETHEREUM".into(),
        chain_id: 1,
        expiration: 4_102_444_800_000,
        contract_address: "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".into(),
        token_id: token_id.into(),
        nonces: vec!["0".into(), "1".into(), "2".into()],
        periods: vec![
            RentalListingPeriodInput {
                min_days: 30,
                max_days: 60,
                price_per_day: "2000000000000000000".into(),
            },
            RentalListingPeriodInput {
                min_days: 7,
                max_days: 14,
                price_per_day: "1000000000000000000".into(),
            },
        ],
        rental_contract_address: "0x92159c78f0f4523b9c60382bb888f30f10a46b3b".into(),
        signature: "0xsig".into(),
        target: "0x0000000000000000000000000000000000000000".into(),
    }
}

async fn insert(
    db: &Database,
    token_id: &str,
) -> Result<catalyrst_signatures::types::RentalListing, sqlx::Error> {
    let nft_id = format!("0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d-{token_id}");
    let now = chrono::Utc::now().naive_utc();
    db.insert_listing(
        &nft_id,
        "parcel",
        "-10,20",
        Some(3),
        Some(true),
        None,
        now,
        now,
        &creation(token_id),
        LESSOR,
    )
    .await
}

#[tokio::test]
async fn insert_listing_lands_all_rows_in_one_statement() {
    let Some((pool, schema)) = scratch("insert").await else {
        return;
    };
    let db = Database::new(pool.clone());

    let listing = insert(&db, "42").await.unwrap();
    assert_eq!(
        listing.nft_id,
        "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d-42"
    );
    assert_eq!(listing.category, "parcel");
    assert_eq!(listing.search_text, "-10,20");
    assert_eq!(listing.lessor.as_deref(), Some(LESSOR));
    assert_eq!(listing.tenant, None);
    assert_eq!(listing.status, "open");
    assert_eq!(listing.chain_id, 1);
    assert_eq!(listing.expiration, 4_102_444_800_000);
    assert_eq!(listing.nonces, vec!["0", "1", "2"]);
    assert_eq!(listing.token_id, "42");
    assert!(listing.created_at > 0 && listing.updated_at > 0);
    let periods: Vec<(i64, i64, &str)> = listing
        .periods
        .iter()
        .map(|p| (p.min_days, p.max_days, p.price_per_day.as_str()))
        .collect();
    assert_eq!(
        periods,
        vec![
            (7, 14, "1000000000000000000"),
            (30, 60, "2000000000000000000")
        ]
    );

    let same = db.get_listing_by_id(&listing.id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&same).unwrap(),
        serde_json::to_value(&listing).unwrap()
    );

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM metadata), (SELECT count(*) FROM rentals), \
                (SELECT count(*) FROM rentals_listings), (SELECT count(*) FROM periods)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 1, 2));

    let err = insert(&db, "42").await.unwrap_err();
    assert!(Database::is_open_conflict(&err), "{err}");
    let rentals: i64 = sqlx::query_scalar("SELECT count(*) FROM rentals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rentals, 1, "a conflicting insert rolls back every row");

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn refresh_updates_metadata_and_returns_the_listing() {
    let Some((pool, schema)) = scratch("refresh").await else {
        return;
    };
    let db = Database::new(pool.clone());
    let listing = insert(&db, "7").await.unwrap();

    assert_eq!(
        db.listing_asset(&listing.id).await.unwrap(),
        Some((
            "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".to_string(),
            "7".to_string()
        ))
    );
    assert_eq!(db.listing_asset("not-a-uuid").await.unwrap(), None);
    assert_eq!(
        db.listing_asset(&uuid::Uuid::new_v4().to_string())
            .await
            .unwrap(),
        None
    );

    let later = chrono::Utc::now().naive_utc();
    let refreshed = db
        .refresh_metadata_for_rental(
            &listing.id,
            "estate",
            "Big estate",
            Some(1),
            Some(false),
            Some(4),
            later,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.id, listing.id);
    assert_eq!(refreshed.category, "estate");
    assert_eq!(refreshed.search_text, "Big estate");
    assert_eq!(refreshed.periods.len(), 2);
    assert_eq!(refreshed.lessor.as_deref(), Some(LESSOR));

    let stored = db.get_listing_by_id(&listing.id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::to_value(&refreshed).unwrap()
    );
    let (distance, adjacent, size): (Option<i16>, Option<bool>, Option<i16>) =
        sqlx::query_as("SELECT distance_to_plaza, adjacent_to_road, estate_size FROM metadata")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((distance, adjacent, size), (Some(1), Some(false), Some(4)));

    assert!(db
        .refresh_metadata_for_rental(
            &uuid::Uuid::new_v4().to_string(),
            "estate",
            "x",
            None,
            None,
            None,
            later
        )
        .await
        .unwrap()
        .is_none());

    drop_schema(&pool, &schema).await;
}
