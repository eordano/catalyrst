use super::*;

use std::collections::HashSet;

use crate::decentraland::pulse::{
    ParcelRect, SceneListenerAoi, SceneListenerHandshakeRequest, SceneListenerUpdate,
};
use crate::hardening::{
    GameplayRateLimiter, DEFAULT_DISCRETE_BURST, DEFAULT_SCENE_LISTENER_MAX_PARCELS,
    SCENE_LISTENER_REALM_BUDGET_COST,
};
use crate::interest::{SceneListenerCellMapper, SceneListenerState};

fn rect(min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> ParcelRect {
    ParcelRect {
        min_x,
        min_z,
        max_x,
        max_z,
    }
}

fn aoi(realm: &str, rects: Vec<ParcelRect>) -> SceneListenerAoi {
    SceneListenerAoi {
        realm: realm.into(),
        parcel_rects: rects,
    }
}

fn single_realm(srv: &PulseServer, realm: &str, parcels: &[i32]) -> SceneListenerState {
    SceneListenerState::from_parcels(
        [(realm.to_string(), parcels.iter().copied().collect())]
            .into_iter()
            .collect(),
        &srv.encoder,
        &SceneListenerCellMapper::new(&srv.grid, &srv.encoder),
    )
}

fn listener(srv: &mut PulseServer, peer: u32, wallet: &str, realm: &str, parcels: &[i32]) {
    let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
    st.wallet_id = Some(wallet.into());
    st.scene_listener = Some(Arc::new(single_realm(srv, realm, parcels)));
    srv.peers.insert(peer, st);
    srv.identity.set(peer, wallet.into());
}

fn scene_listener_msg(base: &[u8], aoi: Vec<SceneListenerAoi>, features: u32) -> Vec<u8> {
    let auth_chain = match ClientMessage::decode(base).unwrap().message {
        Some(client_message::Message::Handshake(h)) => h.auth_chain,
        _ => panic!("base must be a player handshake"),
    };
    client_msg(client_message::Message::SceneListenerHandshake(
        SceneListenerHandshakeRequest {
            auth_chain,
            aoi,
            protocol_features: features,
        },
    ))
}

fn one_realm_msg(base: &[u8], realm: &str, rects: Vec<ParcelRect>, features: u32) -> Vec<u8> {
    scene_listener_msg(base, vec![aoi(realm, rects)], features)
}

fn update_msg(aoi: Vec<SceneListenerAoi>) -> Vec<u8> {
    client_msg(client_message::Message::SceneListenerUpdate(
        SceneListenerUpdate { aoi },
    ))
}

fn parcels_of<'a>(srv: &'a PulseServer, peer: u32, realm: &str) -> &'a HashSet<i32> {
    &srv.peers[&peer]
        .scene_listener
        .as_ref()
        .unwrap()
        .parcels_by_realm[realm]
}

fn encoded(srv: &PulseServer, coords: &[(i32, i32)]) -> HashSet<i32> {
    coords
        .iter()
        .map(|&(x, z)| srv.encoder.encode(x, z))
        .collect()
}

async fn handshake_case(max_parcels: usize, aoi: Vec<SceneListenerAoi>) -> Action {
    let mut srv = PulseServer::new();
    srv.max_scene_listener_parcels = max_parcels;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _w, now_ms) = signed_handshake_request().await;
    let bytes = scene_listener_msg(&base, aoi, 0);
    srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0)
}

