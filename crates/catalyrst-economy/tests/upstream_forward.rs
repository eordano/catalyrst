mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;

const COLLECTION: &str = "0x7ad72b9f944ea9793cf4055d88f81138cc2c63a0";
const BUYER: &str = "0x3333333333333333333333333333333333333333";

#[derive(Clone, Default)]
struct Captured {
    body: Vec<u8>,
    content_type: Option<String>,
    authorization: Option<String>,
}

#[derive(Clone)]
struct MockBehaviour {
    status: u16,
    body: &'static str,
    delay: Duration,
}

type Seen = Arc<Mutex<Option<Captured>>>;

/// A stand-in for transactions-api.decentraland.org: one canned response on
/// POST /v1/transactions, recording what the forwarder actually sent.
async fn spawn_mock_upstream(behaviour: MockBehaviour, seen: Seen) -> String {
    async fn handle(
        State((behaviour, seen)): State<(MockBehaviour, Seen)>,
        headers: HeaderMap,
        body: Bytes,
    ) -> axum::response::Response {
        *seen.lock().unwrap() = Some(Captured {
            body: body.to_vec(),
            content_type: headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            authorization: headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        });
        tokio::time::sleep(behaviour.delay).await;
        (
            axum::http::StatusCode::from_u16(behaviour.status).unwrap(),
            [("content-type", "application/json")],
            behaviour.body,
        )
            .into_response()
    }

    let app = axum::Router::new()
        .route("/v1/transactions", axum::routing::post(handle))
        .with_state((behaviour, seen));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let addr = listener.local_addr().expect("mock upstream addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn tx_body(from: &str, contract: &str, calldata: &str) -> String {
    serde_json::json!({
        "transactionData": { "from": from, "params": [contract, calldata] }
    })
    .to_string()
}

async fn post_with_auth(base: &str, body: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/v1/transactions"))
        .header("content-type", "application/json")
        .header("authorization", "Bearer loki-local-secret")
        .body(body.to_string())
        .send()
        .await
        .expect("post /v1/transactions");
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    (status, text)
}

async fn confirmed_hash(pool: &sqlx::PgPool, addr: &str) -> Option<String> {
    sqlx::query_scalar("SELECT tx_hash FROM transactions WHERE user_address = $1")
        .bind(addr.to_lowercase())
        .fetch_one(pool)
        .await
        .expect("tx_hash row")
}

#[tokio::test]
async fn upstream_200_passes_through_verbatim_and_confirms_the_slot() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    const UPSTREAM_BODY: &str = r#"{"ok":true,"txHash":"0x00000000000000000000000000000000000000000000000000000000000000aa","upstreamOnlyKey":true}"#;
    let seen: Seen = Arc::new(Mutex::new(None));
    let mock = spawn_mock_upstream(
        MockBehaviour {
            status: 200,
            body: UPSTREAM_BODY,
            delay: Duration::ZERO,
        },
        seen.clone(),
    )
    .await;
    let base = support::spawn_app_upstream(&scratch, 10, &mock, Duration::from_secs(5)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    let hash = confirmed_hash(&scratch.pool, BUYER).await;
    let captured = seen.lock().unwrap().clone().expect("upstream was called");
    scratch.cleanup().await;

    assert_eq!(status, 200, "body {text}");
    assert_eq!(
        text, UPSTREAM_BODY,
        "the upstream body must reach the client byte-for-byte, not re-wrapped"
    );
    assert_eq!(
        captured.body,
        body.as_bytes(),
        "the validated request body must be forwarded verbatim"
    );
    assert_eq!(
        captured.content_type.as_deref(),
        Some("application/json"),
        "the inbound content-type is preserved"
    );
    assert_eq!(
        captured.authorization, None,
        "node-local auth headers must never be forwarded upstream"
    );
    assert_eq!(rows, 1, "the relay consumed one quota slot");
    assert_eq!(
        hash.as_deref(),
        Some("0x00000000000000000000000000000000000000000000000000000000000000aa"),
        "the reservation is confirmed with the upstream txHash"
    );
}

#[tokio::test]
async fn upstream_400_passes_through_verbatim_and_refunds_the_slot() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    const UPSTREAM_BODY: &str = r#"{"ok":false,"message":"Invalid transaction data. Errors: [upstream-shaped]","code":"invalid_schema"}"#;
    let seen: Seen = Arc::new(Mutex::new(None));
    let mock = spawn_mock_upstream(
        MockBehaviour {
            status: 400,
            body: UPSTREAM_BODY,
            delay: Duration::ZERO,
        },
        seen.clone(),
    )
    .await;
    let base = support::spawn_app_upstream(&scratch, 10, &mock, Duration::from_secs(5)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    scratch.cleanup().await;

    assert_eq!(status, 400, "body {text}");
    assert_eq!(
        text, UPSTREAM_BODY,
        "the upstream rejection must reach the client byte-for-byte"
    );
    assert_eq!(
        rows, 0,
        "an upstream rejection broadcast nothing, so the quota slot is refunded"
    );
}

#[tokio::test]
async fn upstream_timeout_keeps_the_slot_even_when_the_broadcast_lands_late() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    // The mock receives the request, then answers 200 with a txHash long after
    // the forwarder's timeout: the transaction lands upstream even though the
    // client saw a timeout.
    let seen: Seen = Arc::new(Mutex::new(None));
    let mock = spawn_mock_upstream(
        MockBehaviour {
            status: 200,
            body: r#"{"ok":true,"txHash":"0xtoolate"}"#,
            delay: Duration::from_secs(5),
        },
        seen.clone(),
    )
    .await;
    let base = support::spawn_app_upstream(&scratch, 10, &mock, Duration::from_millis(200)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    let hash = confirmed_hash(&scratch.pool, BUYER).await;
    let request_reached_upstream = seen.lock().unwrap().is_some();
    scratch.cleanup().await;

    assert!(
        request_reached_upstream,
        "the request body went out, so the broadcast outcome is indeterminate"
    );
    assert_eq!(status, 504, "body {text}");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("error envelope is JSON");
    assert_eq!(parsed["ok"], false, "body {text}");
    assert_eq!(parsed["error"], "unknown", "body {text}");
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or_default()
            .contains("broadcast outcome is unknown"),
        "body {text}"
    );
    assert_eq!(
        rows, 1,
        "a timed-out forward may still have broadcast, so the quota slot stays consumed"
    );
    assert_eq!(
        hash, None,
        "the slot is kept but unconfirmed: no txHash was observed"
    );
}

