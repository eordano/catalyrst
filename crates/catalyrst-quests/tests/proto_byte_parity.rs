use catalyrst_quests::proto::ProtocolMessage;
use catalyrst_quests::proto::*;
use std::collections::HashMap;

#[test]
fn action_byte_parity() {
    let mut parameters = HashMap::new();
    parameters.insert("x".to_string(), "10".to_string());
    let action = Action {
        r#type: "LOCATION".to_string(),
        parameters,
    };

    let expected: &[u8] = &[
        10, 8, 76, 79, 67, 65, 84, 73, 79, 78, 18, 7, 10, 1, 120, 18, 2, 49, 48,
    ];
    assert_eq!(action.encode_to_vec(), expected);
}

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
        10, 2, 101, 49, 18, 5, 48, 120, 97, 98, 99, 26, 19, 10, 8, 76, 79, 67, 65, 84, 73, 79, 78,
        18, 7, 10, 1, 120, 18, 2, 49, 48,
    ];
    let encoded = event.encode_to_vec();
    assert_eq!(encoded, expected);

    let decoded = Event::decode(encoded.as_slice()).unwrap();
    assert_eq!(decoded.id, "e1");
    assert_eq!(decoded.address, "0xabc");
    assert_eq!(decoded.action.unwrap().r#type, "LOCATION");
}

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

    let expected: &[u8] = &[
        10, 2, 113, 49, 18, 1, 110, 26, 1, 100, 56, 1, 69, 210, 4, 0, 0,
    ];
    assert_eq!(quest.encode_to_vec(), expected);
}

#[test]
fn quest_state_byte_parity() {
    let state = QuestState {
        current_steps: HashMap::new(),
        steps_left: 2,
        steps_completed: vec!["A".to_string()],
        required_steps: vec!["C".to_string()],
    };
    let expected: &[u8] = &[29, 2, 0, 0, 0, 34, 1, 65, 42, 1, 67];
    assert_eq!(state.encode_to_vec(), expected);
}

#[test]
fn start_quest_accepted_byte_parity() {
    let resp = StartQuestResponse {
        response: Some(start_quest_response::Response::Accepted(
            start_quest_response::Accepted {},
        )),
    };

    let expected: &[u8] = &[10, 0];
    assert_eq!(resp.encode_to_vec(), expected);
}

#[test]
fn event_response_ignored_byte_parity() {
    let resp = EventResponse {
        response: Some(event_response::Response::IgnoredEvent(IgnoredEvent {})),
    };

    let expected: &[u8] = &[18, 0];
    assert_eq!(resp.encode_to_vec(), expected);
}
