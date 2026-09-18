use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::config::{Config, ServerConfig};
use catalyrst_archipelago::control_v4::{
    canonical_signing_payload, AssignmentAuthority, AssignmentPayload, LaneKey, SigningTranscript,
    V4Owner, SUBPROTOCOL,
};
use catalyrst_archipelago::nats::RecordingPublisher;
use catalyrst_archipelago::proto::archipelago as v3;
use catalyrst_archipelago::proto::{archipelago_v4 as pb, decentraland::common};
use catalyrst_archipelago::{api_router, build_state_with, AppState};
use futures::{SinkExt, StreamExt};
use prost::Message as _;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

#[path = "support/control_v4.rs"]
mod support;
use support::Fixture;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(3);

struct Server {
    url: String,
    state: AppState,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start(fixture: &Fixture) -> Self {
        let config = Config {
            http_host: "127.0.0.1".into(),
            http_port: 0,
            cluster: Default::default(),
            server: ServerConfig {
                control_v4_audience: "test-realm".into(),
                ..Default::default()
            },
            auth: Default::default(),
            livekit: Default::default(),
            nats: Default::default(),
            content_database_url: None,
            control_database_url: None,
            content_base_url: String::new(),
            commit_hash: String::new(),
        };
        let mut state = build_state_with(&config, RecordingPublisher::new())
            .await
            .unwrap();
        Arc::get_mut(&mut state).unwrap().control_v4 =
            AssignmentAuthority::pg_with_namespace(fixture.pool.clone(), "test-realm")
                .await
                .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = api_router().with_state(state.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url: format!("ws://{address}/ws/v4"),
            state,
            task,
        }
    }

    async fn connect(&self) -> (Socket, pb::Hello) {
        let mut request = self.url.clone().into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
        let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(response.headers()["Sec-WebSocket-Protocol"], SUBPROTOCOL);
        let pb::server_packet::Message::Hello(hello) = receive(&mut socket).await else {
            panic!("expected hello");
        };
        assert_eq!(hello.protocol_version, 4);
        (socket, hello)
    }
}

async fn receive(socket: &mut Socket) -> pb::server_packet::Message {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket
                .next()
                .await
                .expect("socket still connected")
                .expect("valid frame")
            {
                Message::Binary(bytes) => {
                    return pb::ServerPacket::decode(bytes.as_ref())
                        .expect("valid protobuf frame")
                        .message
                        .expect("typed server message");
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => panic!("expected binary protobuf response"),
            }
        }
    })
    .await
    .expect("bounded response")
}

