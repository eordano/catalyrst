use std::str::FromStr;
use std::time::Duration;

use catalyrst_comms::assignment_fence::{
    AssignmentDocument, AssignmentFence, FenceError, PgAssignmentFence, RealmAssignmentReader,
};
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool, Row};

#[path = "v4_assignment_fence/recovery.rs"]
mod recovery;
#[path = "v4_assignment_fence/snapshot.rs"]
mod snapshot;

const AUDIENCE: &str = "comms-v4-assignment-fence-test";
const WALLET: &str = "0x00000000000000000000000000000000000000f1";
const SESSION: &str = "session-current";

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    url: String,
}

impl Fixture {
    async fn new() -> Option<Self> {
        let url = test_pg_url()?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap_or_else(|err| panic!("connect to configured test PostgreSQL: {err}"));
        let schema = format!("comms_v4_fence_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap_or_else(|err| panic!("create isolated schema {schema}: {err}"));
        let options = PgConnectOptions::from_str(&url)
            .unwrap_or_else(|err| panic!("parse configured PostgreSQL URL: {err}"))
            .options([("search_path", schema.as_str())]);
        let scoped_url = url_with_search_path(&url, &schema);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap_or_else(|err| panic!("connect isolated schema {schema}: {err}"));
        create_authority_table(&pool).await;
        Some(Self {
            admin,
            pool,
            schema,
            url: scoped_url,
        })
    }

    async fn finish(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .unwrap_or_else(|err| panic!("drop isolated schema: {err}"));
        self.admin.close().await;
    }

    async fn fence(&self) -> PgAssignmentFence {
        PgAssignmentFence::connect(&self.url, AUDIENCE.to_string())
            .await
            .expect("connect PgAssignmentFence to isolated schema")
    }
}

fn test_pg_url() -> Option<String> {
    std::env::var("CATALYRST_COMMS_TEST_PG")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CATALYRST_ARCHIPELAGO_TEST_PG")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| catalyrst_testgate::require_pg("CATALYRST_COMMS_TEST_PG"))
}

fn url_with_search_path(url: &str, schema: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}options[search_path]={schema}")
}

async fn create_authority_table(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE archipelago_v4_assignments (
            deployment_audience text NOT NULL,
            owner_address text NOT NULL,
            lane_key text NOT NULL,
            authority_incarnation text NOT NULL,
            assignment_revision bigint NOT NULL,
            owner_session text NOT NULL,
            owner_epoch bigint NOT NULL,
            fencing_token bigint NOT NULL,
            assignment_json jsonb,
            acknowledged_revision bigint NOT NULL DEFAULT 0,
            updated_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (deployment_audience, owner_address, lane_key)
        )",
    )
    .execute(pool)
    .await
    .expect("create shared v4 assignment authority table");
}

async fn seed_realm(
    pool: &PgPool,
    wallet: &str,
    session: &str,
    owner_epoch: i64,
    assignment_revision: i64,
    fencing_token: i64,
    assignment_json: Option<serde_json::Value>,
) {
    sqlx::query(
        "INSERT INTO archipelago_v4_assignments (
            deployment_audience, owner_address, lane_key, authority_incarnation,
            assignment_revision, owner_session, owner_epoch, fencing_token, assignment_json
        )
        VALUES ($1, $2, 'realm', 'authority-test-incarnation', $3, $4, $5, $6, $7)",
    )
    .bind(AUDIENCE)
    .bind(wallet)
    .bind(assignment_revision)
    .bind(session)
    .bind(owner_epoch)
    .bind(fencing_token)
    .bind(assignment_json)
    .execute(pool)
    .await
    .expect("seed realm owner row");
}

fn assignment(island_id: &str, from_island_id: Option<&str>) -> AssignmentDocument {
    AssignmentDocument {
        island_id: island_id.to_string(),
        connection_string: format!("livekit:wss://example.invalid/{island_id}"),
        from_island_id: from_island_id.map(str::to_string),
        token_expires_at_unix: None,
        peers: Default::default(),
    }
}

