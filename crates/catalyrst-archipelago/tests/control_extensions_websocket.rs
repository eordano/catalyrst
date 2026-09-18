use std::{sync::Arc, time::Duration};

use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::{
    api_router, build_state_with,
    config::{Config, ServerConfig},
    control_extensions::{
        canonical_challenge, CORE_FEATURES, FEATURE_INHERIT_CONTEXT, KNOWN_FEATURES,
    },
    control_v4::AssignmentAuthority,
    low_latency_listener,
    nats::RecordingPublisher,
    proto::{archipelago as pb, decentraland::common},
    AppState,
};
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
    publisher: Arc<RecordingPublisher>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start(fixture: &Fixture) -> Self {
        Self::with_authority(fixture, true).await
    }

    async fn with_authority(fixture: &Fixture, available: bool) -> Self {
        Self::with_pool(fixture.pool.clone(), available, None).await
    }

    async fn with_pool(
        pool: sqlx::PgPool,
        available: bool,
        handshake_timeout_ms: Option<u64>,
    ) -> Self {
        let mut config = Config {
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
        if let Some(timeout) = handshake_timeout_ms {
            config.auth.handshake_timeout_ms = timeout;
        }
        let publisher = RecordingPublisher::new();
        let mut state = build_state_with(&config, publisher.clone()).await.unwrap();
        if available {
            Arc::get_mut(&mut state).unwrap().control_v4 =
                AssignmentAuthority::pg_with_namespace(pool, "test-realm")
                    .await
                    .unwrap();
        } else {
            Arc::get_mut(&mut state).unwrap().cfg.control_database_url =
                Some("postgresql://unavailable.invalid/control".into());
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        let app = api_router().with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(low_latency_listener(listener), app)
                .await
                .unwrap()
        });
        Self {
            url,
            state,
            publisher,
            task,
        }
    }

    async fn connect(&self) -> Socket {
        let mut request = self.url.clone().into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "archipelago".parse().unwrap());
        let (socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(response.headers()["Sec-WebSocket-Protocol"], "archipelago");
        socket
    }
}

fn packet(sequence: u64, message: pb::client_packet::Message) -> Vec<u8> {
    pb::ClientPacket {
        request_sequence: sequence,
        message: Some(message),
    }
    .encode_to_vec()
}

async fn send(socket: &mut Socket, bytes: &[u8]) {
    socket
        .send(Message::Binary(bytes.to_vec().into()))
        .await
        .unwrap();
}

async fn receive(socket: &mut Socket) -> pb::ServerPacket {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket
                .next()
                .await
                .expect("socket open")
                .expect("valid frame")
            {
                Message::Binary(bytes) => return pb::ServerPacket::decode(bytes.as_ref()).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected frame: {other:?}"),
            }
        }
    })
    .await
    .expect("bounded response")
}

fn offer(wallet: &PrivateKeySigner, lanes: &[&str]) -> pb::ConnectionOffer {
    pb::ConnectionOffer {
        version: 4,
        offered_features: KNOWN_FEATURES,
        required_features: CORE_FEATURES,
        client_nonce: uuid::Uuid::new_v4().as_bytes().to_vec(),
        session_id: wallet.address().as_slice().to_vec(),
        lanes: lanes.iter().map(|s| s.to_string()).collect(),
    }
}

async fn opening(
    socket: &mut Socket,
    wallet: &PrivateKeySigner,
    offer: pb::ConnectionOffer,
) -> pb::ChallengeResponseMessage {
    send(
        socket,
        &packet(
            0,
            pb::client_packet::Message::ChallengeRequest(pb::ChallengeRequestMessage {
                address: format!("{:#x}", wallet.address()),
                offer: Some(offer.clone()),
            }),
        ),
    )
    .await;
    let response = receive(socket).await;
    let Some(pb::server_packet::Message::ChallengeResponse(challenge)) = response.message else {
        panic!("challenge expected: {response:?}")
    };
    let selection = challenge.selection.as_ref().unwrap();
    assert_eq!(selection.context.as_ref().unwrap().audience, "test-realm");
    assert_eq!(
        canonical_challenge(&format!("{:#x}", wallet.address()), &offer, selection).unwrap(),
        challenge.challenge_to_sign
    );
    challenge
}

fn signed(wallet: &PrivateKeySigner, challenge: &str) -> pb::SignedChallengeMessage {
    pb::SignedChallengeMessage {
        auth_chain_json: String::new(),
        typed_auth_chain: Some(common::AuthChain {
            links: vec![
                common::AuthLink {
                    r#type: common::AuthLinkType::Signer as i32,
                    payload: format!("{:#x}", wallet.address()),
                    signature: None,
                },
                common::AuthLink {
                    r#type: common::AuthLinkType::EcdsaSignedEntity as i32,
                    payload: challenge.into(),
                    signature: Some(
                        wallet
                            .sign_message_sync(challenge.as_bytes())
                            .unwrap()
                            .to_string(),
                    ),
                },
            ],
        }),
    }
}

