//! Round-trip-collapse regression suite for `catalyrst-credits`.
//!
//! Each test pins a query-count optimization: it drives a real call path against
//! a scratch PostgreSQL, counts the sqlx statements it emitted (via the
//! thread-routed `catalyrst_testgate::sql_capture` recorder), and asserts BOTH the
//! collapsed count and that the observable result (money strings, ordering, state,
//! response shape) is byte-identical to the pre-optimization behavior. Revert the
//! source change and the count assertion fails while the behavioral assertions
//! still pass.
//!
//! PG-gated exactly like `formal_money.rs`: without
//! `CREDITS_TEST_PG_CONNECTION_STRING` (or the workspace-wide gate) every test
//! self-skips. Tests run on the default current-thread `#[tokio::test]` flavor so
//! the pool's query futures are polled on the thread that registered the capture
//! sink.

mod common;

use std::net::SocketAddr;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::Path;
use axum::extract::Query;
use axum::http::HeaderMap;
use catalyrst_credits::dto::ClaimCreditsBody;
use catalyrst_credits::handlers::captcha::{claim, generate};
use catalyrst_credits::handlers::orders::order_status;
use catalyrst_credits::handlers::prices::{quote, QuoteBody};
use catalyrst_credits::handlers::users::progress;
use catalyrst_credits::http::ApiError;
use catalyrst_credits::ports::authorize::NewAuthorization;
use catalyrst_credits::ports::checkout::{OutboxWorker, RepricedLine};
use catalyrst_credits::ports::credits::CreditsComponent;
use catalyrst_credits::ports::packs::PackRow;
use catalyrst_credits::ports::pricing::ensure_charge_covers_payment;
use catalyrst_credits::ports::pricing::PricingClient;

/// Mirror of `handlers::prices::valid_wei` (private there) so the oracle in the
/// batch test classifies each amount exactly as the handler does.
fn valid_wei(raw: &str) -> Option<&str> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 30 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s)
}

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

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let outcome = credits
        .spend(&addr, "4", "checkout:qc-spend", None)
        .await
        .unwrap();
    let user_credits_stmts = cap.count_containing("user_credits");
    drop(cap);

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

    let expected_total: String = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ci.unit_price_credits * ci.qty), 0)::text \
         FROM cart_items ci JOIN carts c ON c.id = ci.cart_id WHERE c.address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let cap = catalyrst_testgate::sql_capture::sql_capture();
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
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let empty = credits.get_cart(&empty_addr).await.unwrap();
    let empty_stmts = cap.count();
    drop(cap);
    assert_eq!(empty_stmts, 1, "empty cart is still a single round trip");
    assert!(empty.items.is_empty());
    assert_eq!(empty.total_credits, "0");
}

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

    let mana_usd = state.pricing.fetch_mana_usd().await.unwrap();

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

    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = quote(State(state.clone()), Json(body)).await.unwrap();
    let ceil_stmts = cap.count_containing("ceil");
    drop(cap);

    assert_eq!(
        ceil_stmts, 1,
        "all valid amounts must reprice in one batched ceil statement"
    );
    let wire = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        wire["amounts"],
        serde_json::to_value(&expected).unwrap(),
        "batched amounts must be element-wise byte-identical to the per-entry computation"
    );
}

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

    let cap = catalyrst_testgate::sql_capture::sql_capture();
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

const COLLECTION: &str = "0xeede64bfaf8055492aa500846ec7c6e6a9f533d5";

fn urn_for(item_id: &str) -> String {
    format!("urn:decentraland:matic:collections-v2:{COLLECTION}:{item_id}")
}