async fn row_state(pool: &PgPool) -> (i64, i64, Option<serde_json::Value>) {
    let row = sqlx::query(
        "SELECT assignment_revision, fencing_token, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(WALLET)
    .fetch_one(pool)
    .await
    .expect("load realm row");
    (
        row.try_get("assignment_revision").unwrap(),
        row.try_get("fencing_token").unwrap(),
        row.try_get("assignment_json").unwrap(),
    )
}

#[tokio::test]
async fn lease_holds_row_lock_until_commit() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 4, 1, 9, None).await;

    let first_fence = fixture.fence().await;
    let second_fence = fixture.fence().await;
    let first = first_fence
        .lock_realm(WALLET, SESSION)
        .await
        .expect("first lease");
    let first_token = first.token_fence();
    assert_eq!(first_token.assignment_revision, 2);
    assert_eq!(first_token.fencing_token, 9);

    let mut waiter = tokio::spawn(async move { second_fence.lock_realm(WALLET, SESSION).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut waiter)
            .await
            .is_err(),
        "second lease must wait on the first lease's row lock"
    );

    first
        .commit(assignment("island-after-first", None))
        .await
        .expect("first commit");
    let second = waiter
        .await
        .expect("second lock task")
        .expect("second lease after first commit");
    let second_token = second.token_fence();
    assert_eq!(second_token.assignment_revision, 3);
    assert_eq!(second_token.fencing_token, 9);
    drop(second);

    let (revision, fence, document) = row_state(&fixture.pool).await;
    assert_eq!(revision, 2);
    assert_eq!(fence, 9);
    assert_eq!(
        document
            .as_ref()
            .and_then(|value| value["islandId"].as_str()),
        Some("island-after-first")
    );

    fixture.finish().await;
}

#[tokio::test]
async fn a_wallet_no_socket_claimed_is_unowned_and_is_told_apart_from_a_stale_session() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;

    let fence = fixture.fence().await;
    let unclaimed = "0x00000000000000000000000000000000000000f2";
    let err = match fence.lock_realm(unclaimed, SESSION).await {
        Ok(_) => panic!("a wallet without a row has no lease to give"),
        Err(err) => err,
    };
    assert_eq!(err, FenceError::Unowned);
    assert_eq!(row_state(&fixture.pool).await, (3, 5, None));

    fixture.finish().await;
}

#[tokio::test]
async fn stale_session_gets_no_lease() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;

    let fence = fixture.fence().await;
    let err = match fence.lock_realm(WALLET, "session-stale").await {
        Ok(_) => panic!("stale session must not receive a lease"),
        Err(err) => err,
    };
    assert_eq!(err, FenceError::Conflict);
    assert_eq!(row_state(&fixture.pool).await, (3, 5, None));

    fixture.finish().await;
}

async fn set_realm_renewal(pool: &PgPool, renewed_at: &str) {
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE archipelago_v4_assignments SET updated_at = {renewed_at}
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'"
    )))
    .bind(AUDIENCE)
    .bind(WALLET)
    .execute(pool)
    .await
    .expect("set realm renewal time");
}

#[tokio::test]
async fn lapsed_and_released_owners_cannot_remint_or_enable_legacy_fallback() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;
    let fence = fixture.fence().await;

    for lapsed in ["now() - interval '10 minutes'", "'-infinity'"] {
        set_realm_renewal(&fixture.pool, lapsed).await;
        let before: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(a) FROM archipelago_v4_assignments a
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .fetch_one(&fixture.pool)
        .await
        .expect("snapshot the complete authority row");
        for session in [SESSION, "session-other"] {
            let err = match fence.lock_realm(WALLET, session).await {
                Ok(_) => panic!("a lapsed or released row must not grant any mutation lease"),
                Err(err) => err,
            };
            assert_eq!(err, FenceError::Conflict);
            assert_eq!(
                fence
                    .current_realm_assignment(WALLET, session)
                    .await
                    .unwrap_err(),
                FenceError::Unowned
            );
            let after: serde_json::Value = sqlx::query_scalar(
                "SELECT to_jsonb(a) FROM archipelago_v4_assignments a
                 WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
            )
            .bind(AUDIENCE)
            .bind(WALLET)
            .fetch_one(&fixture.pool)
            .await
            .expect("read the unchanged authority row");
            assert_eq!(after, before);
        }
    }
    assert_eq!(row_state(&fixture.pool).await, (3, 5, None));

    set_realm_renewal(&fixture.pool, "now() - interval '20 seconds'").await;
    let err = match fence.lock_realm(WALLET, "session-other").await {
        Ok(_) => panic!("a renewing owner keeps the wallet"),
        Err(err) => err,
    };
    assert_eq!(err, FenceError::Conflict);

    fixture.finish().await;
}

