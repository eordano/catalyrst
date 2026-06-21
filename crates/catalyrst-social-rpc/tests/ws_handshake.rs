use catalyrst_social_rpc::auth_chain::{build_payload, FIVE_MINUTES_SECS};
use catalyrst_social_rpc::config::Config;
use catalyrst_social_rpc::state::AppStateInner;
use catalyrst_social_rpc::ws::ws_upgrade;
use chrono::Utc;
use ethers_signers::{LocalWallet, Signer};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

async fn spawn_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://localhost/social_rpc_test")
        .unwrap();
    let db = catalyrst_social_rpc::db::Db::new(pool);
    let profiles = catalyrst_social_rpc::profiles::Profiles::new(None, String::new());
    let state = Arc::new(AppStateInner::new(
        Config {
            http_host: "127.0.0.1".into(),
            http_port: addr.port(),
            auth_window_secs: 5,
            database_url: "postgres://localhost/social_rpc_test".into(),
            comms_gatekeeper_url: "http://127.0.0.1:5138".into(),
            content_database_url: None,
            content_server_address: String::new(),
            private_voice_chat_expiration_ms: 60000,
            private_voice_chat_job_interval_ms: 1000,
            private_voice_chat_expiration_batch_size: 20,
        },
        db,
        profiles,
    ));
    state.init_rpc().await;
    let app = axum::Router::new()
        .route("/", axum::routing::get(ws_upgrade))
        .with_state(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn make_frame(ts_ms: i64) -> (String, String) {
    let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let root_address = format!("{:#x}", root.address());

    let ephemeral: LocalWallet = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
        .parse()
        .unwrap();
    let ephemeral_address = format!("{:#x}", ephemeral.address());

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_address
    );
    let ephemeral_sig = root
        .sign_message(ephemeral_payload.as_bytes())
        .await
        .unwrap();

    let metadata = "{}";
    let payload = build_payload("get", "/", &ts_ms.to_string(), metadata);
    let entity_sig = ephemeral.sign_message(payload.as_bytes()).await.unwrap();

    let frame = json!({
        "x-identity-auth-chain-0": json!({
            "type": "SIGNER",
            "payload": root_address,
            "signature": ""
        }).to_string(),
        "x-identity-auth-chain-1": json!({
            "type": "ECDSA_EPHEMERAL",
            "payload": ephemeral_payload,
            "signature": format!("0x{}", ephemeral_sig)
        }).to_string(),
        "x-identity-auth-chain-2": json!({
            "type": "ECDSA_SIGNED_ENTITY",
            "payload": payload,
            "signature": format!("0x{}", entity_sig)
        }).to_string(),
        "x-identity-timestamp": ts_ms.to_string(),
        "x-identity-metadata": metadata
    });

    (root_address.to_lowercase(), frame.to_string())
}

#[tokio::test]
async fn ws_handshake_valid_chain_receives_transport_frame() {
    let _ = FIVE_MINUTES_SECS;
    let addr = spawn_server().await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{}/", addr);
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let now_ms = Utc::now().timestamp_millis();
    let (_expected_signer, frame) = make_frame(now_ms).await;
    socket.send(Message::Text(frame.into())).await.unwrap();

    // After a successful handshake the server attaches the dcl-rpc transport
    // SILENTLY. The post-auth welcome text frame was removed (commit 8924682):
    // the Unity WebSocketRpcTransport feeds every WS message to the protobuf
    // parser, so a text welcome frame caused InvalidProtocolBufferException and
    // killed the transport. The first frame the client receives is therefore the
    // binary dcl-rpc transport frame, not text.
    let msg = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("transport frame should arrive within 3s")
        .expect("stream should yield a message")
        .expect("frame should be Ok");

    match msg {
        Message::Binary(b) => assert!(!b.is_empty(), "expected a non-empty binary transport frame"),
        other => panic!(
            "expected binary transport frame (no text welcome), got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn ws_handshake_garbage_frame_gets_close_3003() {
    let addr = spawn_server().await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{}/", addr);
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();

    socket
        .send(Message::Text("definitely not signed-fetch json".into()))
        .await
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("close frame should arrive within 3s")
        .expect("stream should yield a message")
        .expect("frame should be Ok");

    match msg {
        Message::Close(Some(frame)) => {
            assert_eq!(
                frame.code,
                tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(3003u16)
            );
        }
        other => panic!("expected Close(3003), got {:?}", other),
    }
}

#[tokio::test]
async fn ws_post_handshake_receives_rpc_server_ready() {
    let addr = spawn_server().await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{}/", addr);
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let now_ms = Utc::now().timestamp_millis();
    let (_, frame) = make_frame(now_ms).await;
    socket.send(Message::Text(frame.into())).await.unwrap();

    // No text welcome anymore (see ws_handshake_valid_chain_receives_transport_frame):
    // the first frame after a successful handshake is the binary dcl-rpc
    // transport / server-ready frame.
    let server_ready = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("server-ready should arrive after attach")
        .expect("stream yields")
        .expect("frame ok");
    match server_ready {
        Message::Binary(b) => {
            assert!(!b.is_empty(), "server-ready frame should be non-empty");
        }
        other => panic!("expected binary server-ready, got {:?}", other),
    }
}

#[tokio::test]
async fn ws_handshake_timeout_closes_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://localhost/social_rpc_test")
        .unwrap();
    let db = catalyrst_social_rpc::db::Db::new(pool);
    let profiles = catalyrst_social_rpc::profiles::Profiles::new(None, String::new());
    let state = Arc::new(AppStateInner::new(
        Config {
            http_host: "127.0.0.1".into(),
            http_port: addr.port(),
            auth_window_secs: 1,
            database_url: "postgres://localhost/social_rpc_test".into(),
            comms_gatekeeper_url: "http://127.0.0.1:5138".into(),
            content_database_url: None,
            content_server_address: String::new(),
            private_voice_chat_expiration_ms: 60000,
            private_voice_chat_job_interval_ms: 1000,
            private_voice_chat_expiration_batch_size: 20,
        },
        db,
        profiles,
    ));
    state.init_rpc().await;
    let app = axum::Router::new()
        .route("/", axum::routing::get(ws_upgrade))
        .with_state(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{}/", addr);
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("close should arrive within 3s");
    match msg {
        Some(Ok(Message::Close(_))) | None => {}
        Some(other) => panic!("expected Close or stream-end, got {:?}", other),
    }
}