#[tokio::test]
async fn cart_add_and_remove_are_single_statements() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let addr = common::wallet_addr(&common::scratch_wallet());

    for (item_id, qty, price) in [("1", 2, "1.50"), ("2", 1, "0.00"), ("3", 3, "2")] {
        let cap = catalyrst_testgate::sql_capture::sql_capture();
        credits
            .add_item(
                &addr,
                item_id,
                COLLECTION,
                &urn_for(item_id),
                "wearable",
                qty,
                price,
            )
            .await
            .unwrap();
        let n = cap.count();
        drop(cap);
        assert_eq!(n, 1, "cart upsert + line upsert must be one statement");
    }

    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let view = credits.remove_item(&addr, COLLECTION, "2").await.unwrap();
    let n = cap.count();
    drop(cap);
    assert_eq!(n, 1, "delete + remaining-cart read must be one statement");

    let ids: Vec<&str> = view.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids, ["1", "3"]);
    let reread = credits.get_cart(&addr).await.unwrap();
    assert_eq!(view.total_credits, reread.total_credits);
    assert_eq!(view.total_credits, "9.00");
    assert_eq!(view.items.len(), reread.items.len());

    let same = credits.remove_item(&addr, COLLECTION, "404").await.unwrap();
    assert_eq!(same.items.len(), 2);
    assert_eq!(same.total_credits, "9.00");
}

#[tokio::test]
async fn add_item_priced_inlines_the_reprice() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state(pool.clone(), false);
    let credits = &state.credits;
    let addr = common::wallet_addr(&common::scratch_wallet());
    let wei = "2500000000000000000";
    let markup = state.pricing.markup_bps();

    let expected = state
        .pricing
        .compute_credit_price(&pool, wei, "0.5")
        .await
        .unwrap();

    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let price = credits
        .add_item_priced(
            &addr,
            "4",
            COLLECTION,
            &urn_for("4"),
            "wearable",
            1,
            wei,
            "0.5",
            markup,
            true,
        )
        .await
        .unwrap();
    let n = cap.count();
    drop(cap);
    assert_eq!(
        n, 1,
        "price + cart upsert + line upsert must be one statement"
    );
    assert_eq!(
        price, expected,
        "inline ceil must byte-equal compute_credit_price"
    );
    ensure_charge_covers_payment(wei, &price).unwrap();

    let zero = credits
        .add_item_priced(
            &addr,
            "5",
            COLLECTION,
            &urn_for("5"),
            "wearable",
            1,
            wei,
            "0",
            markup,
            true,
        )
        .await
        .unwrap();
    assert!(
        ensure_charge_covers_payment(wei, &zero).is_err(),
        "a zero charge for a positive payment is the same conflict as before"
    );

    let cart = credits.get_cart(&addr).await.unwrap();
    let ids: Vec<&str> = cart.items.iter().map(|i| i.item_id.as_str()).collect();
    assert_eq!(ids, ["4"], "the unpriceable line must not be written");
    assert_eq!(cart.items[0].unit_price_credits, expected);
}

fn checkout_line(item_id: &str, unit_price_credits: &str) -> RepricedLine {
    RepricedLine {
        item_id: item_id.into(),
        collection: COLLECTION.into(),
        urn: urn_for(item_id),
        category: "wearable".into(),
        qty: 1,
        unit_price_credits: unit_price_credits.into(),
        token_id: None,
        trade_id: None,
        basis_wei: Some("0".into()),
        mode: "primary".into(),
    }
}

