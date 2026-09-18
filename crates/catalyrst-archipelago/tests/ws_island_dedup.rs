use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, LivekitConfig, NatsConfig, ServerConfig,
};
use catalyrst_archipelago::feed::{dispatch, Routed};
use catalyrst_archipelago::nats::{reannounce_all, FeedPublisher, RecordingPublisher};
use catalyrst_archipelago::proto::archipelago::{
    client_packet, server_packet, ChallengeRequestMessage, ClientPacket, IslandChangedMessage,
    ServerPacket, SignedChallengeMessage,
};
use catalyrst_archipelago::{api_router, build_state_with, AppState};
use futures::{SinkExt, StreamExt};
use prost::Message as _;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message as WsMessage;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn test_config(island_changed_dedup_ms: u64) -> Config {
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
            ..AuthConfig::default()
        },
        livekit: LivekitConfig::default(),
        nats: NatsConfig {
            island_changed_dedup_ms,
            ..NatsConfig::default()
        },
        content_database_url: None,
        content_base_url: String::new(),
        commit_hash: String::new(),
    }
}

async fn start_server(dedup_ms: u64) -> (u16, AppState, Arc<RecordingPublisher>) {
    let publisher = RecordingPublisher::new();
    let state = build_state_with(
        &test_config(dedup_ms),
        Arc::clone(&publisher) as Arc<dyn FeedPublisher>,
    )
    .await
    .expect("state");
    let app = api_router().with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, state, publisher)
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

async fn recv_island(ws: &mut WsStream, timeout: Duration) -> Option<String> {
    match recv_msg(ws, timeout).await {
        Some(server_packet::Message::IslandChanged(m)) => Some(m.island_id),
        _ => None,
    }
}

struct Device {
    ephemeral: PrivateKeySigner,
}

impl Device {
    fn new() -> Self {
        Self {
            ephemeral: PrivateKeySigner::random(),
        }
    }

    fn session(&self) -> String {
        format!("{:#x}", self.ephemeral.address())
    }
}

/// A full delegating chain: the wallet signs the ephemeral's mandate, the ephemeral signs the
/// challenge. This is the only shape that gives one wallet two distinct session keys, which is
/// what the session-addressed feed subjects are routed on.
async fn handshake(port: u16, wallet: &PrivateKeySigner, device: &Device) -> WsStream {
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

    let mandate = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-12-31T00:00:00.000Z",
        device.session()
    );
    let mandate_sig = wallet
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(mandate.as_bytes()))
        .expect("sign mandate");
    let challenge_sig = device
        .ephemeral
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(
            challenge.as_bytes(),
        ))
        .expect("sign challenge");

    let chain = serde_json::json!([
        { "type": "SIGNER", "payload": address, "signature": "" },
        { "type": "ECDSA_EPHEMERAL", "payload": mandate, "signature": mandate_sig.to_string() },
        { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": challenge_sig.to_string() }
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

fn island_payload(island_id: &str) -> Vec<u8> {
    IslandChangedMessage {
        island_id: island_id.into(),
        conn_str: "livekit:wss://example.invalid?access_token=t".into(),
        from_island_id: None,
        peers: Default::default(),
    }
    .encode_to_vec()
}

fn session_subject(address: &str, session: &str) -> String {
    format!("engine.peer.{address}.island_changed.{session}")
}

fn legacy_subject(address: &str) -> String {
    format!("engine.peer.{address}.island_changed")
}

#[tokio::test]
async fn the_feed_delivers_a_room_only_to_the_session_it_names() {
    let (port, state, _pub) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());

    let desktop = Device::new();
    let mobile = Device::new();
    let mut ws_desktop = handshake(port, &wallet, &desktop).await;
    let mut ws_mobile = handshake(port, &wallet, &mobile).await;

    assert_eq!(
        dispatch(
            &state,
            &session_subject(&address, &desktop.session()),
            &island_payload("island-A"),
        ),
        Routed::Delivered
    );

    assert_eq!(
        recv_island(&mut ws_desktop, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
    assert_eq!(
        recv_island(&mut ws_mobile, Duration::from_millis(400)).await,
        None,
        "the other device of the same wallet is not told about this assignment"
    );
    assert_eq!(
        state.peers.island_of(&address).as_deref(),
        Some("island-A"),
        "the assignment is recorded even though no heartbeat has arrived yet"
    );
}

#[tokio::test]
async fn a_repeated_room_inside_the_window_is_dropped_and_a_new_one_is_not() {
    let (port, state, _pub) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());

    dispatch(&state, &subject, &island_payload("island-A"));
    dispatch(&state, &subject, &island_payload("island-A"));
    dispatch(&state, &subject, &island_payload("island-B"));

    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-B".into()),
        "the repeat is dropped, so the next packet is the new room"
    );
    assert_eq!(
        recv_island(&mut ws, Duration::from_millis(400)).await,
        None,
        "nothing else follows"
    );
    assert_eq!(
        state.feed.delivered_count(),
        3,
        "the feed itself delivered all three; the window is a socket-side filter"
    );
    assert_eq!(
        state.feed.deduplicated_count(),
        1,
        "the suppressed repeat is the counter the runbook reads"
    );
}

#[tokio::test]
async fn a_zero_window_forwards_every_repeat() {
    let (port, state, _pub) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());

    dispatch(&state, &subject, &island_payload("island-A"));
    dispatch(&state, &subject, &island_payload("island-A"));

    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
}

#[tokio::test]
async fn the_legacy_subject_reaches_the_newest_socket_of_the_wallet() {
    let (port, state, _pub) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());

    let first = Device::new();
    let second = Device::new();
    let mut ws_first = handshake(port, &wallet, &first).await;
    let mut ws_second = handshake(port, &wallet, &second).await;

    assert_eq!(
        dispatch(
            &state,
            &legacy_subject(&address),
            &island_payload("island-L")
        ),
        Routed::Delivered
    );

    assert_eq!(
        recv_island(&mut ws_second, Duration::from_secs(3)).await,
        Some("island-L".into())
    );
    assert_eq!(
        recv_island(&mut ws_first, Duration::from_millis(400)).await,
        None
    );
}

