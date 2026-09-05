use catalyrst_credits::ports::credits::CreditsComponent;

mod common;

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
    let url = catalyrst_testgate::require_pg("CREDITS_TEST_PG_CONNECTION_STRING")?;
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

async fn seed(pool: &sqlx::PgPool, addr: &str, earned: f64, paid: f64) {
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) \
         VALUES ($1, $2::numeric + $3::numeric, $2::numeric)",
    )
    .bind(addr)
    .bind(earned)
    .bind(paid)
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

async fn earned_expiry(pool: &sqlx::PgPool, addr: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT earned_expires_at FROM user_credits WHERE address = $1",
    )
    .bind(addr)
    .fetch_one(pool)
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
    seed(&pool, &addr, 20.0, 15.0).await;
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
    assert_eq!(
        earned_expiry(&pool, &addr).await,
        None,
        "earned credits never expire: refunds restore the split with NULL expiry"
    );

    let second = credits
        .refund(&addr, "25", "test:bucket-spend", None)
        .await
        .unwrap();
    assert_eq!(second.applied.parse::<f64>().unwrap(), 0.0);
    assert_eq!(balances(&pool, &addr).await, (35.0, 20.0));
    assert_eq!(
        ledger(&pool, &addr, "refund").await,
        vec![("earned".into(), 20.0), ("paid".into(), 5.0)]
    );

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn partial_refund_does_not_over_restore_earned() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 20.0, 15.0).await;
    let credits = CreditsComponent::new(pool.clone());

    credits
        .spend(&addr, "25", "test:bucket-partial", None)
        .await
        .unwrap();
    assert_eq!(balances(&pool, &addr).await, (10.0, 0.0));

    credits
        .refund(&addr, "3", "test:bucket-partial", None)
        .await
        .unwrap();
    assert_eq!(balances(&pool, &addr).await, (13.0, 3.0));

    credits
        .refund(&addr, "22", "test:bucket-partial", None)
        .await
        .unwrap();
    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!(avail, 35.0);
    assert_eq!(earned, 20.0);

    let sums: Vec<(String, f64)> = sqlx::query_as(
        "SELECT bucket, SUM(amount)::float8 FROM credit_ledger \
         WHERE address = $1 AND kind = 'refund' GROUP BY bucket ORDER BY bucket",
    )
    .bind(&addr)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(sums, vec![("earned".into(), 20.0), ("paid".into(), 5.0)]);
    assert_eq!(earned_expiry(&pool, &addr).await, None);

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn expired_history_wallet_refunds_normally_without_expiry() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    // A wallet whose earned bucket was historically expired under the old
    // seasons regime: the balance holds only paid credits, but the ledger
    // carries claim/expire history. Historical 'expire' rows stay valid;
    // refunds work normally and no expiry is ever set again.
    seed(&pool, &addr, 0.0, 15.0).await;
    for (kind, amt) in [("claim", 20.0), ("expire", 20.0)] {
        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
             VALUES ($1, $2, $3::numeric, 'season-expiry', 'earned', FALSE)",
        )
        .bind(&addr)
        .bind(kind)
        .bind(amt)
        .execute(&pool)
        .await
        .unwrap();
    }
    let credits = CreditsComponent::new(pool.clone());

    // A refund under a tx_ref that carries NO spend rows restores nothing.
    let outcome = credits
        .refund(&addr, "5", "test:bucket-no-spend", None)
        .await
        .unwrap();
    assert_eq!(
        outcome.applied.parse::<f64>().unwrap(),
        0.0,
        "a refund is bounded by what was spent under its tx_ref"
    );
    assert_eq!(balances(&pool, &addr).await, (15.0, 0.0));
    assert!(ledger(&pool, &addr, "refund").await.is_empty());

    // The expiry behaviour this test exists for: a wallet with historical
    // claim/expire rows refunds a REAL spend normally and never re-acquires an
    // expiry.
    credits
        .spend(&addr, "5", "test:bucket-historic-spend", None)
        .await
        .unwrap();
    let outcome = credits
        .refund(&addr, "5", "test:bucket-historic-spend", None)
        .await
        .unwrap();
    assert_eq!(outcome.applied.parse::<f64>().unwrap(), 5.0);
    let (avail, earned) = balances(&pool, &addr).await;
    assert_eq!(avail, 15.0);
    assert_eq!(earned, 0.0);
    assert_eq!(earned_expiry(&pool, &addr).await, None);
    assert_eq!(
        ledger(&pool, &addr, "refund").await,
        vec![("paid".into(), 5.0)]
    );

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn refund_rejects_invalid_amounts() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    let credits = CreditsComponent::new(pool.clone());

    for bad in ["-5", "NaN", "1e400", "0"] {
        let err = credits
            .refund(&addr, bad, "test:bucket-bad-amount", None)
            .await
            .expect_err("invalid refund amount must be rejected");
        assert_eq!(common::status_of(err), 400, "for amount {bad:?}");
    }
}

