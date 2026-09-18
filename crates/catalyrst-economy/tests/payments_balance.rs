mod support;

use alloy::signers::local::PrivateKeySigner;
use serde_json::{json, Value};

async fn get(base: &str, path: &str) -> (u16, Value) {
    let res = reqwest::get(format!("{base}{path}"))
        .await
        .expect("request");
    let status = res.status().as_u16();
    let body = res.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn balance_endpoint_reads_the_mana_contract_through_the_relayer_rpc() {
    let base = support::spawn_app_chain_only(true).await;
    let wallet = format!("{:#x}", PrivateKeySigner::random().address());

    let (status, body) = get(&base, &format!("/v1/payments/balance/{wallet}")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({ "balance": support::TEST_MANA_BALANCE_WEI.to_string() })
    );

    let (status, body) = get(
        &base,
        &format!("/v1/payments/balance/{}", support::CHAIN_EMPTY_WALLET),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "balance": "0" }));

    let (status, body) = get(&base, &format!("/v1/payments/nonce/{wallet}")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "nonce": "0" }));
}

#[tokio::test]
async fn balance_endpoint_rejects_a_malformed_address() {
    let base = support::spawn_app_chain_only(true).await;
    let (status, body) = get(&base, "/v1/payments/balance/not-an-address").await;
    assert_eq!(status, 400, "{body}");
}

#[tokio::test]
async fn balance_endpoint_is_unavailable_without_an_rpc_provider() {
    let base = support::spawn_app_chain_only(false).await;
    let (status, body) = get(
        &base,
        "/v1/payments/balance/0x951bb66ce4a5d4b1c667e386af5313753d14ba2e",
    )
    .await;
    assert_eq!(status, 503, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("cannot read the MANA balance"),
        "{body}"
    );
}
