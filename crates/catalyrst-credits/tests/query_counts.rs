//! Round-trip-collapse regression suite for `catalyrst-credits`.
//!
//! Each test pins a query-count optimization: it drives a real call path against
//! a scratch PostgreSQL, counts the sqlx statements it emitted (via the
//! thread-routed `common::sql_capture()` recorder), and asserts BOTH the
//! collapsed count and that the observable result (money strings, ordering,
//! state, response shape) is
//! byte-identical to the pre-optimization behavior. Revert the source change and
//! the count assertion fails while the behavioral assertions still pass --
//! isolating the round-trip collapse from any shape/money regression.
//!
//! PG-gated exactly like `formal_money.rs`: without
//! `CREDITS_TEST_PG_CONNECTION_STRING` (or the workspace-wide gate) every test
//! self-skips. Tests run on the default current-thread `#[tokio::test]` flavor so
//! the pool's query futures are polled on the thread that registered the
//! capture sink.

mod common;

use std::net::SocketAddr;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use catalyrst_credits::handlers::captcha::generate;
use catalyrst_credits::handlers::prices::{quote, QuoteBody};
use catalyrst_credits::ports::credits::CreditsComponent;

/// Mirror of `handlers::prices::valid_wei` (private there) so the oracle in the
/// batch test classifies each amount exactly as the handler does.
fn valid_wei(raw: &str) -> Option<&str> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 30 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s)
}

// wallet.rs spend_in_tx: the dead `SELECT 1 ... FOR UPDATE` probe is gone, so a
// funded spend touches user_credits exactly twice (SELECT ... FOR UPDATE, then
// UPDATE) instead of three times.

#[tokio::test]
async fn spend_in_tx_locks_user_credits_once() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let addr = common::wallet_addr(&common::scratch_wallet());

    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) \
         VALUES ($1, 5, 2)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();

    // Warm the pool so connection-setup statements don't pollute the capture.
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = common::sql_capture();
    let outcome = credits
        .spend(&addr, "4", "checkout:qc-spend", None)
        .await
        .unwrap();
    let user_credits_stmts = cap.count_containing("user_credits");
    drop(cap);

    // Collapse: SELECT ... FOR UPDATE + UPDATE, and nothing else touching the
    // row (BEGIN/COMMIT/ledger-insert don't match the substring).
    assert_eq!(
        user_credits_stmts, 2,
        "a funded spend must touch user_credits exactly twice (the dead SELECT 1 \
         FOR UPDATE probe is deleted)"
    );

    assert_eq!(outcome.applied, "4");
    assert!(!outcome.replayed);
    let independent: String =
        sqlx::query_scalar("SELECT available::text FROM user_credits WHERE address = $1")
            .bind(&addr)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        outcome.available, independent,
        "returned available must string-equal the persisted balance"
    );

    let missing = common::wallet_addr(&common::scratch_wallet());
    let err = credits
        .spend(&missing, "4", "checkout:qc-missing", None)
        .await
        .unwrap_err();
    assert_eq!(common::status_of(err), 402);

    // (c) idempotent replay: identical strings, replayed flips to true. The key
    // and tx_ref carry the unique address so reruns against a persistent DB
    // don't collide on the globally-unique credit_spend_idempotency key.
    let key = format!("qc-spend-idem-{addr}");
    let replay_ref = format!("checkout:qc-replay-{addr}");
    let first = credits
        .spend(&addr, "1", &replay_ref, Some(&key))
        .await
        .unwrap();
    let second = credits
        .spend(&addr, "1", &replay_ref, Some(&key))
        .await
        .unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(first.available, second.available);
    assert_eq!(first.applied, second.applied);
}

// checkout.rs get_cart: the cart total now rides a window aggregate on the
// line-rows query, so a populated cart is ONE statement, not two.

#[tokio::test]
async fn get_cart_total_in_single_query() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let addr = common::wallet_addr(&common::scratch_wallet());
    let collection = "0xeede64bfaf8055492aa500846ec7c6e6a9f533d5";

    for (item_id, qty, price) in [("1", 2, "1.50"), ("2", 1, "0.00"), ("3", 3, "2")] {
        let urn = format!("urn:decentraland:matic:collections-v2:{collection}:{item_id}");
        credits
            .add_item(&addr, item_id, collection, &urn, "wearable", qty, price)
            .await
            .unwrap();
    }

    // Expected total via the OLD aggregate SQL, verbatim.
    let expected_total: String = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ci.unit_price_credits * ci.qty), 0)::text \
         FROM cart_items ci JOIN carts c ON c.id = ci.cart_id WHERE c.address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = common::sql_capture();
    let view = credits.get_cart(&addr).await.unwrap();
    let cart_items_stmts = cap.count_containing("cart_items");
    let total_stmts = cap.count();
    drop(cap);

    assert_eq!(
        cart_items_stmts, 1,
        "populated cart must issue one cart_items statement"
    );
    assert_eq!(total_stmts, 1, "get_cart must be a single round trip");
    assert_eq!(
        view.total_credits, expected_total,
        "folded window total must byte-equal the old aggregate"
    );

    let ids: Vec<&str> = view.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "3"]);
    let prices: Vec<&str> = view
        .items
        .iter()
        .map(|i| i.unit_price_credits.as_str())
        .collect();
    assert_eq!(prices, ["1.50", "0.00", "2"]);

    let empty_addr = common::wallet_addr(&common::scratch_wallet());
    let cap = common::sql_capture();
    let empty = credits.get_cart(&empty_addr).await.unwrap();
    let empty_stmts = cap.count();
    drop(cap);
    assert_eq!(empty_stmts, 1, "empty cart is still a single round trip");
    assert!(empty.items.is_empty());
    assert_eq!(empty.total_credits, "0");
}

