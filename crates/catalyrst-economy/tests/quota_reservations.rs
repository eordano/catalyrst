use std::time::Duration;

use catalyrst_economy::admin::RuntimeConfig;
use catalyrst_economy::http::errors::ApiError;
use catalyrst_economy::ports::transaction::{MetaTxSender, TransactionComponent};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

mod support;

fn sender(addr: &str) -> MetaTxSender {
    MetaTxSender::from_meta_tx_calldata(addr, &support::split_sig_calldata(addr))
        .expect("calldata signed by the same address it is posted for")
}

fn pg_url() -> Option<String> {
    catalyrst_testgate::require_pg(support::PG_VAR)
}

fn unique_schema() -> String {
    format!("test_economy_{}", Uuid::new_v4().simple())
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return catalyrst_testgate::pg_unusable(
                support::PG_VAR,
                &format!("connect to {url} failed: {e}"),
            )
        }
    };
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .unwrap_or_else(|e| panic!("connect to scratch schema {schema} failed: {e}"));

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
    TransactionComponent::new(pool, None, None, None, None, RuntimeConfig::new())
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
        return;
    };
    let tc = component(pool.clone());
    let addr = "0xAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaaAAaa";
    let session = Uuid::new_v4().to_string();

    tc.reserve_quota(10, &sender(addr), &session)
        .await
        .expect("reserve");

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
        return;
    };
    let tc = component(pool.clone());
    let addr = "0xbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBBbBBB";
    const MAX: i64 = 3;

    let mut sessions = Vec::new();
    for _ in 0..MAX {
        let s = Uuid::new_v4().to_string();
        tc.reserve_quota(MAX, &sender(addr), &s)
            .await
            .expect("reserve within budget");
        sessions.push(s);
    }

    let over = Uuid::new_v4().to_string();
    let err = tc
        .reserve_quota(MAX, &sender(addr), &over)
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
    tc.reserve_quota(MAX, &sender(addr), &refunded)
        .await
        .expect("reserve after release");
    assert_eq!(row_count(&pool, addr).await, MAX);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn concurrent_reservations_never_exceed_the_limit() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        return;
    };
    let tc = std::sync::Arc::new(component(pool.clone()));
    let addr = "0xcCcCcCcCcCcCcCcCcCcCcCcCcCcCcCcCcCcCcCcC";
    const MAX: i64 = 3;
    const ATTEMPTS: i64 = 8;

    let tasks: Vec<_> = (0..ATTEMPTS)
        .map(|_| {
            let tc = tc.clone();
            tokio::spawn(async move {
                tc.reserve_quota(MAX, &sender(addr), &Uuid::new_v4().to_string())
                    .await
            })
        })
        .collect();

    let (mut reserved, mut refused) = (0, 0);
    for task in tasks {
        match task.await.expect("task") {
            Ok(()) => reserved += 1,
            Err(ApiError::QuotaReached(msg)) => {
                assert!(msg.ends_with(&format!("Quota: {MAX}")), "{msg}");
                refused += 1;
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
    assert_eq!(
        reserved, MAX,
        "exactly MAX reservations land under contention"
    );
    assert_eq!(refused, ATTEMPTS - MAX);
    assert_eq!(row_count(&pool, addr).await, MAX);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn reservation_refuses_a_non_canonical_session_id() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        return;
    };
    let tc = component(pool.clone());
    let addr = "0xdDdDdDdDdDdDdDdDdDdDdDdDdDdDdDdDdDdDdDdD";

    let err = tc
        .reserve_quota(10, &sender(addr), "x'; DELETE FROM transactions; --")
        .await
        .expect_err("a session id outside [A-Za-z0-9-] is refused before any SQL runs");
    assert!(matches!(err, ApiError::Internal(_)), "{err:?}");
    assert_eq!(row_count(&pool, addr).await, 0);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn reservations_are_isolated_per_user() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        return;
    };
    let tc = component(pool.clone());
    let alice = "0x1111111111111111111111111111111111111111";
    let bob = "0x2222222222222222222222222222222222222222";
    const MAX: i64 = 2;

    for _ in 0..MAX {
        tc.reserve_quota(MAX, &sender(alice), &Uuid::new_v4().to_string())
            .await
            .expect("alice reserve");
    }
    assert!(matches!(
        tc.reserve_quota(MAX, &sender(alice), &Uuid::new_v4().to_string())
            .await,
        Err(ApiError::QuotaReached(_))
    ));

    for _ in 0..MAX {
        tc.reserve_quota(MAX, &sender(bob), &Uuid::new_v4().to_string())
            .await
            .expect("bob reserve");
    }
    assert_eq!(row_count(&pool, alice).await, MAX);
    assert_eq!(row_count(&pool, bob).await, MAX);

    cleanup(&admin_url, &schema).await;
}