#[tokio::test]
async fn run_checkout_statement_budget_and_ledger_id() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let addr = common::wallet_addr(&common::scratch_wallet());
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) VALUES ($1, 10, 4)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();
    for item_id in ["1", "2"] {
        credits
            .add_item(
                &addr,
                item_id,
                COLLECTION,
                &urn_for(item_id),
                "wearable",
                1,
                "3",
            )
            .await
            .unwrap();
    }

    let idem = format!("qc-checkout-{addr}");
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let outcome = credits
        .run_checkout(
            &addr,
            &idem,
            &[checkout_line("1", "3"), checkout_line("2", "3")],
        )
        .await
        .unwrap();
    let total_stmts = cap.count();
    let user_credits_stmts = cap.count_containing("user_credits");
    drop(cap);

    assert_eq!(outcome.status, "fulfilling");
    assert!(!outcome.replayed);
    assert_eq!(
        total_stmts, 9,
        "claim, total, spend x5, outbox+status+cart, plus the transaction bracket"
    );
    assert_eq!(
        user_credits_stmts, 2,
        "the balance is locked once, inside the spend"
    );

    let (total, status, ledger_id): (String, String, Option<i64>) = sqlx::query_as(
        "SELECT total_credits::text, status, ledger_id FROM checkouts WHERE id = $1",
    )
    .bind(outcome.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, "6");
    assert_eq!(status, "fulfilling");
    let expected_ledger: Option<i64> = sqlx::query_scalar(
        "SELECT max(id) FROM credit_ledger WHERE tx_ref = $1 AND kind = 'spend'",
    )
    .bind(format!("checkout:{}", outcome.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(expected_ledger.is_some());
    assert_eq!(ledger_id, expected_ledger);

    let outbox: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fulfillment_outbox WHERE checkout_id = $1")
            .bind(outcome.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outbox, 2);
    assert!(credits.get_cart(&addr).await.unwrap().items.is_empty());

    // The admin listing folds the outbox lines into the head query.
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let listed = credits
        .admin_list_checkouts(Some(&addr), None, 10, 0)
        .await
        .unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "checkouts + lines in one statement");
    let head = listed.iter().find(|c| c.id == outcome.id).expect("listed");
    assert_eq!(head.total_credits, "6");
    assert_eq!(head.status, "fulfilling");
    assert_eq!(head.lines.len(), 2);
    assert!(head.lines[0].id < head.lines[1].id);
    assert!(head
        .lines
        .iter()
        .all(|l| l.status == "pending" && l.attempts == 0));
    assert!(head
        .lines
        .iter()
        .all(|l| l.last_error.is_none() && l.external_ref.is_none()));
    assert_eq!(
        head.lines
            .iter()
            .map(|l| l.unit_price_credits.parse::<f64>().unwrap())
            .sum::<f64>(),
        6.0
    );
    assert!(credits
        .admin_list_checkouts(Some(&addr), Some("failed"), 10, 0)
        .await
        .unwrap()
        .is_empty());

    // One outbox tick: expiry sweep + reversing sweep + pending scan in one statement.
    let stale_auth = format!("qc-stale-{}", &addr[2..14]);
    sqlx::query(
        "INSERT INTO credit_authorizations \
             (id, address, usd_cents, amount_wei, status, expires_at) \
         VALUES ($1, $2, 1, '1', 'authorized', now() - interval '1 minute')",
    )
    .bind(&stale_auth)
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();
    let base = "http://127.0.0.1:9".to_string();
    let worker = OutboxWorker {
        credits: credits.clone(),
        pricing: PricingClient::new(reqwest::Client::new(), base.clone(), base.clone(), 0, 3600),
        http: reqwest::Client::new(),
        economy_base_url: base,
        economy_admin_token: None,
        escrow_address: None,
        max_attempts: 5,
        usage_grants_pool: None,
        escrow_lock_days: 15,
        mock_fulfillment: false,
    };
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let processed = worker.run_once().await.unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "a tick without broker config is one statement");
    assert_eq!(processed, 0, "no broker token: the pending lines idle");
    let auth_status: String =
        sqlx::query_scalar("SELECT status FROM credit_authorizations WHERE id = $1")
            .bind(&stale_auth)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        auth_status, "expired",
        "the tick expires stale authorizations"
    );
    let still_pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fulfillment_outbox WHERE checkout_id = $1 AND status = 'pending'",
    )
    .bind(outcome.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(still_pending, 2);

    let broke = common::wallet_addr(&common::scratch_wallet());
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) VALUES ($1, 1, 0)",
    )
    .bind(&broke)
    .execute(&pool)
    .await
    .unwrap();
    credits
        .add_item(&broke, "1", COLLECTION, &urn_for("1"), "wearable", 1, "3")
        .await
        .unwrap();
    let idem = format!("qc-checkout-broke-{broke}");
    let err = credits
        .run_checkout(&broke, &idem, &[checkout_line("1", "3")])
        .await
        .unwrap_err();
    assert_eq!(common::status_of(err), 402);
    let status: String =
        sqlx::query_scalar("SELECT status FROM checkouts WHERE idempotency_key = $1")
            .bind(&idem)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        status, "failed",
        "an unfunded checkout is still recorded as failed"
    );
    assert_eq!(
        credits.get_cart(&broke).await.unwrap().items.len(),
        1,
        "a failed checkout leaves the cart alone"
    );

    // Manual refund: the audit row rides in the statement that closes the checkout.
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let (refund, closed) = credits
        .refund_checkout_manual_audited(
            outcome.id,
            &addr,
            "6",
            Some("qc-admin"),
            |o| json!({ "applied": o.applied, "replayed": o.replayed, "reason": "qc" }),
        )
        .await
        .unwrap();
    let audited_stmts = cap.count();
    let close_stmts = cap.count_containing("UPDATE checkouts");
    drop(cap);
    assert!(closed);
    assert_eq!(refund.applied, "6");
    assert!(!refund.replayed);
    assert_eq!(close_stmts, 1);
    let broke_checkout: i64 =
        sqlx::query_scalar("SELECT id FROM checkouts WHERE idempotency_key = $1")
            .bind(&idem)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let (_, plain_closed) = credits
        .refund_checkout_manual(broke_checkout, &broke, "3")
        .await
        .unwrap();
    let plain_stmts = cap.count();
    drop(cap);
    assert!(!plain_closed, "an already-failed checkout is not re-closed");
    assert_eq!(
        audited_stmts, plain_stmts,
        "the audit row adds no statement over the un-audited refund"
    );
    let (status, detail, actor): (String, serde_json::Value, Option<String>) = sqlx::query_as(
        "SELECT c.status, a.detail, a.actor FROM checkouts c \
         JOIN admin_audit a ON a.entity_id = c.id AND a.action = 'checkout.refund' \
         WHERE c.id = $1 ORDER BY a.id DESC LIMIT 1",
    )
    .bind(outcome.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(actor.as_deref(), Some("qc-admin"));
    assert_eq!(detail["closed"], true);
    assert_eq!(detail["applied"], "6");
    assert_eq!(detail["replayed"], false);
    assert_eq!(detail["reason"], "qc");
}