async fn legacy_open(server: &Server, wallet: &PrivateKeySigner) -> (Socket, String) {
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &packet(
            0,
            pb::client_packet::Message::ChallengeRequest(pb::ChallengeRequestMessage {
                address: format!("{:#x}", wallet.address()),
                offer: None,
            }),
        ),
    )
    .await;
    let Some(pb::server_packet::Message::ChallengeResponse(challenge)) =
        receive(&mut socket).await.message
    else {
        panic!("legacy challenge expected")
    };
    assert!(challenge.selection.is_none());
    (socket, challenge.challenge_to_sign)
}

async fn legacy_sign(
    socket: &mut Socket,
    wallet: &PrivateKeySigner,
    signer: &PrivateKeySigner,
    challenge: &str,
) {
    let mut links = vec![
        serde_json::json!({"type": "SIGNER", "payload": format!("{:#x}", wallet.address()), "signature": ""}),
    ];
    if wallet.address() != signer.address() {
        let mandate = format!(
            "Decentraland Login\nEphemeral address: {:#x}\nExpiration: 2099-12-31T00:00:00.000Z",
            signer.address()
        );
        let signature = wallet
            .sign_message_sync(mandate.as_bytes())
            .unwrap()
            .to_string();
        links.push(serde_json::json!({"type": "ECDSA_EPHEMERAL", "payload": mandate, "signature": signature}));
    }
    links.push(serde_json::json!({"type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": signer.sign_message_sync(challenge.as_bytes()).unwrap().to_string()}));
    send(
        socket,
        &packet(
            0,
            pb::client_packet::Message::SignedChallenge(pb::SignedChallengeMessage {
                auth_chain_json: serde_json::to_string(&links).unwrap(),
                typed_auth_chain: None,
            }),
        ),
    )
    .await;
}

async fn current_owner(
    server: &Server,
    fixture: &Fixture,
    wallet: &PrivateKeySigner,
) -> catalyrst_archipelago::control_v4::V4Owner {
    let address = format!("{:#x}", wallet.address());
    let (session, epoch): (String, i64) = sqlx::query_as("SELECT owner_session, owner_epoch FROM archipelago_v4_assignments WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'")
        .bind(&server.state.cfg.server.control_v4_audience).bind(&address).fetch_one(&fixture.pool).await.unwrap();
    catalyrst_archipelago::control_v4::V4Owner {
        address,
        session,
        epoch: epoch as u64,
    }
}