fn expect_handshake_reject(action: Action) {
    match action {
        Action::Reject { reply, reason } => {
            assert_eq!(reason, DisconnectReason::InvalidHandshakeField);
            assert!(reply.is_none(), "a field reject sends no HandshakeResponse");
        }
        other => panic!("expected Reject(InvalidHandshakeField), got {other:?}"),
    }
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
        one_realm_msg(
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

    let update = update_msg(vec![aoi("realm-a", vec![rect(0, 0, 0, 0)])]);
    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, &update, 0, 0),
        Action::Applied
    );
    assert_eq!(
        srv.scene_listener_forbidden_drops, expected,
        "the listener's own AoI is the one thing it may change"
    );
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
    let bytes = one_realm_msg(&base, "realm-a", vec![rect(0, 0, 1, 1)], 0);

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
            assert_eq!(listener.realm_count(), 1);
            assert_eq!(
                listener.parcels_by_realm["realm-a"],
                encoded(&srv, &[(0, 0), (1, 0), (0, 1), (1, 1)]),
                "2x2 rect expands to 4 parcels"
            );
            assert_eq!(listener.parcel_count(), 4);
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
    let bytes = one_realm_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0xFFFF_FFFF);

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
async fn scene_listener_handshake_expands_per_realm() {
    let srv = PulseServer::new();
    let action = handshake_case(
        DEFAULT_SCENE_LISTENER_MAX_PARCELS,
        vec![
            aoi("world-a", vec![rect(0, 0, 0, 0)]),
            aoi("world-b", vec![rect(0, 0, 1, 0)]),
        ],
    )
    .await;
    match action {
        Action::AuthenticatedListener { listener, .. } => {
            assert_eq!(listener.realm_count(), 2);
            assert_eq!(
                listener.parcels_by_realm["world-a"],
                encoded(&srv, &[(0, 0)])
            );
            assert_eq!(
                listener.parcels_by_realm["world-b"],
                encoded(&srv, &[(0, 0), (1, 0)])
            );
            assert_eq!(listener.parcel_count(), 3);
            assert!(listener.observes(Some("world-a"), srv.encoder.encode(0, 0)));
            assert!(listener.observes(Some("world-b"), srv.encoder.encode(1, 0)));
            assert!(
                !listener.observes(Some("world-a"), srv.encoder.encode(1, 0)),
                "a parcel announced for one realm is not observed in another"
            );
            assert!(!listener.observes(None, srv.encoder.encode(0, 0)));
        }
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
}

#[tokio::test]
async fn scene_listener_announcement_covers_cells_not_parcels() {
    let mut srv = PulseServer::new();
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _w, now_ms) = signed_handshake_request().await;
    let bytes = one_realm_msg(&base, "main", vec![rect(0, 0, 63, 62)], 0);
    let listener = match srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0) {
        Action::AuthenticatedListener { listener, .. } => listener,
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    };
    assert_eq!(listener.parcel_count(), 4032);
    assert!(
        listener.parcel_count() + SCENE_LISTENER_REALM_BUDGET_COST
            <= DEFAULT_SCENE_LISTENER_MAX_PARCELS
    );
    assert_eq!(listener.cell_count(), 121);
    let mut expected = HashSet::new();
    srv.cell_mapper
        .add_covering_cells(&mut expected, 0, 0, 63, 62);
    assert_eq!(
        listener.cell_keys().iter().copied().collect::<HashSet<_>>(),
        expected,
        "the descriptor carries exactly the mapper's cover of the announced rect"
    );

    let mut st = PeerState::new(PeerConnectionState::Authenticated, 0);
    st.scene_listener = Some(listener);
    srv.peers.insert(1, st);
    srv.gameplay_limiter = GameplayRateLimiter::new(20, 16, 5, DEFAULT_DISCRETE_BURST);
    assert_eq!(
        srv.dispatch(
            1,
            channel::RELIABLE,
            &update_msg(vec![aoi("main", vec![rect(5, 5, 5, 5)])]),
            0,
            1000
        ),
        Action::Applied
    );
    let updated = srv.peers[&1].scene_listener.as_ref().unwrap();
    assert_eq!(updated.parcel_count(), 1);
    assert_eq!(
        updated.cell_count(),
        1,
        "parcel (5,5) spans world [80,96)^2, inside one 100-unit cell"
    );
}