struct MarketHits {
    items: AtomicUsize,
    oracle: AtomicUsize,
}

/// A catalog that answers `/v1/items?id=<collection>-<item>&...` for every id it is asked,
/// counting calls, plus a fixed oracle.
async fn spawn_batch_market_mock(hits: Arc<MarketHits>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let items_hits = hits.clone();
    let app = Router::new()
        .route(
            "/v1/items",
            get(move |Query(params): Query<Vec<(String, String)>>| {
                let hits = items_hits.clone();
                async move {
                    hits.items.fetch_add(1, Ordering::SeqCst);
                    let data: Vec<serde_json::Value> = params
                        .iter()
                        .filter(|(k, _)| k == "id")
                        .filter_map(|(_, v)| v.split_once('-'))
                        .map(|(collection, item_id)| {
                            json!({
                                "id": format!("{collection}-{item_id}"),
                                "itemId": item_id,
                                "category": "wearable",
                                "price": "2500000000000000000",
                                "urn": format!("urn:decentraland:matic:collections-v2:{collection}:{item_id}"),
                                "contractAddress": collection,
                                "isOnSale": true,
                            })
                        })
                        .collect();
                    Json(json!({ "data": data }))
                }
            }),
        )
        .route(
            "/v1/orders/open-by-items",
            get(move || async move { Json(json!({ "data": [], "total": "0" })) }),
        )
        .route(
            "/api/v3/simple/price",
            get(move || {
                let hits = hits.clone();
                async move {
                    hits.oracle.fetch_add(1, Ordering::SeqCst);
                    Json(json!({ "decentraland": { "usd": 0.5, "last_updated_at": now } }))
                }
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn quote_misses_batch_the_catalog_oracle_and_reprice() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let hits = Arc::new(MarketHits {
        items: AtomicUsize::new(0),
        oracle: AtomicUsize::new(0),
    });
    let mock = spawn_batch_market_mock(hits.clone()).await;
    let state =
        common::test_state_with_market(pool.clone(), false, &format!("http://{mock}"), "auto");

    let items: Vec<serde_json::Value> = (1..=12)
        .map(|i| json!({ "itemId": i.to_string(), "collection": COLLECTION }))
        .collect();
    let body = || -> QuoteBody {
        serde_json::from_value(json!({
            "items": items,
            "amounts": ["1000000000000000000", "abc", "500000000000000000"],
        }))
        .unwrap()
    };

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = quote(State(state.clone()), Json(body())).await.unwrap();
    let ceil_stmts = cap.count_containing("ceil");
    let total_stmts = cap.count();
    drop(cap);

    assert_eq!(
        hits.items.load(Ordering::SeqCst),
        1,
        "one catalog call for 12 misses"
    );
    assert_eq!(hits.oracle.load(Ordering::SeqCst), 1);
    assert_eq!(ceil_stmts, 1, "items and amounts reprice in one statement");
    assert_eq!(total_stmts, 1);

    let wire = serde_json::to_value(&out.0).unwrap();
    for i in 0..12 {
        assert_eq!(wire["items"][i]["credits"], "13", "{wire}");
    }
    assert_eq!(wire["amounts"], json!(["5", null, "3"]));

    let again = quote(State(state.clone()), Json(body())).await.unwrap();
    assert_eq!(
        serde_json::to_value(&again.0).unwrap(),
        wire,
        "cached items and repriced amounts are stable"
    );
    assert_eq!(
        hits.items.load(Ordering::SeqCst),
        1,
        "cache hits skip the catalog"
    );
    assert_eq!(
        hits.oracle.load(Ordering::SeqCst),
        1,
        "the second quote reads the memoized oracle"
    );
}

async fn response_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn progress_is_a_single_statement() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state(pool.clone(), false);
    let credits = CreditsComponent::new(pool.clone());
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let path = format!("/users/{addr}/progress");

    // Unknown wallet: no program row, no credits row.
    let headers = common::signed_headers(&wallet, "get", &path).await;
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = progress(State(state.clone()), Path(addr.clone()), headers)
        .await
        .unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "progress for an unknown wallet is one statement");
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(v["user"]["hasStartedProgram"], false);
    assert_eq!(v["credits"]["available"], 0.0);
    assert_eq!(v["credits"]["earned"], 0.0);
    assert_eq!(v["credits"]["paid"], 0.0);
    assert_eq!(v["credits"]["isBlockedForClaiming"], false);

    credits.mark_started(&addr).await.unwrap();
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available, is_blocked_for_claiming) \
         VALUES ($1, 5, 2, TRUE)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();

    let headers = common::signed_headers(&wallet, "get", &path).await;
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = progress(State(state), Path(addr.clone()), headers)
        .await
        .unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "progress for a known wallet is one statement");
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(v["user"]["hasStartedProgram"], true);
    assert_eq!(v["credits"]["available"], 5.0);
    assert_eq!(v["credits"]["earned"], 2.0);
    assert_eq!(v["credits"]["paid"], 3.0);
    assert_eq!(v["credits"]["isBlockedForClaiming"], true);
}