#[tokio::test]
async fn manual_checkout_refund_closes_fulfilling_checkout() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 20.0, 15.0).await;
    let credits = CreditsComponent::new(pool.clone());

    let checkout_id: i64 = sqlx::query_scalar(
        "INSERT INTO checkouts (idempotency_key, address, total_credits, status) \
         VALUES ($1, $2, 25, 'fulfilling') RETURNING id",
    )
    .bind(format!("test-close-{}", addr))
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    let tx_ref = format!("checkout:{}", checkout_id);
    credits.spend(&addr, "25", &tx_ref, None).await.unwrap();

    let (outcome, closed) = credits
        .refund_checkout_manual(checkout_id, &addr, "25")
        .await
        .unwrap();
    assert!(closed, "fulfilling checkout must transition on refund");
    assert_eq!(outcome.applied.parse::<f64>().unwrap(), 25.0);
    let status: String = sqlx::query_scalar("SELECT status FROM checkouts WHERE id = $1")
        .bind(checkout_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(balances(&pool, &addr).await, (35.0, 20.0));

    let (replay, closed_again) = credits
        .refund_checkout_manual(checkout_id, &addr, "25")
        .await
        .unwrap();
    assert!(replay.replayed);
    assert!(!closed_again);
    assert_eq!(balances(&pool, &addr).await, (35.0, 20.0));

    sqlx::query("DELETE FROM credit_refund_idempotency WHERE address = $1")
        .bind(&addr)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM checkouts WHERE id = $1")
        .bind(checkout_id)
        .execute(&pool)
        .await
        .unwrap();
    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn reclaim_line_lookup_resolves_checkout_and_matches_earned_window() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    seed(&pool, &addr, 20.0, 15.0).await;
    let credits = CreditsComponent::new(pool.clone());

    let checkout_id: i64 = sqlx::query_scalar(
        "INSERT INTO checkouts (idempotency_key, address, total_credits, status) \
         VALUES ($1, $2, 25, 'fulfilled') RETURNING id",
    )
    .bind(format!("test-reclaim-{}", addr))
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    let escrow_ref = format!("escrow-{}", addr);
    sqlx::query(
        "INSERT INTO fulfillment_outbox \
             (checkout_id, item_id, urn, unit_price_credits, mode, status, external_ref) \
         VALUES ($1, 'item-1', 'urn:test', 25, 'secondary', 'confirmed', $2)",
    )
    .bind(checkout_id)
    .bind(&escrow_ref)
    .execute(&pool)
    .await
    .unwrap();
    let tx_ref = format!("checkout:{}", checkout_id);
    credits.spend(&addr, "25", &tx_ref, None).await.unwrap();
    assert_eq!(balances(&pool, &addr).await, (10.0, 0.0));

    let (found_id, found_addr, found_amount) = credits
        .find_confirmed_line_by_ref(&escrow_ref)
        .await
        .unwrap()
        .expect("confirmed line must resolve");
    assert_eq!(found_id, checkout_id);
    assert_eq!(found_addr, addr);
    assert_eq!(found_amount.parse::<f64>().unwrap(), 25.0);

    credits
        .refund(
            &found_addr,
            &found_amount,
            &format!("checkout:{}", found_id),
            None,
        )
        .await
        .unwrap();
    assert_eq!(balances(&pool, &addr).await, (35.0, 20.0));
    assert_eq!(
        ledger(&pool, &addr, "refund").await,
        vec![("earned".into(), 20.0), ("paid".into(), 5.0)]
    );

    sqlx::query("DELETE FROM fulfillment_outbox WHERE checkout_id = $1")
        .bind(checkout_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM checkouts WHERE id = $1")
        .bind(checkout_id)
        .execute(&pool)
        .await
        .unwrap();
    cleanup(&pool, &addr).await;
}
