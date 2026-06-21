//! Byte-parity tests for the `decentraland.quests` protobuf wire (task #43).
//!
//! These pin the EXACT protobuf-encoded bytes of known messages — the wire the
//! upstream `decentraland/quests` service (and the explorer's generated client)
//! produces from the same `definitions.proto` (pinned commit
//! 03626d76db879afcdfd4fbcdc0342a04e5b4f663, vendored byte-identically). The
//! expected byte vectors below were hand-derived from the protobuf3 wire spec
//! (field<<3|wire_type tags, length-delimited strings/submessages, little-endian
//! fixed32, map entries as key=1/value=2 submessages); asserting prost matches
//! them proves catalyrst encodes the identical wire any upstream peer expects.

use catalyrst_quests::proto::ProtocolMessage; // prost::Message: encode_to_vec / decode
use catalyrst_quests::proto::*;
use std::collections::HashMap;

/// Action { string type = 1; map<string,string> parameters = 2 }
/// type="LOCATION", parameters={"x":"10"} (single entry → deterministic order).
#[test]
fn action_byte_parity() {
    let mut parameters = HashMap::new();
    parameters.insert("x".to_string(), "10".to_string());
    let action = Action {
        r#type: "LOCATION".to_string(),
        parameters,
    };
    // tag1(str) len8 "LOCATION" | tag2(msg) len7 [ tag1 len1 "x" | tag2 len2 "10" ]
    let expected: &[u8] = &[
        10, 8, 76, 79, 67, 65, 84, 73, 79, 78, // type = "LOCATION"
        18, 7, 10, 1, 120, 18, 2, 49, 48, // parameters: {"x":"10"}
    ];
    assert_eq!(action.encode_to_vec(), expected);
}

/// Event { string id = 1; string address = 2; Action action = 3 }.
#[test]
fn event_byte_parity() {
    let mut parameters = HashMap::new();
    parameters.insert("x".to_string(), "10".to_string());
    let event = Event {
        id: "e1".to_string(),
        address: "0xabc".to_string(),
        action: Some(Action {
            r#type: "LOCATION".to_string(),
            parameters,
        }),
    };
    let expected: &[u8] = &[
        10, 2, 101, 49, // id = "e1"
        18, 5, 48, 120, 97, 98, 99, // address = "0xabc"
        26, 19, 10, 8, 76, 79, 67, 65, 84, 73, 79, 78, 18, 7, 10, 1, 120, 18, 2, 49,
        48, // action = Action{...}
    ];
    let encoded = event.encode_to_vec();
    assert_eq!(encoded, expected);
    // Round-trips back to the same message.
    let decoded = Event::decode(encoded.as_slice()).unwrap();
    assert_eq!(decoded.id, "e1");
    assert_eq!(decoded.address, "0xabc");
    assert_eq!(decoded.action.unwrap().r#type, "LOCATION");
}

/// Quest { id=1, name=2, description=3, definition=4, creator_address=5,
/// image_url=6, active=7(bool), created_at=8(fixed32) }. `created_at` is a
/// FIXED32 (not a varint) per the proto — pinned here so a regression to varint
/// is caught.
#[test]
fn quest_byte_parity_created_at_is_fixed32() {
    let quest = Quest {
        id: "q1".to_string(),
        name: "n".to_string(),
        description: "d".to_string(),
        definition: None,
        creator_address: String::new(),
        image_url: String::new(),
        active: true,
        created_at: 1234,
    };
    // Empty proto3 scalar fields (creator_address, image_url) are omitted.
    let expected: &[u8] = &[
        10, 2, 113, 49, // id = "q1"
        18, 1, 110, // name = "n"
        26, 1, 100, // description = "d"
        56, 1, // active = true
        69, 210, 4, 0, 0, // created_at = 1234 as fixed32 LE (tag 8<<3|5 = 69)
    ];
    assert_eq!(quest.encode_to_vec(), expected);
}

/// QuestState { current_steps=2(map), steps_left=3(fixed32), steps_completed=4,
/// required_steps=5 }. Pins steps_left as FIXED32.
#[test]
fn quest_state_byte_parity() {
    let state = QuestState {
        current_steps: HashMap::new(),
        steps_left: 2,
        steps_completed: vec!["A".to_string()],
        required_steps: vec!["C".to_string()],
    };
    let expected: &[u8] = &[
        29, 2, 0, 0, 0, // steps_left = 2 as fixed32 LE (tag 3<<3|5 = 29)
        34, 1, 65, // steps_completed = ["A"]
        42, 1, 67, // required_steps = ["C"]
    ];
    assert_eq!(state.encode_to_vec(), expected);
}

/// The `Accepted` oneof arm of StartQuestResponse — an empty submessage carried
/// as response field 1 (the success wire the explorer matches on StartQuest).
#[test]
fn start_quest_accepted_byte_parity() {
    let resp = StartQuestResponse {
        response: Some(start_quest_response::Response::Accepted(
            start_quest_response::Accepted {},
        )),
    };
    // field 1, wire type 2 (len-delimited), length 0 → empty Accepted{}.
    let expected: &[u8] = &[10, 0];
    assert_eq!(resp.encode_to_vec(), expected);
}

/// EventResponse::IgnoredEvent — the wire returned by SendEvent when an event
/// matches no active task (upstream `add_event_controller` NoAction path is
/// IgnoredEvent on field 2).
#[test]
fn event_response_ignored_byte_parity() {
    let resp = EventResponse {
        response: Some(event_response::Response::IgnoredEvent(IgnoredEvent {})),
    };
    // field 2, wire type 2, length 0.
    let expected: &[u8] = &[18, 0];
    assert_eq!(resp.encode_to_vec(), expected);
}