#[tokio::test]
async fn a_stale_session_and_an_unknown_wallet_are_told_apart() {
    let (port, state, _pub) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let _ws = handshake(port, &wallet, &device).await;

    let stale = format!("{:#x}", PrivateKeySigner::random().address());
    assert_eq!(
        dispatch(
            &state,
            &session_subject(&address, &stale),
            &island_payload("island-A")
        ),
        Routed::NoSessionSocket
    );
    assert_eq!(
        dispatch(
            &state,
            &session_subject("0x00000000000000000000000000000000000000ff", &stale),
            &island_payload("island-A"),
        ),
        Routed::NoSocket
    );
    assert_eq!(state.feed.no_session_socket_count(), 1);
}

#[tokio::test]
async fn a_reconnect_of_the_same_device_supersedes_the_previous_socket() {
    let (port, state, _pub) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();

    let mut old = handshake(port, &wallet, &device).await;
    let mut fresh = handshake(port, &wallet, &device).await;

    match recv_msg(&mut old, Duration::from_secs(3)).await {
        Some(server_packet::Message::Kicked(_)) => {}
        other => panic!("the superseded socket must be kicked, got {other:?}"),
    }

    assert_eq!(
        dispatch(
            &state,
            &session_subject(&address, &device.session()),
            &island_payload("island-A"),
        ),
        Routed::Delivered
    );
    assert_eq!(
        recv_island(&mut fresh, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
}

#[tokio::test]
async fn the_socket_announces_its_session_on_connect_and_announces_the_disconnect() {
    let (port, _state, publisher) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();

    let mut ws = handshake(port, &wallet, &device).await;

    let connect = publisher
        .sent()
        .into_iter()
        .find(|(subject, _)| subject == &format!("peer.{address}.connect"))
        .expect("the handshake announces a connect");
    assert_eq!(
        String::from_utf8(connect.1).expect("utf8 session payload"),
        device.session(),
        "the connect payload is the session key, so the minter can address this device"
    );

    ws.close(None).await.ok();
    for _ in 0..60 {
        if publisher
            .subjects()
            .iter()
            .any(|s| s == &format!("peer.{address}.disconnect"))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the close never announced a disconnect");
}

#[tokio::test]
async fn a_re_announce_pass_publishes_one_connect_per_live_session() {
    let (port, state, publisher) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());

    let desktop = Device::new();
    let mobile = Device::new();
    let _ws_desktop = handshake(port, &wallet, &desktop).await;
    let _ws_mobile = handshake(port, &wallet, &mobile).await;

    let fresh = RecordingPublisher::new();
    assert_eq!(reannounce_all(&state, fresh.as_ref()), 2);

    let mut sessions: Vec<String> = fresh
        .sent()
        .into_iter()
        .map(|(subject, payload)| {
            assert_eq!(subject, format!("peer.{address}.connect"));
            String::from_utf8(payload).expect("utf8 session payload")
        })
        .collect();
    sessions.sort();
    let mut expected = vec![desktop.session(), mobile.session()];
    expected.sort();
    assert_eq!(
        sessions, expected,
        "every live session is re-announced, so a connect lost while the link was down is replayed"
    );

    assert!(
        publisher
            .subjects()
            .iter()
            .filter(|s| *s == &format!("peer.{address}.connect"))
            .count()
            == 2,
        "the handshakes themselves announced once each"
    );
}

#[tokio::test]
async fn a_live_socket_retains_its_assignment_until_teardown_after_heartbeat_expiry() {
    let (port, state, _) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());
    assert_eq!(
        dispatch(&state, &subject, &island_payload("island-kept")),
        Routed::Delivered
    );
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-kept".into())
    );
    state.peers.upsert_peer_at(
        address.clone(),
        [0.0; 3],
        [0, 0],
        "realm".into(),
        chrono::Utc::now() - chrono::Duration::days(1),
    );
    assert_eq!(state.peers.expire_stale_once(), 1);
    assert_eq!(state.peers.island_of(&address), Some("island-kept".into()));
    ws.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.registry.has_peer(&address) || state.peers.island_of(&address).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("socket teardown must clear its assignment");
}

#[tokio::test]
async fn malformed_assignment_counts_an_error_without_breaking_the_live_socket() {
    let (port, state, _) = start_server(10_000).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());
    assert_eq!(dispatch(&state, &subject, &[255]), Routed::Undecodable);
    assert_eq!(state.feed.undecodable_count(), 1);
    assert_eq!(
        dispatch(&state, &subject, &island_payload("island-valid")),
        Routed::Delivered
    );
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-valid".into())
    );
}
