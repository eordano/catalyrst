use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, GossipConfig, LivekitConfig, ServerConfig,
};
use catalyrst_archipelago::proto::archipelago::{
    client_packet, server_packet, ChallengeRequestMessage, ClientPacket, Heartbeat, ServerPacket,
    SignedChallengeMessage,
};
use catalyrst_archipelago::proto::Position;
use catalyrst_archipelago::{api_router, build_state};
use ethers_signers::{LocalWallet, Signer};
use futures::{SinkExt, StreamExt};
use prost::Message as _;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message as WsMessage;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn test_config() -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        cluster: ClusterConfig::default(),
        server: ServerConfig::default(),
        auth: AuthConfig {
            require_signed_challenge: true,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
            deny_list_url: None,
        },
        livekit: LivekitConfig::default(),
        gossip: GossipConfig::default(),
        content_database_url: None,
        content_base_url: String::new(),
        commit_hash: String::new(),
    }
}

async fn start_server() -> u16 {
    let state = build_state(&test_config()).await.expect("state");
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

fn encode(msg: client_packet::Message) -> tokio_tungstenite::tungstenite::Bytes {
    ClientPacket { message: Some(msg) }.encode_to_vec().into()
}

async fn recv_msg(ws: &mut WsStream, timeout: Duration) -> Option<server_packet::Message> {
    loop {
        let frame = tokio::time::timeout(timeout, ws.next()).await.ok()??;
        match frame.ok()? {
            WsMessage::Binary(bytes) => {
                return ServerPacket::decode(bytes.as_ref()).ok()?.message;
            }
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            _ => return None,
        }
    }
}

async fn connect_and_handshake(port: u16, wallet: &LocalWallet) -> WsStream {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("ws connect");

    let address = format!("{:#x}", wallet.address());
    ws.send(WsMessage::Binary(encode(
        client_packet::Message::ChallengeRequest(ChallengeRequestMessage {
            address: address.clone(),
        }),
    )))
    .await
    .expect("send challenge request");

    let challenge = match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::ChallengeResponse(r)) => r.challenge_to_sign,
        other => panic!("expected ChallengeResponse, got {other:?}"),
    };

    let hash = ethers_core::utils::hash_message(challenge.as_bytes());
    let sig = wallet.sign_hash(hash).expect("sign");
    let chain = serde_json::json!([
        { "type": "SIGNER", "payload": address, "signature": "" },
        { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": format!("0x{sig}") }
    ]);

    ws.send(WsMessage::Binary(encode(
        client_packet::Message::SignedChallenge(SignedChallengeMessage {
            auth_chain_json: chain.to_string(),
        }),
    )))
    .await
    .expect("send signed challenge");

    match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::Welcome(_)) => {}
        other => panic!("expected Welcome, got {other:?}"),
    }
    ws
}

async fn send_heartbeat(ws: &mut WsStream) {
    ws.send(WsMessage::Binary(encode(
        client_packet::Message::Heartbeat(Heartbeat {
            position: Some(Position {
                x: 1.0,
                y: 0.0,
                z: 1.0,
            }),
            desired_room: None,
        }),
    )))
    .await
    .expect("send heartbeat");
}

async fn count_island_changed(ws: &mut WsStream, window: Duration) -> usize {
    let mut n = 0;
    let deadline = tokio::time::Instant::now() + window;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return n;
        }
        if let Some(server_packet::Message::IslandChanged(_)) = recv_msg(ws, left).await {
            n += 1;
        }
    }
}

#[tokio::test]
async fn island_changed_is_delivered_exactly_once() {
    let port = start_server().await;
    let wallet = LocalWallet::new(&mut ethers_core::rand::thread_rng());

    let mut ws = connect_and_handshake(port, &wallet).await;
    send_heartbeat(&mut ws).await;

    let n = count_island_changed(&mut ws, Duration::from_millis(3500)).await;
    assert_eq!(
        n, 1,
        "fresh peer must receive IslandChanged exactly once, got {n}"
    );
}

#[tokio::test]
async fn second_socket_same_wallet_gets_one_island_and_survives_stale_close() {
    let port = start_server().await;
    let wallet = LocalWallet::new(&mut ethers_core::rand::thread_rng());

    let mut ws_a = connect_and_handshake(port, &wallet).await;
    send_heartbeat(&mut ws_a).await;
    assert_eq!(
        count_island_changed(&mut ws_a, Duration::from_millis(3000)).await,
        1
    );

    let mut ws_b = connect_and_handshake(port, &wallet).await;
    send_heartbeat(&mut ws_b).await;
    let n_b = count_island_changed(&mut ws_b, Duration::from_millis(3000)).await;
    assert_eq!(
        n_b, 1,
        "reconnect socket must receive its inherited island exactly once, got {n_b}"
    );

    ws_a.close(None).await.ok();
    send_heartbeat(&mut ws_b).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_heartbeat(&mut ws_b).await;
    let n_after = count_island_changed(&mut ws_b, Duration::from_millis(2500)).await;
    assert_eq!(
        n_after, 0,
        "stale socket close must not disturb the surviving registration (got {n_after} reassignments)"
    );
}
