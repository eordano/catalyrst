use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::ban::BanChecker;
use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, LivekitConfig, NatsConfig, ServerConfig,
};
use catalyrst_archipelago::feed::dispatch;
use catalyrst_archipelago::livekit::LivekitMinter;
use catalyrst_archipelago::peers::PeerDirectory;
use catalyrst_archipelago::proto::archipelago::{
    client_packet, server_packet, ChallengeRequestMessage, ClientPacket, Heartbeat,
    IslandChangedMessage, KickedReason, ServerPacket, SignedChallengeMessage,
};
use catalyrst_archipelago::proto::Position;
use catalyrst_archipelago::registry::{PeersRegistry, SocketEvent};
use catalyrst_archipelago::{api_router, build_state, AppState};
use futures::{SinkExt, StreamExt};
use prost::Message as _;
use tokio_tungstenite::tungstenite::Message as WsMessage;

type BanSet = Arc<Mutex<HashSet<String>>>;

async fn bans(State(set): State<BanSet>, Path(addr): Path<String>) -> Json<Value> {
    let banned = set.lock().unwrap().contains(&addr.to_ascii_lowercase());
    Json(json!({ "data": { "isBanned": banned } }))
}

async fn start_gatekeeper() -> (u16, BanSet) {
    let set: BanSet = Arc::new(Mutex::new(HashSet::new()));
    let app = Router::new()
        .route("/users/{addr}/bans", get(bans))
        .with_state(set.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gatekeeper");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, set)
}

fn config(gatekeeper_url: String, ban_sweep_interval_secs: u64) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        cluster: ClusterConfig {
            ban_sweep_interval_secs,
            ..ClusterConfig::default()
        },
        server: ServerConfig::default(),
        auth: AuthConfig {
            require_signed_challenge: true,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
            deny_list_url: None,
            ..AuthConfig::default()
        },
        livekit: LivekitConfig {
            comms_gatekeeper_url: Some(gatekeeper_url),
            ..LivekitConfig::default()
        },
        nats: NatsConfig::default(),
        content_database_url: None,
        content_base_url: String::new(),
        commit_hash: String::new(),
    }
}

