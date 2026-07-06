use catalyrst_credits::ports::credits::CreditsComponent;

fn scratch_wallet() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    let pid = std::process::id() as u64;
    format!("0xtopup{:016x}{:016x}000", nanos, pid)
        .chars()
        .take(42)
        .collect()
}

async fn pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("CREDITS_TEST_PG_CONNECTION_STRING").ok()?;
    Some(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("test PG unreachable"),
    )
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str, idem: &str) {
    for q in ["DELETE FROM credit_grant_idempotency WHERE idempotency_key = $1"] {
        sqlx::query(q).bind(idem).execute(pool).await.unwrap();
    }
    for q in [
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(q).bind(addr).execute(pool).await.unwrap();
    }
}

async fn credits_for_wei(pool: &sqlx::PgPool, value_wei: &str, mana_usd: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT floor(($1::numeric / 1e18) * $2::numeric / 0.10)::text")
        .bind(value_wei)
        .bind(mana_usd)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn quote_wei(pool: &sqlx::PgPool, credits: i64, mana_usd: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT ceil(ceil($1::numeric * 0.10 / $2::numeric * 1e18) * 102 / 100)::text",
    )
    .bind(credits)
    .bind(mana_usd)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn conversion_floors_and_never_rounds_up() {
    let Some(pool) = pool().await else { return };

    assert_eq!(
        credits_for_wei(&pool, "100000000000000000000", "0.25").await,
        "250"
    );
    assert_eq!(
        credits_for_wei(&pool, "3999900000000000000", "0.25").await,
        "9"
    );
    assert_eq!(credits_for_wei(&pool, "1", "0.25").await, "0");
    assert_eq!(credits_for_wei(&pool, "0", "0.25").await, "0");
}

#[tokio::test]
async fn quote_covers_the_grant_after_the_buffer() {
    let Some(pool) = pool().await else { return };

    assert_eq!(quote_wei(&pool, 250, "0.25").await, "102000000000000000000");

    for (credits, usd) in [
        (1i64, "0.25"),
        (7, "0.333333"),
        (250, "0.25"),
        (99_999, "0.17"),
    ] {
        let wei = quote_wei(&pool, credits, usd).await;
        let granted: i64 = credits_for_wei(&pool, &wei, usd).await.parse().unwrap();
        assert!(
            granted >= credits,
            "quote {wei} wei for {credits} credits at {usd} grants only {granted}"
        );
    }

    let wei = quote_wei(&pool, 250, "0.25").await;
    let granted: i64 = credits_for_wei(&pool, &wei, "0.245250")
        .await
        .parse()
        .unwrap();
    assert!(granted >= 250, "granted {granted} after 1.9% drift");
}

#[tokio::test]
async fn double_post_grants_once_and_replays_original_amounts() {
    let Some(pool) = pool().await else { return };
    let addr = scratch_wallet();
    let tx_hash = format!(
        "0x{:064x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let idem = format!("mana-topup:{tx_hash}");
    let credits = CreditsComponent::new(pool.clone());
    let detail = serde_json::json!({ "source": "mana-topup", "txHash": tx_hash });

    let first = credits
        .admin_grant_credits(
            &addr,
            "250",
            "purchase",
            Some("MANA top-up"),
            Some("mana-topup"),
            Some(&idem),
            &detail,
        )
        .await
        .unwrap();
    assert!(!first.replayed);
    assert_eq!(first.applied, "250");

    let prior = credits
        .find_grant_by_idempotency_key(&idem)
        .await
        .unwrap()
        .expect("committed grant visible");
    assert_eq!(prior.address, addr);
    assert_eq!(prior.amount, "250");
    assert_eq!(prior.available, first.available);

    let second = credits
        .admin_grant_credits(
            &addr,
            "250",
            "purchase",
            Some("MANA top-up"),
            Some("mana-topup"),
            Some(&idem),
            &detail,
        )
        .await
        .unwrap();
    assert!(second.replayed);
    assert_eq!(second.applied, "250");
    assert_eq!(second.available, first.available);

    let ledger_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credit_ledger WHERE address = $1 AND kind = 'purchase'",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ledger_rows, 1);

    let drifted = credits
        .admin_grant_credits(
            &addr,
            "251",
            "purchase",
            Some("MANA top-up"),
            Some("mana-topup"),
            Some(&idem),
            &detail,
        )
        .await;
    assert!(
        drifted.is_err(),
        "drifted amount under a used key must not grant"
    );

    let available: String =
        sqlx::query_scalar("SELECT available::text FROM user_credits WHERE address = $1")
            .bind(&addr)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(available, "250");

    cleanup(&pool, &addr, &idem).await;
}
