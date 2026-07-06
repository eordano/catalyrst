use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::Form;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use catalyrst_credits::provider::CaptchaProvider;

const SECRET: &str = "test-secret";
const GOOD_TOKEN: &str = "valid-token";

async fn siteverify(Form(form): Form<HashMap<String, String>>) -> Json<Value> {
    if form.get("secret").map(String::as_str) != Some(SECRET) {
        return Json(json!({ "success": false, "error-codes": ["invalid-input-secret"] }));
    }
    let ok = form.get("response").map(String::as_str) == Some(GOOD_TOKEN);
    Json(json!({ "success": ok }))
}

async fn spawn_mock() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route("/siteverify", post(siteverify));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn provider_for(addr: SocketAddr) -> CaptchaProvider {
    CaptchaProvider::new(
        SECRET.to_string(),
        format!("http://{addr}/siteverify"),
        reqwest::Client::new(),
    )
}

#[tokio::test]
async fn valid_token_verifies() {
    let addr = spawn_mock().await;
    let provider = provider_for(addr);
    assert!(provider.verify(GOOD_TOKEN, None).await.unwrap());
}

#[tokio::test]
async fn invalid_token_rejected() {
    let addr = spawn_mock().await;
    let provider = provider_for(addr);
    assert!(!provider
        .verify("wrong-token", Some("1.2.3.4"))
        .await
        .unwrap());
}

#[tokio::test]
async fn wrong_secret_rejected() {
    let addr = spawn_mock().await;
    let provider = CaptchaProvider::new(
        "bad-secret".to_string(),
        format!("http://{addr}/siteverify"),
        reqwest::Client::new(),
    );
    assert!(!provider.verify(GOOD_TOKEN, None).await.unwrap());
}

#[tokio::test]
async fn unreachable_provider_errors() {
    let provider = CaptchaProvider::new(
        SECRET.to_string(),
        "http://127.0.0.1:1/siteverify".to_string(),
        reqwest::Client::new(),
    );
    assert!(provider.verify(GOOD_TOKEN, None).await.is_err());
}