#[tokio::test]
async fn an_owner_expiring_during_mint_cannot_commit_or_renew_the_row() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;
    set_realm_renewal(&fixture.pool, "clock_timestamp() - interval '89 seconds'").await;
    let fence = fixture.fence().await;
    let lease = fence.lock_realm(WALLET, SESSION).await.expect("live lease");

    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        lease.commit(assignment("island-too-late", None)).await,
        Err(FenceError::Conflict)
    );
    assert_eq!(row_state(&fixture.pool).await, (3, 5, None));
    assert_eq!(
        fence.current_realm_assignment(WALLET, SESSION).await,
        Err(FenceError::Unowned)
    );
    fixture.finish().await;
}

#[tokio::test]
async fn an_owner_expiring_while_waiting_for_the_row_lock_gets_no_lease() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 2, 3, 5, None).await;
    set_realm_renewal(&fixture.pool, "clock_timestamp() - interval '89 seconds'").await;
    let mut blocker = fixture.pool.begin().await.expect("blocking transaction");
    sqlx::query("SELECT owner_session FROM archipelago_v4_assignments FOR UPDATE")
        .fetch_all(&mut *blocker)
        .await
        .expect("hold the authority row lock");
    let fence = fixture.fence().await;
    let waiter = tokio::spawn(async move { fence.lock_realm(WALLET, SESSION).await });
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(!waiter.is_finished());
    blocker.rollback().await.expect("release the row lock");
    match waiter.await.expect("waiting task") {
        Ok(_) => panic!("a row that expired while waiting must not grant a lease"),
        Err(error) => assert_eq!(error, FenceError::Conflict),
    }
    assert_eq!(row_state(&fixture.pool).await, (3, 5, None));
    fixture.finish().await;
}

#[tokio::test]
async fn conditional_commit_writes_shared_assignment_document_and_revision() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(
        &fixture.pool,
        WALLET,
        SESSION,
        7,
        5,
        11,
        Some(json!({
            "islandId": "island-old",
            "connectionString": "livekit:wss://example.invalid/island-old"
        })),
    )
    .await;

    let fence = fixture.fence().await;
    let lease = fence.lock_realm(WALLET, SESSION).await.expect("lease");
    assert_eq!(lease.previous_island().as_deref(), Some("island-old"));
    let token = lease.token_fence();
    assert_eq!(token.audience, AUDIENCE);
    assert_eq!(token.lane, "realm");
    assert_eq!(token.owner_session, SESSION);
    assert_eq!(token.owner_epoch, 7);
    assert_eq!(token.assignment_revision, 6);
    assert_eq!(token.fencing_token, 11);

    lease
        .commit(assignment("island-new", Some("island-old")))
        .await
        .expect("commit shared assignment document");

    let (revision, fence, document) = row_state(&fixture.pool).await;
    let document = document.expect("assignment document");
    assert_eq!(revision, 6);
    assert_eq!(fence, 11);
    assert_eq!(document["islandId"], "island-new");
    assert_eq!(
        document["connectionString"],
        "livekit:wss://example.invalid/island-new"
    );
    assert_eq!(document["fromIslandId"], "island-old");
    assert!(
        document.get("peers").is_none(),
        "empty peer maps stay omitted in the shared document shape"
    );

    fixture.finish().await;
}

#[tokio::test]
async fn cancelled_lease_releases_lock_without_assignment() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    seed_realm(&fixture.pool, WALLET, SESSION, 3, 1, 2, None).await;

    let first_fence = fixture.fence().await;
    let second_fence = fixture.fence().await;
    let first = first_fence
        .lock_realm(WALLET, SESSION)
        .await
        .expect("first lease");
    let mut waiter = tokio::spawn(async move { second_fence.lock_realm(WALLET, SESSION).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut waiter)
            .await
            .is_err(),
        "second lease must wait while the first uncommitted lease is alive"
    );

    drop(first);
    let second = waiter
        .await
        .expect("second lock task")
        .expect("second lease after first drop");
    assert_eq!(second.token_fence().assignment_revision, 2);
    drop(second);
    assert_eq!(row_state(&fixture.pool).await, (1, 2, None));

    fixture.finish().await;
}
