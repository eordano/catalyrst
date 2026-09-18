use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, LivekitConfig, NatsConfig, ServerConfig,
};
use catalyrst_archipelago::feed::{dispatch, Routed};
use catalyrst_archipelago::nats::{reannounce_all, FeedPublisher, NatsBus, RecordingPublisher};
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
        control_database_url: None,
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
    ClientPacket {
        request_sequence: 0,
        message: Some(msg),
    }
    .encode_to_vec()
    .into()
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
    let (mut ws, challenge) = begin_handshake(port, wallet).await;
    finish_handshake(&mut ws, wallet, device, &challenge).await;
    ws
}

async fn begin_handshake(port: u16, wallet: &PrivateKeySigner) -> (WsStream, String) {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("ws connect");

    let address = format!("{:#x}", wallet.address());
    ws.send(WsMessage::Binary(encode(
        client_packet::Message::ChallengeRequest(ChallengeRequestMessage {
            offer: None,
            address: address.clone(),
        }),
    )))
    .await
    .expect("send challenge request");

    let challenge = match recv_msg(&mut ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::ChallengeResponse(r)) => r.challenge_to_sign,
        other => panic!("expected ChallengeResponse, got {other:?}"),
    };
    assert!(challenge.starts_with("dcl-"));
    assert_eq!(challenge.len(), 52);
    (ws, challenge)
}

