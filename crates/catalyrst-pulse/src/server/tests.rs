use super::*;

#[test]
fn corrupt_packets_exhaust_budget_then_disconnect() {
    let mut srv = PulseServer::new();

    let bad: &[u8] = &[0x0A, 0xFF];

    for _ in 0..5 {
        assert_eq!(
            srv.dispatch(7, channel::RELIABLE, bad, 1000, 1000),
            Action::Ignore
        );
    }

    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, bad, 1000, 1000),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::PacketCorrupted
        }
    );
}

use crate::decentraland::common::{
    AuthChain as ProtoAuthChain, AuthLink as ProtoAuthLink, AuthLinkType as ProtoAuthLinkType,
};
use crate::decentraland::pulse::{
    client_message, ClientMessage, PlayerInitialState, PlayerState, PlayerStateInput,
    ProfileVersionAnnouncement, PulseV4Auth, PulseV4Challenge, PulseV4Hello, PulseV4Role,
    ResyncRequest, SceneListenerAoi, TeleportRequest,
};

fn client_msg(inner: client_message::Message) -> Vec<u8> {
    ClientMessage {
        message: Some(inner),
    }
    .encode_to_vec()
}

fn valid_state(parcel: i32) -> PlayerState {
    let mut state = PlayerState {
        parcel_index: parcel,
        ..Default::default()
    };
    state.set_position_x_f(8.0);
    state.set_position_z_f(8.0);
    state
}

fn authed(srv: &mut PulseServer, peer: u32, wallet: &str) {
    let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
    st.wallet_id = Some(wallet.into());
    srv.peers.insert(peer, st);
    srv.identity.set(peer, wallet.into());
    srv.board.set_active(peer);
}

#[test]
fn gameplay_from_unauthenticated_peer_is_ignored() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(7, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
        state: Some(PlayerState::default()),
    }));
    assert_eq!(
        srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 0),
        Action::Ignore
    );
}

#[test]
fn authenticated_input_publishes_snapshot_with_real_sequence() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 7, "0xabc");
    assert_eq!(srv.board.last_seq(7), crate::snapshot::NO_SEQ);
    let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
        state: Some(valid_state(5)),
    }));
    assert_eq!(
        srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 100),
        Action::Applied
    );

    assert_eq!(srv.board.last_seq(7), 0);
    let snap = srv.board.try_read(7).unwrap();
    assert_eq!(snap.seq, 0);
    assert_eq!(snap.server_tick, 100);
    assert_eq!(snap.parcel, 5);

    let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
        state: Some(valid_state(6)),
    }));
    srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 150);
    assert_eq!(srv.board.last_seq(7), 1);
}

fn teleport_request(parcel: i32, realm: &str) -> TeleportRequest {
    let mut req = TeleportRequest {
        parcel_index: parcel,
        realm: realm.into(),
        ..Default::default()
    };
    req.set_position_x_f(8.0);
    req.set_position_z_f(8.0);
    req
}

#[test]
fn teleport_seeds_realm_and_position() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 3, "0xabc");
    let req = teleport_request(0, "realm-a");
    let expected_x = req.position_x;
    let bytes = client_msg(client_message::Message::Teleport(req));
    assert_eq!(
        srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
        Action::Applied
    );
    let snap = srv.board.try_read(3).unwrap();
    assert_eq!(snap.realm.as_deref(), Some("realm-a"));
    assert_eq!(snap.position_x, expected_x, "raw code stored verbatim");
    assert!(snap.is_teleport);
    assert_eq!(snap.last_teleport_seq, snap.seq);
}

#[test]
fn teleport_with_empty_realm_is_rejected() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 3, "0xabc");
    let bytes = client_msg(client_message::Message::Teleport(teleport_request(0, "")));
    assert_eq!(
        srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
        Action::Ignore
    );
    assert!(
        srv.board.try_read(3).is_none(),
        "rejected teleport publishes nothing"
    );
}

#[test]
fn teleport_with_overlong_realm_is_rejected() {
    let mut srv = PulseServer::new();
    srv.max_realm_length = 4;
    authed(&mut srv, 3, "0xabc");
    let bytes = client_msg(client_message::Message::Teleport(teleport_request(
        0, "abcd",
    )));
    assert_eq!(
        srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
        Action::Applied
    );
    let snap = srv.board.try_read(3).unwrap();
    assert_eq!(snap.realm.as_deref(), Some("abcd"));

    let bytes = client_msg(client_message::Message::Teleport(teleport_request(
        0, "abcde",
    )));
    assert_eq!(
        srv.dispatch(3, channel::RELIABLE, &bytes, 0, 60),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::InvalidTeleportField
        }
    );
    let snap = srv.board.try_read(3).unwrap();
    assert_eq!(
        snap.realm.as_deref(),
        Some("abcd"),
        "rejected teleport publishes nothing"
    );
}

#[test]
fn teleport_with_out_of_range_code_is_rejected() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 3, "0xabc");
    let mut req = teleport_request(0, "realm-a");
    req.position_y = 8192;
    let bytes = client_msg(client_message::Message::Teleport(req));
    assert_eq!(
        srv.dispatch(3, channel::RELIABLE, &bytes, 0, 50),
        Action::Ignore
    );
    assert!(srv.board.try_read(3).is_none());
}

#[test]
fn teleport_request_caps_boundaries() {
    let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
    type Set = fn(&mut TeleportRequest, u32);
    let cases: [(&str, u32, Set); 3] = [
        ("position_x", 255, |r, v| r.position_x = v),
        ("position_y", 8191, |r, v| r.position_y = v),
        ("position_z", 255, |r, v| r.position_z = v),
    ];
    for (name, cap, set) in cases {
        let mut req = teleport_request(0, "realm-a");
        set(&mut req, cap);
        assert!(validate::teleport(&req, &encoder), "{name} at cap accepted");
        set(&mut req, cap + 1);
        assert!(
            !validate::teleport(&req, &encoder),
            "{name} above cap rejected"
        );
    }
}

#[test]
fn player_state_caps_boundaries() {
    let encoder = ParcelEncoder::new(ParcelEncoderOptions::default());
    type Set = fn(&mut PlayerState, u32);
    let cases: [(&str, u32, Set); 14] = [
        ("position_x", 255, |s, v| s.position_x = v),
        ("position_y", 8191, |s, v| s.position_y = v),
        ("position_z", 255, |s, v| s.position_z = v),
        ("velocity_x", 255, |s, v| s.velocity_x = v),
        ("velocity_y", 255, |s, v| s.velocity_y = v),
        ("velocity_z", 255, |s, v| s.velocity_z = v),
        ("rotation_y", 127, |s, v| s.rotation_y = v),
        ("movement_blend", 31, |s, v| s.movement_blend = v),
        ("slide_blend", 15, |s, v| s.slide_blend = v),
        ("head_yaw", 127, |s, v| s.head_yaw = Some(v)),
        ("head_pitch", 127, |s, v| s.head_pitch = Some(v)),
        ("point_at_x", 131071, |s, v| s.point_at_x = Some(v)),
        ("point_at_y", 127, |s, v| s.point_at_y = Some(v)),
        ("point_at_z", 131071, |s, v| s.point_at_z = Some(v)),
    ];
    for (name, cap, set) in cases {
        let mut state = valid_state(0);
        set(&mut state, cap);
        assert!(
            validate::player_state(&state, &encoder),
            "{name} at cap accepted"
        );
        set(&mut state, cap + 1);
        assert!(
            !validate::player_state(&state, &encoder),
            "{name} above cap rejected"
        );
    }
}

