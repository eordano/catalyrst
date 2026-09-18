use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use catalyrst_credits::ports::pricing::PricingClient;

const COLLECTION: &str = "0xeede64bfaf8055492aa500846ec7c6e6a9f533d5";

async fn spawn_market_mock(orders_hits: Arc<AtomicUsize>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/v1/items",
            get(move || async move {
                Json(json!({
                    "data": [{
                        "id": format!("{COLLECTION}-1"),
                        "category": "wearable",
                        "price": "2500000000000000000",
                        "urn": format!("urn:decentraland:matic:collections-v2:{COLLECTION}:1"),
                        "contractAddress": COLLECTION,
                        "isOnSale": true,
                    }]
                }))
            }),
        )
        .route(
            "/v1/orders",
            get(move || {
                let hits = orders_hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    Json(json!({ "data": [] }))
                }
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn pricing_for(addr: SocketAddr) -> PricingClient {
    let base = format!("http://{addr}");
    PricingClient::new(reqwest::Client::new(), base.clone(), base, 0, 3600)
}

#[tokio::test]
async fn quote_lookups_share_one_open_orders_scan_per_collection() {
    let hits = Arc::new(AtomicUsize::new(0));
    let pricing = pricing_for(spawn_market_mock(hits.clone()).await);

    for item_id in ["1", "2", "3", "4"] {
        let basis = pricing
            .fetch_charge_basis_quote(COLLECTION, item_id, "auto")
            .await
            .unwrap();
        assert_eq!(basis.basis_wei, "2500000000000000000");
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "four quoted items of one collection read the open-orders page once"
    );
}

#[tokio::test]
async fn concurrent_quote_lookups_elect_one_orders_fetch() {
    let hits = Arc::new(AtomicUsize::new(0));
    let pricing = pricing_for(spawn_market_mock(hits.clone()).await);

    let jobs = (0..16).map(|i| {
        let pricing = pricing.clone();
        async move {
            pricing
                .fetch_charge_basis_quote(COLLECTION, &i.to_string(), "auto")
                .await
                .unwrap()
        }
    });
    let bases = futures::future::join_all(jobs).await;
    assert_eq!(bases.len(), 16);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "concurrent misses on the same page collapse into one market request"
    );
}

#[tokio::test]
async fn the_charge_path_still_reads_the_live_order_book() {
    let hits = Arc::new(AtomicUsize::new(0));
    let pricing = pricing_for(spawn_market_mock(hits.clone()).await);

    pricing
        .fetch_charge_basis_quote(COLLECTION, "1", "auto")
        .await
        .unwrap();
    pricing
        .fetch_charge_basis(COLLECTION, "1", "auto")
        .await
        .unwrap();
    pricing
        .fetch_charge_basis(COLLECTION, "1", "auto")
        .await
        .unwrap();
    assert_eq!(
        hits.load(Ordering::SeqCst),
        3,
        "every checkout-time basis lookup goes to the market, only the quote path is memoized"
    );
}
