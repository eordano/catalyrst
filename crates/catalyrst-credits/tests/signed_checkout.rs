mod common;

use std::sync::Arc;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use catalyrst_credits::purchase_intent::{intent_digest, PurchaseIntentIn};
use serde_json::{json, Value};

const COLLECTION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

async fn market() -> (String, tokio::task::JoinHandle<()>) {
    let now = chrono::Utc::now().timestamp();
    let app =
        Router::new()
            .route(
                "/v1/items",
                get(|| async {
                    Json(json!({"data":[{
                        "id":format!("{COLLECTION}-1"), "itemId":"1", "category":"wearable",
                        "price":"2500000000000000000", "contractAddress":COLLECTION,
                        "urn":format!("urn:decentraland:matic:collections-v2:{COLLECTION}:1"),
                        "isOnSale":true
                    }]}))
                }),
            )
            .route(
                "/v1/orders/open-by-items",
                get(|| async { Json(json!({"data":[],"total":"0"})) }),
            )
            .route(
                "/api/v3/simple/price",
                get(move || async move {
                    Json(json!({"decentraland":{"usd":0.5,"last_updated_at":now}}))
                }),
            );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, task)
}

async fn call(
    app: &str,
    wallet: &PrivateKeySigner,
    method: &str,
    path: &str,
    key: &str,
    body: Value,
    expected: StatusCode,
) -> Value {
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
        .request(
            method.to_uppercase().parse().unwrap(),
            format!("{app}{path}"),
        )
        .header("idempotency-key", key)
        .headers(common::signed_headers(wallet, method, path).await)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let value: Value = response.json().await.unwrap();
    assert_eq!(status, expected, "{method} {path}: {value}");
    value
}

async fn intent(wallet: &PrivateKeySigner, key: &str, total: &str) -> Value {
    let intent = PurchaseIntentIn {
        buyer: common::wallet_addr(wallet),
        items: json!([[COLLECTION, "1", 1]]).to_string(),
        total_credits: total.into(),
        currency: "CREDITS".into(),
        nonce: key.into(),
        expires_at: chrono::Utc::now().timestamp() as u64 + 300,
    };
    let signature = wallet
        .sign_hash(&intent_digest(&intent).unwrap().into())
        .await
        .unwrap();
    json!({"intent":intent, "intentSignature":signature.to_string()})
}

#[tokio::test]
async fn signed_checkout_binds_price_and_owner_and_debits_once() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let buyer = common::scratch_wallet();
    let stranger = common::scratch_wallet();
    let address = common::wallet_addr(&buyer);
    let key = format!("signed-checkout-{address}");
    let (market_url, market_task) = market().await;
    let mut state = common::test_state_with_market(pool.clone(), false, &market_url, "auto");
    Arc::get_mut(&mut state).unwrap().require_purchase_intent = true;
    assert!(!state.mock_card);
    assert!(state.stripe.is_none());
    let router = catalyrst_credits::api_router().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app = format!("http://{}", listener.local_addr().unwrap());
    let app_task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    sqlx::query("INSERT INTO user_credits(address,available) VALUES ($1,20)")
        .bind(&address)
        .execute(&pool)
        .await
        .unwrap();
    let cart = call(
        &app,
        &buyer,
        "post",
        "/cart/items",
        &key,
        json!({"collection":COLLECTION,"itemId":"1","qty":1}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(cart["totalCredits"], "13");
    call(
        &app,
        &buyer,
        "post",
        "/checkout",
        &key,
        json!({}),
        StatusCode::UNAUTHORIZED,
    )
    .await;
    call(
        &app,
        &buyer,
        "post",
        "/checkout",
        &key,
        intent(&buyer, &key, "12").await,
        StatusCode::CONFLICT,
    )
    .await;
    let before: String =
        sqlx::query_scalar("SELECT available::text FROM user_credits WHERE address=$1")
            .bind(&address)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, "20");
    let signed = intent(&buyer, &key, "13").await;
    let first = call(
        &app,
        &buyer,
        "post",
        "/checkout",
        &key,
        signed.clone(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(first["status"], "fulfilling");
    assert_eq!(first["replayed"], false);
    let replay = call(
        &app,
        &buyer,
        "post",
        "/checkout",
        &key,
        signed.clone(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(replay["id"], first["id"]);
    assert_eq!(replay["replayed"], true);
    call(
        &app,
        &stranger,
        "post",
        "/checkout",
        &key,
        signed,
        StatusCode::CONFLICT,
    )
    .await;
    let path = format!("/checkout/{}", first["id"]);
    let order = call(&app, &buyer, "get", &path, &key, json!({}), StatusCode::OK).await;
    assert_eq!(order["totalCredits"], "13");
    call(
        &app,
        &stranger,
        "get",
        &path,
        &key,
        json!({}),
        StatusCode::FORBIDDEN,
    )
    .await;
    let cart = call(
        &app,
        &buyer,
        "get",
        "/cart",
        &key,
        json!({}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(cart["items"], json!([]));
    let balance: String =
        sqlx::query_scalar("SELECT available::text FROM user_credits WHERE address=$1")
            .bind(&address)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(balance, "7");
    let debits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM credit_ledger WHERE address=$1 AND kind='spend'")
            .bind(&address)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(debits, 1);
    let jobs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fulfillment_outbox WHERE checkout_id=$1")
            .bind(first["id"].as_i64().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(jobs, 1);
    for query in [
        "DELETE FROM fulfillment_outbox WHERE checkout_id IN (SELECT id FROM checkouts WHERE address=$1)",
        "DELETE FROM checkouts WHERE address=$1",
        "DELETE FROM cart_items WHERE cart_id IN (SELECT id FROM carts WHERE address=$1)",
        "DELETE FROM carts WHERE address=$1",
        "DELETE FROM credit_ledger WHERE address=$1",
        "DELETE FROM user_credits WHERE address=$1",
    ] {
        sqlx::query(query).bind(&address).execute(&pool).await.unwrap();
    }
    app_task.abort();
    market_task.abort();
}
