mod support;

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use catalyrst_comms::api_router;
use sqlx::PgPool;
use tower::ServiceExt;

#[tokio::test]
async fn livekit_webhook_acknowledgement_never_reflects_submitted_metadata() {
    const CANARY: &str = "livekit-webhook-private-participant-metadata-canary";

    let pool = PgPool::connect_lazy("postgres://postgres@127.0.0.1:1/postgres")
        .expect("lazy pool never connects in this test");
    let mut state = support::test_state(pool, None, None, "squid_marketplace");
    Arc::get_mut(&mut state)
        .expect("new test state has one owner")
        .livekit_configured = false;
    let app = api_router(state.clone()).with_state(state);
    let payload = format!(r#"{{"event":"unknown","participant":{{"metadata":"{CANARY}"}}}}"#);

    let response = app
        .oneshot(
            Request::post("/livekit-webhook")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert!(
        body.is_empty(),
        "webhook acknowledgement reflected submitted bytes"
    );
    assert!(!String::from_utf8_lossy(&body).contains(CANARY));
}