#[tokio::test]
async fn control_heartbeats_publish_only_current_realm_owners_and_epoch_scoped_closes() {
    use catalyrst_types::control_position::{ControlPosition, SUBJECT};
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut legacy, challenge) = legacy_open(&server, &wallet).await;
    legacy_sign(&mut legacy, &wallet, &wallet, &challenge).await;
    assert!(matches!(
        receive(&mut legacy).await.message,
        Some(pb::server_packet::Message::Welcome(_))
    ));
    let old = current_owner(&server, &fixture, &wallet).await;
    let heartbeat = packet(
        0,
        pb::client_packet::Message::Heartbeat(pb::Heartbeat {
            position: Some(common::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            desired_room: Some("shared-realm".into()),
            sample_sequence: None,
        }),
    );
    let records = || {
        server
            .publisher
            .sent()
            .into_iter()
            .filter(|(subject, _)| subject == SUBJECT)
            .map(|(_, payload)| ControlPosition::decode(&payload).expect("valid control position"))
            .collect::<Vec<_>>()
    };
    send(&mut legacy, &heartbeat).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !records()
            .iter()
            .any(|record| record.epoch == old.epoch && record.position.is_some())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("legacy heartbeat publishes without a Pulse connection");
    let first = records()
        .into_iter()
        .find(|record| record.position.is_some())
        .unwrap();
    assert_eq!(first.wallet, old.address);
    assert_eq!(first.session, old.session);
    assert_eq!(first.audience, "test-realm");
    assert_eq!(first.realm, "shared-realm");
    assert_eq!(first.position, Some([1.0, 2.0, 3.0]));

    let mut enhanced = server.connect().await;
    admit(&mut enhanced, &wallet, &["realm"]).await;
    let next = current_owner(&server, &fixture, &wallet).await;
    assert!(next.epoch > old.epoch);
    send(&mut enhanced, &heartbeat).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !records()
            .iter()
            .any(|record| record.epoch == next.epoch && record.position.is_some())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("additive heartbeat publishes without a Pulse connection");
    drop(legacy);
    tokio::time::timeout(Duration::from_secs(3), async {
        while !records()
            .iter()
            .any(|record| record.epoch == old.epoch && record.position.is_none())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("superseded socket closes its own epoch only");
    assert!(!records()
        .iter()
        .any(|record| record.epoch == next.epoch && record.position.is_none()));

    let mut scene = server.connect().await;
    admit(&mut scene, &wallet, &["scene:plaza"]).await;
    let before = records().len();
    send(&mut scene, &heartbeat).await;
    send(
        &mut scene,
        &packet(
            2,
            pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                lane_handle: 1,
                from_revision: None,
            }),
        ),
    )
    .await;
    assert_eq!(receive(&mut scene).await.request_sequence, 2);
    scene.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        records().len(),
        before,
        "scene-only socket cannot move or close realm membership"
    );
    enhanced.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !records()
            .iter()
            .any(|record| record.epoch == next.epoch && record.position.is_none())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("current realm close publishes tombstone");
    for pair in records().windows(2) {
        assert!(pair[1].sequence > pair[0].sequence);
    }
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn additive_heartbeat_authority_deadline_never_publishes_an_unverified_position() {
    use catalyrst_types::control_position::{ControlPosition, SUBJECT};
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let mut socket = server.connect().await;
    admit(&mut socket, &wallet, &["realm"]).await;
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(fixture.pool.acquire().await.unwrap());
    }
    let started = std::time::Instant::now();
    send(
        &mut socket,
        &packet(
            0,
            pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                position: Some(common::Position {
                    x: 99.0,
                    y: 0.0,
                    z: 99.0,
                }),
                desired_room: Some("test-realm".into()),
                sample_sequence: None,
            }),
        ),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Binary(bytes))) => {
                    let response = pb::ServerPacket::decode(bytes).unwrap();
                    let Some(pb::server_packet::Message::Error(error)) = response.message else {
                        panic!("unavailable authority must not grant or acknowledge position")
                    };
                    assert_eq!(error.code, 1005);
                    break;
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                other => panic!("unexpected authority failure reply {other:?}"),
            }
        }
    })
    .await
    .expect("whole authority operation is bounded, including pool acquisition");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!server
        .publisher
        .sent()
        .iter()
        .filter(|(subject, _)| subject == SUBJECT)
        .filter_map(|(_, bytes)| ControlPosition::decode(bytes))
        .any(|record| record.position.is_some()));
    assert!(server
        .state
        .peers
        .peer(&format!("{:#x}", wallet.address()))
        .is_none());
    drop(held);
    drop(socket);
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn a_legacy_socket_receives_the_new_fenced_same_room_assignment() {
    use catalyrst_archipelago::control_v4::{AssignmentPayload, LaneKey};
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let realm = LaneKey::parse("realm").unwrap();
    for delegated in [false, true] {
        let wallet = PrivateKeySigner::random();
        let signer = if delegated {
            PrivateKeySigner::random()
        } else {
            wallet.clone()
        };
        let mut old_socket = server.connect().await;
        admit(&mut old_socket, &wallet, &["realm"]).await;
        let old = current_owner(&server, &fixture, &wallet).await;
        let old_state = server
            .state
            .control_v4
            .snapshot(&old, &realm)
            .await
            .unwrap();
        let (mut legacy, challenge) = legacy_open(&server, &wallet).await;
        legacy_sign(&mut legacy, &wallet, &signer, &challenge).await;
        let welcome = receive(&mut legacy).await;
        assert_eq!(welcome.request_sequence, 0);
        let Some(pb::server_packet::Message::Welcome(welcome)) = welcome.message else {
            panic!("legacy welcome")
        };
        assert!(welcome.context.is_none());
        let next = current_owner(&server, &fixture, &wallet).await;
        assert!(next.epoch > old.epoch);
        assert_eq!(next.session, format!("{:#x}", signer.address()));
        let claimed = server
            .state
            .control_v4
            .snapshot(&next, &realm)
            .await
            .unwrap();
        assert!(claimed.assignment.is_none());
        assert!(claimed.fencing_token > old_state.fencing_token);
        assert!(server.state.control_v4.renew(&old).await.is_err());
        assert!(server.state.control_v4.release(&old).await.is_err());
        assert!(server
            .state
            .control_v4
            .snapshot(&old, &realm)
            .await
            .is_err());
        for token in ["first-current-credential", "renewed-same-room-credential"] {
            server
                .state
                .control_v4
                .publish_assignment(
                    &next,
                    &realm,
                    AssignmentPayload {
                        island_id: "same-room".into(),
                        connection_string: token.into(),
                        from_island_id: None,
                        peers: Default::default(),
                    },
                )
                .await
                .unwrap();
            let packet = receive(&mut legacy).await;
            assert_eq!(packet.request_sequence, 0);
            let Some(pb::server_packet::Message::IslandChanged(assignment)) = packet.message else {
                panic!("legacy assignment")
            };
            assert_eq!(assignment.island_id, "same-room");
            assert_eq!(assignment.conn_str, token);
            assert!(assignment.assignment_context.is_none());
        }
        assert!(
            server
                .state
                .registry
                .peer_link(&next.address, &next.session)
                .is_none(),
            "protected socket must not consume the unfenced feed"
        );
        let mut replacement = server.connect().await;
        admit(&mut replacement, &wallet, &["realm"]).await;
        let replacement_owner = current_owner(&server, &fixture, &wallet).await;
        drop(legacy);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            server
                .state
                .control_v4
                .snapshot(&replacement_owner, &realm)
                .await
                .is_ok(),
            "old legacy close must not release the newer owner"
        );
        drop((old_socket, replacement));
    }
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn a_delayed_legacy_authentication_cannot_take_a_newer_additive_owner() {
    use catalyrst_archipelago::control_v4::LaneKey;
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let other_replica = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut delayed, challenge) = legacy_open(&server, &wallet).await;
    let mut newer = other_replica.connect().await;
    admit(&mut newer, &wallet, &["realm"]).await;
    let owner = current_owner(&server, &fixture, &wallet).await;
    let realm = LaneKey::parse("realm").unwrap();
    let before = server
        .state
        .control_v4
        .snapshot(&owner, &realm)
        .await
        .unwrap();
    legacy_sign(&mut delayed, &wallet, &wallet, &challenge).await;
    assert!(matches!(
        receive(&mut delayed).await.message,
        Some(pb::server_packet::Message::Kicked(_))
    ));
    assert_eq!(
        before,
        server
            .state
            .control_v4
            .snapshot(&owner, &realm)
            .await
            .unwrap()
    );
    drop((delayed, newer, server, other_replica));
    fixture.finish().await;
}

