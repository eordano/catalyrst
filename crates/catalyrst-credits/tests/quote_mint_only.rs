mod common;

use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use catalyrst_credits::handlers::prices::{quote, QuoteBody};

const COLLECTION: &str = "0xeede64bfaf8055492aa500846ec7c6e6a9f533d5";
const ITEM_ID: &str = "4";

async fn spawn_market_mock(open_orders: serde_json::Value) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let app = Router::new()
        .route(
            "/v1/items",
            get(move || async move {
                Json(json!({
                    "data": [{
                        "id": format!("{COLLECTION}-{ITEM_ID}"),
                        "category": "wearable",
                        "price": "2500000000000000000",
                        "urn": format!("urn:decentraland:matic:collections-v2:{COLLECTION}:{ITEM_ID}"),
                        "contractAddress": COLLECTION,
                        "isOnSale": true,
                    }]
                }))
            }),
        )
        .route(
            "/v1/orders",
            get(move || {
                let orders = open_orders.clone();
                async move { Json(json!({ "data": orders })) }
            }),
        )
        .route(
            "/api/v3/simple/price",
            get(move || async move {
                Json(json!({
                    "decentraland": { "usd": 0.5, "last_updated_at": now }
                }))
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn quote_body() -> Json<QuoteBody> {
    Json(
        serde_json::from_value(json!({
            "items": [{ "itemId": ITEM_ID, "collection": COLLECTION }],
        }))
        .unwrap(),
    )
}

#[tokio::test]
async fn mint_only_item_quotes_real_credits_under_auto() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let mock = spawn_market_mock(json!([])).await;
    let state = common::test_state_with_market(pool, false, &format!("http://{mock}"), "auto");

    let out = quote(State(state), quote_body()).await.unwrap();
    let v = serde_json::to_value(&out.0).unwrap();
    let credits = &v["items"][0]["credits"];
    assert!(
        credits.is_string(),
        "auto mode must price the mint when no listing exists, got {v}"
    );

    assert_eq!(
        credits, "13",
        "2.5 MANA * 0.5 USD / 0.10 per credit, ceiled"
    );
}

#[tokio::test]
async fn mint_only_item_stays_unquotable_under_secondary() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let mock = spawn_market_mock(json!([])).await;
    let state = common::test_state_with_market(pool, false, &format!("http://{mock}"), "secondary");

    let out = quote(State(state), quote_body()).await.unwrap();
    let v = serde_json::to_value(&out.0).unwrap();
    assert!(
        v["items"][0]["credits"].is_null(),
        "secondary mode has no listing to price against, got {v}"
    );
}