#[test]
fn resync_request_is_recorded_on_peer_state() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 1, "0xobs");
    let bytes = client_msg(client_message::Message::Resync(ResyncRequest {
        subject_id: 9,
        known_seq: 42,
    }));
    assert_eq!(
        srv.dispatch(1, channel::RELIABLE, &bytes, 0, 0),
        Action::Applied
    );
    let reqs = srv.peers[&1].resync_requests.as_ref().unwrap();
    assert_eq!(reqs.get(&9), Some(&42));
}

#[test]
fn profile_announcement_is_monotonic() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 2, "0xabc");
    let bytes = client_msg(client_message::Message::ProfileAnnouncement(
        ProfileVersionAnnouncement { version: 5 },
    ));
    srv.dispatch(2, channel::RELIABLE, &bytes, 0, 0);
    assert_eq!(srv.profiles.get(2), 5);

    let bytes = client_msg(client_message::Message::ProfileAnnouncement(
        ProfileVersionAnnouncement { version: 3 },
    ));
    srv.dispatch(2, channel::RELIABLE, &bytes, 0, 0);
    assert_eq!(srv.profiles.get(2), 5);
}

#[test]
fn malformed_packet_is_ignored() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 1, "0xabc");
    assert_eq!(
        srv.dispatch(1, 0, &[0xFF, 0xFF, 0xFF], 0, 0),
        Action::Ignore
    );
}

#[test]
fn bad_handshake_replies_with_failure() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let bytes = client_msg(client_message::Message::Handshake(
        crate::decentraland::pulse::HandshakeRequest {
            auth_chain: b"not json".to_vec(),
            profile_version: 0,
            initial_state: None,
            protocol_features: 0,
        },
    ));
    match srv.dispatch(1, channel::RELIABLE, &bytes, 1000, 0) {
        Action::Reply(ServerMessage {
            message: Some(server_message::Message::Handshake(h)),
        }) => {
            assert!(!h.success);
            assert!(h.error.is_some());
        }
        other => panic!("unexpected: {other:?}"),
    }
}

async fn signed_handshake_request() -> (Vec<u8>, String, i64) {
    use crate::handshake::build_signed_fetch_payload;
    use alloy::signers::{local::PrivateKeySigner, Signer};
    use catalyrst_types::{AuthLink, AuthLinkType};

    let root: PrivateKeySigner = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let root_addr = format!("{:#x}", root.address());
    let ephemeral: PrivateKeySigner =
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
            .parse()
            .unwrap();
    let eph_addr = format!("{:#x}", ephemeral.address());

    let ts = "1700000000000";
    let now_ms: i64 = ts.parse().unwrap();
    let metadata = "{\"signer\":\"dcl:explorer\"}";
    let connect_payload = build_signed_fetch_payload("connect", "/", ts, metadata);

    let eph_payload = format!(
        "Decentraland Login\nEphemeral address: {eph_addr}\nExpiration: 2099-01-01T00:00:00.000Z"
    );
    let eph_sig = root
        .sign_message(eph_payload.as_bytes())
        .await
        .unwrap()
        .to_string();
    let final_sig = ephemeral
        .sign_message(connect_payload.as_bytes())
        .await
        .unwrap()
        .to_string();

    let chain = [
        AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: root_addr.clone(),
            signature: None,
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaEphemeral,
            payload: eph_payload,
            signature: Some(eph_sig),
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaSignedEntity,
            payload: connect_payload,
            signature: Some(final_sig),
        },
    ];

    let mut map = serde_json::Map::new();
    for (i, link) in chain.iter().enumerate() {
        map.insert(
            format!("x-identity-auth-chain-{i}"),
            serde_json::Value::String(serde_json::to_string(link).unwrap()),
        );
    }
    map.insert(
        "x-identity-timestamp".into(),
        serde_json::Value::String(ts.into()),
    );
    map.insert(
        "x-identity-metadata".into(),
        serde_json::Value::String(metadata.into()),
    );
    let bag = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();

    let bytes = client_msg(client_message::Message::Handshake(
        crate::decentraland::pulse::HandshakeRequest {
            auth_chain: bag.into_bytes(),
            profile_version: 0,
            initial_state: None,
            protocol_features: 0,
        },
    ));
    (bytes, root_addr.to_lowercase(), now_ms)
}

fn with_initial_state(bytes: &[u8], init: Option<PlayerInitialState>) -> Vec<u8> {
    let mut msg = ClientMessage::decode(bytes).unwrap();
    if let Some(client_message::Message::Handshake(h)) = msg.message.as_mut() {
        h.initial_state = init;
    }
    msg.encode_to_vec()
}

#[tokio::test]
async fn handshake_seeds_valid_initial_state() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, wallet, now_ms) = signed_handshake_request().await;

    let init = PlayerInitialState {
        state: Some(valid_state(7)),
        realm: "realm-a".into(),
        ..Default::default()
    };
    let bytes = with_initial_state(&base, Some(init));

    let action = srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0);
    match &action {
        Action::Authenticated {
            wallet: w,
            initial_state,
            ..
        } => {
            assert_eq!(*w, wallet);
            assert!(
                initial_state.is_some(),
                "validated initial state carried on the action"
            );
        }
        other => panic!("expected Authenticated, got {other:?}"),
    }

    srv.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::Authenticated;
    srv.identity.set(1, wallet.clone());
    srv.board.set_active(1);
    if let Action::Authenticated {
        initial_state: Some(init),
        ..
    } = action
    {
        srv.seed_initial_state(1, 500, &init);
    }
    let snap = srv.board.try_read(1).expect("seeded snapshot present");
    assert_eq!(snap.parcel, 7);
    assert_eq!(snap.server_tick, 500);
}

#[tokio::test]
async fn handshake_with_malformed_initial_state_is_rejected() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _wallet, now_ms) = signed_handshake_request().await;

    let mut bad = valid_state(7);
    bad.position_y = 8192;
    let init = PlayerInitialState {
        state: Some(bad),
        realm: "realm-a".into(),
        ..Default::default()
    };
    let bytes = with_initial_state(&base, Some(init));

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
            match reply.and_then(|m| m.message) {
                Some(server_message::Message::Handshake(h)) => {
                    assert!(!h.success, "malformed initial state must not authenticate");
                    assert!(h.error.is_some());
                }
                other => panic!("expected handshake failure reply, got {other:?}"),
            }
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));
}

#[test]
fn input_with_out_of_range_code_is_rejected() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 7, "0xabc");
    let mut state = valid_state(3);
    state.rotation_y = 128;
    let bytes = client_msg(client_message::Message::Input(PlayerStateInput {
        state: Some(state),
    }));
    assert_eq!(
        srv.dispatch(7, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 100),
        Action::Ignore
    );
    assert!(
        srv.board.try_read(7).is_none(),
        "rejected input publishes nothing"
    );
}

#[tokio::test]
async fn valid_handshake_authenticates_and_binds_wallet() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (bytes, wallet, now_ms) = signed_handshake_request().await;

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Authenticated {
            wallet: w,
            session,
            duplicate_of,
            initial_state,
            features,
        } => {
            assert_ne!(
                session, w,
                "a delegated chain names its ephemeral as the session"
            );
            assert_eq!(w, wallet);
            assert_eq!(duplicate_of, None);
            assert_eq!(features, 0, "nothing offered negotiates the baseline");
            assert!(
                initial_state.is_none(),
                "no initial state in this handshake"
            );
        }
        other => panic!("expected Authenticated, got {other:?}"),
    }

    srv.peers.get_mut(&1).unwrap().wallet_id = Some(wallet.clone());
    srv.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::Authenticated;
    srv.identity.set(1, wallet.clone());
    srv.board.set_active(1);
    assert!(srv.is_authenticated(1));
    assert_eq!(srv.identity.peer_by_wallet(&wallet), Some(1));
}

