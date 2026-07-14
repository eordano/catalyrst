mod support;

use axum::body::{to_bytes, Body};
use axum::http::request::Builder;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_crypto::{build_payload_v6, create_simple_auth_chain, Wallet};
use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state};

const OWNER_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const EPHEMERAL_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const FAR_FUTURE: &str = "2100-01-01T00:00:00.000Z";

async fn app() -> Router {
    let cfg = Config::from_env().expect("config from env defaults");
    let state = build_state(&cfg).await.expect("build state");
    api_router().with_state(state)
}

fn ephemeral_payload(ephemeral_address: &str) -> String {
    format!("Decentraland Login\nEphemeral address: {ephemeral_address}\nExpiration: {FAR_FUTURE}")
}

fn identity_body(owner: &Wallet, ephemeral: &Wallet) -> Value {
    let payload = ephemeral_payload(&ephemeral.address());
    let ephemeral_sig = owner.sign_message(payload.as_bytes()).unwrap();
    let identity = json!({
        "ephemeralIdentity": {
            "address": ephemeral.address(),
            "publicKey": "0x",
            "privateKey": EPHEMERAL_KEY,
        },
        "expiration": FAR_FUTURE,
        "authChain": [
            { "type": "SIGNER", "payload": owner.address(), "signature": "" },
            { "type": "ECDSA_EPHEMERAL", "payload": payload, "signature": ephemeral_sig },
        ],
    });
    json!({ "identity": identity })
}

fn signed_post(signer: &Wallet, real_ip: &str, body: &Value) -> Request<Body> {
    let ts = chrono::Utc::now().timestamp_millis().to_string();
    let payload = build_payload_v6("post", "/identities", &ts, "{}");
    let chain = create_simple_auth_chain(signer, &payload).unwrap();
    let mut builder: Builder = Request::builder()
        .method("POST")
        .uri("/auth/identities")
        .header("content-type", "application/json")
        .header("x-real-ip", real_ip)
        .header("x-identity-timestamp", &ts)
        .header("x-identity-metadata", "{}");
    for (i, link) in chain.as_array().unwrap().iter().enumerate() {
        builder = builder.header(
            format!("x-identity-auth-chain-{i}"),
            serde_json::to_string(link).unwrap(),
        );
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn create_identity(app: &Router, real_ip: &str) -> String {
    let (_, req) = support::signed_identity_post(real_ip);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["identityId"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn spoofed_client_headers_do_not_bypass_ip_binding() {
    let app = app().await;
    let id = create_identity(&app, "203.0.113.9").await;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/auth/identities/{id}"))
        .header("x-real-ip", "198.51.100.7")
        .header("true-client-ip", "203.0.113.9")
        .header("cf-connecting-ip", "203.0.113.9")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn matching_proxy_ip_consumes_identity() {
    let app = app().await;
    let id = create_identity(&app, "203.0.113.9").await;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/auth/identities/{id}"))
        .header("x-real-ip", "203.0.113.9")
        .header("true-client-ip", "6.6.6.6")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn unsigned_post_is_refused() {
    let app = app().await;
    let body = r#"{"identity":{"authChain":[{"type":"SIGNER","payload":"0x1111111111111111111111111111111111111111","signature":""}]}}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/auth/identities")
        .header("content-type", "application/json")
        .header("x-real-ip", "203.0.113.9")
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::CREATED);
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signed_fetch_by_a_stranger_is_refused() {
    let app = app().await;
    let owner = Wallet::from_hex(OWNER_KEY).unwrap();
    let ephemeral = Wallet::from_hex(EPHEMERAL_KEY).unwrap();
    let body = identity_body(&owner, &ephemeral);
    let stranger = ephemeral;
    let req = signed_post(&stranger, "203.0.113.9", &body);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn mismatched_ephemeral_private_key_is_refused() {
    let app = app().await;
    let owner = Wallet::from_hex(OWNER_KEY).unwrap();
    let ephemeral = Wallet::from_hex(EPHEMERAL_KEY).unwrap();
    let mut body = identity_body(&owner, &ephemeral);
    body["identity"]["ephemeralIdentity"]["privateKey"] = json!(OWNER_KEY);
    let req = signed_post(&owner, "203.0.113.9", &body);
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