// prices.rs quote(): the serial per-amount repricing loop is now one unnest
// batch, so 60 amounts issue a SINGLE `ceil(...)` statement.

async fn spawn_oracle_mock() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let app = Router::new().route(
        "/api/v3/simple/price",
        get(move || async move {
            Json(json!({ "decentraland": { "usd": 0.5, "last_updated_at": now } }))
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn quote_amounts_batched_single_query() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let mock = spawn_oracle_mock().await;
    let state =
        common::test_state_with_market(pool.clone(), false, &format!("http://{mock}"), "auto");

    let valid_weis = [
        "1000000000000000000",
        "2500000000000000000",
        "500000000000000000",
        "1",
        "999999999999999999",
    ];
    let invalids = ["", "abc", "0x10", &"9".repeat(31)];
    let mut raw_amounts: Vec<String> = Vec::with_capacity(60);
    for i in 0..60usize {
        if i % 3 == 0 {
            raw_amounts.push(invalids[(i / 3) % invalids.len()].to_string());
        } else {
            raw_amounts.push(valid_weis[i % valid_weis.len()].to_string());
        }
    }

    // Oracle string the handler will use, fetched once outside the capture.
    let mana_usd = state.pricing.fetch_mana_usd().await.unwrap();

    // Expected vector: the literal pre-change per-entry computation.
    let mut expected: Vec<Option<String>> = Vec::with_capacity(60);
    for raw in &raw_amounts {
        match valid_wei(raw) {
            Some(wei) => expected.push(Some(
                state
                    .pricing
                    .compute_credit_price(&pool, wei, &mana_usd)
                    .await
                    .unwrap(),
            )),
            None => expected.push(None),
        }
    }

    let body: QuoteBody =
        serde_json::from_value(json!({ "items": [], "amounts": raw_amounts })).unwrap();

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = common::sql_capture();
    let out = quote(State(state.clone()), Json(body)).await.unwrap();
    let ceil_stmts = cap.count_containing("ceil");
    drop(cap);

    assert_eq!(
        ceil_stmts, 1,
        "all valid amounts must reprice in one batched ceil statement"
    );
    // `amounts` is a private field; compare through the wire form (the same
    // serialization the response ships), which renders None -> null, Some -> str.
    let wire = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        wire["amounts"],
        serde_json::to_value(&expected).unwrap(),
        "batched amounts must be element-wise byte-identical to the per-entry computation"
    );
}

// captcha.rs generate(): the paired UPDATE-invalidate + INSERT is now one
// data-modifying CTE -- a single statement -- and issuance semantics are preserved
// (the prior open challenge is consumed, exactly one open challenge remains).

#[tokio::test]
async fn captcha_generate_single_statement() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state(pool.clone(), false);
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let headers = common::signed_headers(&wallet, "get", "/captcha").await;

    let seed_id: i64 = sqlx::query_scalar(
        "INSERT INTO captcha_challenges (address, answer_x, expires_at) \
         VALUES ($1, 10, now() + interval '2 minutes') RETURNING id",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = common::sql_capture();
    let resp = generate(State(state), headers).await.unwrap();
    let challenge_stmts = cap.count_containing("captcha_challenges");
    let total_stmts = cap.count();
    drop(cap);

    assert_eq!(
        challenge_stmts, 1,
        "issuance must be a single captcha_challenges statement"
    );
    assert_eq!(total_stmts, 1, "generate must be a single round trip");

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/png"
    );

    let seeded_consumed: bool =
        sqlx::query_scalar("SELECT consumed_at IS NOT NULL FROM captcha_challenges WHERE id = $1")
            .bind(seed_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(seeded_consumed, "the prior open challenge must be consumed");

    let open_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM captcha_challenges WHERE address = $1 AND consumed_at IS NULL",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        open_count, 1,
        "exactly one open challenge remains (the new one)"
    );
}
