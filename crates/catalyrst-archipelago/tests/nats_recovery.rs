use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::{
    api_router, build_state_with,
    config::{AuthConfig, ClusterConfig, Config, LivekitConfig, NatsConfig, ServerConfig},
    nats::{FeedPublisher, NatsBus, OUTBOUND_CAPACITY},
    proto::archipelago::{
        client_packet, server_packet, ChallengeRequestMessage, ClientPacket, IslandChangedMessage,
        ServerPacket, SignedChallengeMessage,
    },
    registry::SocketEvent,
};
use futures::{SinkExt, StreamExt};
use prost::Message;
use std::{
    collections::HashSet,
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc},
    thread::JoinHandle,
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message as WsMessage;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Broker {
    child: Child,
    reader: Option<JoinHandle<()>>,
    port: u16,
}

impl Broker {
    fn start(binary: &str, port: Option<u16>) -> Self {
        let mut child = Command::new(binary)
            .args([
                "--addr",
                "127.0.0.1",
                "--port",
                &port.map(|p| p.to_string()).unwrap_or("-1".into()),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start test-owned NATS broker");
        let stderr = child.stderr.take().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if let Some((_, address)) = line.split_once("Listening for client connections on ")
                {
                    if let Ok(address) = address.trim().parse::<SocketAddr>() {
                        let _ = send.try_send(address.port());
                    }
                }
            }
        });
        let mut broker = Self {
            child,
            reader: Some(reader),
            port: 0,
        };
        broker.port = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("broker did not announce its loopback port");
        broker
    }

    fn url(&self) -> String {
        format!("nats://127.0.0.1:{}", self.port)
    }

    #[cfg(unix)]
    fn signal(&mut self, signal: &str) {
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "test broker already exited"
        );
        assert!(Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()
            .unwrap()
            .success());
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct Task(tokio::task::JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn config(url: String) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        cluster: ClusterConfig::default(),
        server: ServerConfig::default(),
        auth: AuthConfig::default(),
        livekit: LivekitConfig::default(),
        nats: NatsConfig {
            url: Some(url),
            ..Default::default()
        },
        content_database_url: None,
        control_database_url: None,
        content_base_url: String::new(),
        commit_hash: String::new(),
    }
}

async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("broker state did not converge");
}

async fn next(subscriber: &mut async_nats::Subscriber) -> async_nats::Message {
    tokio::time::timeout(Duration::from_secs(12), subscriber.next())
        .await
        .expect("session was not reconciled")
        .expect("observer subscription ended")
}

fn packet(message: client_packet::Message) -> tokio_tungstenite::tungstenite::Bytes {
    ClientPacket {
        request_sequence: 0,
        message: Some(message),
    }
    .encode_to_vec()
    .into()
}

async fn next_packet(ws: &mut WsStream) -> server_packet::Message {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .expect("server packet timed out")
            .expect("websocket closed")
            .expect("websocket frame");
        if let WsMessage::Binary(bytes) = frame {
            return ServerPacket::decode(bytes.as_ref())
                .expect("valid server packet")
                .message
                .expect("server packet body");
        }
    }
}

