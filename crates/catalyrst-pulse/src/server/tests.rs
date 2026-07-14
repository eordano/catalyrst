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

use crate::decentraland::pulse::{
    client_message, ClientMessage, PlayerInitialState, PlayerState, PlayerStateInput,
    ProfileVersionAnnouncement, ResyncRequest, TeleportRequest,
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
            duplicate_of,
            initial_state,
            features,
        } => {
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
        srv.pre_auth_for(extra).try_admit(extra, "10.255.255.1"),
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

use crate::decentraland::pulse::{ParcelRect, SceneListenerHandshakeRequest};
use crate::interest::SceneListenerState;

fn rect(min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> ParcelRect {
    ParcelRect {
        min_x,
        min_z,
        max_x,
        max_z,
    }
}

fn listener(srv: &mut PulseServer, peer: u32, wallet: &str, realm: &str, parcels: &[i32]) {
    let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
    st.wallet_id = Some(wallet.into());
    st.scene_listener = Some(SceneListenerState {
        realm: realm.into(),
        parcels: parcels.iter().copied().collect(),
    });
    srv.peers.insert(peer, st);
    srv.identity.set(peer, wallet.into());
}

fn scene_listener_msg(base: &[u8], realm: &str, rects: Vec<ParcelRect>, features: u32) -> Vec<u8> {
    let auth_chain = match ClientMessage::decode(base).unwrap().message {
        Some(client_message::Message::Handshake(h)) => h.auth_chain,
        _ => panic!("base must be a player handshake"),
    };
    client_msg(client_message::Message::SceneListenerHandshake(
        SceneListenerHandshakeRequest {
            auth_chain,
            realm: realm.into(),
            parcel_rects: rects,
            protocol_features: features,
        },
    ))
}

#[test]
fn scene_listener_forbidden_messages_are_dropped_and_counted() {
    let mut srv = PulseServer::new();
    listener(&mut srv, 7, "0xabc", "realm-a", &[10]);

    let forbidden: Vec<Vec<u8>> = vec![
        client_msg(client_message::Message::Input(PlayerStateInput {
            state: Some(valid_state(3)),
        })),
        client_msg(client_message::Message::Teleport(teleport_request(
            3, "realm-a",
        ))),
        client_msg(client_message::Message::EmoteStart(
            crate::decentraland::pulse::EmoteStart {
                emote_id: "wave".into(),
                duration_ms: None,
                player_state: Some(valid_state(3)),
                mask: None,
            },
        )),
        client_msg(client_message::Message::EmoteStop(
            crate::decentraland::pulse::EmoteStop {},
        )),
        client_msg(client_message::Message::ProfileAnnouncement(
            ProfileVersionAnnouncement { version: 4 },
        )),
        client_msg(client_message::Message::Handshake(
            crate::decentraland::pulse::HandshakeRequest {
                auth_chain: b"x".to_vec(),
                profile_version: 0,
                initial_state: None,
                protocol_features: 0,
            },
        )),
        scene_listener_msg(
            &client_msg(client_message::Message::Handshake(
                crate::decentraland::pulse::HandshakeRequest {
                    auth_chain: b"x".to_vec(),
                    profile_version: 0,
                    initial_state: None,
                    protocol_features: 0,
                },
            )),
            "realm-a",
            vec![rect(0, 0, 0, 0)],
            0,
        ),
    ];

    let mut expected = 0u64;
    for bytes in &forbidden {
        assert_eq!(
            srv.dispatch(7, channel::RELIABLE, bytes, 0, 0),
            Action::Ignore
        );
        expected += 1;
        assert_eq!(srv.scene_listener_forbidden_drops, expected);
    }

    let resync = client_msg(client_message::Message::Resync(ResyncRequest {
        subject_id: 1,
        known_seq: 0,
    }));
    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, &resync, 0, 0),
        Action::Applied
    );
    assert_eq!(
        srv.scene_listener_forbidden_drops, expected,
        "resync is never a forbidden drop"
    );
    assert!(srv.peers[&7].resync_requests.is_some());
}

