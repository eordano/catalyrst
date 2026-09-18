mod common;

use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use catalyrst_credits::handlers::prices::{quote, QuoteBody};

const COLLECTION: &str = "0xeede64bfaf8055492aa500846ec7c6e6a9f533d5";
const ITEM_ID: &str = "4";
const OTHER_COLLECTION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MARKETPLACE_V2: &str = "0x480a0f4e360e8964e68858dd231c2922f1df45ef";
/// `ITEM_ID << 216`: the token id a collections-v2 NFT minted from item 4 carries.
const ITEM_TOKEN_ID: &str = "421249166674228746791672110734681729275580381602196445017243910144";

fn v2_listing(contract: &str, price_wei: &str) -> serde_json::Value {
    json!({
        "id": format!("{contract}-{ITEM_TOKEN_ID}"),
        "contractAddress": contract,
        "tokenId": ITEM_TOKEN_ID,
        "marketplaceAddress": MARKETPLACE_V2,
        "status": "open",
        "price": price_wei,
        "expiresAt": 0,
    })
}

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
            get({
                let orders = open_orders.clone();
                move || {
                    let orders = orders.clone();
                    async move { Json(json!({ "data": orders })) }
                }
            }),
        )
        .route(
            "/v1/orders/open-by-items",
            get(move || {
                let orders = open_orders.clone();
                async move { Json(json!({ "data": orders, "total": "0" })) }
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

/// The batched open-by-items answer carries every requested collection's orders in one
/// list: the item must be priced from its own collection's cheapest listing, not from a
/// cheaper order of another collection whose token id happens to encode the same item.
#[tokio::test]
async fn listed_item_quotes_its_own_collections_cheapest_listing() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let mock = spawn_market_mock(json!([
        v2_listing(OTHER_COLLECTION, "100000000000000000"),
        v2_listing(COLLECTION, "2000000000000000000"),
        v2_listing(COLLECTION, "1000000000000000000"),
    ]))
    .await;
    let state = common::test_state_with_market(pool, false, &format!("http://{mock}"), "secondary");

    let out = quote(State(state), quote_body()).await.unwrap();
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        v["items"][0]["credits"], "5",
        "1 MANA listing * 0.5 USD / 0.10 per credit, got {v}"
    );
}

#[tokio::test]
async fn auto_prefers_a_listing_cheaper_than_the_mint() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let mock = spawn_market_mock(json!([v2_listing(COLLECTION, "1000000000000000000")])).await;
    let state = common::test_state_with_market(pool, false, &format!("http://{mock}"), "auto");

    let out = quote(State(state), quote_body()).await.unwrap();
    let v = serde_json::to_value(&out.0).unwrap();
    assert_eq!(
        v["items"][0]["credits"], "5",
        "the 1 MANA listing undercuts the 2.5 MANA mint, got {v}"
    );
}
