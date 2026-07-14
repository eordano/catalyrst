use std::str::FromStr;
use std::sync::{Arc, Mutex};

use alloy::primitives::{Address, Signature};
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
use catalyrst_crypto::signed_fetch::build_payload;
use catalyrst_governance::config::SnapshotConfig;
use catalyrst_governance::snapshot::eip712::ProposalMessage;
use catalyrst_governance::snapshot::SnapshotGate;
use catalyrst_governance::write_router;

const POSTER_KEY: &str = "0x0101010101010101010101010101010101010101010101010101010101010101";
const AUTHOR_KEY: &str = "0x0202020202020202020202020202020202020202020202020202020202020202";
const MOCK_BLOCK: u64 = 22_000_000;

type Captured = Arc<Mutex<Vec<Value>>>;

async fn capture_envelope(
    State(captured): State<Captured>,
    Json(body): Json<Value>,
) -> Json<Value> {
    captured.lock().unwrap().push(body);
    Json(json!({
        "id": "0xmockproposalid",
        "ipfs": "bafkreimockipfscid",
        "relayer": { "address": "0x0000000000000000000000000000000000000009", "receipt": "0xdeadbeef" },
    }))
}

async fn block_number() -> Json<Value> {
    Json(json!({ "jsonrpc": "2.0", "id": 1, "result": format!("0x{MOCK_BLOCK:x}") }))
}

async fn mock_snapshot() -> (String, Captured) {
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/msg", post(capture_envelope))
        .with_state(captured.clone())
        .merge(Router::new().route("/rpc", post(block_number)));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), captured)
}

fn ready_config(base_url: &str) -> SnapshotConfig {
    SnapshotConfig {
        private_key: Some(POSTER_KEY.to_string()),
        space: Some("gate.dcl.eth".to_string()),
        space_council: Some("council.gate.dcl.eth".to_string()),
        api_url: Some(format!("{base_url}/msg")),
        block_rpc_url: Some(format!("{base_url}/rpc")),
        duration_seconds: 600,
        ..SnapshotConfig::default()
    }
}

fn catalyst_payload() -> Value {
    json!({
        "request": "add",
        "type": "catalyst_add",
        "owner": "0x3333333333333333333333333333333333333333",
        "domain": "peer.example.org",
        "description": "A new node for the network.",
        "coAuthors": [],
    })
}

fn signed_request(kind: &str, payload: &Value) -> Request<Body> {
    let wallet = Wallet::from_hex(AUTHOR_KEY).expect("author wallet");
    let path = format!("/proposals/{kind}");
    let timestamp = (chrono::Utc::now().timestamp_millis()).to_string();
    let sign_payload = build_payload("post", &path, &timestamp, "{}");
    let chain = create_simple_auth_chain(&wallet, &sign_payload).expect("auth chain");
    let links = chain.as_array().expect("auth chain links");

    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("x-identity-timestamp", timestamp)
        .header("x-identity-metadata", "{}");
    for (index, link) in links.iter().enumerate() {
        builder = builder.header(format!("x-identity-auth-chain-{index}"), link.to_string());
    }
    builder
        .body(Body::from(serde_json::to_vec(payload).unwrap()))
        .unwrap()
}