async fn start_server(cfg: Config) -> (u16, AppState) {
    let state = build_state(&cfg).await.expect("state");
    let app = api_router().with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind server");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, state)
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn encode(msg: client_packet::Message) -> tokio_tungstenite::tungstenite::Bytes {
    ClientPacket { message: Some(msg) }.encode_to_vec().into()
}

async fn recv_msg(ws: &mut WsStream, timeout: Duration) -> Option<server_packet::Message> {
    loop {
        let frame = tokio::time::timeout(timeout, ws.next()).await.ok()??;
        match frame.ok()? {
            WsMessage::Binary(bytes) => return ServerPacket::decode(bytes.as_ref()).ok()?.message,
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            _ => return None,
        }
    }
}

async fn handshake(port: u16, wallet: &PrivateKeySigner) -> WsStream {
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
        other => panic!("expected challenge response, got {other:?}"),
    };

    let hash = alloy::primitives::eip191_hash_message(challenge.as_bytes());
    let sig = wallet.sign_hash_sync(&hash).expect("sign");
    let chain = json!([
        { "type": "SIGNER", "payload": address, "signature": "" },
        { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": sig.to_string() }
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
        other => panic!("expected welcome, got {other:?}"),
    }
    ws
}

async fn wait_for_peer(state: &AppState, addr: &str) {
    for _ in 0..60 {
        if state.peers.peer(addr).is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the heartbeat never reached the peer directory");
}

async fn send_heartbeat(ws: &mut WsStream) {
    let hb = Heartbeat {
        position: Some(Position {
            x: 8.0,
            y: 0.0,
            z: 8.0,
        }),
        desired_room: Some("catalyrst".into()),
    };
    ws.send(WsMessage::Binary(encode(
        client_packet::Message::Heartbeat(hb),
    )))
    .await
    .expect("send heartbeat");
}

#[tokio::test]
async fn mid_session_ban_sweep_kicks_and_closes_socket() {
    let (gk_port, ban_set) = start_gatekeeper().await;
    let (port, state) = start_server(config(format!("http://127.0.0.1:{gk_port}"), 30)).await;

    let wallet = PrivateKeySigner::random();
    let addr = format!("{:#x}", wallet.address());
    let mut ws = handshake(port, &wallet).await;

    send_heartbeat(&mut ws).await;
    wait_for_peer(&state, &addr).await;

    ban_set.lock().unwrap().insert(addr.clone());
    state.peers.ban_sweep_once().await;

    match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::Kicked(k)) => {
            assert_eq!(k.reason, KickedReason::KrNewSession as i32);
        }
        other => panic!("banned peer should receive a Kicked packet, got {other:?}"),
    }
    assert!(
        recv_msg(&mut ws, Duration::from_secs(3)).await.is_none(),
        "socket is closed after the kick"
    );
    assert!(
        state.peers.peer(&addr).is_none(),
        "banned peer evicted from the peer directory"
    );
}

#[tokio::test]
async fn periodic_ban_sweep_evicts_without_manual_trigger() {
    let (gk_port, ban_set) = start_gatekeeper().await;
    let (port, state) = start_server(config(format!("http://127.0.0.1:{gk_port}"), 1)).await;

    let wallet = PrivateKeySigner::random();
    let addr = format!("{:#x}", wallet.address());
    let mut ws = handshake(port, &wallet).await;

    send_heartbeat(&mut ws).await;
    wait_for_peer(&state, &addr).await;

    ban_set.lock().unwrap().insert(addr.clone());

    match recv_msg(&mut ws, Duration::from_secs(6)).await {
        Some(server_packet::Message::Kicked(_)) => {}
        other => panic!("the periodic sweep should kick the banned peer, got {other:?}"),
    }
    assert!(state.peers.peer(&addr).is_none());
}

#[tokio::test]
async fn the_sweep_kicks_every_session_of_the_banned_wallet_and_drops_it() {
    let (gk_port, ban_set) = start_gatekeeper().await;
    let ban = BanChecker::new(
        Some(format!("http://127.0.0.1:{gk_port}")),
        reqwest::Client::new(),
    );
    let lk = Arc::new(LivekitMinter::new(LivekitConfig::default()));
    let registry = PeersRegistry::new();
    let peers = PeerDirectory::new(ClusterConfig::default(), lk, ban, Arc::clone(&registry));
    let (_desktop, mut desktop_events, _) = registry.on_peer_connected("0xabc", "0xsession1");
    let (_mobile, mut mobile_events, _) = registry.on_peer_connected("0xabc", "0xsession2");

    peers.upsert_peer("0xabc".into(), [8.0, 0.0, 8.0], [0, 0], "r".into());

    ban_set.lock().unwrap().insert("0xabc".into());
    peers.ban_sweep_once().await;

    assert!(
        matches!(desktop_events.try_recv(), Ok(SocketEvent::Kicked)),
        "every live session of the banned wallet is kicked"
    );
    assert!(matches!(mobile_events.try_recv(), Ok(SocketEvent::Kicked)));
    assert!(
        peers.peer("0xabc").is_none(),
        "the sweep removes the banned peer"
    );

    peers.upsert_peer("0xabc".into(), [8.0, 0.0, 8.0], [0, 0], "r".into());
    assert!(
        peers.peer("0xabc").is_some(),
        "a later heartbeat re-admits the wallet; the ban gate is the sweep and the handshake, \
         not a local kicked set"
    );
}

#[tokio::test]
async fn a_wallet_assigned_before_its_first_heartbeat_is_minted_a_token_for_that_room() {
    let (gk_port, _ban_set) = start_gatekeeper().await;
    let mut cfg = config(format!("http://127.0.0.1:{gk_port}"), 3600);
    cfg.livekit.api_key = Some("APIkey".into());
    cfg.livekit.api_secret = Some("secret".into());
    let (port, state) = start_server(cfg).await;

    let wallet = PrivateKeySigner::random();
    let addr = format!("{:#x}", wallet.address());
    let mut ws = handshake(port, &wallet).await;

    dispatch(
        &state,
        &format!("engine.peer.{addr}.island_changed.{addr}"),
        &IslandChangedMessage {
            island_id: "I7".into(),
            conn_str: "livekit:wss://example.invalid?access_token=t".into(),
            from_island_id: None,
            peers: Default::default(),
        }
        .encode_to_vec(),
    );
    match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::IslandChanged(m)) => assert_eq!(m.island_id, "I7"),
        other => panic!("expected the island assignment, got {other:?}"),
    }
    assert!(
        state.peers.peer(&addr).is_none(),
        "no heartbeat has arrived, so the directory holds nothing for this wallet"
    );

    let client = reqwest::Client::new();
    let issued: Value = client
        .post(format!("http://127.0.0.1:{port}/auth/challenge"))
        .json(&json!({ "address": addr }))
        .send()
        .await
        .expect("challenge request")
        .json()
        .await
        .expect("challenge json");
    let challenge = issued["challenge"]
        .as_str()
        .expect("challenge string")
        .to_string();

    let hash = alloy::primitives::eip191_hash_message(challenge.as_bytes());
    let sig = wallet.sign_hash_sync(&hash).expect("sign");
    let resp = client
        .post(format!("http://127.0.0.1:{port}/auth/livekit-token"))
        .json(&json!({
            "address": addr,
            "challenge": challenge,
            "room": "",
            "auth_chain": [
                { "type": "SIGNER", "payload": addr, "signature": "" },
                {
                    "type": "ECDSA_SIGNED_ENTITY",
                    "payload": challenge,
                    "signature": sig.to_string()
                }
            ]
        }))
        .send()
        .await
        .expect("token request");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "an assignment the feed made before the first heartbeat still authorizes a token"
    );
    let grant: Value = resp.json().await.expect("grant json");
    assert_eq!(grant["room"], "I7");
    assert_eq!(grant["identity"], addr);
    assert!(
        grant["token"].as_str().is_some_and(|t| !t.is_empty()),
        "the grant carries a signed token, got {grant}"
    );
}

