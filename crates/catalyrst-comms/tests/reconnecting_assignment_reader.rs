use std::time::{Duration, Instant};

use catalyrst_comms::assignment_fence::{
    AssignmentFence, FenceError, RealmAssignmentReader, ReconnectingPgAssignmentReader,
};
use catalyrst_comms::config::ClusterConfig;
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool, Row};

const AUDIENCE: &str = "reconnecting-reader-test";
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
        let url = std::env::var("CATALYRST_COMMS_TEST_PG")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("CATALYRST_ARCHIPELAGO_TEST_PG")
                    .ok()
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| catalyrst_testgate::require_pg("CATALYRST_COMMS_TEST_PG"))?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect configured test PostgreSQL");
        let schema = format!(
            "comms_reconnecting_reader_{}",
            uuid::Uuid::new_v4().simple()
        );
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create isolated schema");
        let options: PgConnectOptions = url.parse().expect("parse PostgreSQL URL");
        let options = options.options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .expect("connect isolated schema");
        let separator = if url.contains('?') { '&' } else { '?' };
        Some(Self {
            admin,
            pool,
            schema: schema.clone(),
            url: format!("{url}{separator}options[search_path]={schema}"),
        })
    }

    async fn create_authority(&self) {
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
        .execute(&self.pool)
        .await
        .expect("create authority table");
    }

    async fn seed(&self) {
        sqlx::query(
            "INSERT INTO archipelago_v4_assignments (
                deployment_audience, owner_address, lane_key, authority_incarnation,
                assignment_revision, owner_session, owner_epoch, fencing_token, assignment_json
             ) VALUES ($1, $2, 'realm', 'authority-incarnation', 7, $3, 11, 13, $4)",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .bind(SESSION)
        .bind(json!({
            "islandId": "island-current",
            "connectionString": "livekit:token",
            "peers": {}
        }))
        .execute(&self.pool)
        .await
        .expect("seed authority row");
    }

    async fn finish(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .expect("drop isolated schema");
        self.admin.close().await;
    }
}

async fn bounded_read(
    reader: &ReconnectingPgAssignmentReader,
) -> Result<catalyrst_comms::assignment_fence::RealmAssignmentSnapshot, FenceError> {
    let started = Instant::now();
    let result = reader.current_realm_assignment(WALLET, SESSION).await;
    assert!(
        started.elapsed() <= Duration::from_millis(2500),
        "assignment read exceeded its whole-operation deadline"
    );
    result
}

#[tokio::test]
async fn constructor_validates_configuration_without_connecting() {
    assert!(ReconnectingPgAssignmentReader::new(
        "postgresql://127.0.0.1:1/not-running",
        AUDIENCE.into(),
    )
    .is_ok());
    assert!(matches!(
        ReconnectingPgAssignmentReader::new("not a PostgreSQL URL", AUDIENCE.into()),
        Err(FenceError::Invalid)
    ));
    assert!(matches!(
        ReconnectingPgAssignmentReader::new("postgresql://localhost/postgres", String::new()),
        Err(FenceError::Invalid)
    ));
    assert!(matches!(
        ReconnectingPgAssignmentReader::new("postgresql://localhost/postgres", "x".repeat(257),),
        Err(FenceError::Invalid)
    ));
}

#[tokio::test]
async fn social_factory_distinguishes_disabled_invalid_and_configured_states() {
    let mut config = ClusterConfig::default();
    assert!(catalyrst_comms::reconnecting_assignment_reader(&config)
        .expect("absent authority configuration is valid")
        .is_none());

    config.control_database_url = Some("postgresql://127.0.0.1:1/not-running".into());
    assert!(catalyrst_comms::reconnecting_assignment_reader(&config).is_err());
    config.control_database_url = None;
    config.control_v4_audience = Some(AUDIENCE.into());
    assert!(catalyrst_comms::reconnecting_assignment_reader(&config).is_err());

    config.control_database_url = Some("not a PostgreSQL URL".into());
    assert!(catalyrst_comms::reconnecting_assignment_reader(&config).is_err());
    config.control_database_url = Some("postgresql://127.0.0.1:1/not-running".into());
    assert!(catalyrst_comms::reconnecting_assignment_reader(&config)
        .expect("valid configured authority does no startup I/O")
        .is_some());
}