fn unsigned_request(kind: &str, payload: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/proposals/{kind}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(payload).unwrap()))
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn message_from_json(message: &Value) -> ProposalMessage {
    ProposalMessage {
        from: Address::from_str(message["from"].as_str().unwrap()).unwrap(),
        space: message["space"].as_str().unwrap().to_string(),
        timestamp: message["timestamp"].as_u64().unwrap(),
        voting_type: message["type"].as_str().unwrap().to_string(),
        title: message["title"].as_str().unwrap().to_string(),
        body: message["body"].as_str().unwrap().to_string(),
        discussion: message["discussion"].as_str().unwrap().to_string(),
        choices: message["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect(),
        start: message["start"].as_u64().unwrap(),
        end: message["end"].as_u64().unwrap(),
        snapshot: message["snapshot"].as_u64().unwrap(),
        plugins: message["plugins"].as_str().unwrap().to_string(),
        app: message["app"].as_str().unwrap().to_string(),
    }
}

#[tokio::test]
async fn a_catalyst_proposal_reaches_the_sequencer_as_a_snapshot_js_envelope() {
    let (base_url, captured) = mock_snapshot().await;
    let gate = Arc::new(SnapshotGate::build(ready_config(&base_url)));
    let app = write_router(gate);

    let response = app
        .oneshot(signed_request("catalyst", &catalyst_payload()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let created = body_json(response).await;
    assert_eq!(created["id"], "0xmockproposalid");
    assert_eq!(created["type"], "catalyst");
    assert_eq!(created["snapshot_space"], "gate.dcl.eth");
    assert_eq!(created["ipfs"], "bafkreimockipfscid");
    assert_eq!(created["pending"], false);
    assert_eq!(created["published"], true);

    let envelopes = captured.lock().unwrap().clone();
    assert_eq!(envelopes.len(), 1, "exactly one submission");
    let envelope = &envelopes[0];

    let poster = Wallet::from_hex(POSTER_KEY).unwrap().address();
    let poster = Address::from_str(&poster).unwrap();
    assert_eq!(envelope["address"], poster.to_checksum(None));

    let sig = envelope["sig"].as_str().unwrap();
    assert!(sig.starts_with("0x"), "sig is hex: {sig}");
    assert_eq!(sig.len(), 132, "65-byte r||s||v signature");

    assert_eq!(
        envelope["data"]["domain"],
        json!({ "name": "snapshot", "version": "0.1.4" })
    );
    assert_eq!(
        envelope["data"]["types"]["Proposal"],
        json!([
            { "name": "from", "type": "address" },
            { "name": "space", "type": "string" },
            { "name": "timestamp", "type": "uint64" },
            { "name": "type", "type": "string" },
            { "name": "title", "type": "string" },
            { "name": "body", "type": "string" },
            { "name": "discussion", "type": "string" },
            { "name": "choices", "type": "string[]" },
            { "name": "start", "type": "uint64" },
            { "name": "end", "type": "uint64" },
            { "name": "snapshot", "type": "uint64" },
            { "name": "plugins", "type": "string" },
            { "name": "app", "type": "string" },
        ])
    );

    let message = &envelope["data"]["message"];
    assert_eq!(message["from"], poster.to_checksum(None));
    assert_eq!(message["space"], "gate.dcl.eth");
    assert_eq!(message["type"], "single-choice");
    assert_eq!(message["app"], "decentraland-governance");
    assert_eq!(message["plugins"], "{}");
    assert_eq!(message["discussion"], "");
    assert_eq!(message["choices"], json!(["yes", "no", "abstain"]));
    assert_eq!(message["snapshot"], MOCK_BLOCK);
    assert_eq!(
        message["title"],
        "Add catalyst node with domain peer.example.org to the catalyst network"
    );
    let author = Wallet::from_hex(AUTHOR_KEY).unwrap().address();
    assert!(
        message["body"]
            .as_str()
            .unwrap()
            .starts_with(&format!("> by {author}")),
        "the body attributes the signed-fetch signer: {}",
        message["body"]
    );

    let timestamp = message["timestamp"].as_u64().unwrap();
    assert_eq!(timestamp % 60, 0, "snapshot timestamps are minute-floored");
    let start = message["start"].as_u64().unwrap();
    let end = message["end"].as_u64().unwrap();
    assert_eq!(start, timestamp);
    assert_eq!(end - start, 600);

    let signature = Signature::from_str(sig).expect("parse signature");
    let recovered = signature
        .recover_address_from_prehash(&message_from_json(message).digest())
        .expect("recover");
    assert_eq!(
        recovered, poster,
        "the signature must cover the eip-712 digest of the envelope's own message"
    );
}

#[tokio::test]
async fn a_tender_starts_after_its_submission_window_and_is_reported_pending() {
    let (base_url, captured) = mock_snapshot().await;
    let mut cfg = ready_config(&base_url);
    cfg.tender_submission_window_seconds = 300;
    cfg.duration_tender_seconds = Some(1200);
    let app = write_router(Arc::new(SnapshotGate::build(cfg)));

    let payload = json!({
        "type": "tender",
        "linked_proposal_id": "pitch-1",
        "project_name": "Better Roads",
        "summary": "s",
        "problem_statement": "p",
        "technical_specification": "t",
        "use_cases": "u",
        "deliverables": "d",
        "target_release_quarter": "Q4 2026",
        "coAuthors": [],
    });

    let response = app
        .oneshot(signed_request("tender", &payload))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = body_json(response).await;
    assert_eq!(created["pending"], true);

    let envelopes = captured.lock().unwrap().clone();
    let message = &envelopes[0]["data"]["message"];
    let timestamp = message["timestamp"].as_u64().unwrap();
    let start = message["start"].as_u64().unwrap();
    let end = message["end"].as_u64().unwrap();
    assert_eq!(start, timestamp + 300);
    assert_eq!(end - start, 1200);
    assert_eq!(message["title"], "Better Roads");
}

#[tokio::test]
async fn without_a_private_key_the_route_fails_closed_and_signs_nothing() {
    let (base_url, captured) = mock_snapshot().await;
    let cfg = SnapshotConfig {
        api_url: Some(format!("{base_url}/msg")),
        block_rpc_url: Some(format!("{base_url}/rpc")),
        space: Some("gate.dcl.eth".to_string()),
        ..SnapshotConfig::default()
    };
    let app = write_router(Arc::new(SnapshotGate::build(cfg)));

    let response = app
        .oneshot(signed_request("catalyst", &catalyst_payload()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = body_json(response).await;
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("SNAPSHOT_PRIVATE_KEY"), "got: {error}");
    assert!(error.contains("not configured"), "got: {error}");
    assert!(
        captured.lock().unwrap().is_empty(),
        "nothing may reach the sequencer without a signing key"
    );
}

#[tokio::test]
async fn an_unsigned_request_is_rejected_before_anything_is_submitted() {
    let (base_url, captured) = mock_snapshot().await;
    let app = write_router(Arc::new(SnapshotGate::build(ready_config(&base_url))));

    let response = app
        .oneshot(unsigned_request("catalyst", &catalyst_payload()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_bid_is_refused_because_it_is_not_a_snapshot_write() {
    let (base_url, captured) = mock_snapshot().await;
    let app = write_router(Arc::new(SnapshotGate::build(ready_config(&base_url))));

    let response = app
        .oneshot(signed_request(
            "bid",
            &json!({ "linked_proposal_id": "tender-1" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = body_json(response).await;
    assert!(
        body["error"].as_str().unwrap().contains("not implemented"),
        "got: {body}"
    );
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_proposal_type_is_a_404() {
    let (base_url, _captured) = mock_snapshot().await;
    let app = write_router(Arc::new(SnapshotGate::build(ready_config(&base_url))));

    let response = app
        .oneshot(signed_request("grant", &json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_malformed_payload_is_refused_before_the_sequencer_is_called() {
    let (base_url, captured) = mock_snapshot().await;
    let app = write_router(Arc::new(SnapshotGate::build(ready_config(&base_url))));

    let response = app
        .oneshot(signed_request(
            "catalyst",
            &json!({ "type": "catalyst_add" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_sequencer_rejection_is_surfaced_as_a_bad_gateway() {
    let refuse = Router::new().route(
        "/msg",
        post(|| async {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "client_error", "error_description": "not authorized" })),
            )
        }),
    );
    let app_router = refuse.merge(Router::new().route("/rpc", post(block_number)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app_router).await.unwrap();
    });

    let app = write_router(Arc::new(SnapshotGate::build(ready_config(&format!(
        "http://{addr}"
    )))));
    let response = app
        .oneshot(signed_request("catalyst", &catalyst_payload()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = body_json(response).await;
    assert!(
        body["error"].as_str().unwrap().contains("not authorized"),
        "got: {body}"
    );
}
