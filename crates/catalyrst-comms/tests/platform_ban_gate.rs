//! Upstream answers every LiveKit credential request through one platform-access
//! gate: a connection ban lookup that widens to the device recorded for the
//! address when the caller sends none (`getActiveBanForConnection` in
//! logic/user-moderation), and an `assertNoActivePlatformBan` in front of the
//! voice credentials (logic/voice), which carry no signed metadata at all. Without
//! the widening a wallet switch on a banned device buys back voice and private
//! messages; without the assertion a platform ban never reaches a voice room.
//!
//! DB-gated via the shared scratch cluster (`CATALYRST_COMMS_TEST_PG` /
//! `CATALYRST_TEST_PG`); skips cleanly when unset.

mod support;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::Method;
use axum::Json;
use catalyrst_comms::access_gate::is_connection_banned;
use catalyrst_comms::handlers::deferred::scene_stream_access;
use catalyrst_comms::handlers::voice::{
    community_voice_chat_create_or_join, CommunityVoiceChatBody,
};
use catalyrst_comms::ports::player_connection::UpsertPlayerConnection;
use catalyrst_comms::ports::user_bans::CreateBan;
use catalyrst_comms::voice_logic::get_private_voice_chat_room_credentials;
use catalyrst_comms::AppState;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_crypto::Wallet;
use serde_json::json;

const BANNED_WALLET: &str = "0x1111111111111111111111111111111111111111";
const SAME_DEVICE_WALLET: &str = "0x2222222222222222222222222222222222222222";
const CLEAN_WALLET: &str = "0x3333333333333333333333333333333333333333";
const MODERATOR: &str = "0x9999999999999999999999999999999999999999";
const DEVICE: &str = "shared-device-42";

const PLATFORM_BANNED_MSG: &str = "Access denied, platform-banned user";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_banage").await?;
    scratch
        .apply_sql(include_str!("../migrations/0001_comms.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0002_user_moderation.sql"))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0006_player_connection_and_device_bans.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0007_community_voice_chat_sid.sql"
        ))
        .await;
    Some(scratch)
}

/// A device ban on `BANNED_WALLET`, and `SAME_DEVICE_WALLET` recorded on that same
/// device by an earlier scene-adapter connection.
async fn seed(state: &AppState) {
    state
        .user_bans
        .create_ban(CreateBan {
            banned_address: BANNED_WALLET.into(),
            banned_by: MODERATOR.into(),
            reason: "abuse".into(),
            custom_message: None,
            banned_device_id: Some(DEVICE.into()),
            duration_ms: None,
        })
        .await
        .expect("create_ban");

    state
        .player_connection
        .upsert(UpsertPlayerConnection {
            address: SAME_DEVICE_WALLET.into(),
            ip_address: None,
            device_id: Some(DEVICE.into()),
        })
        .await
        .expect("upsert connection info");
}

fn community_body(address: &str) -> CommunityVoiceChatBody {
    serde_json::from_value(json!({
        "community_id": "community-1",
        "user_address": address,
        "user_role": "member",
        "action": "join",
    }))
    .expect("community voice chat body")
}

#[tokio::test]
async fn a_lookup_without_a_device_id_is_widened_to_the_recorded_device() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = support::test_state(scratch.pool.clone(), None, None, "squid_marketplace");
    seed(&state).await;

    assert!(
        is_connection_banned(&state, SAME_DEVICE_WALLET, None)
            .await
            .unwrap(),
        "a route that sends no device id must still match the device recorded for the wallet"
    );
    assert!(
        is_connection_banned(&state, BANNED_WALLET, None)
            .await
            .unwrap(),
        "the banned wallet itself stays banned with no device id"
    );
    assert!(
        !is_connection_banned(&state, CLEAN_WALLET, None)
            .await
            .unwrap(),
        "a wallet with no recorded connection must not be widened into a ban"
    );
    assert!(
        !is_connection_banned(&state, SAME_DEVICE_WALLET, Some("some-other-device"))
            .await
            .unwrap(),
        "a device id the caller does send is the one that is matched"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn private_voice_credentials_are_refused_when_a_participant_is_platform_banned() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = support::test_state(scratch.pool.clone(), None, None, "squid_marketplace");
    seed(&state).await;

    let err = get_private_voice_chat_room_credentials(
        &state,
        "room-1",
        &[CLEAN_WALLET.to_string(), SAME_DEVICE_WALLET.to_string()],
    )
    .await
    .expect_err("either side banned refuses the call");
    assert_eq!(err.code, 403);
    assert_eq!(err.message, PLATFORM_BANNED_MSG);

    let rooms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voice_chat_users")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(rooms, 0, "the refused call must not open the room");

    get_private_voice_chat_room_credentials(
        &state,
        "room-2",
        &[CLEAN_WALLET.to_string(), MODERATOR.to_string()],
    )
    .await
    .expect("two unbanned participants still get credentials");

    scratch.drop().await;
}

