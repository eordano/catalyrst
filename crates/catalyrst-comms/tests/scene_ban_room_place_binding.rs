//! The LiveKit side effects of /scene-bans and /scene-admin are aimed at the room
//! named in the CALLER's signed metadata, while authorization is checked against
//! the request's `place_id`. Upstream reads both out of the same metadata
//! (`getWorldScenePlace(realmName, parcel)` / `getPlaceByParcel(parcel)` in
//! scene-bans.ts), so the two must be tied back together or an admin of one place
//! can kick and re-ban inside another scene's room.
//!
//! A places lookup that cannot run is answered apart from a genuine mismatch:
//! upstream distinguishes them too (`PlaceNotFoundError` vs a throwing fetch in
//! adapters/places.ts), and a scene admin must not read a transient DB fault as
//! "your place_id is wrong".
//!
//! DB-gated via the shared scratch cluster; skips cleanly when unset.

use std::sync::Arc;

use catalyrst_comms::livekit::{scene_room_name, world_room_name, world_scene_room_name};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::room_metadata_sync::{resolve_rooms, RoomContext};
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::{json, Value};
use sqlx::PgPool;

const GENESIS_PLACE: &str = "place-genesis";
const WORLD_PLACE: &str = "place-world";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_roombind").await?;
    scratch
        .apply_sql(
            "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, \
             base_position text NOT NULL DEFAULT '0,0');",
        )
        .await;
    seed(&scratch.pool).await;
    Some(scratch)
}

async fn seed(pool: &PgPool) {
    insert_place(
        pool,
        GENESIS_PLACE,
        json!({ "world": false, "positions": ["10,20", "10,21"] }),
        "10,20",
    )
    .await;
    insert_place(
        pool,
        WORLD_PLACE,
        json!({ "world": true, "world_name": "foo.dcl.eth", "positions": ["0,0"] }),
        "0,0",
    )
    .await;
}

async fn insert_place(pool: &PgPool, id: &str, raw: Value, base_position: &str) {
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, $3)")
        .bind(id)
        .bind(raw.to_string())
        .bind(base_position)
        .execute(pool)
        .await
        .expect("insert place");
}

fn test_state(pool: PgPool) -> AppState {
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool: Some(pool.clone()),
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_api_url: "http://127.0.0.1:1".into(),
        livekit_ws_url: "wss://livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: "test-peer".into(),
        world_permissions: Default::default(),
    })
}

fn ctx(place_id: &str, realm: &str, scene_id: &str, parcel: Option<&str>) -> RoomContext {
    let mut metadata = serde_json::Map::new();
    metadata.insert("realmName".into(), json!(realm));
    metadata.insert("sceneId".into(), json!(scene_id));
    if let Some(parcel) = parcel {
        metadata.insert("parcel".into(), json!(parcel));
    }
    RoomContext::from_metadata(&Value::Object(metadata), place_id)
}

#[tokio::test]
async fn a_genesis_mutation_reaches_the_room_of_the_place_it_is_authorized_on() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = test_state(scratch.pool.clone());

    assert_eq!(
        resolve_rooms(
            &state,
            &ctx(GENESIS_PLACE, "main", "bafkreiabc", Some("10,21"))
        )
        .await
        .expect("a caller standing on the place it administers is in bounds"),
        vec![scene_room_name("main", "bafkreiabc")]
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_world_mutation_reaches_the_rooms_of_the_world_it_is_authorized_on() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = test_state(scratch.pool.clone());

    assert_eq!(
        resolve_rooms(
            &state,
            &ctx(WORLD_PLACE, "Foo.dcl.eth", "bafkreiabc", Some("0,0"))
        )
        .await
        .expect("the realm names the world the place record carries"),
        vec![
            world_room_name("Foo.dcl.eth"),
            world_scene_room_name("Foo.dcl.eth", "bafkreiabc")
        ]
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_context_naming_another_scene_is_refused() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = test_state(scratch.pool.clone());

    for (place_id, realm, parcel) in [
        (GENESIS_PLACE, "main", Some("99,99")),
        (GENESIS_PLACE, "main", None),
        (GENESIS_PLACE, "victim.dcl.eth", Some("10,20")),
        (WORLD_PLACE, "victim.dcl.eth", Some("0,0")),
        (WORLD_PLACE, "main", Some("0,0")),
        ("place-missing", "main", Some("10,20")),
    ] {
        let err = resolve_rooms(&state, &ctx(place_id, realm, "bafkreiabc", parcel))
            .await
            .expect_err("a room outside the authorized place must not be reached");
        assert_eq!(err.code, 400);
        assert_eq!(
            err.message,
            "place_id does not match the realm and parcel this request is signed for"
        );
    }

    scratch.drop().await;
}

#[tokio::test]
async fn a_places_lookup_that_cannot_run_is_not_reported_as_a_mismatch() {
    let Some(scratch) = setup().await else {
        return;
    };
    let state = test_state(scratch.pool.clone());
    scratch.pool.close().await;

    let err = resolve_rooms(
        &state,
        &ctx(GENESIS_PLACE, "main", "bafkreiabc", Some("10,21")),
    )
    .await
    .expect_err("a places pool that cannot answer must not read as a disagreeing place");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        "the place catalog is unavailable, so this request cannot be bound to its room"
    );

    scratch.drop().await;
}
