use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::config::{Config, ServerConfig};
use catalyrst_archipelago::control_v4::{
    canonical_signing_payload, AssignmentAuthority, LaneKey, SigningTranscript, SUBPROTOCOL,
};
use catalyrst_archipelago::nats::RecordingPublisher;
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
        let packet = receive(&mut socket).await;
        assert_eq!(packet.request_sequence, 0);
        let Some(pb::server_packet::Message::Hello(hello)) = packet.message else {
            panic!("expected hello");
        };
        (socket, hello)
    }
}

async fn receive(socket: &mut Socket) -> pb::ServerPacket {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket
                .next()
                .await
                .expect("socket still connected")
                .expect("valid frame")
            {
                Message::Binary(bytes) => {
                    return pb::ServerPacket::decode(bytes.as_ref()).expect("valid protobuf frame");
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => panic!("expected binary protobuf response"),
            }
        }
    })
    .await
    .expect("bounded response")
}

async fn send(socket: &mut Socket, packet: &pb::ClientPacket) {
    socket
        .send(Message::Binary(packet.encode_to_vec().into()))
        .await
        .unwrap();
}

fn signed_auth(wallet: &PrivateKeySigner, hello: &pb::Hello) -> pb::Authenticate {
    let address = format!("{:#x}", wallet.address());
    let lanes = [LaneKey::parse("realm").unwrap()];
    let capabilities = hello.capabilities.clone();
    let payload = canonical_signing_payload(SigningTranscript {
        audience: &hello.audience,
        replica_id: &hello.replica_id,
        authority_incarnation: &hello.authority_incarnation,
        connection_id: &hello.connection_id,
        connection_epoch: hello.connection_epoch,
        request_id: "authenticate-1",
        expires_in_ms: hello.expires_in_ms,
        challenge_id: &hello.challenge_id,
        challenge: &hello.challenge,
        address: &address,
        session_id: &address,
        lanes: &lanes,
        capabilities: &capabilities,
    });
    let signature = wallet
        .sign_hash_sync(&alloy::primitives::eip191_hash_message(payload.as_bytes()))
        .unwrap();
    pb::Authenticate {
        request_id: "authenticate-1".into(),
        address: address.clone(),
        session_id: address.clone(),
        connection_epoch: hello.connection_epoch,
        auth_chain: Some(common::AuthChain {
            links: vec![
                common::AuthLink {
                    r#type: common::AuthLinkType::Signer.into(),
                    payload: address,
                    signature: None,
                },
                common::AuthLink {
                    r#type: common::AuthLinkType::EcdsaSignedEntity.into(),
                    payload,
                    signature: Some(signature.to_string()),
                },
            ],
        }),
        capabilities,
        lanes: vec!["realm".into()],
        resume: Default::default(),
    }
}

fn heartbeat(sequence: u64, x: f32) -> pb::ClientPacket {
    pb::ClientPacket {
        message: Some(pb::client_packet::Message::Heartbeat(pb::Heartbeat {
            position: Some(common::Position { x, y: 1.0, z: 2.0 }),
            ..Default::default()
        })),
        request_sequence: sequence,
    }
}

fn acknowledgement(
    auth: &pb::Authenticate,
    sequence: u64,
    applied_revision: u64,
) -> pb::ClientPacket {
    let id = format!("ack-{sequence}");
    pb::ClientPacket {
        message: Some(pb::client_packet::Message::Ack(pb::Ack {
            request_id: id.clone(),
            operation_id: id,
            session_id: auth.session_id.clone(),
            connection_epoch: auth.connection_epoch,
            lane: "realm".into(),
            applied_revision,
        })),
        request_sequence: sequence,
    }
}

fn snapshot(auth: &pb::Authenticate, sequence: u64) -> pb::ClientPacket {
    let id = format!("snapshot-{sequence}");
    pb::ClientPacket {
        message: Some(pb::client_packet::Message::RequestSnapshot(
            pb::RequestSnapshot {
                request_id: id.clone(),
                operation_id: id,
                session_id: auth.session_id.clone(),
                connection_epoch: auth.connection_epoch,
                lane: "realm".into(),
                from_revision: None,
            },
        )),
        request_sequence: sequence,
    }
}