async fn send(socket: &mut Socket, frame: pb::client_packet::Message) {
    let request_sequence = match &frame {
        pb::client_packet::Message::Authenticate(auth) => {
            auth.request_id.rsplit('-').next().unwrap().parse().unwrap()
        }
        _ => NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    };
    socket
        .send(Message::Binary(
            pb::ClientPacket {
                request_sequence,
                message: Some(frame),
            }
            .encode_to_vec()
            .into(),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn common_ws_path_dispatches_legacy_and_v4_by_negotiated_subprotocol() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let common_url = server.url.replace("/ws/v4", "/ws");

    let mut v4_request = common_url.clone().into_client_request().unwrap();
    v4_request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
    let (mut v4_socket, v4_response) = tokio_tungstenite::connect_async(v4_request).await.unwrap();
    assert_eq!(v4_response.headers()["Sec-WebSocket-Protocol"], SUBPROTOCOL);
    let pb::server_packet::Message::Hello(hello) = receive(&mut v4_socket).await else {
        panic!("expected v4 hello on common websocket path");
    };
    assert_eq!(hello.protocol_version, 4);

    let mut legacy_request = common_url.into_client_request().unwrap();
    legacy_request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "archipelago".parse().unwrap());
    let (mut legacy_socket, legacy_response) = tokio_tungstenite::connect_async(legacy_request)
        .await
        .unwrap();
    assert_eq!(
        legacy_response.headers()["Sec-WebSocket-Protocol"],
        "archipelago"
    );
    let wallet = PrivateKeySigner::random();
    legacy_socket
        .send(Message::Binary(
            v3::ClientPacket {
                request_sequence: 0,
                message: Some(v3::client_packet::Message::ChallengeRequest(
                    v3::ChallengeRequestMessage {
                        offer: None,
                        address: format!("{:#x}", wallet.address()),
                    },
                )),
            }
            .encode_to_vec()
            .into(),
        ))
        .await
        .unwrap();
    let legacy_frame = tokio::time::timeout(Duration::from_secs(5), legacy_socket.next())
        .await
        .expect("bounded legacy response")
        .expect("legacy socket open")
        .expect("valid legacy frame");
    let Message::Binary(legacy_bytes) = legacy_frame else {
        panic!("expected legacy binary response");
    };
    let legacy_packet = v3::ServerPacket::decode(legacy_bytes.as_ref()).unwrap();
    assert!(matches!(
        legacy_packet.message,
        Some(v3::server_packet::Message::ChallengeResponse(_))
    ));

    v4_socket.close(None).await.unwrap();
    legacy_socket.close(None).await.unwrap();
    drop(server);
    fixture.finish().await;
}

fn signed_auth(
    wallet: &PrivateKeySigner,
    signer: &PrivateKeySigner,
    hello: &pb::Hello,
    lanes: &[&str],
) -> pb::Authenticate {
    signed_auth_with_request(wallet, signer, hello, lanes, "authenticate-1")
}

fn signed_auth_with_request(
    wallet: &PrivateKeySigner,
    signer: &PrivateKeySigner,
    hello: &pb::Hello,
    lanes: &[&str],
    request_id: &str,
) -> pb::Authenticate {
    let address = format!("{:#x}", wallet.address());
    let session = format!("{:#x}", signer.address());
    let lanes: Vec<_> = lanes
        .iter()
        .map(|lane| LaneKey::parse(lane).unwrap())
        .collect();
    let capabilities = hello.capabilities.clone();
    let payload = canonical_signing_payload(SigningTranscript {
        audience: &hello.audience,
        replica_id: &hello.replica_id,
        authority_incarnation: &hello.authority_incarnation,
        connection_id: &hello.connection_id,
        connection_epoch: hello.connection_epoch,
        request_id,
        expires_in_ms: hello.expires_in_ms,
        challenge_id: &hello.challenge_id,
        challenge: &hello.challenge,
        address: &address,
        session_id: &session,
        lanes: &lanes,
        capabilities: &capabilities,
    });
    let signature = signer
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(payload.as_bytes()))
        .unwrap();
    let mut links = vec![common::AuthLink {
        r#type: common::AuthLinkType::Signer.into(),
        payload: address.clone(),
        signature: None,
    }];
    if wallet.address() != signer.address() {
        let delegation = format!("Decentraland Login\nEphemeral address: {session}\nExpiration: 2099-12-31T00:00:00.000Z");
        let signature = wallet
            .sign_hash_sync(&alloy::primitives::eip191_hash_message(
                delegation.as_bytes(),
            ))
            .unwrap();
        links.push(common::AuthLink {
            r#type: common::AuthLinkType::EcdsaEphemeral.into(),
            payload: delegation,
            signature: Some(signature.to_string()),
        });
    }
    links.push(common::AuthLink {
        r#type: common::AuthLinkType::EcdsaSignedEntity.into(),
        payload,
        signature: Some(signature.to_string()),
    });
    pb::Authenticate {
        request_id: request_id.into(),
        address,
        session_id: session,
        connection_epoch: hello.connection_epoch,
        auth_chain: Some(common::AuthChain { links }),
        capabilities,
        lanes: lanes.iter().map(|lane| lane.as_str().to_owned()).collect(),
        resume: Default::default(),
    }
}