#[tokio::test]
async fn an_established_protected_socket_is_superseded_across_replicas() {
    use catalyrst_archipelago::control_v4::{AssignmentPayload, LaneKey};
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let first_replica = Server::start(&fixture).await;
    let second_replica = Server::start(&fixture).await;
    let realm = LaneKey::parse("realm").unwrap();
    for delegated in [false, true] {
        let wallet = PrivateKeySigner::random();
        let next_signer = if delegated {
            PrivateKeySigner::random()
        } else {
            wallet.clone()
        };
        let (mut old_socket, old_challenge) = legacy_open(&first_replica, &wallet).await;
        legacy_sign(&mut old_socket, &wallet, &wallet, &old_challenge).await;
        assert!(matches!(
            receive(&mut old_socket).await.message,
            Some(pb::server_packet::Message::Welcome(_))
        ));
        let old_owner = current_owner(&first_replica, &fixture, &wallet).await;

        let (mut current_socket, current_challenge) = legacy_open(&second_replica, &wallet).await;
        legacy_sign(
            &mut current_socket,
            &wallet,
            &next_signer,
            &current_challenge,
        )
        .await;
        assert!(matches!(
            receive(&mut current_socket).await.message,
            Some(pb::server_packet::Message::Welcome(_))
        ));
        let current_owner = current_owner(&second_replica, &fixture, &wallet).await;
        assert!(current_owner.epoch > old_owner.epoch);
        assert_eq!(
            current_owner.session,
            format!("{:#x}", next_signer.address())
        );
        assert!(matches!(
            receive(&mut old_socket).await.message,
            Some(pb::server_packet::Message::Kicked(_))
        ));

        second_replica
            .state
            .control_v4
            .publish_assignment(
                &current_owner,
                &realm,
                AssignmentPayload {
                    island_id: "replica-current-room".into(),
                    connection_string: "replica-current-credential".into(),
                    from_island_id: None,
                    peers: Default::default(),
                },
            )
            .await
            .unwrap();
        let Some(pb::server_packet::Message::IslandChanged(assignment)) =
            receive(&mut current_socket).await.message
        else {
            panic!("current replica assignment expected")
        };
        assert_eq!(assignment.island_id, "replica-current-room");
        assert_eq!(assignment.conn_str, "replica-current-credential");
        assert!(assignment.assignment_context.is_none());
        assert!(first_replica
            .state
            .control_v4
            .snapshot(&old_owner, &realm)
            .await
            .is_err());
        drop((old_socket, current_socket));
    }
    drop((first_replica, second_replica));
    fixture.finish().await;
}

