mod common;

use axum::extract::State;
use serde_json::json;

use catalyrst_credits::handlers::authorize::authorize;
use catalyrst_credits::http::ApiError;
use catalyrst_credits::ports::authorize::NewAuthorization;
use catalyrst_credits::ports::credits::CreditsComponent;

fn future() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now() + chrono::Duration::minutes(10)
}

async fn seed_credits(pool: &sqlx::PgPool, addr: &str, available: &str) {
    sqlx::query("INSERT INTO user_credits (address, available) VALUES ($1, $2::numeric)")
        .bind(addr)
        .bind(available)
        .execute(pool)
        .await
        .unwrap();
}

async fn authorized_sum_cents(pool: &sqlx::PgPool, addr: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(usd_cents), 0)::bigint FROM credit_authorizations \
         WHERE address = $1 AND status = 'authorized'",
    )
    .bind(addr)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn authorized_count(pool: &sqlx::PgPool, addr: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM credit_authorizations WHERE address = $1 AND status = 'authorized'",
    )
    .bind(addr)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str) {
    for q in [
        "DELETE FROM credit_authorizations WHERE address = $1",
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM credit_spend_idempotency WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(q).bind(addr).execute(pool).await.unwrap();
    }
}

fn na<'a>(
    id: &'a str,
    addr: &'a str,
    usd_cents: i64,
    expires: chrono::DateTime<chrono::Utc>,
) -> NewAuthorization<'a> {
    NewAuthorization {
        id,
        address: addr,
        usd_cents,
        amount_wei: "1000000000000000000",
        trade_id: Some("trade-x"),
        contract_address: None,
        item_id: None,
        source: None,
        expires_at: expires,
    }
}