#[tokio::test]
async fn upstream_connection_refused_refunds_the_slot() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    // Bind a port, then drop the listener: connecting to it is refused, so the
    // request provably never reached an upstream.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind throwaway port");
    let refused = format!("http://{}", listener.local_addr().expect("addr"));
    drop(listener);
    let base = support::spawn_app_upstream(&scratch, 10, &refused, Duration::from_secs(5)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    scratch.cleanup().await;

    assert_eq!(status, 503, "body {text}");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("error envelope is JSON");
    assert_eq!(parsed["ok"], false, "body {text}");
    assert_eq!(parsed["error"], "unknown", "body {text}");
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or_default()
            .contains("could not be reached"),
        "body {text}"
    );
    assert_eq!(
        rows, 0,
        "a refused connection provably sent nothing, so the quota slot is refunded"
    );
}

#[tokio::test]
async fn upstream_429_passes_through_verbatim_and_refunds_the_slot() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    const UPSTREAM_BODY: &str = r#"{"ok":false,"message":"Max amount of transactions reached for address","code":"quota_reached"}"#;
    let seen: Seen = Arc::new(Mutex::new(None));
    let mock = spawn_mock_upstream(
        MockBehaviour {
            status: 429,
            body: UPSTREAM_BODY,
            delay: Duration::ZERO,
        },
        seen.clone(),
    )
    .await;
    let base = support::spawn_app_upstream(&scratch, 10, &mock, Duration::from_secs(5)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    scratch.cleanup().await;

    assert_eq!(status, 429, "body {text}");
    assert_eq!(
        text, UPSTREAM_BODY,
        "the upstream rate-limit rejection must reach the client byte-for-byte"
    );
    assert_eq!(
        rows, 0,
        "an upstream 429 rejects before any broadcast, so the quota slot is refunded"
    );
}

#[tokio::test]
async fn upstream_524_keeps_the_slot_as_indeterminate() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;

    // 524 is Cloudflare's origin-timeout: the request reached the intermediary
    // and the origin may have broadcast before the intermediary gave up.
    const UPSTREAM_BODY: &str = "<html>524: a timeout occurred</html>";
    let seen: Seen = Arc::new(Mutex::new(None));
    let mock = spawn_mock_upstream(
        MockBehaviour {
            status: 524,
            body: UPSTREAM_BODY,
            delay: Duration::ZERO,
        },
        seen.clone(),
    )
    .await;
    let base = support::spawn_app_upstream(&scratch, 10, &mock, Duration::from_secs(5)).await;

    let body = tx_body(BUYER, COLLECTION, &support::split_sig_calldata(BUYER));
    let (status, text) = post_with_auth(&base, &body).await;

    let rows = support::row_count(&scratch.pool, BUYER).await;
    let hash = confirmed_hash(&scratch.pool, BUYER).await;
    scratch.cleanup().await;

    assert_eq!(status, 524, "body {text}");
    assert_eq!(
        text, UPSTREAM_BODY,
        "the intermediary error must reach the client byte-for-byte"
    );
    assert_eq!(
        rows, 1,
        "a Cloudflare origin-timeout is broadcast-indeterminate, so the slot stays consumed"
    );
    assert_eq!(
        hash, None,
        "the slot is kept but unconfirmed: no txHash was observed"
    );
}
