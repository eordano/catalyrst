use catalyrst_credits::ports::credits::CreditsComponent;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn scratch_wallet() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    let pid = std::process::id() as u64;
    format!("0xtest{:016x}{:016x}0000", nanos, pid)
        .chars()
        .take(42)
        .collect()
}

async fn pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("CREDITS_TEST_PG_CONNECTION_STRING").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("test PG unreachable");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("test PG migrations failed");
    Some(pool)
}

async fn seed(pool: &sqlx::PgPool, addr: &str, earned: f64, paid: f64, expires_days: i32) {
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available, earned_expires_at) \
         VALUES ($1, $2::numeric + $3::numeric, $2::numeric, now() + make_interval(days => $4))",
    )
    .bind(addr)
    .bind(earned)
    .bind(paid)
    .bind(expires_days)
    .execute(pool)
    .await
    .unwrap();
    for (kind, bucket, amt) in [("claim", "earned", earned), ("grant", "paid", paid)] {
        if amt > 0.0 {
            sqlx::query(
                "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
                 VALUES ($1, $2, $3::numeric, $4, FALSE)",
            )
            .bind(addr)
            .bind(kind)
            .bind(amt)
            .bind(bucket)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn balances(pool: &sqlx::PgPool, addr: &str) -> (f64, f64) {
    let row: (f64, f64) = sqlx::query_as(
        "SELECT available::float8, earned_available::float8 FROM user_credits WHERE address = $1",
    )
    .bind(addr)
    .fetch_one(pool)
    .await
    .unwrap();
    row
}

async fn ledger(pool: &sqlx::PgPool, addr: &str, kind: &str) -> Vec<(String, f64)> {
    sqlx::query_as::<_, (String, f64)>(
        "SELECT bucket, amount::float8 FROM credit_ledger \
         WHERE address = $1 AND kind = $2 ORDER BY bucket",
    )
    .bind(addr)
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str) {
    sqlx::query("DELETE FROM credit_ledger WHERE address = $1")
        .bind(addr)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_credits WHERE address = $1")
        .bind(addr)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn spend_debits_earned_first_and_refund_restores_split() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 20.0, 15.0, 30).await;
    let credits = CreditsComponent::new(pool.clone());

    credits
        .spend(&addr, "25", "test:bucket-spend", None)
        .await
        .unwrap();
    assert_eq!(balances(&pool, &addr).await, (10.0, 0.0));
    assert_eq!(
        ledger(&pool, &addr, "spend").await,
        vec![("earned".into(), 20.0), ("paid".into(), 5.0)]
    );

    credits
        .refund(&addr, "25", "test:bucket-spend", None)
        .await
        .unwrap();
    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!(avail, 35.0);
    assert_eq!(earned, 20.0);
    assert_eq!(
        ledger(&pool, &addr, "refund").await,
        vec![("earned".into(), 20.0), ("paid".into(), 5.0)]
    );

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn expired_earned_is_not_spendable_and_sweeps() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 20.0, 15.0, -1).await;
    let credits = CreditsComponent::new(pool.clone());

    let err = credits
        .spend(&addr, "16", "test:bucket-expired", None)
        .await;
    assert!(err.is_err(), "expired earned credits were spendable");

    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!((avail, earned), (35.0, 20.0));

    credits.sweep_expired_earned().await.unwrap();
    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!((avail, earned), (15.0, 0.0));
    assert_eq!(
        ledger(&pool, &addr, "expire").await,
        vec![("earned".into(), 20.0)]
    );

    credits
        .spend(&addr, "15", "test:bucket-paid", None)
        .await
        .unwrap();
    assert_eq!(balances(&pool, &addr).await, (0.0, 0.0));

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn sweeper_expires_untouched_wallets() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 7.0, 3.0, -1).await;
    let credits = CreditsComponent::new(pool.clone());

    let swept = credits.sweep_expired_earned().await.unwrap();
    assert!(swept >= 1);
    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!((avail, earned), (3.0, 0.0));
    assert_eq!(
        ledger(&pool, &addr, "expire").await,
        vec![("earned".into(), 7.0)]
    );

    cleanup(&pool, &addr).await;
}
