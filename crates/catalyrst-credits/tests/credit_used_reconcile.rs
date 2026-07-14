mod common;

use catalyrst_credits::ports::authorize::NewAuthorization;

async fn seed_credits(pool: &sqlx::PgPool, addr: &str, available: &str) {
    sqlx::query("INSERT INTO user_credits (address, available) VALUES ($1, $2::numeric)")
        .bind(addr)
        .bind(available)
        .execute(pool)
        .await
        .unwrap();
}

async fn available(pool: &sqlx::PgPool, addr: &str) -> f64 {
    sqlx::query_scalar("SELECT available::float8 FROM user_credits WHERE address = $1")
        .bind(addr)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn status(pool: &sqlx::PgPool, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM credit_authorizations WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn spend_rows(pool: &sqlx::PgPool, addr: &str, tx_ref: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM credit_ledger \
         WHERE address = $1 AND kind = 'spend' AND tx_ref = $2",
    )
    .bind(addr)
    .bind(tx_ref)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str, id: &str) {
    sqlx::query("DELETE FROM credit_usages WHERE credit_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    for q in [
        "DELETE FROM credit_authorizations WHERE address = $1",
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM credit_spend_idempotency WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(q).bind(addr).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn credit_used_reconciler_debits_once_across_two_runs() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let state = common::test_state(pool.clone(), false);

    seed_credits(&pool, &addr, "10").await;

    let id = format!("{addr}-credit");
    let tx_ref = format!("credit-used:{id}");
    state
        .credits
        .insert_authorization(&NewAuthorization {
            id: &id,
            address: &addr,
            usd_cents: 500,
            amount_wei: "1000000000000000000",
            trade_id: Some("trade-x"),
            contract_address: None,
            item_id: None,
            source: None,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        })
        .await
        .unwrap();

    sqlx::query("INSERT INTO credit_usages (credit_id, address) VALUES ($1, $2)")
        .bind(&id)
        .bind(&addr)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(available(&pool, &addr).await, 10.0);
    assert_eq!(status(&pool, &id).await, "authorized");

    state.credits.reconcile(None).await.unwrap();

    assert_eq!(available(&pool, &addr).await, 5.0, "debited $5 once");
    assert_eq!(status(&pool, &id).await, "consumed");
    assert_eq!(spend_rows(&pool, &addr, &tx_ref).await, 1);

    state.credits.reconcile(None).await.unwrap();

    assert_eq!(
        available(&pool, &addr).await,
        5.0,
        "a second reconcile must not debit again"
    );
    assert_eq!(status(&pool, &id).await, "consumed");
    assert_eq!(spend_rows(&pool, &addr, &tx_ref).await, 1);

    cleanup(&pool, &addr, &id).await;
}
