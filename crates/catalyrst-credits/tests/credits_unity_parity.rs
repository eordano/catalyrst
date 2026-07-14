mod common;

use serde_json::{json, Value};

use catalyrst_credits::handlers::authorize::{
    AuthorizeCreditOut, AuthorizedCreditOut, ReleaseIntentsOut,
};
use catalyrst_credits::handlers::orders::{CheckoutSessionOut, CreditsOrderStatusOut};

fn adr44() -> Value {
    json!({
        "error": "Invalid Auth Chain",
        "message": "This endpoint requires a signed fetch request. See ADR-44.",
    })
}

fn keys_of(v: &Value) -> Vec<String> {
    let mut ks: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
    ks.sort();
    ks
}

#[test]
fn checkout_session_out_matches_unity_checkout_response() {
    let v = serde_json::to_value(CheckoutSessionOut {
        order_id: "ord_deadbeef".into(),
        url: "https://checkout.example/c/cs_test_123".into(),
    })
    .unwrap();
    assert_eq!(
        v,
        json!({ "orderId": "ord_deadbeef", "url": "https://checkout.example/c/cs_test_123" })
    );
    assert_eq!(keys_of(&v), vec!["orderId", "url"]);
}

#[test]
fn order_status_out_matches_unity_status_response_for_every_status() {
    for status in ["processing", "credited", "failed", "abandoned"] {
        let v = serde_json::to_value(CreditsOrderStatusOut {
            status: status.to_string(),
            credits_granted: if status == "credited" { 100 } else { 0 },
            new_balance: 250,
            error: if status == "failed" {
                "card declined".to_string()
            } else {
                String::new()
            },
        })
        .unwrap();
        assert_eq!(
            keys_of(&v),
            vec!["creditsGranted", "error", "newBalance", "status"]
        );
        assert!(
            ["processing", "credited", "failed", "abandoned"]
                .contains(&v["status"].as_str().unwrap()),
            "status vocabulary is load-bearing: {v}"
        );
        assert!(v["creditsGranted"].is_i64());
        assert!(v["newBalance"].is_i64());
        assert!(v["error"].is_string());
    }
}

#[test]
fn authorize_out_matches_unity_authorize_credit_response() {
    let v = serde_json::to_value(AuthorizeCreditOut {
        credit: AuthorizedCreditOut {
            id: "0x1234".into(),
            amount: "1000000000000000000".into(),
            available_amount: "1000000000000000000".into(),
            expires_at: 1_690_000_600_000,
            signature: "0xsig".into(),
            contract: "0xcreditsmanager".into(),
        },
        max_credited_value: "1000000000000000000".into(),
        usd_cents: 500,
        oracle_rate: "420000000000000000".into(),
    })
    .unwrap();
    assert_eq!(
        v,
        json!({
            "credit": {
                "id": "0x1234",
                "amount": "1000000000000000000",
                "availableAmount": "1000000000000000000",
                "expiresAt": 1_690_000_600_000i64,
                "signature": "0xsig",
                "contract": "0xcreditsmanager",
            },
            "maxCreditedValue": "1000000000000000000",
            "usdCents": 500,
            "oracleRate": "420000000000000000",
        })
    );
    assert_eq!(
        keys_of(&v),
        vec!["credit", "maxCreditedValue", "oracleRate", "usdCents"]
    );
    assert_eq!(
        keys_of(&v["credit"]),
        vec![
            "amount",
            "availableAmount",
            "contract",
            "expiresAt",
            "id",
            "signature"
        ]
    );
}

#[test]
fn release_out_is_minimal_ok_true() {
    let v = serde_json::to_value(ReleaseIntentsOut { ok: true }).unwrap();
    assert_eq!(v, json!({ "ok": true }));
    assert_eq!(keys_of(&v), vec!["ok"]);
}

