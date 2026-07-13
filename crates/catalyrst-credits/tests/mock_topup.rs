mod common;

use axum::extract::{Json, State};

use catalyrst_credits::handlers::packs::{mock_topup, MockTopupBody};
use catalyrst_credits::http::ApiError;

fn body(credits: &str) -> Json<MockTopupBody> {
    Json(
        serde_json::from_value(serde_json::json!({ "credits": credits }))
            .expect("body deserializes"),
    )
}

#[tokio::test]
async fn gate_off_is_a_501() {
    let state = common::test_state(common::lazy_pool(), false);
    let err = mock_topup(State(state), axum::http::HeaderMap::new(), body("5"))
        .await
        .expect_err("mock_card off must refuse");
    assert!(matches!(err, ApiError::NotImplemented(_)), "got {err:?}");
    assert_eq!(common::status_of(err), 501);
}

#[tokio::test]
async fn bad_amount_is_a_400() {
    let state = common::test_state(common::lazy_pool(), true);
    let wallet = common::scratch_wallet();

    for bad in ["abc", "", "  ", "0", "0.000", "-5", "1e3", "1.2.3"] {
        let headers = common::signed_headers(&wallet, "post", "/topup/mock-card").await;
        let Err(err) = mock_topup(State(state.clone()), headers, body(bad)).await else {
            panic!("amount {bad:?} must be rejected");
        };
        assert!(matches!(err, ApiError::BadRequest(_)), "{bad:?} → {err:?}");
        assert_eq!(common::status_of(err), 400, "for amount {bad:?}");
    }
}

#[tokio::test]
async fn over_cap_is_a_400() {
    let state = common::test_state(common::lazy_pool(), true);
    let wallet = common::scratch_wallet();

    for over in ["10000.01", "10001", "999999"] {
        let headers = common::signed_headers(&wallet, "post", "/topup/mock-card").await;
        let Err(err) = mock_topup(State(state.clone()), headers, body(over)).await else {
            panic!("amount {over:?} must be over the cap");
        };
        match &err {
            ApiError::BadRequest(m) => assert!(m.contains("capped at 10000"), "got: {m}"),
            other => panic!("expected 400 BadRequest for {over:?}, got {other:?}"),
        }
        assert_eq!(common::status_of(err), 400);
    }
}

#[tokio::test]
async fn grant_at_cap_succeeds_and_replays_idempotently() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    let state = common::test_state(pool.clone(), true);

    let headers = common::signed_headers(&wallet, "post", "/topup/mock-card").await;
    let out = mock_topup(State(state.clone()), headers.clone(), body("10000"))
        .await
        .expect("exactly the cap is allowed");
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "creditsGranted": "10000",
            "available": "10000",
            "mock": true,
        })
    );

    let replay = mock_topup(State(state), headers, body("10000"))
        .await
        .expect("idempotent retry must succeed");
    let rv = serde_json::to_value(&replay.0).unwrap();
    assert_eq!(rv["creditsGranted"], serde_json::json!("10000"));
    assert_eq!(rv["mock"], serde_json::json!(true));

    let available: f64 =
        sqlx::query_scalar("SELECT available::float8 FROM user_credits WHERE address = $1")
            .bind(&addr)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(available, 10000.0, "replay must not grant twice");
    let purchases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credit_ledger WHERE address = $1 AND kind = 'purchase'",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(purchases, 1);

    for q in [
        "DELETE FROM credit_grant_idempotency WHERE address = $1",
        "DELETE FROM admin_audit WHERE address = $1",
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(q).bind(&addr).execute(&pool).await.unwrap();
    }
}
