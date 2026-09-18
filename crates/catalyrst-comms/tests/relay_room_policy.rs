mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{routing::get, Json, Router};
use catalyrst_comms::{
    assignment_fence::{FenceError, RealmAssignmentReader, RealmAssignmentSnapshot},
    relay_authority::{policy::CurrentRoomPolicy, DenialReason, RelayPolicy},
};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_pulse::application_relay::auth::{RoomClaims, RoomGrant, VerifiedRoom};
use serde_json::json;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION: &str = "0x2222222222222222222222222222222222222222";

async fn database() -> Option<ScratchSchema> {
    let db = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "relay_policy").await?;
    for sql in [
        include_str!("../migrations/0001_comms.sql"),
        include_str!("../migrations/0002_user_moderation.sql"),
        include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    Some(db)
}

fn room(name: &str) -> VerifiedRoom {
    VerifiedRoom {
        claims: RoomClaims {
            iss: "key".into(),
            sub: WALLET.into(),
            exp: 1,
            nbf: 0,
            video: RoomGrant {
                room: name.into(),
                room_join: true,
                can_publish_data: true,
            },
            metadata: String::new(),
        },
        wallet: WALLET.into(),
        session: SESSION.into(),
        is_guest: false,
        header_payload: String::new(),
        nonce: [1; 32],
        proof: vec![2; 32],
    }
}

#[tokio::test]
async fn scene_renewal_observes_current_scene_platform_and_recorded_device_bans() {
    let Some(db) = database().await else { return };
    let state = support::test_state(db.pool.clone(), None, None, "unused");
    let policy = CurrentRoomPolicy::new(state, None).unwrap();
    let room = room("scene:main:scene-a");
    assert_eq!(policy.authorize(&room).await, Ok(()));
    sqlx::query(
        "INSERT INTO scene_bans(place_id,banned_address,banned_by) VALUES ('scene-b',$1,'admin')",
    )
    .bind(WALLET)
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(policy.authorize(&room).await, Ok(()));
    sqlx::query("UPDATE scene_bans SET place_id='scene-a'")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        policy.authorize(&room).await,
        Err(DenialReason::NotAuthorized)
    );
    sqlx::query("DELETE FROM scene_bans")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_bans(banned_address,banned_by,reason) VALUES ($1,'admin','test')",
    )
    .bind(WALLET)
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        policy.authorize(&room).await,
        Err(DenialReason::NotAuthorized)
    );
    sqlx::query("UPDATE user_bans SET banned_address='other-wallet',banned_device_id='device-a'")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO player_connection_info(address,device_id,created_at,updated_at) VALUES ($1,'device-a',0,0)")
        .bind(WALLET).execute(&db.pool).await.unwrap();
    assert_eq!(
        policy.authorize(&room).await,
        Err(DenialReason::NotAuthorized)
    );
    sqlx::query("UPDATE user_bans SET lifted_at=now()")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(policy.authorize(&room).await, Ok(()));
    db.pool.close().await;
    assert_eq!(
        policy.authorize(&room).await,
        Err(DenialReason::Unavailable)
    );
    db.drop().await;
}

struct Assignments(Mutex<Result<RealmAssignmentSnapshot, FenceError>>);

