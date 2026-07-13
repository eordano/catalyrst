mod common;

use axum::extract::{Json, State};
use axum::http::HeaderValue;

use catalyrst_credits::handlers::cart::{checkout, CheckoutBody};
use catalyrst_credits::http::ApiError;
use catalyrst_credits::ports::checkout::RepricedLine;

const COLLECTION: &str = "0x59a90bad9570ecd08895f132daf7b79696337f61";
const URN: &str =
    "urn:decentraland:matic:collections-v2:0x59a90bad9570ecd08895f132daf7b79696337f61:1";

fn unique_idem(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("test-replay-{tag}-{}-{nanos}", std::process::id())
}

async fn seed_paid_credits(pool: &sqlx::PgPool, addr: &str, amount: &str) {
    sqlx::query("INSERT INTO user_credits (address, available) VALUES ($1, $2::numeric)")
        .bind(addr)
        .bind(amount)
        .execute(pool)
        .await
        .unwrap();
}

fn line(unit_price_credits: &str) -> RepricedLine {
    RepricedLine {
        item_id: "1".into(),
        collection: COLLECTION.into(),
        urn: URN.into(),
        category: "wearable".into(),
        qty: 1,
        unit_price_credits: unit_price_credits.into(),
        token_id: None,
        trade_id: None,
        basis_wei: Some("0".into()),
        mode: "primary".into(),
    }
}

async fn spend_count(pool: &sqlx::PgPool, addr: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM credit_ledger WHERE address = $1 AND kind = 'spend'")
        .bind(addr)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn available(pool: &sqlx::PgPool, addr: &str) -> f64 {
    sqlx::query_scalar("SELECT available::float8 FROM user_credits WHERE address = $1")
        .bind(addr)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str) {
    sqlx::query(
        "DELETE FROM fulfillment_outbox \
         WHERE checkout_id IN (SELECT id FROM checkouts WHERE address = $1)",
    )
    .bind(addr)
    .execute(pool)
    .await
    .unwrap();
    for q in [
        "DELETE FROM checkouts WHERE address = $1",
        "DELETE FROM cart_items WHERE cart_id IN (SELECT id FROM carts WHERE address = $1)",
        "DELETE FROM carts WHERE address = $1",
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(q).bind(addr).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn replay_after_commit_returns_original_checkout_without_a_new_debit() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let state = common::test_state(pool.clone(), false);

    seed_paid_credits(&pool, &addr, "10").await;
    state
        .credits
        .add_item(&addr, "1", COLLECTION, URN, "wearable", 1, "3")
        .await
        .unwrap();

    let idem = unique_idem("commit");
    let first = state
        .credits
        .run_checkout(&addr, &idem, &[line("3")])
        .await
        .unwrap();
    assert!(!first.replayed);
    assert_eq!(first.status, "fulfilling");
    assert!(
        state
            .credits
            .get_cart(&addr)
            .await
            .unwrap()
            .items
            .is_empty(),
        "a committed checkout consumes its cart lines"
    );
    assert_eq!(available(&pool, &addr).await, 7.0);
    assert_eq!(spend_count(&pool, &addr).await, 1);

    let mut headers = common::signed_headers(&wallet, "post", "/checkout").await;
    headers.insert("idempotency-key", HeaderValue::from_str(&idem).unwrap());
    let out = checkout(
        State(state.clone()),
        headers,
        Ok(Json(CheckoutBody::default())),
    )
    .await
    .expect("retry must replay the committed checkout, not fail its pre-checks");

    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        v["id"].as_i64(),
        Some(first.id),
        "must be the ORIGINAL checkout"
    );
    assert_eq!(v["status"], serde_json::json!("fulfilling"));
    assert_eq!(v["replayed"], serde_json::json!(true));

    assert_eq!(
        spend_count(&pool, &addr).await,
        1,
        "replay must not debit again"
    );
    assert_eq!(available(&pool, &addr).await, 7.0);

    cleanup(&pool, &addr).await;
}

#[tokio::test]
async fn replay_with_a_different_signer_is_a_409() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let owner = common::scratch_wallet();
    let owner_addr = common::wallet_addr(&owner);
    let intruder = common::scratch_wallet();
    let state = common::test_state(pool.clone(), false);

    seed_paid_credits(&pool, &owner_addr, "10").await;
    let idem = unique_idem("intruder");
    state
        .credits
        .add_item(&owner_addr, "1", COLLECTION, URN, "wearable", 1, "3")
        .await
        .unwrap();
    state
        .credits
        .run_checkout(&owner_addr, &idem, &[line("3")])
        .await
        .unwrap();

    let mut headers = common::signed_headers(&intruder, "post", "/checkout").await;
    headers.insert("idempotency-key", HeaderValue::from_str(&idem).unwrap());
    let err = checkout(State(state), headers, Ok(Json(CheckoutBody::default())))
        .await
        .expect_err("someone else's key must not replay");
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    assert_eq!(common::status_of(err), 409);

    let intruder_addr = common::wallet_addr(&intruder);
    assert_eq!(spend_count(&pool, &intruder_addr).await, 0);

    cleanup(&pool, &owner_addr).await;
}

#[tokio::test]
async fn fresh_key_still_hits_the_empty_cart_gate() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let state = common::test_state(pool.clone(), false);

    let mut headers = common::signed_headers(&wallet, "post", "/checkout").await;
    headers.insert(
        "idempotency-key",
        HeaderValue::from_str(&unique_idem("fresh")).unwrap(),
    );
    let err = checkout(State(state), headers, Ok(Json(CheckoutBody::default())))
        .await
        .expect_err("empty cart on a fresh key must still 400");
    match &err {
        ApiError::BadRequest(m) => assert!(m.contains("cart is empty"), "got: {m}"),
        other => panic!("expected 400 BadRequest, got {other:?}"),
    }
    assert_eq!(common::status_of(err), 400);
    assert_eq!(spend_count(&pool, &addr).await, 0);

    cleanup(&pool, &addr).await;
}