#[tokio::test]
async fn order_status_joins_the_balance() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state(pool.clone(), false);
    let credits = CreditsComponent::new(pool.clone());
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let other = common::wallet_addr(&common::scratch_wallet());
    let pack = PackRow {
        sku: "qc-order".into(),
        title: "qc".into(),
        credits: "50".into(),
        price_cents: 500,
        currency: "usd".into(),
        sort_order: 0,
    };
    let order_id = format!("qc-{}", &addr[2..14]);
    credits
        .insert_pending_order(&order_id, &addr, &pack, &format!("cs_{order_id}"))
        .await
        .unwrap();

    // Without a wallet row the balance reads 0, and someone else's order is not found.
    let path = format!("/credits/orders/{order_id}");
    let headers = common::signed_headers(&wallet, "get", &path).await;
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = order_status(State(state.clone()), Path(order_id.clone()), headers)
        .await
        .unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "order status is one statement");
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(v["newBalance"], 0);
    assert_eq!(v["creditsGranted"], 0);
    assert_eq!(v["error"], "");

    sqlx::query("INSERT INTO user_credits (address, available) VALUES ($1, 7.9)")
        .bind(&addr)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE credit_purchases SET status = 'paid', credits = 50 WHERE order_id = $1")
        .bind(&order_id)
        .execute(&pool)
        .await
        .unwrap();
    let headers = common::signed_headers(&wallet, "get", &path).await;
    let out = order_status(State(state.clone()), Path(order_id.clone()), headers)
        .await
        .unwrap();
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(v["newBalance"], 7);
    assert_eq!(v["status"], "credited");
    assert_eq!(v["creditsGranted"], 50);

    let stranger = common::scratch_wallet();
    assert_ne!(common::wallet_addr(&stranger), other);
    let headers = common::signed_headers(&stranger, "get", &path).await;
    let err = order_status(State(state), Path(order_id), headers)
        .await
        .expect_err("another signer must not see the order");
    assert_eq!(common::status_of(err), 404);
}

