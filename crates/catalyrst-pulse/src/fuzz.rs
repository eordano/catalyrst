use crate::server::{Action, PulseServer};
use crate::simulation::{PeerConnectionState, PeerState};

pub const FUZZ_PEER: u32 = 7;

fn fuzz_wallet(peer: u32) -> String {
    format!("0x{peer:040x}")
}

pub fn pending_server(peer: u32) -> PulseServer {
    let mut server = PulseServer::new();
    server
        .peers
        .insert(peer, PeerState::new(PeerConnectionState::PendingAuth, 0));
    server
}

pub fn authenticated_server(peer: u32) -> PulseServer {
    let mut server = PulseServer::new();
    let wallet = fuzz_wallet(peer);
    let mut state = PeerState::new(PeerConnectionState::Authenticated, 0);
    state.wallet_id = Some(wallet.clone());
    server.peers.insert(peer, state);
    server.identity.set(peer, wallet);
    server.board.set_active(peer);
    server
}

pub fn drive(server: &mut PulseServer, peer: u32, channel: u8, data: &[u8]) -> Action {
    server.dispatch(peer, channel, data, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decentraland::pulse::{
        client_message, ClientMessage, EmoteStart, PlayerState, PlayerStateInput, TeleportRequest,
    };
    use crate::hardening::{
        DisconnectReason, DEFAULT_MAX_EMOTE_DURATION_MS, DEFAULT_MAX_EMOTE_ID_LENGTH,
    };
    use crate::server::channel;
    use proptest::prelude::*;
    use prost::Message as _;

    fn encode(inner: client_message::Message) -> Vec<u8> {
        ClientMessage {
            message: Some(inner),
        }
        .encode_to_vec()
    }

    fn player_state_at(parcel: i32) -> PlayerState {
        let mut state = PlayerState {
            parcel_index: parcel,
            ..Default::default()
        };
        state.set_position_x_f(8.0);
        state.set_position_z_f(8.0);
        state
    }

    proptest! {
        #[test]
        fn pre_auth_random_never_promotes(
            data in proptest::collection::vec(any::<u8>(), 0..512),
            ch in 0u8..3,
        ) {
            let mut server = pending_server(FUZZ_PEER);
            let action = drive(&mut server, FUZZ_PEER, ch, &data);
            prop_assert!(
                !matches!(action, Action::Authenticated { .. }),
                "random bytes must never authenticate a peer"
            );
            prop_assert!(
                !matches!(action, Action::Applied),
                "an unauthenticated peer must never apply gameplay"
            );
        }

        #[test]
        fn post_auth_mutation_never_panics(
            flips in proptest::collection::vec((any::<usize>(), 1u8..=255), 0..8),
            parcel in -200i32..200,
        ) {
            let mut bytes = encode(client_message::Message::Input(PlayerStateInput {
                state: Some(player_state_at(parcel)),
            }));
            for (idx, val) in flips {
                if !bytes.is_empty() {
                    let i = idx % bytes.len();
                    bytes[i] ^= val;
                }
            }
            let mut server = authenticated_server(FUZZ_PEER);
            let _ = drive(&mut server, FUZZ_PEER, channel::UNRELIABLE_SEQUENCED, &bytes);
        }

        #[test]
        fn out_of_range_code_is_never_stored(code in any::<u32>(), field in 0u8..14) {
            use crate::messages::spec;
            let mut state = player_state_at(0);
            let max_code = match field {
                0 => { state.position_x = code; spec::POSITION_X.max_code() }
                1 => { state.position_y = code; spec::POSITION_Y.max_code() }
                2 => { state.position_z = code; spec::POSITION_Z.max_code() }
                3 => { state.velocity_x = code; spec::VELOCITY_X.max_code() }
                4 => { state.velocity_y = code; spec::VELOCITY_Y.max_code() }
                5 => { state.velocity_z = code; spec::VELOCITY_Z.max_code() }
                6 => { state.rotation_y = code; spec::ROTATION_Y.max_code() }
                7 => { state.movement_blend = code; spec::MOVEMENT_BLEND.max_code() }
                8 => { state.slide_blend = code; spec::SLIDE_BLEND.max_code() }
                9 => { state.head_yaw = Some(code); spec::HEAD_YAW.max_code() }
                10 => { state.head_pitch = Some(code); spec::HEAD_PITCH.max_code() }
                11 => { state.point_at_x = Some(code); spec::POINT_AT_X.max_code() }
                12 => { state.point_at_y = Some(code); spec::POINT_AT_Y.max_code() }
                _ => { state.point_at_z = Some(code); spec::POINT_AT_Z.max_code() }
            };
            let in_range = code <= max_code;
            let bytes = encode(client_message::Message::Input(PlayerStateInput {
                state: Some(state),
            }));
            let mut server = authenticated_server(FUZZ_PEER);
            let action = drive(&mut server, FUZZ_PEER, channel::UNRELIABLE_SEQUENCED, &bytes);
            if !in_range {
                prop_assert_eq!(action, Action::Ignore, "out-of-range code (field {}) must be ignored", field);
                prop_assert_eq!(
                    server.board.last_seq(FUZZ_PEER),
                    crate::snapshot::NO_SEQ,
                    "nothing may be published from a rejected state"
                );
            }
        }

        #[test]
        fn oversized_emote_is_capped(id_len in 0usize..1200, dur in 0u32..200_000) {
            let emote = EmoteStart {
                emote_id: "e".repeat(id_len),
                duration_ms: Some(dur),
                player_state: Some(player_state_at(0)),
                ..Default::default()
            };
            let bytes = encode(client_message::Message::EmoteStart(emote));
            let mut server = authenticated_server(FUZZ_PEER);
            let action = drive(&mut server, FUZZ_PEER, channel::RELIABLE, &bytes);
            let over_cap =
                id_len > DEFAULT_MAX_EMOTE_ID_LENGTH || dur > DEFAULT_MAX_EMOTE_DURATION_MS;
            if over_cap {
                prop_assert!(
                    matches!(
                        action,
                        Action::Reject { reason: DisconnectReason::InvalidEmoteField, .. }
                    ),
                    "over-cap emote must be rejected, got {:?}",
                    action
                );
            }
        }

        #[test]
        fn gameplay_before_auth_is_always_ignored(kinds in proptest::collection::vec(0u8..5, 0..12)) {
            let mut server = pending_server(FUZZ_PEER);
            for k in kinds {
                let msg = match k {
                    0 => client_message::Message::Input(PlayerStateInput {
                        state: Some(player_state_at(0)),
                    }),
                    1 => client_message::Message::Teleport(TeleportRequest {
                        parcel_index: 0,
                        realm: "realm".into(),
                        ..Default::default()
                    }),
                    2 => client_message::Message::EmoteStop(Default::default()),
                    3 => client_message::Message::Resync(Default::default()),
                    _ => client_message::Message::ProfileAnnouncement(Default::default()),
                };
                let bytes = encode(msg);
                let action = drive(&mut server, FUZZ_PEER, channel::RELIABLE, &bytes);
                prop_assert_eq!(action, Action::Ignore, "gameplay before auth must be ignored");
            }
        }

        #[test]
        fn movement_flood_never_exceeds_burst(n in 0usize..1024) {
            let bytes = encode(client_message::Message::Input(PlayerStateInput {
                state: Some(player_state_at(0)),
            }));
            let mut server = authenticated_server(FUZZ_PEER);
            let applied = (0..n)
                .filter(|_| matches!(
                    server.dispatch(FUZZ_PEER, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 1000),
                    Action::Applied))
                .count();
            prop_assert!(applied <= crate::hardening::DEFAULT_INPUT_BURST as usize,
                "a frozen-time movement flood must never apply more than the burst capacity");
        }

        #[test]
        fn discrete_flood_never_exceeds_burst(n in 0usize..1024) {
            let bytes = encode(client_message::Message::Teleport(TeleportRequest {
                parcel_index: 0, realm: "realm".into(), ..Default::default()
            }));
            let mut server = authenticated_server(FUZZ_PEER);
            let applied = (0..n)
                .filter(|_| matches!(
                    server.dispatch(FUZZ_PEER, channel::RELIABLE, &bytes, 0, 1000),
                    Action::Applied))
                .count();
            prop_assert!(applied <= crate::hardening::DEFAULT_DISCRETE_BURST as usize);
        }
    }

    #[test]
    fn movement_flood_is_bounded_but_normal_rate_never_dropped() {
        use crate::hardening::{DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ};
        let bytes = encode(client_message::Message::Input(PlayerStateInput {
            state: Some(player_state_at(0)),
        }));
        let mut server = authenticated_server(FUZZ_PEER);
        let applied = (0..200)
            .filter(|_| {
                matches!(
                    server.dispatch(FUZZ_PEER, channel::UNRELIABLE_SEQUENCED, &bytes, 0, 1000),
                    Action::Applied
                )
            })
            .count();
        assert_eq!(applied, DEFAULT_INPUT_BURST as usize);

        let interval = 1000 / DEFAULT_INPUT_MAX_HZ;
        let mut server = authenticated_server(FUZZ_PEER);
        for i in 0..500u32 {
            assert_eq!(
                server.dispatch(
                    FUZZ_PEER,
                    channel::UNRELIABLE_SEQUENCED,
                    &bytes,
                    0,
                    1000 + i * interval
                ),
                Action::Applied,
                "normal-rate movement must never be dropped"
            );
        }
    }

    async fn sign_handshake(ts_ms: i64) -> (Vec<u8>, String) {
        use crate::handshake::build_signed_fetch_payload;
        use alloy::signers::{local::PrivateKeySigner, Signer};
        use catalyrst_types::{AuthLink, AuthLinkType};

        let root: PrivateKeySigner =
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .unwrap();
        let root_addr = format!("{:#x}", root.address());
        let ephemeral: PrivateKeySigner =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let eph_addr = format!("{:#x}", ephemeral.address());
        let ts = ts_ms.to_string();
        let metadata = "{\"signer\":\"dcl:explorer\"}";
        let connect_payload = build_signed_fetch_payload("connect", "/", &ts, metadata);
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
        map.insert("x-identity-timestamp".into(), serde_json::Value::String(ts));
        map.insert(
            "x-identity-metadata".into(),
            serde_json::Value::String(metadata.into()),
        );
        let bag = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();
        let msg = encode(client_message::Message::Handshake(
            crate::decentraland::pulse::HandshakeRequest {
                auth_chain: bag.into_bytes(),
                profile_version: 0,
                initial_state: None,
                protocol_features: 0,
            },
        ));
        (msg, root_addr.to_lowercase())
    }

    #[tokio::test]
    async fn handshake_verifier_and_ghost_guard() {
        let ts: i64 = 1_700_000_000_000;
        let (valid, wallet) = sign_handshake(ts).await;

        let mut server = pending_server(FUZZ_PEER);
        let action = server.dispatch(FUZZ_PEER, channel::RELIABLE, &valid, ts, 0);
        assert!(
            matches!(&action, Action::Authenticated { wallet: w, .. } if *w == wallet),
            "valid handshake must authenticate, got {action:?}"
        );

        let mut server = pending_server(FUZZ_PEER);
        let action = server.dispatch(FUZZ_PEER, channel::RELIABLE, &valid, ts + 600_000, 0);
        assert!(
            !matches!(action, Action::Authenticated { .. }),
            "stale-timestamp handshake must not authenticate, got {action:?}"
        );

        let mut tampered = valid.clone();
        let mid = tampered.len() / 2;
        tampered[mid] ^= 0xFF;
        let mut server = pending_server(FUZZ_PEER);
        let action = server.dispatch(FUZZ_PEER, channel::RELIABLE, &tampered, ts, 0);
        assert!(
            !matches!(action, Action::Authenticated { .. }),
            "tampered handshake must not authenticate, got {action:?}"
        );

        let mut server = PulseServer::new();
        let action = server.dispatch(FUZZ_PEER, channel::RELIABLE, &valid, ts, 0);
        assert_eq!(
            action,
            Action::Ignore,
            "handshake from an untracked (refused) peer must be ignored, not authenticated"
        );
    }

    #[test]
    fn corrupt_flood_eventually_disconnects() {
        let mut server = pending_server(FUZZ_PEER);
        let corrupt: &[u8] = &[0x0A, 0xFF];
        let mut rejected = false;
        for _ in 0..64 {
            if matches!(
                drive(&mut server, FUZZ_PEER, channel::RELIABLE, corrupt),
                Action::Reject {
                    reason: DisconnectReason::PacketCorrupted,
                    ..
                }
            ) {
                rejected = true;
                break;
            }
        }
        assert!(
            rejected,
            "a corrupt-packet flood must eventually trip the corruption budget"
        );
    }
}