#[tokio::test]
async fn community_voice_join_is_refused_when_the_named_wallet_is_platform_banned() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = support::test_state(scratch.pool.clone(), None, None, "squid_marketplace");
    seed(&state).await;

    let err = community_voice_chat_create_or_join(
        State(state.clone()),
        Json(community_body(SAME_DEVICE_WALLET)),
    )
    .await
    .expect_err("a community role never outranks a platform ban");
    assert_eq!(err.code, 403);
    assert_eq!(err.message, PLATFORM_BANNED_MSG);

    let joined: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM community_voice_chat_users")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(joined, 0, "the refused join must not reach the room roster");

    let joined_ok = community_voice_chat_create_or_join(
        State(state.clone()),
        Json(community_body(CLEAN_WALLET)),
    )
    .await
    .expect("an unbanned wallet still joins");
    assert!(joined_ok.0["connection_url"].is_string());

    scratch.drop().await;
}

const STREAM_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
const STREAM_PLACE: &str = "place-stream";
const STREAM_REALM: &str = "LocalPreview";
const STREAM_PARCEL: &str = "10,20";

fn stream_metadata() -> String {
    json!({
        "signer": "decentraland-kernel-scene",
        "realmName": STREAM_REALM,
        "parcel": STREAM_PARCEL,
        "sceneId": "bafkreiStreamScene",
    })
    .to_string()
}

async fn stream_access_error(
    state: &AppState,
    wallet: &Wallet,
    method: Method,
) -> catalyrst_comms::http::ApiError {
    let metadata = stream_metadata();
    let headers = support::signed_headers_with_metadata(
        wallet,
        method.as_str().to_lowercase().as_str(),
        "/scene-stream-access",
        &metadata,
    );
    match scene_stream_access(State(state.clone()), method, headers, Bytes::new()).await {
        Ok(_) => panic!("the handler must not answer this request with a body"),
        Err(err) => err,
    }
}

/// Upstream gates add / list / reset on the platform ban BEFORE the scene
/// owner-or-admin check, because the streaming key they hand back is honoured by
/// `validateStreamerToken` without re-checking the wallet: a ban that lands after
/// the mint never reaches the stream. `remove` is deliberately left ungated.
#[tokio::test]
async fn stream_keys_are_refused_to_a_platform_banned_scene_admin() {
    let Some(scratch) = setup().await else {
        return;
    };
    scratch
        .apply_sql(
            "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, \
             base_position text NOT NULL DEFAULT '0,0');",
        )
        .await;
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, $3)")
        .bind(STREAM_PLACE)
        .bind(json!({ "world": false, "positions": [STREAM_PARCEL] }).to_string())
        .bind(STREAM_PARCEL)
        .execute(&scratch.pool)
        .await
        .expect("insert place");

    let state = support::test_state(
        scratch.pool.clone(),
        Some(scratch.pool.clone()),
        None,
        "squid_marketplace",
    );
    seed(&state).await;

    let wallet = Wallet::from_hex(STREAM_KEY).expect("stream wallet");
    let admin = wallet.address().to_lowercase();
    state
        .scene_admin
        .add(STREAM_PLACE, &admin, MODERATOR)
        .await
        .expect("seed the banned wallet as a scene admin");
    state
        .player_connection
        .upsert(UpsertPlayerConnection {
            address: admin.clone(),
            ip_address: None,
            device_id: Some(DEVICE.into()),
        })
        .await
        .expect("record the admin on the banned device");

    for method in [Method::GET, Method::POST, Method::PUT] {
        let err = stream_access_error(&state, &wallet, method.clone()).await;
        assert_eq!(err.code, 403, "{method} must be refused by the ban gate");
        assert_eq!(err.message, PLATFORM_BANNED_MSG);
    }

    let minted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scene_stream_access")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(minted, 0, "a refused request must not mint a streaming key");

    let err = stream_access_error(&state, &wallet, Method::DELETE).await;
    assert_ne!(
        err.message, PLATFORM_BANNED_MSG,
        "upstream leaves remove-scene-stream-access ungated"
    );
    assert_eq!(err.code, 404);

    scratch.drop().await;
}