fn snapshot_request(auth: &pb::Authenticate, lane: &str, id: &str) -> pb::client_packet::Message {
    pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
        request_id: id.into(),
        operation_id: id.into(),
        session_id: auth.session_id.clone(),
        connection_epoch: auth.connection_epoch,
        lane: lane.into(),
        from_revision: None,
    })
}

async fn authenticate(socket: &mut Socket, auth: &pb::Authenticate) {
    send(
        socket,
        pb::client_packet::Message::Authenticate(auth.clone()),
    )
    .await;
}

async fn receive_snapshot(socket: &mut Socket) -> pb::AssignmentSnapshot {
    let pb::server_packet::Message::AssignmentSnapshot(snapshot) = receive(socket).await else {
        panic!("expected assignment snapshot");
    };
    snapshot
}

#[tokio::test]
async fn signed_authentication_is_idempotent_on_one_connection_and_not_replayable_on_another() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let first = Server::start(&fixture).await;
    let second = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let device = PrivateKeySigner::random();
    let (mut socket, hello) = first.connect().await;
    let auth = signed_auth(&wallet, &device, &hello, &["realm"]);
    authenticate(&mut socket, &auth).await;
    let welcome = receive(&mut socket).await;
    assert!(matches!(welcome, pb::server_packet::Message::Welcome(_)));
    authenticate(&mut socket, &auth).await;
    assert_eq!(receive(&mut socket).await, welcome);

    let (mut other, other_hello) = second.connect().await;
    assert_ne!(hello.replica_id, other_hello.replica_id);
    authenticate(&mut other, &auth).await;
    assert!(matches!(
        receive(&mut other).await,
        pb::server_packet::Message::Error(_)
    ));
    send(&mut socket, snapshot_request(&auth, "realm", "snapshot-1")).await;
    assert_eq!(receive_snapshot(&mut socket).await.lane, "realm");
    socket.close(None).await.unwrap();
    other.close(None).await.unwrap();
    drop((first, second, socket, other));
    fixture.finish().await;
}

type TranscriptChange = (&'static str, for<'a> fn(&mut SigningTranscript<'a>));

fn control_payload(
    lanes: &[LaneKey],
    capabilities: &[String],
    change: for<'a> fn(&mut SigningTranscript<'a>),
) -> String {
    let mut transcript = SigningTranscript {
        audience: "test-realm",
        replica_id: "replica-a",
        authority_incarnation: "incarnation-a",
        connection_id: "connection-a",
        connection_epoch: 7,
        request_id: "authenticate-1",
        expires_in_ms: 30_000,
        challenge_id: "challenge-id-a",
        challenge: "challenge-a",
        address: "0x00000000000000000000000000000000000000aa",
        session_id: "0x00000000000000000000000000000000000000bb",
        lanes,
        capabilities,
    };
    change(&mut transcript);
    canonical_signing_payload(transcript)
}

#[test]
fn everything_that_names_the_server_the_socket_or_the_session_is_signed() {
    let lanes = [LaneKey::parse("realm").unwrap()];
    let capabilities = ["assignment.snapshot".to_string()];
    let signed = control_payload(&lanes, &capabilities, |_| {});
    let changes: [TranscriptChange; 13] = [
        ("audience", |t| t.audience = "other-realm"),
        ("replica", |t| t.replica_id = "replica-b"),
        ("authority incarnation", |t| {
            t.authority_incarnation = "incarnation-b"
        }),
        ("connection id", |t| t.connection_id = "connection-b"),
        ("connection epoch", |t| t.connection_epoch += 1),
        ("request id", |t| t.request_id = "authenticate-2"),
        ("expiry", |t| t.expires_in_ms += 1),
        ("challenge id", |t| t.challenge_id = "challenge-id-b"),
        ("challenge", |t| t.challenge = "challenge-b"),
        ("address", |t| {
            t.address = "0x00000000000000000000000000000000000000ab"
        }),
        ("session", |t| {
            t.session_id = "0x00000000000000000000000000000000000000bc"
        }),
        ("lanes", |t| t.lanes = &[]),
        ("capabilities", |t| t.capabilities = &[]),
    ];
    for (field, change) in changes {
        assert_ne!(
            control_payload(&lanes, &capabilities, change),
            signed,
            "{field} is outside the signed transcript"
        );
    }
}

#[tokio::test]
async fn an_authentication_captured_on_one_socket_fails_its_signature_on_another() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let device = PrivateKeySigner::random();
    let (mut owned, hello) = server.connect().await;
    let captured = signed_auth(&wallet, &device, &hello, &["realm"]);

    let (mut other, other_hello) = server.connect().await;
    assert_eq!(hello.replica_id, other_hello.replica_id);
    assert_eq!(
        hello.authority_incarnation,
        other_hello.authority_incarnation
    );
    assert_ne!(hello.connection_id, other_hello.connection_id);
    let mut rewritten = captured.clone();
    rewritten.connection_epoch = other_hello.connection_epoch;
    authenticate(&mut other, &rewritten).await;
    let pb::server_packet::Message::Error(refusal) = receive(&mut other).await else {
        panic!("a captured authentication must be refused");
    };
    assert_eq!(
        refusal.name, "auth_failed",
        "only the signature stands between a rewritten envelope and admission"
    );

    authenticate(&mut owned, &captured).await;
    assert!(matches!(
        receive(&mut owned).await,
        pb::server_packet::Message::Welcome(_)
    ));
    owned.close(None).await.unwrap();
    drop((server, owned, other));
    fixture.finish().await;
}