async fn finish_handshake(
    ws: &mut WsStream,
    wallet: &PrivateKeySigner,
    device: &Device,
    challenge: &str,
) {
    ws.send(WsMessage::Binary(signed_challenge(
        wallet, device, challenge,
    )))
    .await
    .expect("send signed challenge");

    match recv_msg(ws, Duration::from_secs(3)).await {
        Some(server_packet::Message::Welcome(_)) => {}
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn signed_challenge(
    wallet: &PrivateKeySigner,
    device: &Device,
    challenge: &str,
) -> tokio_tungstenite::tungstenite::Bytes {
    let address = format!("{:#x}", wallet.address());
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

    encode(client_packet::Message::SignedChallenge(
        SignedChallengeMessage {
            typed_auth_chain: None,
            auth_chain_json: chain.to_string(),
        },
    ))
}

#[tokio::test]
async fn concurrent_same_wallet_challenges_both_welcome_and_route_independently() {
    let (port, state, _publisher) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let first_device = Device::new();
    let second_device = Device::new();
    let (mut first, a) = begin_handshake(port, &wallet).await;
    let (mut second, b) = begin_handshake(port, &wallet).await;
    assert_ne!(a, b);
    finish_handshake(&mut first, &wallet, &first_device, &a).await;
    finish_handshake(&mut second, &wallet, &second_device, &b).await;
    for (device, socket, island) in [
        (&first_device, &mut first, "first"),
        (&second_device, &mut second, "second"),
    ] {
        assert!(state
            .registry
            .peer_link(&address, &device.session())
            .is_some());
        dispatch(
            &state,
            &session_subject(&address, &device.session()),
            &island_payload(island),
        );
        assert_eq!(
            recv_island(socket, Duration::from_secs(3)).await.as_deref(),
            Some(island)
        );
    }
}

#[tokio::test]
async fn a_delayed_older_handshake_cannot_take_a_newer_sessions_socket() {
    let (port, state, _publisher) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let (mut older, older_challenge) = begin_handshake(port, &wallet).await;
    let (mut newer, newer_challenge) = begin_handshake(port, &wallet).await;

    finish_handshake(&mut newer, &wallet, &device, &newer_challenge).await;
    let current = state
        .registry
        .peer_link(&address, &device.session())
        .expect("newer handshake was admitted");
    older
        .send(WsMessage::Binary(signed_challenge(
            &wallet,
            &device,
            &older_challenge,
        )))
        .await
        .unwrap();
    assert!(matches!(
        recv_msg(&mut older, Duration::from_secs(3)).await,
        Some(server_packet::Message::Kicked(_))
    ));
    assert_eq!(
        state
            .registry
            .peer_link(&address, &device.session())
            .map(|link| link.id),
        Some(current.id)
    );

    dispatch(
        &state,
        &session_subject(&address, &device.session()),
        &island_payload("newer-still-owns"),
    );
    assert_eq!(
        recv_island(&mut newer, Duration::from_secs(3))
            .await
            .as_deref(),
        Some("newer-still-owns")
    );
}

fn island_payload(island_id: &str) -> Vec<u8> {
    IslandChangedMessage {
        assignment_context: None,
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
    assert!(state
        .registry
        .peer_link(&address, &desktop.session())
        .unwrap()
        .needs_assignment());
    assert!(state
        .registry
        .peer_link(&address, &mobile.session())
        .unwrap()
        .needs_assignment());

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
    assert!(!state
        .registry
        .peer_link(&address, &desktop.session())
        .unwrap()
        .needs_assignment());
    assert!(state
        .registry
        .peer_link(&address, &mobile.session())
        .unwrap()
        .needs_assignment());
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
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
    dispatch(&state, &subject, &island_payload("island-A"));
    assert_eq!(recv_island(&mut ws, Duration::from_millis(100)).await, None);
    assert_eq!(state.feed.coalesced_count(), 0);
    assert_eq!(state.feed.socket_assignments_written_count(), 1);
    dispatch(&state, &subject, &island_payload("island-B"));
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
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-A".into())
    );
    dispatch(&state, &subject, &island_payload("island-A"));
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

#[tokio::test]
async fn an_assignment_backlog_keeps_only_the_latest_pending_room() {
    let (port, state, _) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());
    for index in 0..1_024 {
        dispatch(
            &state,
            &subject,
            &island_payload(&format!("island-{index}")),
        );
    }
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("island-1023".into()),
        "a paused consumer must not replay an unbounded history of old rooms"
    );
    assert_eq!(recv_island(&mut ws, Duration::from_millis(100)).await, None);
    assert_eq!(state.feed.coalesced_count(), 1_023);
    assert_eq!(state.feed.socket_assignments_written_count(), 1);
}

#[tokio::test]
async fn a_kick_preempts_the_assignment_backlog_and_rejects_later_updates() {
    let (port, state, _) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let mut ws = handshake(port, &wallet, &device).await;
    let subject = session_subject(&address, &device.session());
    for index in 0..1_024 {
        dispatch(
            &state,
            &subject,
            &island_payload(&format!("island-{index}")),
        );
    }
    assert_eq!(state.registry.kick(&address), 1);
    assert_eq!(
        dispatch(&state, &subject, &island_payload("too-late")),
        Routed::NoSocket
    );
    assert!(
        matches!(
            recv_msg(&mut ws, Duration::from_secs(3)).await,
            Some(server_packet::Message::Kicked(_))
        ),
        "a kick must take precedence over queued assignment work"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.registry.has_peer(&address) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the kicked socket did not retire its registration");
}

#[tokio::test]
async fn an_oversized_client_frame_retires_only_its_socket() {
    let (port, state, _) = start_server(0).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let first_device = Device::new();
    let second_device = Device::new();
    let mut first = handshake(port, &wallet, &first_device).await;
    let mut second = handshake(port, &wallet, &second_device).await;
    first
        .send(WsMessage::Binary(
            vec![0; catalyrst_archipelago::registry::MAX_ASSIGNMENT_BYTES + 1].into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while state
            .registry
            .peer_link(&address, &first_device.session())
            .is_some()
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("oversized frame did not retire its socket");
    assert!(state
        .registry
        .peer_link(&address, &second_device.session())
        .is_some());
    assert_eq!(
        dispatch(
            &state,
            &session_subject(&address, &second_device.session()),
            &island_payload("still-live")
        ),
        Routed::Delivered
    );
    assert_eq!(
        recv_island(&mut second, Duration::from_secs(3)).await,
        Some("still-live".into())
    );
}

#[tokio::test]
async fn a_non_reading_socket_is_retired_on_write_timeout_or_kick() {
    for kick in [false, true] {
        let (port, state, _) = start_server(0).await;
        let wallet = PrivateKeySigner::random();
        let address = format!("{:#x}", wallet.address());
        let device = Device::new();
        let tcp = tokio::net::TcpSocket::new_v4().unwrap();
        tcp.set_recv_buffer_size(2_048).unwrap();
        let tcp = tcp.connect(([127, 0, 0, 1], port).into()).await.unwrap();
        let (mut ws, _) = tokio_tungstenite::client_async(
            format!("ws://127.0.0.1:{port}/ws"),
            tokio_tungstenite::MaybeTlsStream::Plain(tcp),
        )
        .await
        .unwrap();
        ws.send(WsMessage::Binary(encode(
            client_packet::Message::ChallengeRequest(ChallengeRequestMessage {
                offer: None,
                address: address.clone(),
            }),
        )))
        .await
        .unwrap();
        let challenge = match recv_msg(&mut ws, Duration::from_secs(3)).await {
            Some(server_packet::Message::ChallengeResponse(response)) => response.challenge_to_sign,
            _ => panic!("expected challenge"),
        };
        finish_handshake(&mut ws, &wallet, &device, &challenge).await;
        let link = state
            .registry
            .peer_link(&address, &device.session())
            .unwrap();
        let subject = session_subject(&address, &device.session());
        let mut previous = 0;
        let mut unchanged = 0;
        let mut blocked = false;
        // Keep the actual TCP socket alive, but stop polling its WebSocket reader.
        for index in 0..1_024 {
            let payload = IslandChangedMessage {
                assignment_context: None,
                island_id: format!("room-{index}"),
                conn_str: "x".repeat(48 * 1024),
                ..Default::default()
            }
            .encode_to_vec();
            dispatch(&state, &subject, &payload);
            tokio::time::sleep(Duration::from_millis(2)).await;
            let written = state.feed.socket_assignments_written_count();
            if written == previous && written > 0 {
                unchanged += 1;
            } else {
                unchanged = 0;
            }
            previous = written;
            if unchanged == 20 {
                blocked = true;
                break;
            }
        }
        assert!(blocked, "fixture never exerted TCP write backpressure");
        assert!(!link.is_closed());
        if kick {
            assert_eq!(state.registry.kick(&address), 1);
        }
        let budget = if kick {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(7)
        };
        tokio::time::timeout(budget, async {
            while state.registry.has_peer(&address) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("a non-reading socket held its registration after retirement");
        assert!(link.is_closed());
        assert!(state.peers.island_of(&address).is_none());
        assert_eq!(state.feed.socket_write_timeout_count(), u64::from(!kick));
        drop(ws);
    }
}

#[tokio::test]
async fn an_assigned_socket_recovers_a_lost_move_and_reannounces_after_broker_restart() {
    let Some(binary) = catalyrst_testgate::require_env("ARCHIPELAGO_NATS_SERVER_BIN") else {
        return;
    };
    struct Task(tokio::task::JoinHandle<()>);
    impl Drop for Task {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    async fn next_connect(sub: &mut async_nats::Subscriber) -> async_nats::Message {
        tokio::time::timeout(Duration::from_secs(40), sub.next())
            .await
            .expect("an assigned socket was never reconciled")
            .expect("connect subscription ended")
    }
    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(12), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("broker state did not converge");
    }

    let broker = catalyrst_testgate::nats::Broker::start(&binary, None);
    let broker_port = broker.port;
    let mut cfg = test_config(0);
    cfg.nats.url = Some(broker.url());
    let bus = NatsBus::new(cfg.nats.clone());
    let state = build_state_with(&cfg, bus.clone() as Arc<dyn FeedPublisher>)
        .await
        .unwrap();
    let _feed = Task(bus.start(state.clone()).unwrap());
    wait_until(|| bus.is_connected()).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = api_router().with_state(state.clone());
    let _server = Task(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let device = Device::new();
    let observer = async_nats::connect(broker.url()).await.unwrap();
    let mut connects = observer
        .subscribe(format!("peer.{address}.connect"))
        .await
        .unwrap();
    observer.flush().await.unwrap();
    let mut ws = handshake(port, &wallet, &device).await;
    assert_eq!(
        next_connect(&mut connects).await.payload.as_ref(),
        device.session().as_bytes()
    );
    let link = state
        .registry
        .peer_link(&address, &device.session())
        .unwrap();
    let assignment_subject = session_subject(&address, &device.session());
    observer
        .publish(
            assignment_subject.clone(),
            island_payload("before-move").into(),
        )
        .await
        .unwrap();
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("before-move".into())
    );
    assert!(
        !link.needs_assignment(),
        "the first assignment reached the wire"
    );
    // Readiness can race authentication. Silence across two reconciliation ticks ends the
    // bounded startup burst without assuming how many of its passes observed this socket.
    let mut startup_announcements = 0;
    loop {
        let message = match tokio::time::timeout(Duration::from_secs(11), connects.next()).await {
            Ok(Some(message)) => message,
            Ok(None) => panic!("connect subscription ended during startup restoration"),
            Err(_) => break,
        };
        assert_eq!(message.payload.as_ref(), device.session().as_bytes());
        startup_announcements += 1;
        assert!(
            startup_announcements <= 3,
            "startup restoration exceeded its immediate pass and two followups"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(6), connects.next())
            .await
            .is_err(),
        "assigned sockets should not use the five-second initial-assignment retry"
    );

    // The source moves, but its edge-triggered assignment is lost. Only a new
    // current-session announcement can ask the downstream pipeline to recover it.
    assert_eq!(
        next_connect(&mut connects).await.payload.as_ref(),
        device.session().as_bytes()
    );
    assert!(bus.is_connected());
    assert_eq!(
        state
            .registry
            .peer_link(&address, &device.session())
            .unwrap()
            .id,
        link.id,
        "recovery must not require replacing the authenticated socket"
    );
    observer
        .publish(assignment_subject, island_payload("after-move").into())
        .await
        .unwrap();
    assert_eq!(
        recv_island(&mut ws, Duration::from_secs(3)).await,
        Some("after-move".into())
    );
    drop(broker);
    wait_until(|| !bus.is_connected()).await;
    let _restarted = catalyrst_testgate::nats::Broker::start(&binary, Some(broker_port));
    wait_until(|| bus.is_connected()).await;
    let message = tokio::time::timeout(Duration::from_secs(12), connects.next())
        .await
        .expect("broker restoration did not reannounce an already-assigned socket")
        .unwrap();
    assert_eq!(message.payload.as_ref(), device.session().as_bytes());
    assert!(!link.needs_assignment());
    ws.close(None).await.unwrap();
}
