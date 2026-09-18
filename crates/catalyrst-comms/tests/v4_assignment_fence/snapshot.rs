use std::time::Duration;

use catalyrst_comms::assignment_fence::{FenceError, PgAssignmentFence, RealmAssignmentReader};
use serde_json::json;
use sqlx::Row;

use super::{row_state, seed_realm, Fixture, AUDIENCE, SESSION, WALLET};

fn stored_assignment(island_id: &str) -> serde_json::Value {
    json!({
        "islandId": island_id,
        "connectionString": format!("livekit:wss://example.invalid/{island_id}"),
        "fromIslandId": "island-previous",
        "tokenExpiresAtUnix": 1,
        "peers": {}
    })
}

async fn replace_owner(
    fixture: &Fixture,
    session: &str,
    epoch: i64,
    revision: i64,
    fencing_token: i64,
    island_id: &str,
) {
    sqlx::query(
        "UPDATE archipelago_v4_assignments
         SET authority_incarnation = 'authority-new-incarnation',
             owner_session = $3,
             owner_epoch = $4,
             assignment_revision = $5,
             fencing_token = $6,
             assignment_json = $7
         WHERE deployment_audience = $1
           AND owner_address = $2
           AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .bind(session)
    .bind(epoch)
    .bind(revision)
    .bind(fencing_token)
    .bind(stored_assignment(island_id))
    .execute(&fixture.pool)
    .await
    .expect("replace authority owner");
}

#[tokio::test]
async fn snapshot_returns_the_current_database_identity_without_mutating_revision() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let assignment = stored_assignment("island-current");
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        7,
        11,
        13,
        Some(assignment.clone()),
    )
    .await;

    let snapshot = fixture
        .fence()
        .await
        .current_realm_assignment(WALLET, SESSION)
        .await
        .expect("read current assignment");
    assert_eq!(snapshot.audience, AUDIENCE);
    assert_eq!(snapshot.wallet, WALLET);
    assert_eq!(snapshot.lane, "realm");
    assert_eq!(snapshot.authority_incarnation, "authority-test-incarnation");
    assert_eq!(snapshot.owner_session, SESSION);
    assert_eq!(snapshot.owner_epoch, 7);
    assert_eq!(snapshot.assignment_revision, 11);
    assert_eq!(snapshot.fencing_token, 13);
    assert_eq!(snapshot.island_id, "island-current");
    assert_eq!(row_state(&fixture.pool).await, (11, 13, Some(assignment)));

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_follows_a_newer_owner_and_room_while_rejecting_the_old_session() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        2,
        3,
        5,
        Some(stored_assignment("island-old")),
    )
    .await;
    let fence = fixture.fence().await;
    let first = fence
        .current_realm_assignment(WALLET, SESSION)
        .await
        .expect("first assignment");
    assert_eq!(first.island_id, "island-old");

    let newer_session = "session-newer";
    replace_owner(&fixture, newer_session, 8, 9, 10, "island-new").await;
    assert_eq!(
        fence.current_realm_assignment(WALLET, SESSION).await,
        Err(FenceError::Conflict)
    );
    let current = fence
        .current_realm_assignment(WALLET, newer_session)
        .await
        .expect("new owner assignment");
    assert_eq!(current.authority_incarnation, "authority-new-incarnation");
    assert_eq!(current.owner_epoch, 8);
    assert_eq!(current.assignment_revision, 9);
    assert_eq!(current.fencing_token, 10);
    assert_eq!(current.island_id, "island-new");

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_distinguishes_unowned_rows_from_session_conflicts() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        2,
        3,
        5,
        Some(stored_assignment("island-current")),
    )
    .await;
    let fence = fixture.fence().await;
    assert_eq!(
        fence
            .current_realm_assignment(WALLET, "session-stale")
            .await,
        Err(FenceError::Conflict)
    );
    assert_eq!(
        fence
            .current_realm_assignment("0x00000000000000000000000000000000000000f2", SESSION,)
            .await,
        Err(FenceError::Unowned)
    );
    let other_audience = PgAssignmentFence::connect(&fixture.url, "other-audience".into())
        .await
        .expect("connect reader for another audience");
    assert_eq!(
        other_audience
            .current_realm_assignment(WALLET, SESSION)
            .await,
        Err(FenceError::Unowned)
    );

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_rejects_null_malformed_and_oversized_assignments() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;
    let fence = fixture.fence().await;
    assert_eq!(
        fence.current_realm_assignment(WALLET, SESSION).await,
        Err(FenceError::Invalid)
    );

    for malformed in [
        json!({"islandId": "island-a"}),
        json!({"islandId": 7, "connectionString": "livekit:wss://example.invalid"}),
        json!({"islandId": "", "connectionString": "livekit:wss://example.invalid"}),
        json!({
            "islandId": "island-a",
            "connectionString": "livekit:wss://example.invalid",
            "unexpected": true
        }),
    ] {
        sqlx::query(
            "UPDATE archipelago_v4_assignments SET assignment_json = $3
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .bind(malformed)
        .execute(&fixture.pool)
        .await
        .expect("write malformed assignment");
        assert_eq!(
            fence.current_realm_assignment(WALLET, SESSION).await,
            Err(FenceError::Invalid)
        );
    }

    let oversized = json!({
        "islandId": "island-a",
        "connectionString": "x".repeat(64 * 1024),
    });
    sqlx::query(
        "UPDATE archipelago_v4_assignments SET assignment_json = $3
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .bind(oversized)
    .execute(&fixture.pool)
    .await
    .expect("write oversized assignment");
    assert_eq!(
        fence.current_realm_assignment(WALLET, SESSION).await,
        Err(FenceError::Invalid)
    );

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_rejects_nonpositive_owner_generation_fields() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        2,
        3,
        5,
        Some(stored_assignment("island-current")),
    )
    .await;
    let fence = fixture.fence().await;

    for (column, value) in [
        ("owner_epoch", 0_i64),
        ("assignment_revision", 0_i64),
        ("fencing_token", 0_i64),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE archipelago_v4_assignments SET {column} = $3
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'"
        )))
        .bind(AUDIENCE)
        .bind(WALLET)
        .bind(value)
        .execute(&fixture.pool)
        .await
        .expect("write invalid owner generation");
        assert_eq!(
            fence.current_realm_assignment(WALLET, SESSION).await,
            Err(FenceError::Invalid),
            "{column} must be positive"
        );
        replace_owner(&fixture, SESSION, 2, 3, 5, "island-current").await;
    }

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_denies_lapsed_and_explicitly_released_owners() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        2,
        3,
        5,
        Some(stored_assignment("island-current")),
    )
    .await;
    let fence = fixture.fence().await;

    for timestamp in ["now() - interval '91 seconds'", "'-infinity'::timestamptz"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE archipelago_v4_assignments SET updated_at = {timestamp}
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'"
        )))
        .bind(AUDIENCE)
        .bind(WALLET)
        .execute(&fixture.pool)
        .await
        .expect("age authority owner");
        assert_eq!(
            fence.current_realm_assignment(WALLET, SESSION).await,
            Err(FenceError::Unowned)
        );
    }
    assert_eq!(
        row_state(&fixture.pool).await,
        (3, 5, Some(stored_assignment("island-current")))
    );

    fixture.finish().await;
}

#[tokio::test]
async fn snapshot_is_a_plain_read_and_does_not_wait_for_or_retain_a_write_lock() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        2,
        3,
        5,
        Some(stored_assignment("island-current")),
    )
    .await;

    let mut writer = fixture.pool.begin().await.expect("begin writer");
    sqlx::query(
        "SELECT assignment_revision FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'
         FOR UPDATE",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .fetch_one(&mut *writer)
    .await
    .expect("hold row write lock");

    let fence = fixture.fence().await;
    let snapshot = tokio::time::timeout(
        Duration::from_millis(500),
        fence.current_realm_assignment(WALLET, SESSION),
    )
    .await
    .expect("plain snapshot must not wait for a row write lock")
    .expect("snapshot while writer is open");
    assert_eq!(snapshot.assignment_revision, 3);
    writer.rollback().await.expect("release writer lock");

    let updated = sqlx::query(
        "UPDATE archipelago_v4_assignments SET assignment_revision = 4
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'
         RETURNING assignment_revision",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .fetch_one(&fixture.pool)
    .await
    .expect("snapshot retained no row lock");
    assert_eq!(updated.try_get::<i64, _>("assignment_revision").unwrap(), 4);

    fixture.finish().await;
}
