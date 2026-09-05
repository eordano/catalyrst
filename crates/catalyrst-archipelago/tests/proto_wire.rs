use catalyrst_archipelago::proto::archipelago::{
    client_packet, server_packet, ChallengeRequestMessage, ChallengeResponseMessage, ClientPacket,
    Heartbeat, IslandChangedMessage, ServerPacket, SignedChallengeMessage, WelcomeMessage,
};
use catalyrst_archipelago::proto::Position;
use prost::Message as _;
use std::collections::HashMap;

fn server(message: server_packet::Message) -> Vec<u8> {
    ServerPacket {
        message: Some(message),
    }
    .encode_to_vec()
}

fn client(message: client_packet::Message) -> Vec<u8> {
    ClientPacket {
        message: Some(message),
    }
    .encode_to_vec()
}

#[test]
fn challenge_response_wire_is_exact() {
    let bytes = server(server_packet::Message::ChallengeResponse(
        ChallengeResponseMessage {
            challenge_to_sign: "dcl-1".into(),
            already_connected: true,
        },
    ));
    assert_eq!(
        bytes,
        vec![0x0a, 0x09, 0x0a, 0x05, 0x64, 0x63, 0x6c, 0x2d, 0x31, 0x10, 0x01],
    );
}

#[test]
fn challenge_response_drops_default_bool() {
    let bytes = server(server_packet::Message::ChallengeResponse(
        ChallengeResponseMessage {
            challenge_to_sign: "dcl-1".into(),
            already_connected: false,
        },
    ));

    assert_eq!(
        bytes,
        vec![0x0a, 0x07, 0x0a, 0x05, 0x64, 0x63, 0x6c, 0x2d, 0x31],
    );
}

#[test]
fn welcome_wire_is_exact() {
    let bytes = server(server_packet::Message::Welcome(WelcomeMessage {
        peer_id: "0xab".into(),
    }));
    assert_eq!(bytes, vec![0x12, 0x06, 0x0a, 0x04, 0x30, 0x78, 0x61, 0x62]);
}

#[test]
fn challenge_request_wire_is_exact() {
    let bytes = client(client_packet::Message::ChallengeRequest(
        ChallengeRequestMessage {
            address: "0xab".into(),
        },
    ));
    assert_eq!(bytes, vec![0x0a, 0x06, 0x0a, 0x04, 0x30, 0x78, 0x61, 0x62]);
}

#[test]
fn heartbeat_position_wire_is_exact() {
    let bytes = client(client_packet::Message::Heartbeat(Heartbeat {
        position: Some(Position {
            x: 1.0,
            y: 0.0,
            z: 2.0,
        }),
        desired_room: None,
    }));

    assert_eq!(
        bytes,
        vec![0x1a, 0x0c, 0x0a, 0x0a, 0x0d, 0x00, 0x00, 0x80, 0x3f, 0x1d, 0x00, 0x00, 0x00, 0x40,],
    );
}

#[test]
fn heartbeat_desired_room_present_when_some() {
    let bytes = client(client_packet::Message::Heartbeat(Heartbeat {
        position: None,
        desired_room: Some("r".into()),
    }));

    assert_eq!(bytes, vec![0x1a, 0x03, 0x12, 0x01, 0x72]);
}

#[test]
fn signed_challenge_round_trip() {
    let json = r#"[{"type":"SIGNER","payload":"0xab","signature":""}]"#;
    let msg = client_packet::Message::SignedChallenge(SignedChallengeMessage {
        auth_chain_json: json.into(),
    });
    let bytes = client(msg);
    let back = ClientPacket::decode(bytes.as_slice()).unwrap();
    match back.message {
        Some(client_packet::Message::SignedChallenge(s)) => {
            assert_eq!(s.auth_chain_json, json);
        }
        other => panic!("expected SignedChallenge, got {other:?}"),
    }
}

#[test]
fn island_changed_round_trip() {
    let mut peers = HashMap::new();
    peers.insert(
        "0xab".to_string(),
        Position {
            x: 10.0,
            y: 1.0,
            z: -5.0,
        },
    );
    let original = IslandChangedMessage {
        island_id: "i42".into(),
        conn_str: "livekit:wss://lk.example?access_token=tok".into(),
        from_island_id: Some("i7".into()),
        peers: peers.clone(),
    };
    let bytes = server(server_packet::Message::IslandChanged(original.clone()));
    let back = ServerPacket::decode(bytes.as_slice()).unwrap();
    match back.message {
        Some(server_packet::Message::IslandChanged(m)) => {
            assert_eq!(m.island_id, "i42");
            assert_eq!(m.conn_str, "livekit:wss://lk.example?access_token=tok");
            assert_eq!(m.from_island_id.as_deref(), Some("i7"));
            assert_eq!(m.peers, peers);
        }
        other => panic!("expected IslandChanged, got {other:?}"),
    }
}

#[test]
fn unknown_oneof_field_decodes_to_none() {
    let bytes = vec![0x98, 0x06, 0x01];
    let pkt = ServerPacket::decode(bytes.as_slice()).unwrap();
    assert!(pkt.message.is_none());
}