#[tokio::test]
async fn sequential_reservations_never_over_pledge_the_balance() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let credits = CreditsComponent::new(pool.clone());
    seed_credits(&pool, &addr, "10").await;

    let id_a = format!("{addr}-a");
    credits
        .reserve_authorization(&na(&id_a, &addr, 600, future()), &format!("{addr}:k1"))
        .await
        .expect("first $6 reservation fits in $10");

    let id_b = format!("{addr}-b");
    let over = credits
        .reserve_authorization(&na(&id_b, &addr, 600, future()), &format!("{addr}:k2"))
        .await
        .expect_err("second $6 overshoots the remaining $4");
    assert!(matches!(over, ApiError::PaymentRequired(_)), "got {over:?}");

    let id_c = format!("{addr}-c");
    credits
        .reserve_authorization(&na(&id_c, &addr, 400, future()), &format!("{addr}:k3"))
        .await
        .expect("the remaining $4 is still reservable");

    let id_d = format!("{addr}-d");
    let broke = credits
        .reserve_authorization(&na(&id_d, &addr, 1, future()), &format!("{addr}:k4"))
        .await
        .expect_err("nothing left, not even a cent");
    assert!(
        matches!(broke, ApiError::PaymentRequired(_)),
        "got {broke:?}"
    );

    assert_eq!(
        authorized_sum_cents(&pool, &addr).await,
        1000,
        "pledged total equals the balance and never exceeds it"
    );

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn concurrent_full_balance_reservations_admit_exactly_one() {
    let Some(_guard) = common::pool().await else {
        return;
    };
    let Some(url) = catalyrst_testgate::require_pg("CREDITS_TEST_PG_CONNECTION_STRING") else {
        return;
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(12)
        .connect(&url)
        .await
        .expect("wide test pool");

    let addr = common::wallet_addr(&common::scratch_wallet());
    seed_credits(&pool, &addr, "10").await;

    let mut handles = Vec::new();
    for i in 0..8 {
        let credits = CreditsComponent::new(pool.clone());
        let addr = addr.clone();
        handles.push(tokio::spawn(async move {
            let id = format!("{addr}-c{i}");
            let key = format!("{addr}:c{i}");
            credits
                .reserve_authorization(&na(&id, &addr, 1000, future()), &key)
                .await
                .is_ok()
        }));
    }

    let mut wins = 0;
    for h in handles {
        if h.await.unwrap() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "exactly one concurrent full-balance claim may win");
    assert_eq!(authorized_count(&pool, &addr).await, 1);
    assert_eq!(authorized_sum_cents(&pool, &addr).await, 1000);

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn replay_under_the_same_key_returns_the_original_credit() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let credits = CreditsComponent::new(pool.clone());
    seed_credits(&pool, &addr, "10").await;

    let key = format!("{addr}:same");
    let id_first = format!("{addr}-first");
    let first = credits
        .reserve_authorization(&na(&id_first, &addr, 500, future()), &key)
        .await
        .unwrap();
    assert!(!first.replayed);
    assert_eq!(first.id, id_first);

    let id_second = format!("{addr}-second");
    let second = credits
        .reserve_authorization(&na(&id_second, &addr, 500, future()), &key)
        .await
        .unwrap();
    assert!(second.replayed, "same key must replay");
    assert_eq!(second.id, id_first, "replay returns the ORIGINAL id");

    assert_eq!(
        authorized_count(&pool, &addr).await,
        1,
        "a replay must not insert a second row"
    );
    assert_eq!(authorized_sum_cents(&pool, &addr).await, 500);

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn cancel_frees_the_reserved_budget() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let credits = CreditsComponent::new(pool.clone());
    seed_credits(&pool, &addr, "10").await;

    let id_a = format!("{addr}-a");
    credits
        .reserve_authorization(&na(&id_a, &addr, 1000, future()), &format!("{addr}:k1"))
        .await
        .expect("full balance reserved");

    let id_b = format!("{addr}-b");
    let broke = credits
        .reserve_authorization(&na(&id_b, &addr, 1, future()), &format!("{addr}:k2"))
        .await
        .expect_err("no budget left");
    assert!(
        matches!(broke, ApiError::PaymentRequired(_)),
        "got {broke:?}"
    );

    credits
        .release_intents(std::slice::from_ref(&id_a), &addr)
        .await
        .unwrap();

    let id_c = format!("{addr}-c");
    credits
        .reserve_authorization(&na(&id_c, &addr, 1000, future()), &format!("{addr}:k3"))
        .await
        .expect("budget freed by cancel is reservable again");

    assert_eq!(
        authorized_sum_cents(&pool, &addr).await,
        1000,
        "only the live reservation counts; the released one does not"
    );

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn expiry_sweep_frees_stale_reservations() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let credits = CreditsComponent::new(pool.clone());
    seed_credits(&pool, &addr, "10").await;

    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    let id_a = format!("{addr}-stale");
    credits
        .insert_authorization(&na(&id_a, &addr, 1000, past))
        .await
        .unwrap();

    credits.expire_stale_authorizations().await.unwrap();

    let status: String =
        sqlx::query_scalar("SELECT status FROM credit_authorizations WHERE id = $1")
            .bind(&id_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "expired");

    let id_b = format!("{addr}-fresh");
    credits
        .reserve_authorization(&na(&id_b, &addr, 1000, future()), &format!("{addr}:k"))
        .await
        .expect("the expired reservation no longer blocks the budget");

    cleanup(&pool, &addr).await;
}

// Cross-lane conservation: balance pledged to a live authorization cannot be
// re-spent through the shared checkout/escrow spend primitive.
#[tokio::test]
async fn a_live_reservation_blocks_a_checkout_spend_of_the_pledged_balance() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let addr = common::wallet_addr(&common::scratch_wallet());
    let credits = CreditsComponent::new(pool.clone());
    seed_credits(&pool, &addr, "10").await;

    let id_a = format!("{addr}-resv");
    credits
        .reserve_authorization(&na(&id_a, &addr, 1000, future()), &format!("{addr}:k"))
        .await
        .expect("full balance reserved");

    let blocked = credits
        .spend(&addr, "1", "checkout:blocked", None)
        .await
        .expect_err("balance pledged to a live authorization is unspendable");
    assert!(
        matches!(blocked, ApiError::PaymentRequired(_)),
        "got {blocked:?}"
    );

    credits
        .release_intents(std::slice::from_ref(&id_a), &addr)
        .await
        .unwrap();

    credits
        .spend(&addr, "1", "checkout:freed", None)
        .await
        .expect("releasing the reservation frees the balance to spend");

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn authorize_refuses_while_the_enable_flag_is_off() {
    let wallet = common::scratch_wallet();
    let state = common::test_state(common::lazy_pool(), false);
    let headers = common::signed_headers(&wallet, "post", "/credits/authorize").await;
    let body = axum::body::Bytes::from(
        serde_json::to_vec(&json!({ "usdPriceCents": 500, "tradeId": "t1" })).unwrap(),
    );

    let err = authorize(State(state), headers, body)
        .await
        .expect_err("default-off flag must refuse");
    assert!(
        matches!(err, ApiError::ServiceUnavailable(_)),
        "got {err:?}"
    );
    assert_eq!(common::status_of(err), 503);
}

#[tokio::test]
async fn authorize_refuses_without_signer_even_when_enabled() {
    let wallet = common::scratch_wallet();
    let state = common::test_state_onchain(common::lazy_pool(), true, None, None);
    let headers = common::signed_headers(&wallet, "post", "/credits/authorize").await;
    let body = axum::body::Bytes::from(
        serde_json::to_vec(&json!({ "usdPriceCents": 500, "tradeId": "t1" })).unwrap(),
    );

    let err = authorize(State(state), headers, body)
        .await
        .expect_err("enabled but unconfigured must refuse");
    assert!(
        matches!(err, ApiError::ServiceUnavailable(_)),
        "got {err:?}"
    );
    assert_eq!(common::status_of(err), 503);
}
