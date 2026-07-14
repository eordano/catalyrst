use catalyrst_archipelago::config::Config;
use catalyrst_archipelago::{api_router, build_state};
use serde_json::Value;

const VICTIM: &str = "0x1234567890abcdef1234567890abcdef12345678";

fn deployment_config_without_auth_env() -> Config {
    std::env::remove_var("ARCHIPELAGO_REQUIRE_AUTH");
    std::env::remove_var("ARCHIPELAGO_CONFIG_PATH");
    std::env::remove_var("DENY_LIST_URL");
    Config::from_env().expect("config from a deployment that sets no auth env var")
}

async fn spawn(cfg: Config) -> u16 {
    let state = build_state(&cfg).await.expect("state");
    let app = api_router().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

async fn spoof_heartbeat(port: u16, address: &str) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/heartbeat"))
        .json(&serde_json::json!({
            "address": address,
            "position": [640.0, 0.0, -320.0],
            "parcel": [40, -20],
            "realm": "catalyrst",
        }))
        .send()
        .await
        .expect("heartbeat request");
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.expect("json body");
    (status, body)
}

async fn peer_addresses(port: u16) -> Vec<String> {
    let body: Value = reqwest::get(format!("http://127.0.0.1:{port}/peers"))
        .await
        .expect("peers request")
        .json()
        .await
        .expect("peers json");
    body["peers"]
        .as_array()
        .expect("peers array")
        .iter()
        .map(|p| p["address"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn unsigned_heartbeat_cannot_place_a_wallet_on_a_default_deployment() {
    let cfg = deployment_config_without_auth_env();
    let port = spawn(cfg).await;

    let (status, body) = spoof_heartbeat(port, VICTIM).await;
    assert_eq!(
        status, 401,
        "unsigned heartbeat for {VICTIM} was accepted: {body}"
    );

    let peers = peer_addresses(port).await;
    assert!(
        peers.is_empty(),
        "unsigned heartbeat wrote presence for {VICTIM}: {peers:?}"
    );

    let mut opted_out = deployment_config_without_auth_env();
    opted_out.auth.require_signed_challenge = false;
    let open_port = spawn(opted_out).await;

    let (status, body) = spoof_heartbeat(open_port, VICTIM).await;
    assert_eq!(status, 200, "opt-out deployment rejected heartbeat: {body}");
    assert_eq!(peer_addresses(open_port).await, vec![VICTIM.to_string()]);
}