#[test]
fn scene_listener_choke_never_gates_players_or_unknown_peers() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 8, "0xplayer");
    let input = client_msg(client_message::Message::Input(PlayerStateInput {
        state: Some(valid_state(3)),
    }));
    assert_eq!(
        srv.dispatch(8, channel::UNRELIABLE_SEQUENCED, &input, 0, 0),
        Action::Applied
    );
    assert_eq!(
        srv.dispatch(999, channel::UNRELIABLE_SEQUENCED, &input, 0, 0),
        Action::Ignore,
        "unknown peer falls through the normal auth gate, not the choke"
    );
    assert_eq!(srv.scene_listener_forbidden_drops, 0);
}

#[tokio::test]
async fn scene_listener_handshake_accepts_and_is_never_a_subject() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, wallet, now_ms) = signed_handshake_request().await;
    let bytes = scene_listener_msg(&base, "realm-a", vec![rect(0, 0, 1, 1)], 0);

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::AuthenticatedListener {
            wallet: w,
            duplicate_of,
            listener,
            features,
        } => {
            assert_eq!(w, wallet);
            assert_eq!(duplicate_of, None);
            assert_eq!(features, 0);
            assert_eq!(listener.realm, "realm-a");
            assert_eq!(listener.parcels.len(), 4, "2x2 rect expands to 4 parcels");
        }
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
    assert!(
        srv.board.try_read(1).is_none(),
        "handshake path creates no board slot for a listener"
    );
}

#[tokio::test]
async fn scene_listener_handshake_negotiates_features() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _wallet, now_ms) = signed_handshake_request().await;
    let bytes = scene_listener_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0xFFFF_FFFF);

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::AuthenticatedListener { features, .. } => {
            assert_eq!(
                features, SERVER_FEATURES,
                "unknown bits masked to SERVER_FEATURES"
            )
        }
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
}

#[tokio::test]
async fn scene_listener_handshake_over_cap_is_rejected() {
    let mut srv = PulseServer::new();
    srv.max_scene_listener_parcels = 4;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _wallet, now_ms) = signed_handshake_request().await;
    let bytes = scene_listener_msg(&base, "realm-a", vec![rect(0, 0, 2, 2)], 0);

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
            assert!(reply.is_none(), "a field reject sends no HandshakeResponse");
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
    assert!(!srv.is_authenticated(1));
}

#[tokio::test]
async fn scene_listener_handshake_field_rejects() {
    async fn reject_case(realm: &str, rects: Vec<ParcelRect>) -> Action {
        let mut srv = PulseServer::new();
        srv.peers
            .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
        let (base, _w, now_ms) = signed_handshake_request().await;
        let bytes = scene_listener_msg(&base, realm, rects, 0);
        srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0)
    }

    let cases = [
        ("realm-a", vec![]),
        ("", vec![rect(0, 0, 0, 0)]),
        ("realm-a", vec![rect(1, 0, 0, 0)]),
        ("realm-a", vec![rect(9999, 0, 9999, 0)]),
    ];
    for (realm, rects) in cases {
        match reject_case(realm, rects).await {
            Action::Reject { reply, reason } => {
                assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
                assert!(reply.is_none());
            }
            other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
        }
    }
}

#[tokio::test]
async fn scene_listener_handshake_duplicate_wallet_evicts() {
    let mut srv = PulseServer::new();
    let (base, wallet, now_ms) = signed_handshake_request().await;
    authed(&mut srv, 3, &wallet);
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let bytes = scene_listener_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0);

    match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::AuthenticatedListener {
            wallet: w,
            duplicate_of,
            ..
        } => {
            assert_eq!(w, wallet);
            assert_eq!(
                duplicate_of,
                Some(3),
                "duplicate session across player/listener"
            );
        }
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
}

#[tokio::test]
async fn scene_listener_handshake_does_not_convert_authenticated_player() {
    let mut srv = PulseServer::new();
    let (base, wallet, now_ms) = signed_handshake_request().await;
    authed(&mut srv, 1, &wallet);
    let bytes = scene_listener_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0);

    assert_eq!(
        srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0),
        Action::Ignore,
        "an authenticated player cannot convert itself into a listener in place"
    );
    assert!(srv.peers[&1].scene_listener.is_none());
    assert_eq!(
        srv.peers[&1].connection_state,
        PeerConnectionState::Authenticated
    );
}