async fn serve() -> std::net::SocketAddr {
    let app =
        catalyrst_credits::api_router().with_state(common::test_state(common::lazy_pool(), false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn every_new_route_is_mounted_and_serves_the_adr44_envelope_unauthenticated() {
    let addr = serve().await;
    let client = reqwest::Client::new();

    let checkout = client
        .post(format!("http://{addr}/credits/checkout"))
        .send()
        .await
        .unwrap();
    assert_eq!(checkout.status(), 400, "POST /credits/checkout");
    assert_eq!(checkout.json::<Value>().await.unwrap(), adr44());

    let orders = client
        .get(format!("http://{addr}/credits/orders/ord_x"))
        .send()
        .await
        .unwrap();
    assert_eq!(orders.status(), 400, "GET /credits/orders/{{id}}");
    assert_eq!(orders.json::<Value>().await.unwrap(), adr44());

    let authorize = client
        .post(format!("http://{addr}/credits/authorize"))
        .send()
        .await
        .unwrap();
    assert_eq!(authorize.status(), 400, "POST /credits/authorize");
    assert_eq!(authorize.json::<Value>().await.unwrap(), adr44());

    let cancel = client
        .post(format!("http://{addr}/credits/authorize/cancel"))
        .send()
        .await
        .unwrap();
    assert_eq!(cancel.status(), 400, "POST /credits/authorize/cancel");
    assert_eq!(cancel.json::<Value>().await.unwrap(), adr44());
}

#[tokio::test]
async fn preexisting_signed_route_also_flipped_to_the_adr44_envelope() {
    let addr = serve().await;
    let resp = reqwest::get(format!(
        "http://{addr}/users/0x8d6f63e382d73cf53858864f673f39e9ff915a1e/credits"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(resp.json::<Value>().await.unwrap(), adr44());
}

#[tokio::test]
async fn order_status_happy_path_and_foreign_signer_404() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);

    let order_id = format!("ord_{}", "a".repeat(64));
    let pack = catalyrst_credits::ports::packs::PackRow {
        sku: "pack_100".into(),
        title: "100 Credits".into(),
        credits: "100".into(),
        price_cents: 999,
        currency: "usd".into(),
        sort_order: 0,
    };
    let state = common::test_state(pool.clone(), false);
    state
        .credits
        .insert_pending_order(&order_id, &addr, &pack, &format!("cs_{}", order_id))
        .await
        .unwrap();

    let app = catalyrst_credits::api_router().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::new();

    let path = format!("/credits/orders/{order_id}");
    let headers = common::signed_headers(&wallet, "get", &path).await;
    let resp = client
        .get(format!("http://{local}{path}"))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: Value = resp.json().await.unwrap();
    assert_eq!(
        keys_of(&v),
        vec!["creditsGranted", "error", "newBalance", "status"]
    );
    assert_eq!(v["status"], "processing");
    assert_eq!(v["creditsGranted"], 0);
    assert_eq!(v["error"], "");

    let foreign = common::scratch_wallet();
    let fheaders = common::signed_headers(&foreign, "get", &path).await;
    let fresp = client
        .get(format!("http://{local}{path}"))
        .headers(fheaders)
        .send()
        .await
        .unwrap();
    assert_eq!(
        fresp.status(),
        404,
        "an order is only visible to its own signer"
    );
}

#[tokio::test]
async fn cancel_releases_only_the_signers_authorized_intents() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);

    let id = format!("0x{}", "b".repeat(64));
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
    let state = common::test_state(pool.clone(), false);
    state
        .credits
        .insert_authorization(&catalyrst_credits::ports::authorize::NewAuthorization {
            id: &id,
            address: &addr,
            usd_cents: 500,
            amount_wei: "1000000000000000000",
            trade_id: Some("trade-1"),
            contract_address: None,
            item_id: None,
            source: None,
            expires_at,
        })
        .await
        .unwrap();

    let app = catalyrst_credits::api_router().with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let path = "/credits/authorize/cancel";
    let headers = common::signed_headers(&wallet, "post", path).await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{local}{path}"))
        .headers(headers)
        .json(&json!({ "salts": [id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.json::<Value>().await.unwrap(), json!({ "ok": true }));

    let row = state.credits.get_authorization(&id).await.unwrap().unwrap();
    assert_eq!(row.status, "released");
}