#[tokio::test]
async fn captcha_claim_reads_the_block_flag_in_the_same_statement() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state(pool.clone(), false);
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);

    async fn seed_challenge(pool: &sqlx::PgPool, addr: &str) {
        sqlx::query(
            "INSERT INTO captcha_challenges (address, answer_x, expires_at) \
             VALUES ($1, 10, now() + interval '2 minutes')",
        )
        .bind(addr)
        .execute(pool)
        .await
        .unwrap();
    }
    async fn claim_json(
        state: &catalyrst_credits::AppState,
        wallet: &alloy::signers::local::PrivateKeySigner,
        x: f64,
    ) -> (serde_json::Value, usize) {
        let headers: HeaderMap = common::signed_headers(wallet, "post", "/captcha").await;
        let cap = catalyrst_testgate::sql_capture::sql_capture();
        let resp = claim(
            State(state.clone()),
            headers,
            Some(Json(ClaimCreditsBody { x, token: None })),
        )
        .await
        .unwrap();
        let stmts = cap.count();
        drop(cap);
        (response_json(resp).await, stmts)
    }

    // No wallet row: claim allowed.
    seed_challenge(&pool, &addr).await;
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let (v, stmts) = claim_json(&state, &wallet, 10.0).await;
    assert_eq!(stmts, 1, "a solved claim is one statement");
    assert_eq!(v["ok"], true);
    assert_eq!(v["isBlockedForClaiming"], false);
    assert_eq!(v["credits_granted"], 0.0);

    // Blocked wallet: same statement count, flag surfaces.
    sqlx::query(
        "INSERT INTO user_credits (address, available, is_blocked_for_claiming) \
         VALUES ($1, 0, TRUE)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();
    seed_challenge(&pool, &addr).await;
    let (v, stmts) = claim_json(&state, &wallet, 10.0).await;
    assert_eq!(stmts, 1);
    assert_eq!(v["ok"], false);
    assert_eq!(v["isBlockedForClaiming"], true);

    // Wrong slider answer is still rejected without a second statement.
    seed_challenge(&pool, &addr).await;
    let (v, stmts) = claim_json(&state, &wallet, 99.0).await;
    assert_eq!(stmts, 1);
    assert_eq!(v["ok"], false);

    // Consumed challenge: 400, one statement.
    let headers = common::signed_headers(&wallet, "post", "/captcha").await;
    let err = claim(
        State(state),
        headers,
        Some(Json(ClaimCreditsBody {
            x: 10.0,
            token: None,
        })),
    )
    .await
    .expect_err("no open challenge left");
    assert_eq!(common::status_of(err), 400);
}

fn priced_auth<'a>(
    id: &'a str,
    addr: &'a str,
    usd_cents: i64,
    expires: chrono::DateTime<chrono::Utc>,
) -> NewAuthorization<'a> {
    NewAuthorization {
        id,
        address: addr,
        usd_cents,
        amount_wei: "",
        trade_id: Some("trade-x"),
        contract_address: None,
        item_id: None,
        source: Some("qc"),
        expires_at: expires,
    }
}