#[tokio::test]
async fn handshake_negotiates_features_masking_unknown_bits() {
    use crate::server::{FEATURE_DELTA_BATCH, SERVER_FEATURES};

    async fn negotiate(offered: u32) -> u32 {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, _wallet, now_ms) = signed_handshake_request().await;
        let mut msg = ClientMessage::decode(&base[..]).unwrap();
        if let Some(client_message::Message::Handshake(h)) = msg.message.as_mut() {
            h.protocol_features = offered;
        }
        match srv.dispatch(1, channel::RELIABLE, &msg.encode_to_vec(), now_ms, 0) {
            Action::Authenticated { features, .. } => features,
            other => panic!("expected Authenticated, got {other:?}"),
        }
    }

    assert_eq!(negotiate(FEATURE_DELTA_BATCH).await, FEATURE_DELTA_BATCH);
    assert_eq!(
        negotiate(crate::server::FEATURE_DELTA_BATCH_BASELINE).await,
        crate::server::FEATURE_DELTA_BATCH_BASELINE
    );
    assert_eq!(
        negotiate(crate::server::FEATURE_DELTA_BATCH_DICTIONARY).await,
        crate::server::FEATURE_DELTA_BATCH_DICTIONARY
    );
    assert_eq!(negotiate(0).await, 0);
    assert_eq!(negotiate(0xFFFF_FFFF).await, SERVER_FEATURES);
    assert_eq!(
        negotiate(FEATURE_DELTA_BATCH | 1 << 31).await,
        FEATURE_DELTA_BATCH,
        "unknown bits are masked off, the peer still authenticates"
    );
    let Some(server_message::Message::Handshake(resp)) = handshake_response(true, None).message
    else {
        panic!("handshake_response must wrap a HandshakeResponse");
    };
    assert_eq!(resp.protocol_features, SERVER_FEATURES);
}

#[tokio::test]
async fn sample_ticks_are_granted_only_beside_an_arm_11_codec() {
    async fn negotiate(offered: u32) -> u32 {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, _wallet, now_ms) = signed_handshake_request().await;
        let mut msg = ClientMessage::decode(&base[..]).unwrap();
        if let Some(client_message::Message::Handshake(h)) = msg.message.as_mut() {
            h.protocol_features = offered;
        }
        match srv.dispatch(1, channel::RELIABLE, &msg.encode_to_vec(), now_ms, 0) {
            Action::Authenticated { features, .. } => features,
            other => panic!("expected Authenticated, got {other:?}"),
        }
    }

    assert_eq!(FEATURE_DELTA_BATCH_SAMPLE_TICK, 1 << 4);
    assert_eq!(negotiate(FEATURE_DELTA_BATCH_SAMPLE_TICK).await, 0);
    assert_eq!(
        negotiate(FEATURE_DELTA_BATCH | FEATURE_DELTA_BATCH_SAMPLE_TICK).await,
        FEATURE_DELTA_BATCH,
        "arm 10 has no sample ticks to carry"
    );
    for codec in [FEATURE_DELTA_BATCH_BASELINE, FEATURE_DELTA_BATCH_DICTIONARY] {
        assert_eq!(
            negotiate(codec | FEATURE_DELTA_BATCH_SAMPLE_TICK).await,
            codec | FEATURE_DELTA_BATCH_SAMPLE_TICK
        );
        assert_eq!(negotiate(codec).await, codec);
    }
}

#[test]
fn v4_sample_ticks_are_granted_only_beside_an_arm_11_codec() {
    fn admit(capabilities: &[&str]) -> (Vec<String>, u32) {
        let mut srv = PulseServer::new();
        enable_v4(&mut srv);
        let hello = PulseV4Hello {
            request_id: vec![1; 16],
            session_id: vec![2; 16],
            connection_epoch: 1,
            optional_capabilities: capabilities.iter().map(|cap| cap.to_string()).collect(),
            role: PulseV4Role::Player as i32,
            ..Default::default()
        };
        srv.peers
            .insert(30, PeerState::new(PeerConnectionState::PendingAuth, 0));
        srv.v4.connected(30).unwrap();
        let challenge = v4_challenge(srv.dispatch(
            30,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Hello(hello.clone())),
            1_000,
            0,
        ));
        match srv.dispatch(
            30,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Auth(v4_auth(&hello, &challenge))),
            1_001,
            0,
        ) {
            Action::AuthenticatedV4 {
                features, result, ..
            } => (result.negotiated_capabilities, features),
            other => panic!("expected v4 player admission, got {other:?}"),
        }
    }

    assert_eq!(
        admit(&[
            crate::v4::CAPABILITY_DELTA_BATCH,
            crate::v4::CAPABILITY_DELTA_BATCH_SAMPLE_TICK
        ]),
        (
            vec![crate::v4::CAPABILITY_DELTA_BATCH.to_string()],
            FEATURE_DELTA_BATCH
        )
    );
    assert_eq!(
        admit(&[
            crate::v4::CAPABILITY_DELTA_BATCH_DICTIONARY,
            crate::v4::CAPABILITY_DELTA_BATCH_SAMPLE_TICK
        ]),
        (
            vec![
                crate::v4::CAPABILITY_DELTA_BATCH_DICTIONARY.to_string(),
                crate::v4::CAPABILITY_DELTA_BATCH_SAMPLE_TICK.to_string()
            ],
            FEATURE_DELTA_BATCH_DICTIONARY | FEATURE_DELTA_BATCH_SAMPLE_TICK
        )
    );
}

#[tokio::test]
async fn tampered_handshake_replies_with_failure() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));

    let (bytes, _wallet, now_ms) = signed_handshake_request().await;
    let mut req = ClientMessage::decode(&bytes[..]).unwrap();
    if let Some(client_message::Message::Handshake(h)) = req.message.as_mut() {
        let mut bag: serde_json::Value = serde_json::from_slice(&h.auth_chain).unwrap();
        let link_json = bag["x-identity-auth-chain-2"].as_str().unwrap();
        let mut link: serde_json::Value = serde_json::from_str(link_json).unwrap();
        link["payload"] = serde_json::Value::String("connect:/:1700000000000:tampered".into());
        bag["x-identity-auth-chain-2"] =
            serde_json::Value::String(serde_json::to_string(&link).unwrap());
        h.auth_chain = serde_json::to_vec(&bag).unwrap();
    }
    let tampered = req.encode_to_vec();

    match srv.dispatch(2, channel::RELIABLE, &tampered, now_ms, 0) {
        Action::Reply(ServerMessage {
            message: Some(server_message::Message::Handshake(h)),
        }) => {
            assert!(!h.success, "tampered chain must not succeed");
            assert!(h.error.is_some());
        }
        other => panic!("expected failure Reply, got {other:?}"),
    }
    assert!(!srv.is_authenticated(2));
}

#[tokio::test]
async fn banned_wallet_is_rejected_at_handshake() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (bytes, wallet, now_ms) = signed_handshake_request().await;

    srv.ban_list.replace([wallet.clone()]);

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::Banned);

            match reply.and_then(|m| m.message) {
                Some(server_message::Message::Handshake(h)) => {
                    assert!(!h.success);
                    assert_eq!(h.error.as_deref(), Some("banned"));
                }
                other => panic!("expected banned handshake reply, got {other:?}"),
            }
        }
        other => panic!("expected Reject(Banned), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));

    assert_eq!(
        srv.peers[&1].connection_state,
        PeerConnectionState::PendingDisconnect
    );
}

