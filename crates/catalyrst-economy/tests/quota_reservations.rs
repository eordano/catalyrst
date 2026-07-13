use std::time::Duration;

use catalyrst_economy::admin::RuntimeConfig;
use catalyrst_economy::http::errors::ApiError;
use catalyrst_economy::ports::transaction::TransactionComponent;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_ECONOMY_TEST_PG").ok()
}

fn unique_schema() -> String {
    format!("test_economy_{}", Uuid::new_v4().simple())
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    for sql in [
        include_str!("../migrations/0001_transactions.sql"),
        include_str!("../migrations/0002_broker_purchases.sql"),
        include_str!("../migrations/0003_escrow_actions.sql"),
        include_str!("../migrations/0004_broker_forward_confirm.sql"),
        include_str!("../migrations/0005_add_reservation_columns.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.expect("migration");
    }

    Some((pool, schema, url))
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn component(pool: PgPool) -> TransactionComponent {
    TransactionComponent::new(pool, None, None, RuntimeConfig::new())
}

async fn row_count(pool: &PgPool, addr: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE user_address = $1")
        .bind(addr.to_lowercase())
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn reserve_then_confirm_promotes_and_is_user_visible() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping reserve_then_confirm_promotes_and_is_user_visible: set CATALYRST_ECONOMY_TEST_PG to run"
        );
        return;
    };
    let tc = component(pool.clone());
    let addr = "0xAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaa";
    let session = Uuid::new_v4().to_string();

    tc.reserve_quota(10, addr, &session).await.expect("reserve");

    let (tx_hash, sid): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT tx_hash, session_id FROM transactions WHERE user_address = $1")
            .bind(addr.to_lowercase())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(tx_hash.is_none(), "a fresh reservation carries no tx_hash");
    assert_eq!(sid.as_deref(), Some(session.as_str()));

    assert!(
        tc.get_by_user_address(addr).await.unwrap().is_empty(),
        "pending reservation must not appear in getByUserAddress"
    );

    tc.confirm_reservation(&session, "0xdeadbeef")
        .await
        .expect("confirm");
    let rows = tc.get_by_user_address(addr).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tx_hash, "0xdeadbeef");

    let sid_after: Option<String> =
        sqlx::query_scalar("SELECT session_id FROM transactions WHERE user_address = $1")
            .bind(addr.to_lowercase())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(sid_after.is_none(), "confirm clears session_id");

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn reserve_enforces_daily_limit_and_release_refunds_a_slot() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping reserve_enforces_daily_limit_and_release_refunds_a_slot: set CATALYRST_ECONOMY_TEST_PG to run"
        );
        return;
    };
    let tc = component(pool.clone());
    let addr = "0xbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBB";
    const MAX: i64 = 3;

    let mut sessions = Vec::new();
    for _ in 0..MAX {
        let s = Uuid::new_v4().to_string();
        tc.reserve_quota(MAX, addr, &s)
            .await
            .expect("reserve within budget");
        sessions.push(s);
    }

    let over = Uuid::new_v4().to_string();
    let err = tc
        .reserve_quota(MAX, addr, &over)
        .await
        .expect_err("over quota");
    assert!(
        matches!(err, ApiError::QuotaReached(_)),
        "expected QuotaReached, got {err:?}"
    );
    assert_eq!(row_count(&pool, addr).await, MAX);

    tc.release_reservation(&sessions[0]).await.expect("release");
    assert_eq!(row_count(&pool, addr).await, MAX - 1);

    let refunded = Uuid::new_v4().to_string();
    tc.reserve_quota(MAX, addr, &refunded)
        .await
        .expect("reserve after release");
    assert_eq!(row_count(&pool, addr).await, MAX);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn reservations_are_isolated_per_user() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping reservations_are_isolated_per_user: set CATALYRST_ECONOMY_TEST_PG to run"
        );
        return;
    };
    let tc = component(pool.clone());
    let alice = "0x1111111111111111111111111111111111111111";
    let bob = "0x2222222222222222222222222222222222222222";
    const MAX: i64 = 2;

    for _ in 0..MAX {
        tc.reserve_quota(MAX, alice, &Uuid::new_v4().to_string())
            .await
            .expect("alice reserve");
    }
    assert!(matches!(
        tc.reserve_quota(MAX, alice, &Uuid::new_v4().to_string())
            .await,
        Err(ApiError::QuotaReached(_))
    ));

    for _ in 0..MAX {
        tc.reserve_quota(MAX, bob, &Uuid::new_v4().to_string())
            .await
            .expect("bob reserve");
    }
    assert_eq!(row_count(&pool, alice).await, MAX);
    assert_eq!(row_count(&pool, bob).await, MAX);

    cleanup(&admin_url, &schema).await;
}
