//! Byte-exactness of the archipelago v3 protobuf wire format.
//!
//! The Unity, bevy and godot explorers all frame `/ws` as BINARY protobuf
//! (`ServerPacket` / `ClientPacket` from decentraland.kernel.comms.v3.archipelago,
//! generated from the upstream protocol repo's `.proto`). These tests pin the
//! exact on-wire bytes — hand-computed against the protobuf spec — so a stray
//! field-number or wire-type regression that would silently break every client
//! cannot pass CI. They also assert round-trip stability.

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

/// ServerPacket{ challenge_response: { challenge_to_sign: "dcl-1", already_connected: true } }
///
/// inner ChallengeResponseMessage:
///   field 1 (string)  tag 0x0a, len 0x05, "dcl-1" = 64 63 6c 2d 31
///   field 2 (bool)    tag 0x10, value 0x01
/// => inner = 0a 05 64 63 6c 2d 31 10 01   (9 bytes)
/// ServerPacket.challenge_response is oneof field 1 (message): tag 0x0a, len 0x09, <inner>
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

/// proto3 omits default-valued scalars: already_connected=false must not be on the wire.
#[test]
fn challenge_response_drops_default_bool() {
    let bytes = server(server_packet::Message::ChallengeResponse(
        ChallengeResponseMessage {
            challenge_to_sign: "dcl-1".into(),
            already_connected: false,
        },
    ));
    // inner is just the 7-byte string field; outer wraps in oneof field 1.
    assert_eq!(
        bytes,
        vec![0x0a, 0x07, 0x0a, 0x05, 0x64, 0x63, 0x6c, 0x2d, 0x31],
    );
}

/// ServerPacket{ welcome: { peer_id: "0xab" } }
///   inner: field 1 (string) tag 0x0a len 0x04 "0xab" = 30 78 61 62  => 0a 04 30 78 61 62
///   outer: welcome is oneof field 2 (message): tag 0x12 len 0x06
#[test]
fn welcome_wire_is_exact() {
    let bytes = server(server_packet::Message::Welcome(WelcomeMessage {
        peer_id: "0xab".into(),
    }));
    assert_eq!(bytes, vec![0x12, 0x06, 0x0a, 0x04, 0x30, 0x78, 0x61, 0x62]);
}

/// ClientPacket{ challenge_request: { address: "0xab" } }
///   inner: field 1 (string) tag 0x0a len 0x04 "0xab"  => 0a 04 30 78 61 62
///   outer: challenge_request is oneof field 1 (message): tag 0x0a len 0x06
#[test]
fn challenge_request_wire_is_exact() {
    let bytes = client(client_packet::Message::ChallengeRequest(
        ChallengeRequestMessage {
            address: "0xab".into(),
        },
    ));
    assert_eq!(bytes, vec![0x0a, 0x06, 0x0a, 0x04, 0x30, 0x78, 0x61, 0x62]);
}

/// Position is `decentraland.common.Position { float x=1, y=2, z=3 }`.
/// fixed32 wire-type 5. x=1.0 => 0x3f800000 (little-endian: 00 00 80 3f).
/// field 1 tag 0x0d; field 3 (z=2.0 => 0x40000000 LE 00 00 00 40) tag 0x1d.
/// y=0.0 is default and is dropped.
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
    // Position inner: 0d 00 00 80 3f  1d 00 00 00 40   (10 bytes)
    // Heartbeat: position is field 1 (message): tag 0x0a len 0x0a <inner>  (12 bytes)
    // ClientPacket: heartbeat is oneof field 3 (message): tag 0x1a len 0x0c
    assert_eq!(
        bytes,
        vec![
            0x1a, 0x0c, // ClientPacket.heartbeat, len 12
            0x0a, 0x0a, // Heartbeat.position, len 10
            0x0d, 0x00, 0x00, 0x80, 0x3f, // x = 1.0
            0x1d, 0x00, 0x00, 0x00, 0x40, // z = 2.0
        ],
    );
}

/// desired_room is `optional string` (field 2): when Some it is emitted even if empty.
#[test]
fn heartbeat_desired_room_present_when_some() {
    let bytes = client(client_packet::Message::Heartbeat(Heartbeat {
        position: None,
        desired_room: Some("r".into()),
    }));
    // Heartbeat inner: field 2 (string) tag 0x12 len 0x01 "r"(0x72) => 12 01 72
    // ClientPacket.heartbeat oneof field 3: tag 0x1a len 0x03
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

/// Full IslandChanged round-trips, including the `peers` map<string,Position>,
/// the `conn_str` the clients split on ':' and the `optional from_island_id`.
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

/// An unknown / future field number must decode to message: None (forward-compat),
/// not panic — mirrors how prost-generated clients tolerate server upgrades.
#[test]
fn unknown_oneof_field_decodes_to_none() {
    // field 99 (varint), value 1: tag = (99<<3)|0 = 0x18b -> varint 0x9b 0x06
    let bytes = vec![0x98, 0x06, 0x01];
    let pkt = ServerPacket::decode(bytes.as_slice()).unwrap();
    assert!(pkt.message.is_none());
}