#[tokio::test]
async fn scene_listener_handshake_budget_spans_realms_and_charges_each_realm() {
    assert!(
        matches!(
            handshake_case(16, vec![aoi("a", vec![rect(0, 0, 2, 3)])]).await,
            Action::AuthenticatedListener { .. }
        ),
        "one realm of 12 parcels spends exactly the cap"
    );
    expect_handshake_reject(handshake_case(16, vec![aoi("a", vec![rect(0, 0, 3, 3)])]).await);

    assert!(
        matches!(
            handshake_case(
                16,
                vec![
                    aoi("a", vec![rect(10, 10, 11, 11)]),
                    aoi("b", vec![rect(10, 10, 11, 11)]),
                ]
            )
            .await,
            Action::AuthenticatedListener { .. }
        ),
        "two realms of 2x2 spend 2 x (4 + 4) = 16"
    );
    expect_handshake_reject(
        handshake_case(
            16,
            vec![
                aoi("a", vec![rect(10, 10, 11, 11)]),
                aoi("b", vec![rect(10, 10, 11, 11)]),
                aoi("c", vec![rect(20, 20, 20, 20)]),
            ],
        )
        .await,
    );

    assert!(
        matches!(
            handshake_case(
                16,
                vec![
                    aoi("a", vec![rect(0, 0, 0, 0)]),
                    aoi("b", vec![rect(1, 0, 1, 0)]),
                    aoi("c", vec![rect(2, 0, 2, 0)]),
                ]
            )
            .await,
            Action::AuthenticatedListener { .. }
        ),
        "three single-parcel realms spend 3 x (4 + 1) = 15"
    );
    expect_handshake_reject(
        handshake_case(
            16,
            vec![
                aoi("a", vec![rect(0, 0, 0, 0)]),
                aoi("b", vec![rect(1, 0, 1, 0)]),
                aoi("c", vec![rect(2, 0, 2, 0)]),
                aoi("d", vec![rect(3, 0, 3, 0)]),
            ],
        )
        .await,
    );

    match handshake_case(
        16,
        vec![aoi("a", vec![rect(10, 10, 11, 11), rect(10, 10, 11, 11)])],
    )
    .await
    {
        Action::AuthenticatedListener { listener, .. } => {
            assert_eq!(
                listener.parcel_count(),
                4,
                "overlapping rects are budgeted by sum (4 + 8 = 12) but expanded as a union"
            );
        }
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
}

#[tokio::test]
async fn scene_listener_handshake_field_rejects() {
    let cases: Vec<Vec<SceneListenerAoi>> = vec![
        vec![],
        vec![aoi("realm-a", vec![])],
        vec![aoi("", vec![rect(0, 0, 0, 0)])],
        vec![aoi("realm-a", vec![rect(1, 0, 0, 0)])],
        vec![aoi("realm-a", vec![rect(9999, 0, 9999, 0)])],
        vec![aoi("realm-a", vec![rect(i32::MIN, 0, i32::MIN, 0)])],
        vec![
            aoi("realm-a", vec![rect(0, 0, 0, 0)]),
            aoi("realm-a", vec![rect(1, 1, 1, 1)]),
        ],
    ];
    for case in cases {
        expect_handshake_reject(handshake_case(DEFAULT_SCENE_LISTENER_MAX_PARCELS, case).await);
    }
}

#[tokio::test]
async fn scene_listener_handshake_rejects_overlong_realm() {
    let mut srv = PulseServer::new();
    srv.max_realm_length = 4;
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let (base, _w, now_ms) = signed_handshake_request().await;
    let bytes = one_realm_msg(&base, "abcde", vec![rect(0, 0, 0, 0)], 0);
    expect_handshake_reject(srv.dispatch(1, channel::RELIABLE, &bytes, now_ms, 0));
    assert!(!srv.is_authenticated(1));
}

#[tokio::test]
async fn scene_listener_handshake_duplicate_wallet_evicts() {
    let mut srv = PulseServer::new();
    let (base, wallet, now_ms) = signed_handshake_request().await;
    authed(&mut srv, 3, &wallet);
    srv.peers
        .insert(1, PeerState::new(PeerConnectionState::PendingAuth, 0));
    let bytes = one_realm_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0);

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
    let bytes = one_realm_msg(&base, "realm-a", vec![rect(0, 0, 0, 0)], 0);

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

#[test]
fn scene_listener_update_replaces_parcel_set_in_place() {
    let mut srv = PulseServer::new();
    let origin = srv.encoder.encode(0, 0);
    listener(&mut srv, 7, "0xabc", "realm-a", &[origin]);

    let update = update_msg(vec![aoi("realm-a", vec![rect(5, 5, 6, 6)])]);
    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, &update, 0, 1000),
        Action::Applied
    );
    assert_eq!(
        parcels_of(&srv, 7, "realm-a"),
        &encoded(&srv, &[(5, 5), (6, 5), (5, 6), (6, 6)])
    );
    assert_eq!(
        srv.peers[&7]
            .scene_listener
            .as_ref()
            .unwrap()
            .parcel_count(),
        4
    );
    assert_eq!(
        srv.peers[&7].connection_state,
        PeerConnectionState::Authenticated,
        "the connection and identity stay; only the descriptor is swapped"
    );
    assert_eq!(srv.identity.wallet_by_peer(7), Some("0xabc"));

    let narrower = update_msg(vec![aoi("realm-a", vec![rect(5, 5, 5, 5)])]);
    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, &narrower, 0, 1000),
        Action::Applied
    );
    assert_eq!(
        parcels_of(&srv, 7, "realm-a"),
        &encoded(&srv, &[(5, 5)]),
        "an update replaces the set rather than extending it"
    );
}

