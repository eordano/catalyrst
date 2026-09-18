use super::*;
use crate::proto::archipelago::*;
use prost::Message;

const ADDRESS: &str = "0x00112233445566778899aabbccddeeff00112233";
const GOLDEN: &str = "dcl-d98b4b41ebb6960d45dc637ceed8570deb56036078f6b8bd88f1de9b7bc54221";
const PREIMAGE: &str = concat!(
    "64636c2d617263686970656c61676f2d7634000000000100112233445566778899aabbccddeeff00112233",
    "00000004000000000000000f000000000000000f00000010000102030405060708090a0b0c0d0e0f",
    "00000014101112131415161718191a1b1c1d1e1f2021222300000002000000057265616c6d0000000b7363656e653a616c706861",
    "00000004000000000000000f00000020202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
    "0000000e756e6974792d76342d6c6f63616c000000097265706c6963612d6100000010404142434445464748494a4b4c4d4e4f",
    "00000010505152535455565758595a5b5c5d5e5f01020304050607080000018bcfe5687b0000000b617574686f726974792d61",
    "000010000000000800000010000000400000400000002710000000800000000200000000000000057265616c6d000000010000000b7363656e653a616c706861"
);

fn fixture() -> (ConnectionOffer, ConnectionSelection) {
    let offer = ConnectionOffer {
        version: 4,
        offered_features: 15,
        required_features: 15,
        client_nonce: (0..16).collect(),
        session_id: (16..36).collect(),
        lanes: vec!["realm".into(), "scene:alpha".into()],
    };
    let selection = ConnectionSelection {
        version: 4,
        selected_features: 15,
        context: Some(ConnectionContext {
            server_nonce: (32..64).collect(),
            audience: "unity-v4-local".into(),
            issuer: "replica-a".into(),
            process_incarnation: (64..80).collect(),
            connection_id: (80..96).collect(),
            connection_epoch: 0x0102030405060708,
            expires_at_ms: 1_700_000_000_123,
            authority_incarnation: "authority-a".into(),
        }),
        limits: Some(ConnectionLimits {
            max_frame_bytes: 4096,
            max_lanes: 8,
            max_pending_requests: 16,
            max_retained_responses: 64,
            max_retained_response_bytes: 16384,
            handshake_timeout_ms: 10000,
            max_public_detail_bytes: 128,
        }),
        lanes: vec![
            LaneBinding {
                handle: 0,
                name: "realm".into(),
            },
            LaneBinding {
                handle: 1,
                name: "scene:alpha".into(),
            },
        ],
    };
    (offer, selection)
}

#[test]
fn canonical_transcript_matches_shared_csharp_golden() {
    let (offer, selection) = fixture();
    let encoded: String = canonical_preimage(ADDRESS, &offer, &selection)
        .unwrap()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(encoded, PREIMAGE);
    assert_eq!(
        canonical_challenge(ADDRESS, &offer, &selection).unwrap(),
        GOLDEN
    );
    assert_eq!(
        canonical_challenge(&ADDRESS.to_uppercase(), &offer, &selection).unwrap(),
        GOLDEN
    );
}

#[test]
fn inherited_context_transcript_matches_shared_csharp_golden() {
    let (mut offer, mut selection) = fixture();
    offer.offered_features |= FEATURE_INHERIT_CONTEXT;
    selection.selected_features |= FEATURE_INHERIT_CONTEXT;
    assert_eq!(
        canonical_challenge(ADDRESS, &offer, &selection).unwrap(),
        "dcl-b7b789d8209eadde34349bf756e58d07f519c157ee75b84831a54b82dcf6f412"
    );
    selection.selected_features = CORE_FEATURES;
    assert_ne!(
        canonical_challenge(ADDRESS, &offer, &selection).unwrap(),
        GOLDEN
    );
}

#[test]
fn inherited_context_requires_supported_offered_and_selected_capability() {
    let (mut offer, mut selection) = fixture();
    selection.selected_features |= FEATURE_INHERIT_CONTEXT;
    assert_eq!(
        validate_selection(&offer, &selection, KNOWN_FEATURES),
        Err(TranscriptError::Features)
    );
    offer.offered_features |= FEATURE_INHERIT_CONTEXT;
    assert!(validate_selection(&offer, &selection, KNOWN_FEATURES).is_ok());
    assert_eq!(
        validate_selection(&offer, &selection, CORE_FEATURES),
        Err(TranscriptError::Features)
    );
    selection.selected_features = CORE_FEATURES;
    assert!(validate_selection(&offer, &selection, CORE_FEATURES).is_ok());
    offer.required_features |= FEATURE_INHERIT_CONTEXT;
    assert_eq!(
        validate_offer(&offer, CORE_FEATURES),
        Err(TranscriptError::Features)
    );
    assert_eq!(
        validate_selection(&offer, &selection, KNOWN_FEATURES),
        Err(TranscriptError::Features)
    );
}