#[tokio::test]
async fn a_stalled_authority_closes_a_protected_legacy_socket_within_the_operation_budget() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let (mut socket, challenge) = legacy_open(&server, &wallet).await;
    legacy_sign(&mut socket, &wallet, &wallet, &challenge).await;
    assert!(matches!(
        receive(&mut socket).await.message,
        Some(pb::server_packet::Message::Welcome(_))
    ));

    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(fixture.pool.acquire().await.unwrap());
    }
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            match socket.next().await {
                None | Some(Ok(Message::Close(_))) => break,
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Ok(frame)) => {
                    panic!("authority outage must not emit a fallback frame: {frame:?}")
                }
                Some(Err(_)) => break,
            }
        }
    })
    .await
    .expect("authority operation budget must bound the protected socket lifetime");
    drop(held);
    drop((socket, server));
    fixture.finish().await;
}

async fn legacy_claim_recovers_after_database_stall(statement_timeout_ms: u64) {
    use catalyrst_archipelago::control_v4::{AssignmentPayload, LaneKey};
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(fixture.pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let server = Server::with_pool(authority_pool.clone(), true, Some(1500)).await;
    sqlx::query("SELECT set_config('statement_timeout', $1, false)")
        .bind(format!("{statement_timeout_ms}ms"))
        .execute(&authority_pool)
        .await
        .unwrap();
    let wallet = PrivateKeySigner::random();
    let (mut interrupted, challenge) = legacy_open(&server, &wallet).await;
    let mut held = fixture.pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE archipelago_v4_assignments IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *held)
        .await
        .unwrap();
    legacy_sign(&mut interrupted, &wallet, &wallet, &challenge).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match interrupted.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Binary(bytes))) => {
                    let packet = pb::ServerPacket::decode(bytes.as_ref()).unwrap();
                    assert!(
                        !matches!(packet.message, Some(pb::server_packet::Message::Kicked(_))),
                        "a database stall after the challenge must not terminally reject the client"
                    );
                    panic!("no admission is valid while the authority is stalled: {packet:?}");
                }
                Some(Ok(frame)) => panic!("unexpected stalled-claim frame: {frame:?}"),
            }
        }
    })
    .await
    .expect("stalled claim must close within its handshake budget");
    held.rollback().await.unwrap();
    sqlx::query("SELECT 1")
        .execute(&authority_pool)
        .await
        .unwrap();
    let address = format!("{:#x}", wallet.address());
    let claimed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM archipelago_v4_assignments WHERE owner_address = $1",
    )
    .bind(&address)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(claimed, 0, "a cancelled claim must not commit later");

    let (mut retry, fresh_challenge) = legacy_open(&server, &wallet).await;
    assert_ne!(challenge, fresh_challenge);
    legacy_sign(&mut retry, &wallet, &wallet, &fresh_challenge).await;
    assert!(matches!(
        receive(&mut retry).await.message,
        Some(pb::server_packet::Message::Welcome(_))
    ));
    let owner = current_owner(&server, &fixture, &wallet).await;
    server
        .state
        .control_v4
        .publish_assignment(
            &owner,
            &LaneKey::parse("realm").unwrap(),
            AssignmentPayload {
                island_id: "recovered-room".into(),
                connection_string: "recovered-credential".into(),
                from_island_id: None,
                peers: Default::default(),
            },
        )
        .await
        .unwrap();
    let Some(pb::server_packet::Message::IslandChanged(assignment)) =
        receive(&mut retry).await.message
    else {
        panic!("reconnected client must receive the current room")
    };
    assert_eq!(assignment.island_id, "recovered-room");
    drop((interrupted, retry, server));
    authority_pool.close().await;
    fixture.finish().await;
}

#[tokio::test]
async fn authority_unavailable_during_legacy_claim_is_retryable() {
    legacy_claim_recovers_after_database_stall(200).await;
}

#[tokio::test]
async fn authority_deadline_during_legacy_claim_is_retryable() {
    legacy_claim_recovers_after_database_stall(10_000).await;
}

#[tokio::test]
async fn configured_authority_failure_does_not_offer_an_unfenced_legacy_handshake() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::with_authority(&fixture, false).await;
    let wallet = PrivateKeySigner::random();
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &packet(
            0,
            pb::client_packet::Message::ChallengeRequest(pb::ChallengeRequestMessage {
                address: format!("{:#x}", wallet.address()),
                offer: None,
            }),
        ),
    )
    .await;
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap();
    assert!(
        !matches!(frame, Some(Ok(Message::Binary(_)))),
        "no unfenced challenge or welcome may be offered"
    );
    assert!(!server
        .state
        .registry
        .has_any_peer(&format!("{:#x}", wallet.address())));
    drop((socket, server));
    fixture.finish().await;
}