#[test]
fn scene_listener_update_replaces_the_realm_set() {
    let mut srv = PulseServer::new();
    let origin = srv.encoder.encode(0, 0);
    listener(&mut srv, 7, "0xabc", "realm-a", &[origin]);

    let update = update_msg(vec![aoi("world-b", vec![rect(5, 5, 5, 5)])]);
    assert_eq!(
        srv.dispatch(7, channel::RELIABLE, &update, 0, 1000),
        Action::Applied
    );
    let state = srv.peers[&7].scene_listener.as_ref().unwrap();
    assert_eq!(
        state.parcels_by_realm.keys().collect::<Vec<_>>(),
        vec!["world-b"],
        "realms absent from the update are no longer observed"
    );
    assert!(state.observes(Some("world-b"), srv.encoder.encode(5, 5)));
    assert!(!state.observes(Some("realm-a"), srv.encoder.encode(0, 0)));
}

#[test]
fn scene_listener_update_invalid_disconnects_and_leaves_aoi_in_force() {
    let cases: Vec<Vec<SceneListenerAoi>> = vec![
        vec![],
        vec![aoi("realm-a", vec![])],
        vec![aoi("", vec![rect(5, 5, 5, 5)])],
        vec![aoi("realm-a", vec![rect(6, 6, 5, 5)])],
        vec![aoi("realm-a", vec![rect(0, 0, 100, 100)])],
        vec![aoi("realm-a", vec![rect(i32::MIN, 0, i32::MIN, 0)])],
        vec![
            aoi("realm-a", vec![rect(0, 0, 0, 0)]),
            aoi("realm-a", vec![rect(1, 1, 1, 1)]),
        ],
    ];
    for case in cases {
        let mut srv = PulseServer::new();
        srv.max_scene_listener_parcels = 16;
        let origin = srv.encoder.encode(0, 0);
        listener(&mut srv, 7, "0xabc", "realm-a", &[origin]);
        let before = srv.peers[&7].scene_listener.clone();

        assert_eq!(
            srv.dispatch(7, channel::RELIABLE, &update_msg(case.clone()), 0, 1000),
            Action::Reject {
                reply: None,
                reason: DisconnectReason::InvalidSceneListenerField,
            },
            "case {case:?}"
        );
        assert_eq!(
            srv.peers[&7].scene_listener, before,
            "the AoI in force when the bad update arrived is left untouched"
        );
        assert_eq!(
            srv.peers[&7].connection_state,
            PeerConnectionState::PendingDisconnect,
            "it is the connection that goes, not the previous announcement"
        );
    }
}

#[test]
fn scene_listener_update_rejects_overlong_realm() {
    let mut srv = PulseServer::new();
    srv.max_realm_length = 4;
    let origin = srv.encoder.encode(0, 0);
    listener(&mut srv, 7, "0xabc", "main", &[origin]);
    assert_eq!(
        srv.dispatch(
            7,
            channel::RELIABLE,
            &update_msg(vec![aoi("abcde", vec![rect(5, 5, 5, 5)])]),
            0,
            1000
        ),
        Action::Reject {
            reply: None,
            reason: DisconnectReason::InvalidSceneListenerField,
        }
    );
    assert_eq!(parcels_of(&srv, 7, "main"), &encoded(&srv, &[(0, 0)]));
}