#[tokio::test]
async fn replayed_handshake_pair_is_rejected() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.peers
        .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (bytes, wallet, now_ms) = signed_handshake_request().await;

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 100) {
        Action::Authenticated { wallet: w, .. } => assert_eq!(w, wallet),
        other => panic!("first handshake should authenticate, got {other:?}"),
    }

    match srv.dispatch(2, channel::RELIABLE, &bytes, now_ms, 200) {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::HandshakeReplayRejected);
            assert!(
                reply.is_none(),
                "replay rejection has no reply body (PeerDefense)"
            );
        }
        other => panic!("expected Reject(HandshakeReplayRejected), got {other:?}"),
    }
    assert!(!srv.is_authenticated(2));
}

#[tokio::test]
async fn handshake_attempts_are_throttled() {
    let mut srv = PulseServer::new();

    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let bad = client_msg(client_message::Message::Handshake(
        crate::decentraland::pulse::HandshakeRequest {
            auth_chain: b"not json".to_vec(),
            profile_version: 0,
            initial_state: None,
            protocol_features: 0,
        },
    ));

    for _ in 0..2 {
        assert!(matches!(
            srv.dispatch(1, channel::RELIABLE, &bad, 1000, 0),
            Action::Reply(_)
        ));
    }
    assert_eq!(srv.peers[&1].handshake_attempts, 2);

    match srv.dispatch(1, channel::RELIABLE, &bad, 1000, 0) {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::AuthFailed);
            assert!(reply.is_none());
        }
        other => panic!("expected Reject(AuthFailed) on throttle, got {other:?}"),
    }
    assert_eq!(
        srv.peers[&1].connection_state,
        PeerConnectionState::PendingDisconnect
    );
}

#[tokio::test]
async fn duplicate_committed_handshake_replies_without_reapplying_admission() {
    let mut server = PulseServer::with_config(4, 2, &[50], false);
    server
        .peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (bytes, wallet, now_ms) = signed_handshake_request().await;
    let session = match server.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Authenticated { session, .. } => session,
        other => panic!("expected admission: {other:?}"),
    };
    server.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::Authenticated;
    server.peers.get_mut(&1).unwrap().wallet_id = Some(wallet.clone());
    server.identity.set_with_session(1, wallet.clone(), session);
    for delta in [7_000, 31_000, 120_001] {
        assert_eq!(
            server.dispatch(1, channel::RELIABLE, &bytes, now_ms + delta, delta as u32),
            Action::Reply(handshake_response(true, None))
        );
    }
    assert_eq!(server.peers[&1].handshake_attempts, 1);
    assert_eq!(server.identity.peer_by_wallet(&wallet), Some(1));
    let mut different = ClientMessage::decode(bytes.as_slice()).unwrap();
    if let Some(client_message::Message::Handshake(request)) = different.message.as_mut() {
        request.profile_version += 1;
    }
    assert_eq!(
        server.dispatch(1, channel::RELIABLE, &different.encode_to_vec(), now_ms, 0),
        Action::Ignore
    );
    server.peers.get_mut(&1).unwrap().connection_state = PeerConnectionState::PendingDisconnect;
    assert_eq!(
        server.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0),
        Action::Ignore
    );
}

#[tokio::test]
async fn future_dated_signature_is_consumed_before_reply_and_across_restart() {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "catalyrst-pulse-server-future-restart-{}-{id}.tsv",
        std::process::id()
    ));
    let (bytes, _, timestamp) = signed_handshake_request().await;
    let earliest = timestamp - crate::handshake::MAX_TIMESTAMP_SKEW_MS;

    {
        let mut server = PulseServer::with_config(4, 2, &[50], false);
        server.replay_policy = HandshakeReplayPolicy::durable(
            true,
            crate::handshake::MAX_TIMESTAMP_SKEW_MS,
            4,
            &path,
            earliest,
        )
        .unwrap();
        server
            .peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        assert!(matches!(
            server.dispatch(1, channel::RELIABLE, &bytes, earliest, 0),
            Action::Reply(ServerMessage {
                message: Some(server_message::Message::Handshake(HandshakeResponse {
                    success: false,
                    ..
                }))
            })
        ));
    }

    let mut restarted = PulseServer::with_config(4, 2, &[50], false);
    restarted.replay_policy = HandshakeReplayPolicy::durable(
        true,
        crate::handshake::MAX_TIMESTAMP_SKEW_MS,
        4,
        &path,
        earliest + 1,
    )
    .unwrap();
    restarted.replay_policy.forget_before(earliest + 1);
    restarted
        .peers
        .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));
    assert!(matches!(
        restarted.dispatch(2, channel::RELIABLE, &bytes, timestamp, 0),
        Action::Reject {
            reason: DisconnectReason::HandshakeReplayRejected,
            ..
        }
    ));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("replay-journal.tmp"));
}

#[tokio::test]
async fn a_stepped_back_wall_clock_cannot_reopen_a_used_handshake() {
    let (bytes, _, timestamp) = signed_handshake_request().await;
    let skew = crate::handshake::MAX_TIMESTAMP_SKEW_MS;
    let mut server = PulseServer::with_config(4, 2, &[50], false);
    for peer in [1, 2] {
        server
            .peers
            .insert(peer, PeerState::new(PeerConnectionState::PendingAuth, 0));
    }
    assert!(matches!(
        server.dispatch(1, channel::RELIABLE, &bytes, timestamp + skew, 0),
        Action::Authenticated { .. }
    ));
    let elapsed = (2 * skew + 2) as u32;
    assert!(
        !matches!(
            server.dispatch(2, channel::RELIABLE, &bytes, timestamp, elapsed),
            Action::Authenticated { .. }
        ),
        "a used handshake was admitted again after the wall clock stepped back"
    );
    assert!(!server.is_authenticated(2));
}

#[tokio::test]
async fn a_handshake_signed_before_this_process_started_is_refused_with_a_retryable_reply() {
    let (bytes, _, timestamp) = signed_handshake_request().await;
    for (started_at, admitted) in [(timestamp + 1, false), (timestamp, true)] {
        let mut server = PulseServer::with_config(4, 2, &[50], false);
        server.replay_policy.forget_before(started_at);
        server
            .peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        match server.dispatch(1, channel::RELIABLE, &bytes, timestamp + 1_000, 0) {
            Action::Authenticated { .. } => assert!(admitted),
            Action::Reply(message) => {
                assert!(!admitted);
                match message.message {
                    Some(server_message::Message::Handshake(h)) => assert!(!h.success),
                    other => panic!("expected a failed handshake reply, got {other:?}"),
                }
                assert_eq!(
                    server.peers[&1].connection_state,
                    PeerConnectionState::PendingAuth,
                    "the peer must be able to sign a new handshake"
                );
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }
}

#[tokio::test]
async fn replay_memory_exhaustion_is_transient_capacity_without_eviction() {
    let (bytes, _, timestamp) = signed_handshake_request().await;
    let mut server = PulseServer::with_config(4, 2, &[50], false);
    server.replay_policy =
        HandshakeReplayPolicy::new(true, crate::handshake::MAX_TIMESTAMP_SKEW_MS, 1);
    let other = timestamp.to_string();
    assert_eq!(
        server
            .replay_policy
            .try_admit(timestamp, "other-wallet", &other),
        Ok(())
    );
    server
        .peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    assert!(matches!(
        server.dispatch(1, channel::RELIABLE, &bytes, timestamp, 1),
        Action::Reject {
            reason: DisconnectReason::ServerFull,
            ..
        }
    ));
    assert_eq!(
        server
            .replay_policy
            .try_admit(timestamp + 2, "other-wallet", &other),
        Err(ReplayRejection::Duplicate)
    );
}

#[test]
fn oversized_emote_id_is_rejected() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 7, "0xabc");
    let huge = "u".repeat(srv.max_emote_id_length + 1);
    let bytes = client_msg(client_message::Message::EmoteStart(
        crate::decentraland::pulse::EmoteStart {
            emote_id: huge,
            duration_ms: None,
            player_state: Some(valid_state(3)),
            mask: None,
        },
    ));
    match srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 0) {
        Action::Reject { reason, .. } => {
            assert_eq!(reason, DisconnectReason::InvalidEmoteField)
        }
        other => panic!("expected Reject(InvalidEmoteField), got {other:?}"),
    }
    assert!(
        srv.board.try_read(7).is_none(),
        "rejected emote publishes nothing"
    );
}

