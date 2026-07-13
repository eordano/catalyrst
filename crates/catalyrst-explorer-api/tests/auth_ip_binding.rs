use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state};

async fn app() -> Router {
    let cfg = Config::from_env().expect("config from env defaults");
    let state = build_state(&cfg).await.expect("build state");
    api_router().with_state(state)
}

async fn create_identity(app: &Router, real_ip: &str) -> String {
    let body = r#"{"identity":{"authChain":[{"type":"SIGNER","payload":"0x1111111111111111111111111111111111111111","signature":""}]}}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/auth/identities")
        .header("content-type", "application/json")
        .header("x-real-ip", real_ip)
        .body(Body::from(body))
        .unwrap();
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
