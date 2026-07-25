use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state};

const API_KEY: &str = "test-onboarding-key";

async fn app() -> Router {
    std::env::set_var("ONBOARDING_API_KEY", API_KEY);
    let cfg = Config::from_env().expect("config from env defaults");
    let state = build_state(&cfg).await.expect("build state");
    api_router().with_state(state)
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Body,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let resp = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
async fn checkpoint_requires_bearer() {
    let app = app().await;
    let body = r#"{"checkpointId":1,"userIdentifier":"a@b.com","identifierType":"email","action":"reached","email":"a@b.com"}"#;

    let (status, _) = send(
        &app,
        "POST",
        "/onboarding/checkpoint",
        None,
        Body::from(body),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(
        &app,
        "POST",
        "/onboarding/checkpoint",
        Some("wrong"),
        Body::from(body),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pending_nudges_requires_bearer() {
    let app = app().await;
    let (status, _) = send(
        &app,
        "GET",
        "/onboarding/pending-nudges",
        None,
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn checkpoint_records_but_is_not_yet_due_over_http() {
    let app = app().await;
    let body = r#"{"checkpointId":1,"userIdentifier":"a@b.com","identifierType":"email","action":"reached","email":"a@b.com"}"#;

    let (status, json) = send(
        &app,
        "POST",
        "/onboarding/checkpoint",
        Some(API_KEY),
        Body::from(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], serde_json::json!(true));

    let (status, json) = send(
        &app,
        "GET",
        "/onboarding/pending-nudges?sequence=1",
        Some(API_KEY),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["sequence"], serde_json::json!(1));
    assert_eq!(json["count"], serde_json::json!(0));
}

#[tokio::test]
async fn checkpoint_rejects_invalid_payload() {
    let app = app().await;
    let body = r#"{"checkpointId":99,"userIdentifier":"a@b.com","identifierType":"email","action":"reached"}"#;
    let (status, _) = send(
        &app,
        "POST",
        "/onboarding/checkpoint",
        Some(API_KEY),
        Body::from(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body = r#"{"checkpointId":1,"userIdentifier":"a@b.com","identifierType":"email","action":"reached","bogus":true}"#;
    let (status, _) = send(
        &app,
        "POST",
        "/onboarding/checkpoint",
        Some(API_KEY),
        Body::from(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn pending_nudges_rejects_bad_sequence() {
    let app = app().await;
    let (status, _) = send(
        &app,
        "GET",
        "/onboarding/pending-nudges?sequence=9",
        Some(API_KEY),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