#[test]
fn every_authenticated_context_and_limit_changes_the_challenge() {
    let (offer, selection) = fixture();
    let mutations: &[fn(&mut ConnectionSelection)] = &[
        |s| s.context.as_mut().unwrap().server_nonce[0] ^= 1,
        |s| s.context.as_mut().unwrap().audience.push('x'),
        |s| s.context.as_mut().unwrap().issuer.push('x'),
        |s| s.context.as_mut().unwrap().process_incarnation[0] ^= 1,
        |s| s.context.as_mut().unwrap().connection_id[0] ^= 1,
        |s| s.context.as_mut().unwrap().connection_epoch += 1,
        |s| s.context.as_mut().unwrap().expires_at_ms += 1,
        |s| s.context.as_mut().unwrap().authority_incarnation.push('x'),
        |s| s.limits.as_mut().unwrap().max_frame_bytes += 1,
        |s| s.limits.as_mut().unwrap().max_lanes += 1,
        |s| s.limits.as_mut().unwrap().max_pending_requests += 1,
        |s| s.limits.as_mut().unwrap().max_retained_responses += 1,
        |s| s.limits.as_mut().unwrap().max_retained_response_bytes += 1,
        |s| s.limits.as_mut().unwrap().handshake_timeout_ms += 1,
        |s| s.limits.as_mut().unwrap().max_public_detail_bytes += 1,
    ];
    for mutate in mutations {
        let mut changed = selection.clone();
        mutate(&mut changed);
        assert_ne!(
            canonical_challenge(ADDRESS, &offer, &changed).unwrap(),
            GOLDEN
        );
    }
    for mutate in [
        (|o: &mut ConnectionOffer| o.client_nonce[0] ^= 1) as fn(&mut ConnectionOffer),
        |o| o.session_id[0] ^= 1,
        |o| o.offered_features |= 1 << 63,
    ] {
        let mut changed = offer.clone();
        mutate(&mut changed);
        assert_ne!(
            canonical_challenge(ADDRESS, &changed, &selection).unwrap(),
            GOLDEN
        );
    }
}

#[test]
fn rejects_unknown_required_bits_and_unselected_core_features() {
    let (mut offer, mut selection) = fixture();
    offer.offered_features |= 1 << 63;
    assert!(validate_selection(&offer, &selection, KNOWN_FEATURES).is_ok());
    offer.required_features |= 1 << 63;
    assert_eq!(
        validate_offer(&offer, KNOWN_FEATURES),
        Err(TranscriptError::Features)
    );
    offer.required_features = CORE_FEATURES;
    selection.selected_features |= 1 << 63;
    assert_eq!(
        validate_selection(&offer, &selection, KNOWN_FEATURES),
        Err(TranscriptError::Features)
    );
    selection.selected_features = CORE_FEATURES & !FEATURE_TYPED_AUTH;
    assert_eq!(
        validate_selection(&offer, &selection, KNOWN_FEATURES),
        Err(TranscriptError::Features)
    );
}

#[test]
fn rejects_invalid_identifiers_lane_bindings_and_limits() {
    let (offer, selection) = fixture();
    let mutations: &[fn(&mut ConnectionOffer)] = &[
        |o| {
            o.client_nonce.pop();
        },
        |o| o.session_id = vec![1; 16],
        |o| o.lanes.push("realm".into()),
        |o| o.lanes.swap(0, 1),
        |o| o.lanes[1] = "scene:".into(),
        |o| o.lanes[1] = format!("scene:{}", "x".repeat(256)),
    ];
    for mutate in mutations {
        let mut changed = offer.clone();
        mutate(&mut changed);
        assert!(validate_selection(&changed, &selection, KNOWN_FEATURES).is_err());
    }
    let mutations: &[fn(&mut ConnectionSelection)] = &[
        |s| s.context = None,
        |s| {
            s.context.as_mut().unwrap().server_nonce.pop();
        },
        |s| s.context.as_mut().unwrap().issuer = "invalid\nissuer".into(),
        |s| s.context.as_mut().unwrap().audience = "\u{e9}".repeat(129),
        |s| s.lanes[0].handle = 1,
        |s| s.lanes[1].name = "realm".into(),
        |s| s.lanes.swap(0, 1),
        |s| s.limits = None,
        |s| s.limits.as_mut().unwrap().max_lanes = 1,
        |s| s.limits.as_mut().unwrap().handshake_timeout_ms = 30001,
        |s| s.limits.as_mut().unwrap().max_frame_bytes = 0,
    ];
    for mutate in mutations {
        let mut changed = selection.clone();
        mutate(&mut changed);
        assert!(validate_selection(&offer, &changed, KNOWN_FEATURES).is_err());
    }
}

#[test]
fn scene_only_connection_does_not_claim_realm_handle() {
    let (mut offer, mut selection) = fixture();
    offer.lanes.remove(0);
    selection.lanes.remove(0);
    assert!(validate_selection(&offer, &selection, KNOWN_FEATURES).is_ok());
    selection.lanes[0].handle = 0;
    assert_eq!(
        validate_selection(&offer, &selection, KNOWN_FEATURES),
        Err(TranscriptError::Lanes)
    );
}

#[test]
fn rejects_non_hex_wallet_address_pairs() {
    assert!(decode_address("0x+a112233445566778899aabbccddeeff00112233").is_err());
    assert!(decode_address("00112233445566778899aabbccddeeff00112233").is_err());
}

#[test]
fn extension_absence_preserves_legacy_packet_bytes() {
    let packet = ClientPacket {
        message: Some(client_packet::Message::ChallengeRequest(
            ChallengeRequestMessage {
                address: "0xabc".into(),
                ..Default::default()
            },
        )),
        ..Default::default()
    };
    assert_eq!(packet.encode_to_vec(), b"\x0a\x07\x0a\x050xabc");
    let welcome = ServerPacket {
        message: Some(server_packet::Message::Welcome(WelcomeMessage {
            peer_id: "peer".into(),
            ..Default::default()
        })),
        ..Default::default()
    };
    assert_eq!(welcome.encode_to_vec(), b"\x12\x06\x0a\x04peer");
    let heartbeat = ClientPacket {
        message: Some(client_packet::Message::Heartbeat(Heartbeat::default())),
        ..Default::default()
    };
    assert_eq!(heartbeat.encode_to_vec(), b"\x1a\x00");
}