async fn assigned_socket(port: u16, wallet: &PrivateKeySigner) -> (WsStream, String) {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .expect("connect websocket");
    let address = format!("{:#x}", wallet.address());
    ws.send(WsMessage::Binary(packet(
        client_packet::Message::ChallengeRequest(ChallengeRequestMessage {
            offer: None,
            address: address.clone(),
        }),
    )))
    .await
    .expect("send challenge request");
    let challenge = match next_packet(&mut ws).await {
        server_packet::Message::ChallengeResponse(response) => response.challenge_to_sign,
        _ => panic!("challenge response expected"),
    };
    let ephemeral = PrivateKeySigner::random();
    let session = format!("{:#x}", ephemeral.address());
    let mandate = format!(
        "Decentraland Login\nEphemeral address: {session}\nExpiration: 2099-12-31T00:00:00.000Z"
    );
    let mandate_signature = wallet
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(mandate.as_bytes()))
        .expect("sign mandate");
    let challenge_signature = ephemeral
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(
            challenge.as_bytes(),
        ))
        .expect("sign challenge");
    let chain = serde_json::json!([
        { "type": "SIGNER", "payload": address, "signature": "" },
        { "type": "ECDSA_EPHEMERAL", "payload": mandate, "signature": mandate_signature.to_string() },
        { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": challenge_signature.to_string() }
    ]);
    ws.send(WsMessage::Binary(packet(
        client_packet::Message::SignedChallenge(SignedChallengeMessage {
            typed_auth_chain: None,
            auth_chain_json: chain.to_string(),
        }),
    )))
    .await
    .expect("send signed challenge");
    assert!(matches!(
        next_packet(&mut ws).await,
        server_packet::Message::Welcome(_)
    ));
    (ws, session)
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

async fn wait_for_island(ws: &mut WsStream) {
    assert!(matches!(
        next_packet(ws).await,
        server_packet::Message::IslandChanged(_)
    ));
}

#[tokio::test]
async fn bounded_announcements_reconcile_current_sessions_after_a_real_broker_restart() {
    let Some(binary) = catalyrst_testgate::require_env("ARCHIPELAGO_NATS_SERVER_BIN") else {
        return;
    };
    let broker = Broker::start(&binary, None);
    let port = broker.port;
    let cfg = config(broker.url());
    let observer = async_nats::connect(broker.url()).await.unwrap();
    let mut announcements = observer.subscribe("peer.*.connect").await.unwrap();
    observer.flush().await.unwrap();

    let bus = NatsBus::new(cfg.nats.clone());
    let state = build_state_with(&cfg, bus.clone() as Arc<dyn FeedPublisher>)
        .await
        .unwrap();
    let address = format!("0x{:040x}", 1);
    let old_session = format!("0x{:040x}", 2);
    let (old, _old_events, _) = state.registry.on_peer_connected(&address, &old_session);
    let task = Task(bus.start(state.clone()).unwrap());
    wait_for(|| bus.is_connected()).await;
    assert_eq!(
        next(&mut announcements).await.payload.as_ref(),
        old_session.as_bytes()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    while matches!(
        tokio::time::timeout(Duration::from_millis(10), announcements.next()).await,
        Ok(Some(_))
    ) {}
    assert_eq!(
        next(&mut announcements).await.payload.as_ref(),
        old_session.as_bytes(),
        "a live stationary session must be re-announced without a reconnect"
    );

    let subject = format!("peer.{address}.connect");
    let accepted = (0..OUTBOUND_CAPACITY * 8)
        .filter(|_| bus.publish(subject.clone(), old_session.as_bytes().to_vec()))
        .count();
    assert_eq!(
        accepted, OUTBOUND_CAPACITY,
        "the caller cannot grow the queue past its fixed capacity"
    );
    assert!(bus.dropped_count() >= (OUTBOUND_CAPACITY * 7) as u64);
    assert!(
        !bus.publish(subject, vec![0; 2048]),
        "oversized announcements are not retained"
    );

    drop(broker);
    old.close();
    state
        .registry
        .on_peer_disconnected(&address, &old_session, old.id);
    wait_for(|| !bus.is_connected()).await;
    for _ in 0..OUTBOUND_CAPACITY * 8 {
        assert!(!bus.publish(
            format!("peer.{address}.connect"),
            old_session.as_bytes().to_vec()
        ));
    }
    while matches!(
        tokio::time::timeout(Duration::from_millis(10), announcements.next()).await,
        Ok(Some(_))
    ) {}

    let mut expected = HashSet::new();
    let mut receivers = Vec::new();
    let mut first = None;
    for index in 0..OUTBOUND_CAPACITY + 7 {
        let wallet = format!("0x{:040x}", index + 1);
        let session = format!("0x{:040x}", index + 10_000);
        let (_, events, _) = state.registry.on_peer_connected(&wallet, &session);
        if first.is_none() {
            first = Some((wallet.clone(), session.clone()));
        }
        receivers.push(events);
        expected.insert((format!("peer.{wallet}.connect"), session));
    }
    let _recovered = Broker::start(&binary, Some(port));
    wait_for(|| bus.is_connected()).await;
    tokio::time::timeout(Duration::from_secs(20), async {
        while !expected.is_empty() {
            let message = next(&mut announcements).await;
            let session = String::from_utf8(message.payload.to_vec()).unwrap();
            assert_ne!(
                session, old_session,
                "a retired session was replayed on the recovered broker"
            );
            expected.remove(&(message.subject.to_string(), session));
        }
    })
    .await
    .expect("a registry larger than the queue was starved during reconciliation");

    let (wallet, session) = first.unwrap();
    let assignment = IslandChangedMessage {
        assignment_context: None,
        island_id: "recovered-island".into(),
        ..Default::default()
    }
    .encode_to_vec();
    observer
        .publish(
            format!("engine.peer.{wallet}.island_changed.{session}"),
            assignment.clone().into(),
        )
        .await
        .unwrap();
    observer.flush().await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), receivers[0].recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(received, SocketEvent::IslandChanged(bytes) if bytes == assignment));
    assert!(state.registry.peer_link(&address, &old_session).is_none());
    task.0.abort();
    wait_for(|| !bus.is_connected()).await;
}

