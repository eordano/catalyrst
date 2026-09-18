//! The Stripe webhook lands the event claim, the purchase flip and the wallet move in ONE
//! transaction; PG-gated like the rest of the suite.

mod common;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;

use catalyrst_credits::handlers::stripe::webhook;
use catalyrst_credits::ports::packs::PackRow;

const SECRET: &str = "whsec_query_counts";

fn signed(body: &str) -> HeaderMap {
    let t = chrono::Utc::now().timestamp();
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(format!("{t}.").as_bytes());
    mac.update(body.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let mut headers = HeaderMap::new();
    headers.insert(
        "stripe-signature",
        format!("t={t},v1={sig}").parse().unwrap(),
    );
    headers
}

async fn deliver(state: &catalyrst_credits::AppState, body: &str) -> (serde_json::Value, usize) {
    let cap = catalyrst_testgate::sql_capture::sql_capture();
    let out = webhook(
        State(state.clone()),
        signed(body),
        Ok(Bytes::from(body.to_string())),
    )
    .await
    .unwrap();
    let n = cap.count();
    drop(cap);
    (out.0, n)
}

async fn balance(pool: &sqlx::PgPool, addr: &str) -> String {
    sqlx::query_scalar("SELECT available::text FROM user_credits WHERE address = $1")
        .bind(addr)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// NUMERIC equality: the proportional revoke carries division scale in its text form.
async fn balance_is(pool: &sqlx::PgPool, addr: &str, expected: &str) -> bool {
    sqlx::query_scalar("SELECT available = $2::numeric FROM user_credits WHERE address = $1")
        .bind(addr)
        .bind(expected)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn webhook_purchase_and_refund_each_run_in_one_transaction() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let state = common::test_state_with_webhook_secret(pool.clone(), SECRET);
    let addr = common::wallet_addr(&common::scratch_wallet());
    let pack = PackRow {
        sku: "qc-pack".into(),
        title: "QC".into(),
        credits: "50".into(),
        price_cents: 500,
        currency: "usd".into(),
        sort_order: 0,
    };
    let pi = format!("pi_qc_{addr}");
    state
        .credits
        .insert_pending_purchase(&addr, &pack, &pi)
        .await
        .unwrap();

    let evt = format!("evt_qc_paid_{addr}");
    let paid = json!({
        "id": evt,
        "type": "payment_intent.succeeded",
        "data": { "object": { "id": pi, "amount": 500, "metadata": { "sku": "qc-pack" } } },
    })
    .to_string();
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    let (out, n) = deliver(&state, &paid).await;
    assert_eq!(out["detail"], "processed");
    assert_eq!(
        n, 6,
        "event claim, purchase flip, grant claim, balance upsert, ledger+audit+idem, commit"
    );
    assert_eq!(balance(&pool, &addr).await, "50");
    let (status, event_id): (String, Option<String>) = sqlx::query_as(
        "SELECT status, stripe_event_id FROM credit_purchases WHERE stripe_payment_intent = $1",
    )
    .bind(&pi)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "paid");
    assert_eq!(event_id.as_deref(), Some(evt.as_str()));
    let processed: bool = sqlx::query_scalar(
        "SELECT processed_at IS NOT NULL FROM stripe_events WHERE event_id = $1",
    )
    .bind(&evt)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(processed);
    let (ledger, audit): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM credit_ledger WHERE address = $1 AND kind = 'purchase'), \
                (SELECT count(*) FROM admin_audit WHERE address = $1 AND action = 'credits.grant')",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((ledger, audit), (1, 1));

    let (out, n) = deliver(&state, &paid).await;
    assert_eq!(out["detail"], "already processed");
    assert_eq!(n, 2, "a duplicate delivery is the event claim plus commit");
    assert_eq!(balance(&pool, &addr).await, "50", "no double grant");

    let refund = |evt: &str, cents: i64| {
        json!({
            "id": evt,
            "type": "charge.refunded",
            "data": { "object": { "payment_intent": pi, "amount_refunded": cents } },
        })
        .to_string()
    };
    let (out, n) = deliver(&state, &refund(&format!("evt_qc_ref1_{addr}"), 250)).await;
    assert_eq!(out["detail"], "processed");
    assert_eq!(
        n, 6,
        "event claim, purchase reversal, wallet lock, wallet debit, ledger+audit, commit"
    );
    assert!(balance_is(&pool, &addr, "25").await);
    let (refunded, revoked): (i64, bool) = sqlx::query_as(
        "SELECT refunded_cents, revoked_credits = 25 FROM credit_purchases \
         WHERE stripe_payment_intent = $1",
    )
    .bind(&pi)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refunded, 250);
    assert!(revoked);

    let (out, _) = deliver(&state, &refund(&format!("evt_qc_ref2_{addr}"), 250)).await;
    assert_eq!(out["detail"], "processed");
    assert!(
        balance_is(&pool, &addr, "25").await,
        "a repeated cumulative total reverses nothing"
    );

    let dispute = json!({
        "id": format!("evt_qc_disp_{addr}"),
        "type": "charge.dispute.created",
        "data": { "object": { "payment_intent": pi } },
    })
    .to_string();
    let (out, _) = deliver(&state, &dispute).await;
    assert_eq!(out["detail"], "processed");
    assert!(balance_is(&pool, &addr, "0").await);
    let status: String =
        sqlx::query_scalar("SELECT status FROM credit_purchases WHERE stripe_payment_intent = $1")
            .bind(&pi)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "disputed");
}