#[test]
fn excessive_emote_duration_is_rejected() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 7, "0xabc");
    let bytes = client_msg(client_message::Message::EmoteStart(
        crate::decentraland::pulse::EmoteStart {
            emote_id: "wave".into(),
            duration_ms: Some(srv.max_emote_duration_ms + 1),
            player_state: Some(valid_state(3)),
            mask: None,
        },
    ));
    match srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 0) {
        Action::Reject { reason, .. } => {
            assert_eq!(reason, DisconnectReason::InvalidEmoteField)
        }
        other => panic!("expected Reject(InvalidEmoteField), got {other:?}"),
    }
}

#[test]
fn emote_within_caps_is_applied() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 7, "0xabc");
    let bytes = client_msg(client_message::Message::EmoteStart(
        crate::decentraland::pulse::EmoteStart {
            emote_id: "urn:decentraland:off-chain:base-emotes:wave".into(),
            duration_ms: Some(2000),
            player_state: Some(valid_state(3)),
            mask: None,
        },
    ));
    assert_eq!(
        srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 50),
        Action::Applied
    );
    assert!(srv.board.try_read(7).unwrap().is_emoting());
}

#[test]
fn a_one_shot_emote_completes_server_side() {
    let mut srv = PulseServer::new();
    let enter = |srv: &mut PulseServer, peer: u32, wallet: &str, now: u32| {
        authed(srv, peer, wallet);
        let bytes = client_msg(client_message::Message::Teleport(teleport_request(
            0, "realm-a",
        )));
        assert_eq!(
            srv.dispatch(peer, channel::RELIABLE, &bytes, 0, now),
            Action::Applied
        );
    };
    enter(&mut srv, 0, "0xobserver", 900);
    enter(&mut srv, 1, "0xsubject", 900);
    srv.simulate(950);

    let bytes = client_msg(client_message::Message::EmoteStart(
        crate::decentraland::pulse::EmoteStart {
            emote_id: "wave".into(),
            duration_ms: Some(2000),
            player_state: Some(valid_state(0)),
            mask: None,
        },
    ));
    assert_eq!(
        srv.dispatch(1, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 1000),
        Action::Applied
    );

    let started = |out: &[OutgoingMessage], target: u32| {
        out.iter()
            .filter(|m| {
                m.target == target
                    && matches!(&m.message.message,
                        Some(server_message::Message::EmoteStarted(e)) if e.subject_id == 1)
            })
            .count()
    };
    let stopped = |out: &[OutgoingMessage], target: u32| -> Vec<i32> {
        out.iter()
            .filter(|m| m.target == target)
            .filter_map(|m| match &m.message.message {
                Some(server_message::Message::EmoteStopped(e)) if e.subject_id == 1 => {
                    Some(e.reason)
                }
                _ => None,
            })
            .collect()
    };

    assert_eq!(started(&srv.simulate(1050), 0), 1);
    assert!(
        stopped(&srv.simulate(2999), 0).is_empty(),
        "one millisecond short of its duration the emote still plays"
    );

    let mut reasons = Vec::new();
    for now in [3000, 3050, 3100] {
        reasons.extend(stopped(&srv.simulate(now), 0));
    }
    assert_eq!(
        reasons,
        vec![crate::decentraland::pulse::EmoteStopReason::Completed as i32],
        "an elapsed one-shot emote is stopped once, as completed"
    );
    assert!(!srv.board.is_emoting(1));

    enter(&mut srv, 2, "0xlate", 3120);
    let out = srv.simulate(3150);
    assert!(
        out.iter().any(|m| m.target == 2
            && matches!(&m.message.message,
                Some(server_message::Message::PlayerJoined(j)) if j.user_id == "0xsubject")),
        "the late observer does see the subject"
    );
    assert_eq!(
        started(&out, 2),
        0,
        "a late observer is not shown an emote that already finished"
    );
}

#[test]
fn a_looping_emote_is_never_completed_server_side() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 1, "0xsubject");
    let bytes = client_msg(client_message::Message::EmoteStart(
        crate::decentraland::pulse::EmoteStart {
            emote_id: "dance".into(),
            duration_ms: None,
            player_state: Some(valid_state(0)),
            mask: None,
        },
    ));
    assert_eq!(
        srv.dispatch(1, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 1000),
        Action::Applied
    );
    srv.simulate(u32::MAX);
    assert!(srv.board.is_emoting(1), "no duration means no server stop");
}

#[test]
fn emote_start_mask_is_published_and_absence_stays_absent() {
    let masked = |mask: Option<i32>| {
        let mut srv = PulseServer::new();
        authed(&mut srv, 7, "0xabc");
        let bytes = client_msg(client_message::Message::EmoteStart(
            crate::decentraland::pulse::EmoteStart {
                emote_id: "wave".into(),
                duration_ms: None,
                player_state: Some(valid_state(3)),
                mask,
            },
        ));
        assert_eq!(
            srv.dispatch(7, channel::UNRELIABLE_UNSEQUENCED, &bytes, 0, 50),
            Action::Applied
        );
        srv.board.try_read(7).unwrap().emote.clone().unwrap().mask
    };
    assert_eq!(masked(Some(5)), Some(5), "the wire mask reaches the ledger");
    assert_eq!(
        masked(None),
        None,
        "no mask on the wire means no mask stored"
    );
}

#[test]
fn handshake_initial_state_carries_the_emote_mask() {
    let seeded = |emote_mask: Option<i32>| {
        let mut srv = PulseServer::new();
        srv.board.set_active(1);
        let init = PlayerInitialState {
            state: Some(valid_state(7)),
            emote_id: Some("wave".into()),
            realm: "realm-a".into(),
            emote_mask,
            ..Default::default()
        };
        srv.seed_initial_state(1, 500, &init);
        srv.board.try_read(1).unwrap().emote.clone().unwrap().mask
    };
    assert_eq!(seeded(Some(5)), Some(5), "the seeded emote keeps its mask");
    assert_eq!(seeded(None), None, "a maskless seed stays maskless");
}

#[test]
fn handshake_initial_state_lands_the_peer_in_its_realm() {
    let mut srv = PulseServer::new();
    srv.board.set_active(1);
    let init = PlayerInitialState {
        state: Some(valid_state(7)),
        realm: "realm-a".into(),
        ..Default::default()
    };
    srv.seed_initial_state(1, 500, &init);
    assert_eq!(
        srv.board.try_read(1).unwrap().realm.as_deref(),
        Some("realm-a"),
        "the seeded snapshot carries the asserted realm"
    );
    assert_eq!(
        srv.grids.realm_of(1),
        Some("realm-a"),
        "so the peer is visible without a follow-up teleport"
    );
}