#[tokio::test]
async fn separate_realm_and_scene_connections_keep_their_own_lane() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut realm, realm_hello) = server.connect().await;
    let realm_auth = signed_auth(&wallet, &wallet, &realm_hello, &["realm"]);
    authenticate(&mut realm, &realm_auth).await;
    assert!(matches!(
        receive(&mut realm).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let (mut scene, scene_hello) = server.connect().await;
    let scene_auth = signed_auth(&wallet, &wallet, &scene_hello, &["scene:abc"]);
    authenticate(&mut scene, &scene_auth).await;
    assert!(matches!(
        receive(&mut scene).await,
        pb::server_packet::Message::Welcome(_)
    ));
    send(
        &mut realm,
        snapshot_request(&realm_auth, "realm", "realm-snapshot"),
    )
    .await;
    assert_eq!(receive_snapshot(&mut realm).await.lane, "realm");
    send(
        &mut scene,
        snapshot_request(&scene_auth, "scene:abc", "scene-snapshot"),
    )
    .await;
    assert_eq!(receive_snapshot(&mut scene).await.lane, "scene:abc");
    realm.close(None).await.unwrap();
    scene.close(None).await.unwrap();
    drop((server, realm, scene));
    fixture.finish().await;
}

#[tokio::test]
async fn polling_recovers_committed_assignments_without_broker_notifications() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, hello) = server.connect().await;
    let auth = signed_auth(&wallet, &wallet, &hello, &["realm"]);
    authenticate(&mut socket, &auth).await;
    assert!(matches!(
        receive(&mut socket).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let owner = V4Owner {
        address: auth.address.clone(),
        session: auth.session_id.clone(),
        epoch: hello.connection_epoch,
    };
    let state = server
        .state
        .control_v4
        .publish_assignment(
            &owner,
            &LaneKey::parse("realm").unwrap(),
            AssignmentPayload {
                island_id: "current-room".into(),
                connection_string: "offline:offline".into(),
                from_island_id: None,
                peers: Default::default(),
            },
        )
        .await
        .unwrap();
    let snapshot = receive_snapshot(&mut socket).await;
    assert_eq!(snapshot.assignment_revision, state.assignment_revision);
    assert_eq!(snapshot.assignment.unwrap().island_id, "current-room");
    assert_eq!(snapshot.owner_epoch, owner.epoch);
    socket.close(None).await.unwrap();
    drop((server, socket));
    fixture.finish().await;
}

async fn frame_before(
    socket: &mut Socket,
    deadline: tokio::time::Instant,
) -> Option<pb::server_packet::Message> {
    loop {
        match tokio::time::timeout_at(deadline, socket.next()).await {
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                return pb::ServerPacket::decode(bytes.as_ref())
                    .expect("valid protobuf frame")
                    .message;
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            _ => return None,
        }
    }
}

#[tokio::test]
async fn a_lane_lost_to_a_newer_socket_is_reported_once_while_the_other_lanes_keep_recovering() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut both, both_hello) = server.connect().await;
    let both_auth = signed_auth(&wallet, &wallet, &both_hello, &["scene:abc", "realm"]);
    authenticate(&mut both, &both_auth).await;
    assert!(matches!(
        receive(&mut both).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let (mut scene, scene_hello) = server.connect().await;
    let scene_auth = signed_auth(&wallet, &wallet, &scene_hello, &["scene:abc"]);
    authenticate(&mut scene, &scene_auth).await;
    assert!(matches!(
        receive(&mut scene).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let owner = V4Owner {
        address: both_auth.address.clone(),
        session: both_auth.session_id.clone(),
        epoch: both_hello.connection_epoch,
    };
    let state = server
        .state
        .control_v4
        .publish_assignment(
            &owner,
            &LaneKey::parse("realm").unwrap(),
            AssignmentPayload {
                island_id: "realm-room".into(),
                connection_string: "offline:offline".into(),
                from_island_id: None,
                peers: Default::default(),
            },
        )
        .await
        .unwrap();

    let mut conflicts = 0;
    let mut realm_revision = None;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(6_500);
    while let Some(message) = frame_before(&mut both, deadline).await {
        match message {
            pb::server_packet::Message::Error(error) => {
                assert_eq!(error.code, pb::ErrorCode::AuthorityConflict as i32);
                conflicts += 1;
            }
            pb::server_packet::Message::AssignmentSnapshot(snapshot)
                if snapshot.lane == "realm" =>
            {
                realm_revision = Some(snapshot.assignment_revision);
            }
            _ => {}
        }
    }
    assert_eq!(realm_revision, Some(state.assignment_revision));
    assert_eq!(conflicts, 1);
    drop((server, both, scene));
    fixture.finish().await;
}

#[tokio::test]
async fn the_socket_ends_once_every_lane_it_held_is_lost() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut older, older_hello) = server.connect().await;
    let older_auth = signed_auth(&wallet, &wallet, &older_hello, &["scene:abc"]);
    authenticate(&mut older, &older_auth).await;
    assert!(matches!(
        receive(&mut older).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let (mut newer, newer_hello) = server.connect().await;
    let newer_auth = signed_auth(&wallet, &wallet, &newer_hello, &["scene:abc"]);
    authenticate(&mut newer, &newer_auth).await;
    assert!(matches!(
        receive(&mut newer).await,
        pb::server_packet::Message::Welcome(_)
    ));

    let deadline = tokio::time::Instant::now() + Duration::from_millis(4_500);
    let mut conflicts = 0;
    while let Some(message) = frame_before(&mut older, deadline).await {
        if let pb::server_packet::Message::Error(error) = message {
            assert_eq!(error.code, pb::ErrorCode::AuthorityConflict as i32);
            conflicts += 1;
        }
    }
    assert_eq!(conflicts, 1);
    assert!(tokio::time::Instant::now() < deadline);
    drop((server, older, newer));
    fixture.finish().await;
}

#[tokio::test]
async fn non_protobuf_frames_are_rejected_without_consuming_the_challenge() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, hello) = server.connect().await;
    for frame in [
        Message::Text("{\"type\":\"authenticate\"}".into()),
        Message::Binary(b"{\"type\":\"authenticate\"}".to_vec().into()),
        Message::Binary(vec![0xff].into()),
    ] {
        socket.send(frame).await.unwrap();
        let pb::server_packet::Message::Error(error) = receive(&mut socket).await else {
            panic!("invalid frame must return a typed error");
        };
        assert_eq!(error.code, pb::ErrorCode::InvalidFrame as i32);
    }
    let auth = signed_auth(&wallet, &wallet, &hello, &["realm"]);
    authenticate(&mut socket, &auth).await;
    assert!(matches!(
        receive(&mut socket).await,
        pb::server_packet::Message::Welcome(_)
    ));
    socket.close(None).await.unwrap();
    drop((server, socket));
    fixture.finish().await;
}

#[tokio::test]
async fn unknown_authentication_link_types_are_rejected() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    for link_type in [common::AuthLinkType::Unknown as i32, i32::MAX] {
        let (mut socket, hello) = server.connect().await;
        let mut auth = signed_auth(&wallet, &wallet, &hello, &["realm"]);
        auth.auth_chain.as_mut().unwrap().links[0].r#type = link_type;
        authenticate(&mut socket, &auth).await;
        let pb::server_packet::Message::Error(error) = receive(&mut socket).await else {
            panic!("unknown authentication link must return an error");
        };
        assert_eq!(error.code, pb::ErrorCode::AuthFailed as i32);
        authenticate(&mut socket, &auth).await;
        assert_eq!(
            receive(&mut socket).await,
            pb::server_packet::Message::Error(error)
        );
        let valid =
            signed_auth_with_request(&wallet, &wallet, &hello, &["realm"], "authenticate-2");
        authenticate(&mut socket, &valid).await;
        let pb::server_packet::Message::Error(error) = receive(&mut socket).await else {
            panic!("rejected authentication must consume its challenge");
        };
        assert_eq!(error.code, pb::ErrorCode::AuthFailed as i32);
        socket.close(None).await.unwrap();
    }
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn non_finite_heartbeat_positions_are_rejected() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, hello) = server.connect().await;
    let auth = signed_auth(&wallet, &wallet, &hello, &["realm"]);
    authenticate(&mut socket, &auth).await;
    assert!(matches!(
        receive(&mut socket).await,
        pb::server_packet::Message::Welcome(_)
    ));
    for (index, x) in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
        .into_iter()
        .enumerate()
    {
        let id = format!("heartbeat-{index}");
        send(
            &mut socket,
            pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                request_id: id.clone(),
                operation_id: id,
                session_id: auth.session_id.clone(),
                connection_epoch: auth.connection_epoch,
                position: Some(common::Position { x, y: 0.0, z: 0.0 }),
                desired_room: None,
            }),
        )
        .await;
        let pb::server_packet::Message::Error(error) = receive(&mut socket).await else {
            panic!("non-finite position must return an error");
        };
        assert_eq!(error.code, pb::ErrorCode::InvalidFrame as i32);
    }
    send(
        &mut socket,
        pb::client_packet::Message::Heartbeat(pb::Heartbeat {
            request_id: "heartbeat-valid".into(),
            operation_id: "heartbeat-valid".into(),
            session_id: auth.session_id.clone(),
            connection_epoch: auth.connection_epoch,
            position: Some(common::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            desired_room: None,
        }),
    )
    .await;
    assert!(matches!(
        receive(&mut socket).await,
        pb::server_packet::Message::Ack(_)
    ));
    socket.close(None).await.unwrap();
    drop((server, socket));
    fixture.finish().await;
}

#[tokio::test]
async fn a_closed_socket_releases_the_lanes_it_owned() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, hello) = server.connect().await;
    let auth = signed_auth(&wallet, &wallet, &hello, &["realm"]);
    authenticate(&mut socket, &auth).await;
    assert!(matches!(
        receive(&mut socket).await,
        pb::server_packet::Message::Welcome(_)
    ));
    let released = "SELECT updated_at = '-infinity' FROM archipelago_v4_assignments
                    WHERE owner_address = $1 AND lane_key = 'realm'";
    let held: bool = sqlx::query_scalar(released)
        .bind(&auth.address)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert!(!held);

    socket.close(None).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let gone: bool = sqlx::query_scalar(released)
            .bind(&auth.address)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
        if gone {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop((server, socket));
    fixture.finish().await;
}