#[async_trait]
impl RealmAssignmentReader for Assignments {
    async fn current_realm_assignment(
        &self,
        _: &str,
        _: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError> {
        self.0.lock().unwrap().clone()
    }
}

fn snapshot() -> RealmAssignmentSnapshot {
    RealmAssignmentSnapshot {
        audience: "realm-a".into(),
        wallet: WALLET.into(),
        lane: "realm".into(),
        authority_incarnation: "authority-a".into(),
        owner_session: SESSION.into(),
        owner_epoch: 4,
        assignment_revision: 5,
        fencing_token: 6,
        island_id: "island-123".into(),
    }
}

fn island_room() -> VerifiedRoom {
    let mut room = room("island-123");
    room.claims.metadata = json!({"catalyrstIsland":{
        "version":2,"wallet":WALLET,"session":SESSION,"audience":"realm-a","lane":"realm",
        "ownerEpoch":4,"assignmentRevision":5,"fencingToken":6
    }})
    .to_string();
    room
}

#[tokio::test]
async fn island_renewal_matches_every_current_authority_field_and_denies_missing_authority() {
    let Some(db) = database().await else { return };
    let state = support::test_state(db.pool.clone(), None, None, "unused");
    let reader = Arc::new(Assignments(Mutex::new(Ok(snapshot()))));
    let policy = CurrentRoomPolicy::new(state.clone(), Some(reader.clone())).unwrap();
    let room = island_room();
    assert_eq!(policy.authorize(&room).await, Ok(()));
    for change in 0..8 {
        let mut changed = snapshot();
        match change {
            0 => changed.audience.push('x'),
            1 => changed.wallet.push('x'),
            2 => changed.lane = "scene".into(),
            3 => changed.owner_session.push('x'),
            4 => changed.owner_epoch += 1,
            5 => changed.assignment_revision += 1,
            6 => changed.fencing_token += 1,
            _ => changed.island_id.push('x'),
        }
        *reader.0.lock().unwrap() = Ok(changed);
        assert_eq!(
            policy.authorize(&room).await,
            Err(DenialReason::NotAuthorized)
        );
    }
    for error in [
        FenceError::Unowned,
        FenceError::Conflict,
        FenceError::Invalid,
        FenceError::Unavailable,
    ] {
        *reader.0.lock().unwrap() = Err(error);
        assert!(policy.authorize(&room).await.is_err());
    }
    assert_eq!(
        CurrentRoomPolicy::new(state, None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::Unavailable)
    );
    *reader.0.lock().unwrap() = Ok(snapshot());
    let mut legacy = room.clone();
    legacy.claims.metadata =
        json!({"catalyrstIsland":{"version":1,"wallet":WALLET,"session":SESSION}}).to_string();
    assert_eq!(
        policy.authorize(&legacy).await,
        Err(DenialReason::NotAuthorized)
    );
    legacy.claims.metadata.clear();
    assert_eq!(
        policy.authorize(&legacy).await,
        Err(DenialReason::NotAuthorized)
    );
    db.drop().await;
}

#[tokio::test]
async fn authoritative_server_subject_requires_signed_current_authority_metadata() {
    let Some(db) = database().await else { return };
    let mut state = support::test_state(db.pool.clone(), None, None, "unused");
    let mut room = room("scene:main:scene-a");
    room.claims.sub = "authoritative-server".into();
    assert_eq!(
        CurrentRoomPolicy::new(state.clone(), None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::NotAuthorized)
    );
    Arc::get_mut(&mut state)
        .unwrap()
        .authoritative_server_address = Some(SESSION.into());
    assert_eq!(
        CurrentRoomPolicy::new(state.clone(), None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::NotAuthorized)
    );
    room.claims.metadata = json!({"catalyrstAuthoritativeServer":{"wallet":SESSION}}).to_string();
    assert_eq!(
        CurrentRoomPolicy::new(state.clone(), None)
            .unwrap()
            .authorize(&room)
            .await,
        Ok(())
    );
    Arc::get_mut(&mut state)
        .unwrap()
        .authoritative_server_address = Some(WALLET.into());
    assert_eq!(
        CurrentRoomPolicy::new(state.clone(), None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::NotAuthorized)
    );
    Arc::get_mut(&mut state)
        .unwrap()
        .authoritative_server_address = Some(SESSION.into());
    room.claims.video.room = "island-123".into();
    assert_eq!(
        CurrentRoomPolicy::new(state.clone(), None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::NotAuthorized)
    );
    room.claims.video.room = "scene:main:scene-a".into();
    room.claims.sub = WALLET.into();
    assert_eq!(
        CurrentRoomPolicy::new(state, None)
            .unwrap()
            .authorize(&room)
            .await,
        Err(DenialReason::NotAuthorized)
    );
    db.drop().await;
}

#[tokio::test]
async fn world_renewal_reads_uncached_permissions_and_fails_closed_on_acl_loss() {
    let Some(db) = database().await else { return };
    let permissions = Arc::new(Mutex::new(
        json!({"permissions":{"access":{"type":"unrestricted"}}}),
    ));
    let current = permissions.clone();
    let app = Router::new()
        .route(
            "/world/example.eth/permissions",
            get(move || {
                let body = current.lock().unwrap().clone();
                async { Json(body) }
            }),
        )
        .route(
            "/world/example.eth/about",
            get(|| async {
                Json(json!({"configurations":{"scenesUrn":["urn:decentraland:entity:scene-a"]}}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut state = support::test_state(db.pool.clone(), None, None, "unused");
    Arc::get_mut(&mut state).unwrap().world_content_url = format!("http://{addr}");
    let policy = CurrentRoomPolicy::new(state, None).unwrap();
    for grant in ["world-example.eth-scene-a", "world-example.eth"] {
        let room = room(grant);
        assert_eq!(policy.authorize(&room).await, Ok(()));
        *permissions.lock().unwrap() =
            json!({"permissions":{"access":{"type":"allow-list","wallets":[]}}});
        assert_eq!(
            policy.authorize(&room).await,
            Err(DenialReason::NotAuthorized)
        );
        *permissions.lock().unwrap() =
            json!({"permissions":{"access":{"type":"allow-list","wallets":[WALLET]}}});
        assert_eq!(policy.authorize(&room).await, Ok(()));
        sqlx::query("INSERT INTO scene_bans(place_id,banned_address,banned_by) VALUES ('scene-a',$1,'admin')")
            .bind(WALLET).execute(&db.pool).await.unwrap();
        assert_eq!(
            policy.authorize(&room).await,
            Err(DenialReason::NotAuthorized)
        );
        sqlx::query("DELETE FROM scene_bans")
            .execute(&db.pool)
            .await
            .unwrap();
    }
    serving.abort();
    let _ = serving.await;
    assert_eq!(
        policy.authorize(&room("world-example.eth-scene-a")).await,
        Err(DenialReason::Unavailable)
    );
    db.drop().await;
}