#[tokio::test]
async fn handshake_initial_state_rejects_an_empty_realm() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _wallet, now_ms) = signed_handshake_request().await;

    let init = PlayerInitialState {
        state: Some(valid_state(7)),
        ..Default::default()
    };
    let bytes = with_initial_state(&base, Some(init));

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject { reason, .. } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));
}

#[tokio::test]
async fn handshake_initial_state_emote_cap_is_enforced() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _wallet, now_ms) = signed_handshake_request().await;

    let init = PlayerInitialState {
        state: Some(valid_state(7)),
        emote_id: Some("u".repeat(srv.max_emote_id_length + 1)),
        realm: "realm-a".into(),
        ..Default::default()
    };
    let bytes = with_initial_state(&base, Some(init));

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject { reason, .. } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));
}

#[tokio::test]
async fn handshake_initial_state_rejects_overlong_realm() {
    let (base, _wallet, now_ms) = signed_handshake_request().await;
    let init_with_realm = |realm: &str| {
        with_initial_state(
            &base,
            Some(PlayerInitialState {
                state: Some(valid_state(7)),
                realm: realm.into(),
                ..Default::default()
            }),
        )
    };

    let mut srv = PulseServer::new();
    srv.max_realm_length = 4;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    match srv.dispatch(1, channel::RELIABLE, &init_with_realm("abcd"), now_ms, 0) {
        Action::Authenticated { .. } => {}
        other => panic!("expected Authenticated, got {other:?}"),
    }

    let mut srv = PulseServer::new();
    srv.max_realm_length = 4;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    match srv.dispatch(1, channel::RELIABLE, &init_with_realm("abcde"), now_ms, 0) {
        Action::Reject { reason, .. } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));
}

#[test]
fn wt_flood_at_budget_does_not_starve_enet_admission() {
    use crate::hardening::AdmitResult;
    let mut srv = PulseServer::new();
    let wt_base = ENET_CAPACITY as u32;

    for i in 0..DEFAULT_PRE_AUTH_BUDGET_WT as u32 {
        let peer = wt_base + i;
        let ip = format!("10.{}.{}.1", i / 256, i % 256);
        assert_eq!(srv.pre_auth_for(peer).try_admit(peer, &ip), AdmitResult::Ok);
    }
    assert_eq!(srv.pre_auth_wt.in_flight(), DEFAULT_PRE_AUTH_BUDGET_WT);

    let extra = wt_base + DEFAULT_PRE_AUTH_BUDGET_WT as u32;
    assert_eq!(
        srv.pre_auth_for(extra).try_admit(extra, "192.0.2.254"),
        AdmitResult::BudgetExhausted
    );

    assert_eq!(srv.pre_auth_for(7).try_admit(7, "1.2.3.4"), AdmitResult::Ok);
    assert_eq!(srv.pre_auth_enet.in_flight(), 1);
}

#[test]
fn pre_auth_release_credits_the_admitting_transport() {
    use crate::hardening::AdmitResult;
    let mut srv = PulseServer::new();
    let enet = 3u32;
    let wt = ENET_CAPACITY as u32 + 1;

    assert_eq!(
        srv.pre_auth_for(enet).try_admit(enet, "1.1.1.1"),
        AdmitResult::Ok
    );
    assert_eq!(
        srv.pre_auth_for(wt).try_admit(wt, "2.2.2.2"),
        AdmitResult::Ok
    );
    assert_eq!(srv.pre_auth_enet.in_flight(), 1);
    assert_eq!(srv.pre_auth_wt.in_flight(), 1);

    srv.pre_auth_for(wt).release_on_disconnect(wt);
    assert_eq!(srv.pre_auth_wt.in_flight(), 0);
    assert_eq!(
        srv.pre_auth_enet.in_flight(),
        1,
        "enet budget untouched by a WT release"
    );

    srv.pre_auth_for(enet).release_on_promotion(enet);
    assert_eq!(srv.pre_auth_enet.in_flight(), 0);
}

fn enable_v4(srv: &mut PulseServer) {
    srv.v4
        .enable(crate::v4::PulseV4Config {
            audience: "https://realm.example".into(),
            issuer: "replica-a".into(),
            challenge_ttl_ms: crate::v4::DEFAULT_CHALLENGE_TTL_MS,
            max_pending: 16,
            max_auth_chain_bytes: crate::v4::DEFAULT_MAX_AUTH_CHAIN_BYTES,
            max_capabilities: crate::v4::DEFAULT_MAX_CAPABILITIES,
            max_public_detail_bytes: crate::v4::DEFAULT_MAX_PUBLIC_DETAIL_BYTES,
            max_frame_bytes: crate::v4::DEFAULT_MAX_FRAME_BYTES,
        })
        .unwrap();
}

fn v4_challenge(action: Action) -> PulseV4Challenge {
    match action {
        Action::Reply(ServerMessage {
            message: Some(server_message::Message::V4Challenge(challenge)),
        }) => challenge,
        other => panic!("expected Pulse v4 challenge, got {other:?}"),
    }
}

fn v4_auth(hello: &PulseV4Hello, challenge: &PulseV4Challenge) -> PulseV4Auth {
    use catalyrst_crypto::Wallet;
    use catalyrst_types::{AuthLink, AuthLinkType};
    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
    let wallet = Wallet::from_hex(KEY).unwrap();
    let mut auth = PulseV4Auth {
        request_id: hello.request_id.clone(),
        session_id: hello.session_id.clone(),
        connection_epoch: hello.connection_epoch,
        challenge_id: challenge.challenge_id.clone(),
        auth_chain: None,
        idempotency_key: vec![8; 16],
        wallet: wallet.address(),
        auth_session: wallet.address(),
    };
    let payload = crate::v4::signing_payload(&crate::v4::transcript(hello, challenge, &auth));
    auth.auth_chain = Some(ProtoAuthChain {
        links: vec![
            AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: wallet.address(),
                signature: None,
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaSignedEntity,
                payload: payload.clone(),
                signature: Some(wallet.sign_message(payload.as_bytes()).unwrap()),
            },
        ]
        .into_iter()
        .map(|link| ProtoAuthLink {
            r#type: match link.link_type {
                AuthLinkType::SIGNER => ProtoAuthLinkType::Signer,
                AuthLinkType::EcdsaEphemeral => ProtoAuthLinkType::EcdsaEphemeral,
                AuthLinkType::EcdsaSignedEntity => ProtoAuthLinkType::EcdsaSignedEntity,
                AuthLinkType::EcdsaEip1654Ephemeral => ProtoAuthLinkType::EcdsaEip1654Ephemeral,
                AuthLinkType::EcdsaEip1654SignedEntity => {
                    ProtoAuthLinkType::EcdsaEip1654SignedEntity
                }
            } as i32,
            payload: link.payload,
            signature: link.signature,
        })
        .collect(),
    });
    auth
}