async fn admit(socket: &mut Socket, wallet: &PrivateKeySigner, lanes: &[&str]) -> Vec<u8> {
    let challenge = opening(socket, wallet, offer(wallet, lanes)).await;
    let bytes = packet(
        1,
        pb::client_packet::Message::SignedChallenge(signed(wallet, &challenge.challenge_to_sign)),
    );
    send(socket, &bytes).await;
    let welcome = receive(socket).await;
    assert_eq!(welcome.request_sequence, 1);
    let Some(pb::server_packet::Message::Welcome(welcome)) = welcome.message else {
        panic!("welcome expected")
    };
    assert_eq!(welcome.peer_id, format!("{:#x}", wallet.address()));
    assert!(welcome.context.is_none());
    assert_eq!(welcome.encoded_len(), 44);
    for _ in lanes {
        let snapshot = receive(socket).await;
        assert_eq!(snapshot.request_sequence, 0);
        let Some(pb::server_packet::Message::IslandChanged(snapshot)) = snapshot.message else {
            panic!("initial snapshot expected")
        };
        let context = snapshot.assignment_context.unwrap();
        assert!(context.removed);
        assert!(context.assignment_revision > 0);
        assert!(context.fencing_token > 0);
    }
    bytes
}

fn error(packet: pb::ServerPacket, code: i32) {
    let Some(pb::server_packet::Message::Error(error)) = packet.message else {
        panic!("error expected")
    };
    assert_eq!(error.code, code);
}