#[tokio::test]
async fn long_lived_heartbeats_reject_delayed_positions_without_reconnect() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, hello) = server.connect().await;
    send(&mut socket, &heartbeat(2, 1.0)).await;
    let unauthenticated = receive(&mut socket).await;
    assert!(matches!(
        unauthenticated.message,
        Some(pb::server_packet::Message::Error(_))
    ));
    assert!(server.state.peers.peers_snapshot().is_empty());
    assert!(hello
        .capabilities
        .iter()
        .any(|capability| capability == "request.monotonic_sequence"));
    let limits = hello.limits.as_ref().unwrap();
    assert_eq!(limits.max_retained_request_sequences, 256);
    assert_eq!(limits.max_requests_per_connection, u64::MAX);

    let auth = signed_auth(&wallet, &hello);
    send(
        &mut socket,
        &pb::ClientPacket {
            message: Some(pb::client_packet::Message::Authenticate(auth.clone())),
            request_sequence: 1,
        },
    )
    .await;
    let welcome = receive(&mut socket).await;
    assert_eq!(welcome.request_sequence, 1);
    assert!(matches!(
        welcome.message,
        Some(pb::server_packet::Message::Welcome(_))
    ));

    let ancient = heartbeat(2, -500.0);
    assert_eq!(ancient.encoded_len(), 21);
    let mut latest = ancient.clone();
    let mut latest_response = None;
    for sequence in 2..=1_102 {
        latest = heartbeat(sequence, sequence as f32);
        send(&mut socket, &latest).await;
        let response = receive(&mut socket).await;
        assert_eq!(response.request_sequence, sequence);
        assert_eq!(
            response.message,
            Some(pb::server_packet::Message::Ack(pb::Ack::default()))
        );
        assert!(response.encoded_len() <= 5);
        latest_response = Some(response);
    }
    let latest_response = latest_response.unwrap();
    assert_eq!(
        server.state.peers.peers_snapshot()[0].position,
        [1_102.0, 1.0, 2.0]
    );

    send(&mut socket, &latest).await;
    assert_eq!(receive(&mut socket).await, latest_response);

    let mut conflicting = latest.clone();
    let Some(pb::client_packet::Message::Heartbeat(frame)) = conflicting.message.as_mut() else {
        unreachable!();
    };
    frame.position.as_mut().unwrap().x = -1_000.0;
    send(&mut socket, &conflicting).await;
    let conflict = receive(&mut socket).await;
    assert_eq!(conflict.request_sequence, 1_102);
    let Some(pb::server_packet::Message::Error(conflict)) = conflict.message else {
        panic!("expected protocol error");
    };
    assert_eq!(conflict.code, pb::ErrorCode::ProtocolViolation as i32);

    send(&mut socket, &ancient).await;
    let stale = receive(&mut socket).await;
    assert_eq!(stale.request_sequence, 2);
    let Some(pb::server_packet::Message::Error(stale)) = stale.message else {
        panic!("expected stale request error");
    };
    assert_eq!(stale.code, pb::ErrorCode::StaleRequest as i32);
    assert_eq!(
        server.state.peers.peers_snapshot()[0].position,
        [1_102.0, 1.0, 2.0]
    );

    let ancient_ack = acknowledgement(&auth, 1_103, 0);
    send(&mut socket, &ancient_ack).await;
    let response = receive(&mut socket).await;
    assert_eq!(response.request_sequence, 1_103);
    assert!(matches!(
        response.message,
        Some(pb::server_packet::Message::Ack(_))
    ));
    let mut sequence = 1_103;
    for _ in 0..300 {
        sequence += 1;
        send(&mut socket, &acknowledgement(&auth, sequence, 1)).await;
        let response = receive(&mut socket).await;
        assert_eq!(response.request_sequence, sequence);
        assert!(matches!(
            response.message,
            Some(pb::server_packet::Message::Ack(_))
        ));

        sequence += 1;
        send(&mut socket, &snapshot(&auth, sequence)).await;
        let response = receive(&mut socket).await;
        assert_eq!(response.request_sequence, sequence);
        assert!(matches!(
            response.message,
            Some(pb::server_packet::Message::AssignmentSnapshot(_))
        ));
    }
    send(&mut socket, &ancient_ack).await;
    let stale = receive(&mut socket).await;
    assert_eq!(stale.request_sequence, 1_103);
    let Some(pb::server_packet::Message::Error(stale)) = stale.message else {
        panic!("expected stale acknowledgement error");
    };
    assert_eq!(stale.code, pb::ErrorCode::StaleRequest as i32);
    let acknowledged_revision: i64 = sqlx::query_scalar(
        "SELECT acknowledged_revision FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind("test-realm")
    .bind(&auth.address)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(acknowledged_revision, 1);

    socket.close(None).await.unwrap();
    drop((server, socket));
    fixture.finish().await;
}