#[tokio::test]
async fn priced_authorization_reserves_in_two_statements_plus_bracket() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let addr = common::wallet_addr(&common::scratch_wallet());
    let expires = chrono::Utc::now() + chrono::Duration::minutes(10);
    let ids: Vec<String> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|s| format!("qc-auth-{}-{s}", &addr[2..14]))
        .collect();

    sqlx::query("INSERT INTO user_credits (address, available) VALUES ($1, 10)")
        .bind(&addr)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let key = format!("{addr}:qc-auth");
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let (first, rate) = credits
        .reserve_authorization_priced(&priced_auth(&ids[0], &addr, 600, expires), "0.5", &key)
        .await
        .unwrap();
    let stmts = cap.count();
    let lock_stmts = cap.count_containing("FOR UPDATE");
    drop(cap);
    assert_eq!(
        stmts, 3,
        "bracket + wallet lock + merged probe/budget/insert"
    );
    assert_eq!(lock_stmts, 1, "the wallet lock stays its own statement");
    assert!(!first.replayed);
    assert_eq!(first.id, ids[0]);
    assert_eq!(first.usd_cents, 600);
    assert_eq!(
        first.amount_wei, "12000000000000000000",
        "6.00 USD at 0.5 USD/MANA is 12 MANA"
    );
    assert_eq!(rate, "500000000000000000");

    // Same key replays the stored row, ignoring the new id and price.
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let (replay, rate) = credits
        .reserve_authorization_priced(&priced_auth(&ids[1], &addr, 900, expires), "0.25", &key)
        .await
        .unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 3);
    assert!(replay.replayed);
    assert_eq!(replay.id, ids[0]);
    assert_eq!(replay.amount_wei, "12000000000000000000");
    assert_eq!(replay.usd_cents, 600);
    assert_eq!(rate, "250000000000000000");

    // 6.00 outstanding of 10.00: 4.01 more is refused, 3.99 fits.
    let err = credits
        .reserve_authorization_priced(
            &priced_auth(&ids[2], &addr, 401, expires),
            "0.5",
            &format!("{key}-c"),
        )
        .await
        .expect_err("over budget");
    assert!(matches!(err, ApiError::PaymentRequired(_)), "{err:?}");
    let (third, _) = credits
        .reserve_authorization_priced(
            &priced_auth(&ids[3], &addr, 399, expires),
            "0.5",
            &format!("{key}-d"),
        )
        .await
        .unwrap();
    assert_eq!(third.amount_wei, "7980000000000000000");

    // Unknown wallet: no lock row, still refused with 402 rather than an error.
    let stranger = common::wallet_addr(&common::scratch_wallet());
    let err = credits
        .reserve_authorization_priced(
            &priced_auth(&ids[4], &stranger, 1, expires),
            "0.5",
            &format!("{stranger}:qc"),
        )
        .await
        .expect_err("no wallet");
    assert!(matches!(err, ApiError::PaymentRequired(_)), "{err:?}");

    // The given-wei path keeps the caller's amount verbatim (the last cent of budget).
    let given = credits
        .reserve_authorization(
            &NewAuthorization {
                amount_wei: "123",
                ..priced_auth(&ids[5], &addr, 1, expires)
            },
            &format!("{key}-f"),
        )
        .await
        .unwrap();
    assert_eq!(given.amount_wei, "123");
    assert!(!given.replayed);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credit_authorizations WHERE address = $1 AND status = 'authorized'",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn packs_are_served_from_a_memo_until_an_admin_write() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let credits = CreditsComponent::new(pool.clone());
    let sku = format!(
        "qc-{}",
        &common::wallet_addr(&common::scratch_wallet())[2..14]
    );
    let detail = json!({ "actor": "qc" });
    credits
        .admin_create_pack(&sku, "qc", "10", 100, "usd", true, 999, &detail)
        .await
        .unwrap();

    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let listed = credits.list_active_packs().await.unwrap();
    let first = cap.count();
    let again = credits.list_active_packs().await.unwrap();
    let pack = credits.get_pack(&sku).await.unwrap().expect("active pack");
    let total = cap.count();
    drop(cap);
    assert_eq!(first, 1, "the first listing reads credit_packs once");
    assert_eq!(total, 1, "the memo serves the re-list and the sku lookup");
    assert!(listed.iter().any(|p| p.sku == sku));
    assert_eq!(listed.len(), again.len());
    assert_eq!(pack.credits, "10");
    assert_eq!(pack.price_cents, 100);

    credits
        .admin_update_pack(&sku, "qc", "10", 100, "usd", false, 999, &detail)
        .await
        .unwrap();
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let hidden = credits.get_pack(&sku).await.unwrap();
    let stmts = cap.count();
    drop(cap);
    assert_eq!(stmts, 1, "an admin write invalidates the memo");
    assert!(hidden.is_none(), "deactivated packs are not served");

    credits.admin_delete_pack(&sku, &detail).await.unwrap();
}