#[derive(Default)]
struct SeenRemoval {
    room: Mutex<Option<String>>,
    identity: Mutex<Option<String>>,
    auth: Mutex<Option<String>>,
}

async fn remove_participant_handler(
    State(seen): State<Arc<SeenRemoval>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    *seen.room.lock().unwrap() = body.get("room").and_then(|v| v.as_str()).map(String::from);
    *seen.identity.lock().unwrap() = body
        .get("identity")
        .and_then(|v| v.as_str())
        .map(String::from);
    *seen.auth.lock().unwrap() = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    Json(json!({}))
}

async fn start_livekit_mock() -> (u16, Arc<SeenRemoval>) {
    let seen = Arc::new(SeenRemoval::default());
    let app = Router::new()
        .route(
            "/twirp/livekit.RoomService/RemoveParticipant",
            post(remove_participant_handler),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind livekit mock");
    let lk_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (lk_port, seen)
}

#[tokio::test]
async fn remove_participant_calls_livekit_room_service() {
    let (lk_port, seen) = start_livekit_mock().await;

    let minter = LivekitMinter::new(LivekitConfig {
        api_key: Some("APIkey".into()),
        api_secret: Some("secret".into()),
        ws_url: format!("http://127.0.0.1:{lk_port}"),
        token_ttl_secs: 300,
        comms_gatekeeper_url: None,
    });
    minter.remove_participant("I7", "0xpeer").await;

    assert_eq!(seen.room.lock().unwrap().as_deref(), Some("I7"));
    assert_eq!(seen.identity.lock().unwrap().as_deref(), Some("0xpeer"));
    let auth = seen
        .auth
        .lock()
        .unwrap()
        .clone()
        .expect("admin call carries a bearer token");
    assert!(
        auth.starts_with("Bearer "),
        "authorization is a bearer token, got {auth}"
    );
}

#[tokio::test]
async fn the_sweep_evicts_the_livekit_participant_of_a_wallet_that_never_heartbeat() {
    let (gk_port, ban_set) = start_gatekeeper().await;
    let (lk_port, seen) = start_livekit_mock().await;

    let mut cfg = config(format!("http://127.0.0.1:{gk_port}"), 3600);
    cfg.livekit.api_key = Some("APIkey".into());
    cfg.livekit.api_secret = Some("secret".into());
    cfg.livekit.ws_url = format!("http://127.0.0.1:{lk_port}");
    let (port, state) = start_server(cfg).await;

    let wallet = PrivateKeySigner::random();
    let addr = format!("{:#x}", wallet.address());
    let mut ws = handshake(port, &wallet).await;

    dispatch(
        &state,
        &format!("engine.peer.{addr}.island_changed.{addr}"),
        &IslandChangedMessage {
            island_id: "I7".into(),
            conn_str: "livekit:wss://example.invalid?access_token=t".into(),
            from_island_id: None,
            peers: Default::default(),
        }
        .encode_to_vec(),
    );

    match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::IslandChanged(m)) => assert_eq!(m.island_id, "I7"),
        other => panic!("expected the island assignment, got {other:?}"),
    }
    assert!(
        state.peers.peer(&addr).is_none(),
        "no heartbeat has arrived, so the directory holds nothing for this wallet"
    );
    assert_eq!(state.peers.island_of(&addr).as_deref(), Some("I7"));

    ban_set.lock().unwrap().insert(addr.clone());
    state.peers.ban_sweep_once().await;

    for _ in 0..60 {
        if seen.room.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        seen.room.lock().unwrap().as_deref(),
        Some("I7"),
        "the banned wallet's media participant is evicted from the room the feed assigned"
    );
    assert_eq!(
        seen.identity.lock().unwrap().as_deref(),
        Some(addr.as_str())
    );
}