#[tokio::test]
async fn broker_recovery_retries_an_assigned_socket_after_downstream_subscribes_late() {
    let Some(binary) = catalyrst_testgate::require_env("ARCHIPELAGO_NATS_SERVER_BIN") else {
        return;
    };
    let broker = Broker::start(&binary, None);
    let port = broker.port;
    let cfg = config(broker.url());
    let bus = NatsBus::new(cfg.nats.clone());
    let state = build_state_with(&cfg, bus.clone() as Arc<dyn FeedPublisher>)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_port = listener.local_addr().unwrap().port();
    let app = api_router().with_state(state.clone());
    let server = Task(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let feed = Task(bus.start(state.clone()).unwrap());
    wait_for(|| bus.is_connected()).await;

    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let observer = async_nats::connect(broker.url()).await.unwrap();
    let subject = format!("peer.{address}.connect");
    let mut initial = observer.subscribe(subject.clone()).await.unwrap();
    observer.flush().await.unwrap();
    let (mut ws, session) = assigned_socket(ws_port, &wallet).await;
    assert_eq!(
        next(&mut initial).await.payload.as_ref(),
        session.as_bytes()
    );

    observer
        .publish(
            format!("engine.peer.{address}.island_changed.{session}"),
            island_payload("assigned-before-restart").into(),
        )
        .await
        .unwrap();
    observer.flush().await.unwrap();
    wait_for_island(&mut ws).await;
    let link = state.registry.peer_link(&address, &session).unwrap();
    assert!(!link.needs_assignment(), "assignment reached the socket");
    while matches!(
        tokio::time::timeout(Duration::from_millis(100), initial.next()).await,
        Ok(Some(_))
    ) {}

    drop(broker);
    wait_for(|| !bus.is_connected()).await;
    let restarted = Broker::start(&binary, Some(port));
    wait_for(|| bus.is_connected()).await;
    assert_eq!(
        next(&mut initial).await.payload.as_ref(),
        session.as_bytes(),
        "the first recovery announcement is intentionally consumed before downstream subscribes"
    );

    let downstream = async_nats::connect(restarted.url()).await.unwrap();
    let mut after_restore = downstream.subscribe(subject).await.unwrap();
    downstream.flush().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(12), after_restore.next())
            .await
            .expect("post-reconnect retry did not reach a late downstream subscription")
            .expect("downstream subscription ended")
            .payload
            .as_ref(),
        session.as_bytes()
    );
    assert_eq!(
        state.registry.peer_link(&address, &session).unwrap().id,
        link.id,
        "recovery did not resurrect a stale socket owner"
    );

    ws.close(None).await.unwrap();
    drop((server, feed));
}

#[cfg(unix)]
#[tokio::test]
async fn a_stalled_broker_cannot_hold_readiness_or_grow_the_announcement_queue() {
    let Some(binary) = catalyrst_testgate::require_env("ARCHIPELAGO_NATS_SERVER_BIN") else {
        return;
    };
    let mut broker = Broker::start(&binary, None);
    let cfg = config(broker.url());
    let observer = async_nats::connect(broker.url()).await.unwrap();
    let mut announcements = observer.subscribe("peer.*.connect").await.unwrap();
    observer.flush().await.unwrap();
    let bus = NatsBus::new(cfg.nats.clone());
    let state = build_state_with(&cfg, bus.clone() as Arc<dyn FeedPublisher>)
        .await
        .unwrap();
    let address = format!("0x{:040x}", 91);
    let session = format!("0x{:040x}", 92);
    let (_, _events, _) = state.registry.on_peer_connected(&address, &session);
    let _task = Task(bus.start(state).unwrap());
    wait_for(|| bus.is_connected()).await;
    assert_eq!(
        next(&mut announcements).await.payload.as_ref(),
        session.as_bytes()
    );

    broker.signal("-STOP");
    let accepted = (0..OUTBOUND_CAPACITY * 8)
        .filter(|_| {
            bus.publish(
                format!("peer.{address}.connect"),
                session.as_bytes().to_vec(),
            )
        })
        .count();
    assert_eq!(accepted, OUTBOUND_CAPACITY);
    tokio::time::timeout(Duration::from_secs(5), async {
        while bus.is_connected() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("publish/flush deadline did not invalidate readiness");
    assert!(!bus.publish(
        format!("peer.{address}.connect"),
        session.as_bytes().to_vec()
    ));

    broker.signal("-CONT");
    wait_for(|| bus.is_connected()).await;
    assert_eq!(
        next(&mut announcements).await.payload.as_ref(),
        session.as_bytes()
    );
}