#[test]
fn scene_listener_update_from_player_is_dropped_before_the_rate_limiter() {
    let mut srv = PulseServer::new();
    authed(&mut srv, 8, "0xplayer");
    let update = update_msg(vec![aoi("realm-a", vec![rect(5, 5, 5, 5)])]);

    let attempts = DEFAULT_DISCRETE_BURST as u64 + 5;
    for _ in 0..attempts {
        assert_eq!(
            srv.dispatch(8, channel::RELIABLE, &update, 0, 1000),
            Action::Ignore
        );
    }
    assert!(
        srv.peers[&8].scene_listener.is_none(),
        "a player never becomes a listener after the handshake"
    );
    assert_eq!(
        srv.peers[&8].connection_state,
        PeerConnectionState::Authenticated,
        "a client bug, not an attack surface: no disconnect"
    );
    assert_eq!(
        srv.scene_listener_forbidden_drops, attempts,
        "counted, in both directions of the role check"
    );

    let teleport = client_msg(client_message::Message::Teleport(teleport_request(
        3, "realm-a",
    )));
    assert_eq!(
        srv.dispatch(8, channel::RELIABLE, &teleport, 0, 1000),
        Action::Applied,
        "dropped ahead of the limiter, the updates spent none of the player's discrete budget"
    );
}

#[test]
fn scene_listener_update_from_unauthenticated_peer_is_ignored() {
    let mut srv = PulseServer::new();
    let origin = srv.encoder.encode(0, 0);
    let mut st = PeerState::new(PeerConnectionState::PendingAuth, 0);
    st.scene_listener = Some(Arc::new(single_realm(&srv, "main", &[origin])));
    srv.peers.insert(7, st);

    assert_eq!(
        srv.dispatch(
            7,
            channel::RELIABLE,
            &update_msg(vec![aoi("main", vec![rect(5, 5, 5, 5)])]),
            0,
            1000
        ),
        Action::Ignore
    );
    assert_eq!(parcels_of(&srv, 7, "main"), &encoded(&srv, &[(0, 0)]));
    assert_eq!(
        srv.scene_listener_forbidden_drops, 0,
        "pre-auth traffic is the auth gate's business, not the role check's"
    );
}

#[test]
fn scene_listener_update_rides_the_discrete_bucket() {
    let mut srv = PulseServer::new();
    srv.gameplay_limiter = GameplayRateLimiter::new(20, 16, 1, 1);
    let origin = srv.encoder.encode(0, 0);
    listener(&mut srv, 7, "0xabc", "main", &[origin]);

    assert_eq!(
        srv.dispatch(
            7,
            channel::RELIABLE,
            &update_msg(vec![aoi("main", vec![rect(5, 5, 5, 5)])]),
            0,
            1000
        ),
        Action::Applied
    );
    assert_eq!(
        srv.dispatch(
            7,
            channel::RELIABLE,
            &update_msg(vec![aoi("main", vec![rect(7, 7, 7, 7)])]),
            0,
            1000
        ),
        Action::Ignore,
        "the second update in the same instant is throttled like any discrete event"
    );
    assert_eq!(
        parcels_of(&srv, 7, "main"),
        &encoded(&srv, &[(5, 5)]),
        "the throttled update never reached the AoI; the accepted one is still in force"
    );
    assert_eq!(
        srv.scene_listener_forbidden_drops, 0,
        "throttling is not a role violation"
    );
}

#[tokio::test]
async fn scene_listener_default_budget_matches_upstream() {
    assert_eq!(
        DEFAULT_SCENE_LISTENER_MAX_PARCELS, 4096,
        "upstream Pulse SceneListenerOptions.MaxParcels"
    );
    let single_realm_max = DEFAULT_SCENE_LISTENER_MAX_PARCELS - SCENE_LISTENER_REALM_BUDGET_COST;
    let full = || rect(0, 0, 43, 92);
    assert_eq!(44 * 93, single_realm_max);

    match handshake_case(
        DEFAULT_SCENE_LISTENER_MAX_PARCELS,
        vec![aoi("a", vec![full()])],
    )
    .await
    {
        Action::AuthenticatedListener { listener, .. } => assert_eq!(
            listener.parcel_count(),
            single_realm_max,
            "one realm of 4092 parcels plus its 4-parcel charge spends the default exactly"
        ),
        other => panic!("expected AuthenticatedListener, got {other:?}"),
    }
    expect_handshake_reject(
        handshake_case(
            DEFAULT_SCENE_LISTENER_MAX_PARCELS,
            vec![aoi("a", vec![full(), rect(50, 50, 50, 50)])],
        )
        .await,
    );
    expect_handshake_reject(
        handshake_case(
            DEFAULT_SCENE_LISTENER_MAX_PARCELS,
            vec![aoi("a", vec![full()]), aoi("b", vec![rect(0, 0, 0, 0)])],
        )
        .await,
    );
}