#[tokio::test]
async fn common_endpoint_keeps_legacy_bytes_and_negotiates_extensions_without_sample_acks() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let legacy_wallet = PrivateKeySigner::random();
    let mut legacy = server.connect().await;
    send(
        &mut legacy,
        &packet(
            0,
            pb::client_packet::Message::ChallengeRequest(pb::ChallengeRequestMessage {
                address: format!("{:#x}", legacy_wallet.address()),
                offer: None,
            }),
        ),
    )
    .await;
    let Some(pb::server_packet::Message::ChallengeResponse(challenge)) =
        receive(&mut legacy).await.message
    else {
        panic!("legacy challenge expected")
    };
    assert!(challenge.selection.is_none());
    let signature = legacy_wallet
        .sign_message_sync(challenge.challenge_to_sign.as_bytes())
        .unwrap()
        .to_string();
    let chain = serde_json::json!([
        {"type":"SIGNER", "payload":format!("{:#x}", legacy_wallet.address()), "signature":""},
        {"type":"ECDSA_SIGNED_ENTITY", "payload":challenge.challenge_to_sign, "signature":signature}
    ]);
    send(
        &mut legacy,
        &packet(
            0,
            pb::client_packet::Message::SignedChallenge(pb::SignedChallengeMessage {
                auth_chain_json: chain.to_string(),
                typed_auth_chain: None,
            }),
        ),
    )
    .await;
    let welcome = receive(&mut legacy).await;
    assert_eq!(welcome.request_sequence, 0);
    assert!(matches!(
        welcome.message,
        Some(pb::server_packet::Message::Welcome(pb::WelcomeMessage {
            context: None,
            ..
        }))
    ));
    let mut enhanced = server.connect().await;
    admit(&mut enhanced, &wallet, &["realm"]).await;
    let heartbeat = packet(
        0,
        pb::client_packet::Message::Heartbeat(pb::Heartbeat {
            position: Some(common::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            desired_room: None,
            sample_sequence: None,
        }),
    );
    assert_eq!(heartbeat.len(), 19);
    for _ in 0..1101 {
        send(&mut enhanced, &heartbeat).await;
    }
    let snapshot = packet(
        2,
        pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
            lane_handle: 0,
            from_revision: None,
        }),
    );
    send(&mut enhanced, &snapshot).await;
    let reply = receive(&mut enhanced).await;
    assert_eq!(
        reply.request_sequence, 2,
        "no position acknowledgements queued"
    );
    assert!(matches!(
        reply.message,
        Some(pb::server_packet::Message::IslandChanged(_))
    ));
    assert_eq!(
        server
            .state
            .peers
            .peer(&format!("{:#x}", wallet.address()))
            .unwrap()
            .position,
        [1.0, 2.0, 3.0]
    );
    for (sample, x) in [(2, 9.0), (1, -9.0)] {
        send(
            &mut enhanced,
            &packet(
                0,
                pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                    position: Some(common::Position { x, y: 2.0, z: 3.0 }),
                    desired_room: None,
                    sample_sequence: Some(sample),
                }),
            ),
        )
        .await;
    }
    send(
        &mut enhanced,
        &packet(
            3,
            pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                lane_handle: 0,
                from_revision: None,
            }),
        ),
    )
    .await;
    assert_eq!(receive(&mut enhanced).await.request_sequence, 3);
    assert_eq!(
        server
            .state
            .peers
            .peer(&format!("{:#x}", wallet.address()))
            .unwrap()
            .position[0],
        9.0
    );
    for desired_room in ["x".repeat(257), "room\nname".into()] {
        send(
            &mut enhanced,
            &packet(
                0,
                pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                    position: Some(common::Position {
                        x: 99.0,
                        y: 99.0,
                        z: 99.0,
                    }),
                    desired_room: Some(desired_room),
                    sample_sequence: None,
                }),
            ),
        )
        .await;
        error(receive(&mut enhanced).await, 1001);
    }
    assert_eq!(
        server
            .state
            .peers
            .peer(&format!("{:#x}", wallet.address()))
            .unwrap()
            .position[0],
        9.0
    );
    enhanced.close(None).await.unwrap();
    legacy.close(None).await.unwrap();
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn exact_retries_are_bounded_and_scene_only_admission_preserves_realm() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let mut realm = server.connect().await;
    let auth = admit(&mut realm, &wallet, &["realm"]).await;
    let mut scene = server.connect().await;
    admit(&mut scene, &wallet, &["scene:plaza"]).await;
    send(
        &mut realm,
        &packet(
            0,
            pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                position: Some(common::Position {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                }),
                desired_room: None,
                sample_sequence: None,
            }),
        ),
    )
    .await;
    let first = packet(
        2,
        pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
            lane_handle: 0,
            from_revision: None,
        }),
    );
    send(&mut realm, &first).await;
    let original = receive(&mut realm).await;
    send(&mut realm, &first).await;
    assert_eq!(receive(&mut realm).await, original);
    let mut changed = first.clone();
    changed.extend_from_slice(&[0x50, 1]);
    send(&mut realm, &changed).await;
    error(receive(&mut realm).await, 1002);
    for sequence in 3..=302 {
        send(
            &mut realm,
            &packet(
                sequence,
                pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                    lane_handle: 0,
                    from_revision: None,
                }),
            ),
        )
        .await;
        assert_eq!(receive(&mut realm).await.request_sequence, sequence);
    }
    send(&mut realm, &first).await;
    error(receive(&mut realm).await, 1010);
    send(&mut realm, &auth).await;
    assert!(matches!(
        receive(&mut realm).await.message,
        Some(pb::server_packet::Message::Welcome(_))
    ));
    send(
        &mut scene,
        &packet(
            0,
            pb::client_packet::Message::Heartbeat(pb::Heartbeat {
                position: Some(common::Position {
                    x: 99.0,
                    y: 99.0,
                    z: 99.0,
                }),
                desired_room: Some("scene-room".into()),
                sample_sequence: None,
            }),
        ),
    )
    .await;
    send(
        &mut scene,
        &packet(
            2,
            pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                lane_handle: 1,
                from_revision: None,
            }),
        ),
    )
    .await;
    assert_eq!(receive(&mut scene).await.request_sequence, 2);
    assert_eq!(
        server
            .state
            .peers
            .peer(&format!("{:#x}", wallet.address()))
            .unwrap()
            .position,
        [1.0, 2.0, 3.0]
    );
    let invalid_lane = packet(
        3,
        pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
            lane_handle: 0,
            from_revision: None,
        }),
    );
    send(&mut scene, &invalid_lane).await;
    let invalid_result = receive(&mut scene).await;
    error(invalid_result.clone(), 1009);
    send(&mut scene, &invalid_lane).await;
    assert_eq!(receive(&mut scene).await, invalid_result);
    send(
        &mut scene,
        &packet(
            3,
            pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                lane_handle: 1,
                from_revision: None,
            }),
        ),
    )
    .await;
    error(receive(&mut scene).await, 1002);
    realm.close(None).await.unwrap();
    scene.close(None).await.unwrap();
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn welcome_context_inheritance_preserves_older_additive_clients() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    for features in [CORE_FEATURES, KNOWN_FEATURES] {
        let wallet = PrivateKeySigner::random();
        let mut socket = server.connect().await;
        let mut offered = offer(&wallet, &["realm"]);
        offered.offered_features = features;
        let challenge = opening(&mut socket, &wallet, offered).await;
        assert_eq!(
            challenge.selection.as_ref().unwrap().selected_features,
            features
        );
        let authentication = packet(
            1,
            pb::client_packet::Message::SignedChallenge(signed(
                &wallet,
                &challenge.challenge_to_sign,
            )),
        );
        send(&mut socket, &authentication).await;
        let response = receive(&mut socket).await;
        assert_eq!(response.request_sequence, 1);
        let Some(pb::server_packet::Message::Welcome(welcome)) = response.message.as_ref() else {
            panic!("welcome expected")
        };
        assert_eq!(welcome.peer_id, format!("{:#x}", wallet.address()));
        if features & FEATURE_INHERIT_CONTEXT == 0 {
            assert_eq!(welcome.context, challenge.selection);
        } else {
            assert!(welcome.context.is_none());
            assert_eq!(welcome.encoded_len(), 44);
        }
        let initial = receive(&mut socket).await;
        assert_eq!(initial.request_sequence, 0);
        let Some(pb::server_packet::Message::IslandChanged(initial)) = initial.message else {
            panic!("initial snapshot expected")
        };
        assert!(initial.assignment_context.as_ref().unwrap().removed);
        send(&mut socket, &authentication).await;
        assert_eq!(receive(&mut socket).await, response);
        send(
            &mut socket,
            &packet(
                2,
                pb::client_packet::Message::RequestSnapshot(pb::RequestSnapshot {
                    lane_handle: 0,
                    from_revision: None,
                }),
            ),
        )
        .await;
        let recovered = receive(&mut socket).await;
        assert_eq!(recovered.request_sequence, 2);
        assert_eq!(
            recovered.message,
            Some(pb::server_packet::Message::IslandChanged(initial))
        );
        socket.close(None).await.unwrap();
    }
    let wallet = PrivateKeySigner::random();
    let mut socket = server.connect().await;
    let offered = offer(&wallet, &["realm"]);
    let challenge = opening(&mut socket, &wallet, offered.clone()).await;
    let mut changed_selection = challenge.selection.unwrap();
    changed_selection.selected_features &= !FEATURE_INHERIT_CONTEXT;
    let changed_challenge = canonical_challenge(
        &format!("{:#x}", wallet.address()),
        &offered,
        &changed_selection,
    )
    .unwrap();
    send(
        &mut socket,
        &packet(
            1,
            pb::client_packet::Message::SignedChallenge(signed(&wallet, &changed_challenge)),
        ),
    )
    .await;
    error(receive(&mut socket).await, 1003);
    let _ = socket.close(None).await;
    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn negotiation_authentication_and_delayed_owner_admission_fail_closed() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let mut invalid = server.connect().await;
    let mut unsupported = offer(&wallet, &["realm"]);
    unsupported.offered_features |= 1 << 63;
    unsupported.required_features |= 1 << 63;
    send(
        &mut invalid,
        &packet(
            0,
            pb::client_packet::Message::ChallengeRequest(pb::ChallengeRequestMessage {
                address: format!("{:#x}", wallet.address()),
                offer: Some(unsupported),
            }),
        ),
    )
    .await;
    error(receive(&mut invalid).await, 1002);
    let mut ambiguous = server.connect().await;
    let mut optional = offer(&wallet, &["realm"]);
    optional.offered_features |= 1 << 63;
    let challenge = opening(&mut ambiguous, &wallet, optional).await;
    assert_eq!(
        challenge.selection.as_ref().unwrap().selected_features,
        KNOWN_FEATURES
    );
    let mut auth = signed(&wallet, &challenge.challenge_to_sign);
    auth.auth_chain_json = "[]".into();
    send(
        &mut ambiguous,
        &packet(1, pb::client_packet::Message::SignedChallenge(auth)),
    )
    .await;
    error(receive(&mut ambiguous).await, 1003);
    let mut held = server.connect().await;
    let held_challenge = opening(&mut held, &wallet, offer(&wallet, &["realm"])).await;
    let held_auth = packet(
        1,
        pb::client_packet::Message::SignedChallenge(signed(
            &wallet,
            &held_challenge.challenge_to_sign,
        )),
    );
    let mut current = server.connect().await;
    admit(&mut current, &wallet, &["realm"]).await;
    send(&mut held, &held_auth).await;
    error(receive(&mut held).await, 1006);
    let mut other = server.connect().await;
    opening(&mut other, &wallet, offer(&wallet, &["realm"])).await;
    send(&mut other, &held_auth).await;
    error(receive(&mut other).await, 1003);
    for socket in [&mut ambiguous, &mut held, &mut current, &mut other] {
        let _ = socket.close(None).await;
    }
    drop(server);
    fixture.finish().await;
}