#[tokio::test]
async fn unavailable_authority_recovers_on_the_same_reader_without_weakening_owner_checks() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let reader = ReconnectingPgAssignmentReader::new(&fixture.url, AUDIENCE.into())
        .expect("construct lazy reader without database I/O");

    assert_eq!(bounded_read(&reader).await, Err(FenceError::Unavailable));
    assert!(matches!(
        reader.lock_realm(WALLET, SESSION).await,
        Err(FenceError::Unavailable)
    ));
    fixture.create_authority().await;
    fixture.seed().await;

    let lease = reader
        .lock_realm(WALLET, SESSION)
        .await
        .expect("same retained fence connects after authority becomes ready");
    assert_eq!(lease.token_fence().owner_epoch, 11);
    drop(lease);

    let current = bounded_read(&reader)
        .await
        .expect("same retained reader connects after authority becomes ready");
    assert_eq!(current.owner_epoch, 11);
    assert_eq!(current.assignment_revision, 7);
    assert_eq!(current.fencing_token, 13);
    assert_eq!(current.island_id, "island-current");
    assert_eq!(
        reader
            .current_realm_assignment(WALLET, "session-displaced")
            .await,
        Err(FenceError::Conflict)
    );

    sqlx::query("UPDATE archipelago_v4_assignments SET updated_at = '-infinity'")
        .execute(&fixture.pool)
        .await
        .expect("release owner");
    assert_eq!(bounded_read(&reader).await, Err(FenceError::Unowned));

    sqlx::query(
        "UPDATE archipelago_v4_assignments
         SET updated_at = clock_timestamp() - interval '91 seconds'",
    )
    .execute(&fixture.pool)
    .await
    .expect("lapse owner");
    assert_eq!(bounded_read(&reader).await, Err(FenceError::Unowned));

    sqlx::query("UPDATE archipelago_v4_assignments SET updated_at = clock_timestamp()")
        .execute(&fixture.pool)
        .await
        .expect("restore current owner for outage test");

    sqlx::query("DROP TABLE archipelago_v4_assignments")
        .execute(&fixture.pool)
        .await
        .expect("take authority schema offline");
    assert_eq!(bounded_read(&reader).await, Err(FenceError::Unavailable));
    assert!(matches!(
        reader.lock_realm(WALLET, SESSION).await,
        Err(FenceError::Unavailable)
    ));
    fixture.create_authority().await;
    fixture.seed().await;
    let recovered = bounded_read(&reader)
        .await
        .expect("same retained reader recovers after authority outage");
    assert_eq!(recovered, current);

    let row = sqlx::query(
        "SELECT assignment_revision, owner_epoch, fencing_token
         FROM archipelago_v4_assignments",
    )
    .fetch_one(&fixture.pool)
    .await
    .expect("read authority row after snapshots");
    assert_eq!(row.try_get::<i64, _>("assignment_revision").unwrap(), 7);
    assert_eq!(row.try_get::<i64, _>("owner_epoch").unwrap(), 11);
    assert_eq!(row.try_get::<i64, _>("fencing_token").unwrap(), 13);

    fixture.finish().await;
}

#[tokio::test]
async fn blocked_read_times_out_and_same_reader_recovers_after_lock_release() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    fixture.create_authority().await;
    fixture.seed().await;
    let reader = ReconnectingPgAssignmentReader::new(&fixture.url, AUDIENCE.into())
        .expect("construct lazy reader");

    let mut lock = fixture.pool.begin().await.expect("begin lock transaction");
    sqlx::query("LOCK TABLE archipelago_v4_assignments IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .expect("hold authority table lock");

    let started = Instant::now();
    assert_eq!(
        reader.current_realm_assignment(WALLET, SESSION).await,
        Err(FenceError::Unavailable)
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(1800),
        "blocked read returned before the configured deadline: {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(2500),
        "blocked read exceeded the configured deadline: {elapsed:?}"
    );

    lock.rollback().await.expect("release authority table lock");
    let recovered = bounded_read(&reader)
        .await
        .expect("same reader recovers after lock release");
    assert_eq!(recovered.owner_epoch, 11);
    assert_eq!(recovered.assignment_revision, 7);
    assert_eq!(recovered.fencing_token, 13);
    assert_eq!(recovered.island_id, "island-current");

    fixture.finish().await;
}