#[test]
fn v4_player_and_listener_intents_reach_distinct_admission_actions() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);

    let player = PulseV4Hello {
        request_id: vec![1; 16],
        session_id: vec![2; 16],
        connection_epoch: 1,
        optional_capabilities: vec![crate::v4::CAPABILITY_DELTA_BATCH.into()],
        role: PulseV4Role::Player as i32,
        ..Default::default()
    };
    srv.peers
        .insert(30, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(30).unwrap();
    let challenge = v4_challenge(srv.dispatch(
        30,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(player.clone())),
        1_000,
        0,
    ));
    match srv.dispatch(
        30,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Auth(v4_auth(
            &player, &challenge,
        ))),
        1_001,
        0,
    ) {
        Action::AuthenticatedV4 { features, .. } => assert_eq!(features, FEATURE_DELTA_BATCH),
        other => panic!("expected v4 player admission, got {other:?}"),
    }

    let listener = PulseV4Hello {
        request_id: vec![3; 16],
        session_id: vec![4; 16],
        connection_epoch: 2,
        role: PulseV4Role::Listener as i32,
        listener_aoi: vec![SceneListenerAoi {
            realm: "realm-a".into(),
            parcel_rects: vec![crate::decentraland::pulse::ParcelRect {
                min_x: 0,
                min_z: 0,
                max_x: 0,
                max_z: 0,
            }],
        }],
        ..Default::default()
    };
    srv.peers
        .insert(31, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(31).unwrap();
    let challenge = v4_challenge(srv.dispatch(
        31,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(listener.clone())),
        1_000,
        0,
    ));
    match srv.dispatch(
        31,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Auth(v4_auth(
            &listener, &challenge,
        ))),
        1_001,
        0,
    ) {
        Action::AuthenticatedListenerV4 { listener, .. } => {
            assert_eq!(listener.realm_count(), 1);
            assert_eq!(listener.parcel_count(), 1);
        }
        other => panic!("expected v4 listener admission, got {other:?}"),
    }
}

#[tokio::test]
async fn enabled_v4_keeps_legacy_handshake_behavior() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    srv.peers
        .insert(40, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (legacy, _, now_ms) = signed_handshake_request().await;
    assert!(matches!(
        srv.dispatch(40, channel::RELIABLE, &legacy, now_ms, 0),
        Action::Authenticated { .. }
    ));
}

#[tokio::test]
async fn v4_opening_rejects_a_valid_legacy_handshake_on_the_same_peer() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    srv.peers
        .insert(40, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(40).unwrap();
    let (legacy, _, now_ms) = signed_handshake_request().await;
    let hello = PulseV4Hello {
        request_id: vec![1; 16],
        session_id: vec![2; 16],
        connection_epoch: 1,
        ..Default::default()
    };
    v4_challenge(srv.dispatch(
        40,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(hello)),
        now_ms,
        0,
    ));

    assert_eq!(
        srv.dispatch(40, channel::RELIABLE, &legacy, now_ms, 0),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::InvalidHandshakeField,
        }
    );
    assert_eq!(
        srv.peers[&40].handshake_protocol,
        Some(HandshakeProtocol::V4)
    );
    assert_eq!(
        srv.peers[&40].connection_state,
        PeerConnectionState::PendingDisconnect
    );
    assert!(srv.peers[&40].wallet_id.is_none());
}

#[test]
fn failed_legacy_opening_cannot_switch_to_v4_on_the_same_peer() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    srv.peers
        .insert(41, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(41).unwrap();
    let legacy = client_msg(client_message::Message::Handshake(
        crate::decentraland::pulse::HandshakeRequest::default(),
    ));
    assert!(matches!(
        srv.dispatch(41, channel::RELIABLE, &legacy, 1_000, 0),
        Action::Reply(ServerMessage {
            message: Some(server_message::Message::Handshake(HandshakeResponse {
                success: false,
                ..
            })),
        })
    ));
    let hello = PulseV4Hello {
        request_id: vec![1; 16],
        session_id: vec![2; 16],
        connection_epoch: 1,
        ..Default::default()
    };
    assert_eq!(
        srv.dispatch(
            41,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Hello(hello)),
            1_000,
            0,
        ),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::InvalidHandshakeField,
        }
    );
    assert_eq!(
        srv.peers[&41].handshake_protocol,
        Some(HandshakeProtocol::Legacy)
    );
    assert_eq!(
        srv.peers[&41].connection_state,
        PeerConnectionState::PendingDisconnect
    );
}

#[tokio::test]
async fn fresh_peers_select_legacy_and_v4_independently_on_one_server() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    for peer in [42, 43] {
        srv.peers
            .insert(peer, PeerState::new(PeerConnectionState::PendingAuth, 0));
        srv.v4.connected(peer).unwrap();
    }
    let (legacy, _, now_ms) = signed_handshake_request().await;
    assert!(matches!(
        srv.dispatch(42, channel::RELIABLE, &legacy, now_ms, 0),
        Action::Authenticated { .. }
    ));
    let hello = PulseV4Hello {
        request_id: vec![1; 16],
        session_id: vec![2; 16],
        connection_epoch: 1,
        ..Default::default()
    };
    let challenge = v4_challenge(srv.dispatch(
        43,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(hello.clone())),
        now_ms,
        0,
    ));
    assert!(matches!(
        srv.dispatch(
            43,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Auth(v4_auth(&hello, &challenge))),
            now_ms,
            0,
        ),
        Action::AuthenticatedV4 { .. }
    ));
    assert_eq!(
        srv.peers[&42].handshake_protocol,
        Some(HandshakeProtocol::Legacy)
    );
    assert_eq!(
        srv.peers[&43].handshake_protocol,
        Some(HandshakeProtocol::V4)
    );
}

#[test]
fn oversized_v4_frame_is_rejected_before_message_decode() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    let bytes = client_msg(client_message::Message::V4Auth(PulseV4Auth {
        auth_chain: Some(ProtoAuthChain {
            links: vec![ProtoAuthLink {
                r#type: ProtoAuthLinkType::Signer as i32,
                payload: "x".repeat(crate::v4::DEFAULT_MAX_FRAME_BYTES as usize),
                signature: None,
            }],
        }),
        ..Default::default()
    }));
    assert_eq!(
        srv.dispatch(50, channel::RELIABLE, &bytes, 0, 0),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::InvalidHandshakeField,
        }
    );
}

fn admit_v4_player(srv: &mut PulseServer, peer: u32, session_id: u8, epoch: u64) -> Action {
    let hello = PulseV4Hello {
        request_id: vec![peer as u8; 16],
        session_id: vec![session_id; 16],
        connection_epoch: epoch,
        role: PulseV4Role::Player as i32,
        ..Default::default()
    };
    srv.peers
        .insert(peer, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(peer).unwrap();
    let challenge = v4_challenge(srv.dispatch(
        peer,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(hello.clone())),
        1_000,
        0,
    ));
    let action = srv.dispatch(
        peer,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Auth(v4_auth(&hello, &challenge))),
        1_001,
        0,
    );
    if let Action::AuthenticatedV4 {
        wallet, session, ..
    } = &action
    {
        srv.identity
            .set_with_session(peer, wallet.clone(), session.clone());
        if let Some(state) = srv.peers.get_mut(&peer) {
            state.connection_state = PeerConnectionState::Authenticated;
        }
    }
    action
}

fn v4_duplicate_of(action: Action) -> Option<u32> {
    match action {
        Action::AuthenticatedV4 { duplicate_of, .. } => duplicate_of,
        other => panic!("expected v4 player admission, got {other:?}"),
    }
}

#[test]
fn a_delayed_older_connection_of_a_session_never_evicts_the_newer_one() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    assert_eq!(v4_duplicate_of(admit_v4_player(&mut srv, 40, 2, 2)), None);

    for (peer, epoch) in [(41, 1), (42, 2)] {
        match admit_v4_player(&mut srv, peer, 2, epoch) {
            Action::Reject {
                reply:
                    Some(ServerMessage {
                        message: Some(server_message::Message::V4Result(result)),
                    }),
                reason: DisconnectReason::DuplicateSession,
            } => {
                assert!(!result.success);
                assert_eq!(result.connection_epoch, epoch);
                assert_eq!(
                    result.error_code,
                    PulseV4ErrorCode::PulseV4ErrorExpired as i32
                );
                assert_eq!(result.retry, PulseV4RetryClass::PulseV4RetryNone as i32);
            }
            other => panic!("epoch {epoch} must end without touching the holder, got {other:?}"),
        }
        assert_eq!(
            srv.peers[&peer].connection_state,
            PeerConnectionState::PendingDisconnect
        );
        assert_eq!(
            srv.peers[&40].connection_state,
            PeerConnectionState::Authenticated
        );
    }

    assert_eq!(
        v4_duplicate_of(admit_v4_player(&mut srv, 43, 2, 3)),
        Some(40),
        "the next connection of the session replaces its holder"
    );
    assert_eq!(
        v4_duplicate_of(admit_v4_player(&mut srv, 44, 9, 1)),
        Some(43),
        "another session of the wallet is a new login and epochs do not order it"
    );
}

#[test]
fn a_departed_holder_leaves_no_epoch_behind() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    assert_eq!(v4_duplicate_of(admit_v4_player(&mut srv, 40, 2, 5)), None);
    srv.remove_peer_state(40);

    assert_eq!(v4_duplicate_of(admit_v4_player(&mut srv, 41, 2, 1)), None);
}

#[tokio::test]
async fn a_server_that_refuses_legacy_handshakes_consumes_nothing_and_still_admits_v4() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    srv.legacy_handshake = false;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (bytes, _, now_ms) = signed_handshake_request().await;

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject {
            reply: Some(_),
            reason: DisconnectReason::AuthFailed,
        } => {}
        other => panic!("a legacy handshake must be refused, got {other:?}"),
    }
    assert_eq!(
        srv.peers[&1].connection_state,
        PeerConnectionState::PendingDisconnect
    );

    srv.legacy_handshake = true;
    srv.peers
        .insert(2, PeerState::new(PeerConnectionState::PendingAuth, 0));
    assert!(
        matches!(
            srv.dispatch(2, channel::RELIABLE, &bytes, now_ms, 0),
            Action::Authenticated { .. }
        ),
        "the refused handshake was never consumed by the replay policy"
    );

    srv.legacy_handshake = false;
    assert_eq!(v4_duplicate_of(admit_v4_player(&mut srv, 60, 2, 1)), None);
}

fn open_v4_connection(srv: &mut PulseServer, peer: u32, hello: &PulseV4Hello) -> PulseV4Challenge {
    srv.peers
        .insert(peer, PeerState::new(PeerConnectionState::PendingAuth, 0));
    srv.v4.connected(peer).unwrap();
    v4_challenge(srv.dispatch(
        peer,
        channel::RELIABLE,
        &client_msg(client_message::Message::V4Hello(hello.clone())),
        1_000,
        0,
    ))
}

type SignedField<T> = (&'static str, fn(&mut T));

fn v4_refusal_code(action: Action) -> i32 {
    match action {
        Action::Reply(ServerMessage {
            message: Some(server_message::Message::V4Result(result)),
        }) if !result.success => result.error_code,
        other => panic!("expected a refused Pulse v4 result, got {other:?}"),
    }
}

#[test]
fn everything_that_names_the_server_the_connection_or_the_session_is_signed() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    let hello = PulseV4Hello {
        request_id: vec![7; 16],
        session_id: vec![3; 16],
        connection_epoch: 4,
        role: PulseV4Role::Player as i32,
        ..Default::default()
    };
    let challenge = open_v4_connection(&mut srv, 70, &hello);
    let auth = v4_auth(&hello, &challenge);
    let signed = crate::v4::signing_payload(&crate::v4::transcript(&hello, &challenge, &auth));

    let server_fields: [SignedField<PulseV4Challenge>; 7] = [
        ("audience", |c| c.audience.push('x')),
        ("issuer", |c| c.issuer.push('x')),
        ("process incarnation", |c| c.process_incarnation[0] ^= 1),
        ("peer binding", |c| c.peer_binding[0] ^= 1),
        ("challenge id", |c| c.challenge_id[0] ^= 1),
        ("challenge", |c| c.challenge[0] ^= 1),
        ("expiry", |c| c.expires_at_ms += 1),
    ];
    for (field, change) in server_fields {
        let mut other = challenge.clone();
        change(&mut other);
        assert_ne!(
            crate::v4::signing_payload(&crate::v4::transcript(&hello, &other, &auth)),
            signed,
            "{field} is outside the signed transcript"
        );
    }

    let session_fields: [SignedField<PulseV4Hello>; 4] = [
        ("request id", |h| h.request_id[0] ^= 1),
        ("session id", |h| h.session_id[0] ^= 1),
        ("connection epoch", |h| h.connection_epoch += 1),
        ("role", |h| h.role = PulseV4Role::Listener as i32),
    ];
    for (field, change) in session_fields {
        let mut other = hello.clone();
        change(&mut other);
        assert_ne!(
            crate::v4::signing_payload(&crate::v4::transcript(&other, &challenge, &auth)),
            signed,
            "{field} is outside the signed transcript"
        );
    }

    let identity_fields: [SignedField<PulseV4Auth>; 3] = [
        ("idempotency key", |a| a.idempotency_key[0] ^= 1),
        ("wallet", |a| a.wallet.push('0')),
        ("auth session", |a| a.auth_session.push('0')),
    ];
    for (field, change) in identity_fields {
        let mut other = auth.clone();
        change(&mut other);
        assert_ne!(
            crate::v4::signing_payload(&crate::v4::transcript(&hello, &challenge, &other)),
            signed,
            "{field} is outside the signed transcript"
        );
    }
}

#[test]
fn an_authentication_captured_on_one_connection_admits_nobody_on_another() {
    let mut srv = PulseServer::new();
    enable_v4(&mut srv);
    let hello = PulseV4Hello {
        request_id: vec![7; 16],
        session_id: vec![3; 16],
        connection_epoch: 4,
        role: PulseV4Role::Player as i32,
        ..Default::default()
    };
    let owned = open_v4_connection(&mut srv, 71, &hello);
    let captured = v4_auth(&hello, &owned);

    let verbatim = open_v4_connection(&mut srv, 72, &hello);
    assert_ne!(verbatim.peer_binding, owned.peer_binding);
    assert_ne!(verbatim.challenge_id, owned.challenge_id);
    assert_eq!(verbatim.process_incarnation, owned.process_incarnation);
    assert_eq!(
        v4_refusal_code(srv.dispatch(
            72,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Auth(captured.clone())),
            1_001,
            0,
        )),
        PulseV4ErrorCode::PulseV4ErrorIntentMismatch as i32
    );

    let rewritten_for = open_v4_connection(&mut srv, 73, &hello);
    let mut rewritten = captured.clone();
    rewritten.challenge_id = rewritten_for.challenge_id.clone();
    assert_eq!(
        v4_refusal_code(srv.dispatch(
            73,
            channel::RELIABLE,
            &client_msg(client_message::Message::V4Auth(rewritten)),
            1_001,
            0,
        )),
        PulseV4ErrorCode::PulseV4ErrorAuthInvalid as i32,
        "only the signature stands between a rewritten envelope and admission"
    );

    assert!(
        matches!(
            srv.dispatch(
                71,
                channel::RELIABLE,
                &client_msg(client_message::Message::V4Auth(captured)),
                1_002,
                0,
            ),
            Action::AuthenticatedV4 { .. }
        ),
        "the connection the bytes were signed for still admits them"
    );
}

mod scene_listener;
