//! Pins the writer semantics the sync path relies on (upstream snapshots-fetcher 53e9c07
//! parity round): the persisted sync frontier is GREATEST-monotonic through
//! `advance_sync_frontier`, so a stale offer can never rewind the durable frontier or the
//! freshness gauge derived from it. Requires a test postgres; the upsert's GREATEST shape is
//! also pinned without a database by the SQL-shape test in sync/backends.rs.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_server::sync::LiveDeploymentRepository;

const PG_VAR: &str = "CATALYRST_SERVER_TEST_PG";

fn pg_url() -> String {
    catalyrst_testgate::require_pg_or(
        PG_VAR,
        "postgres://postgres:postgres@127.0.0.1:5432/postgres",
    )
}

fn unique_schema() -> String {
    format!("test_sync_frontier_{}", uuid::Uuid::new_v4().simple())
}

async fn setup_db() -> Option<(PgPool, String)> {
    let url = pg_url();
    let admin = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return catalyrst_testgate::pg_unusable(
                PG_VAR,
                &format!("connect to {url} failed: {e}"),
            )
        }
    };
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .unwrap_or_else(|e| panic!("connect to scratch schema {schema} failed: {e}"));

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS system_properties (
            key text NOT NULL,
            value text NOT NULL,
            CONSTRAINT system_properties_pkey PRIMARY KEY (key)
        )",
    )
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("create system_properties failed: {e}"));

    Some((pool, schema))
}

async fn teardown(pool: &PgPool, schema: &str) {
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA {} CASCADE",
        schema
    )))
    .execute(pool)
    .await;
}

#[tokio::test]
async fn a_late_straggler_floor_cannot_rewind_the_persisted_frontier() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let repo = LiveDeploymentRepository::new(pool.clone());

    // Steady-state streams have carried the frontier forward.
    repo.advance_sync_frontier(1_700_000_000_000).await.unwrap();
    assert_eq!(repo.get_sync_frontier().await.unwrap(), 1_700_000_000_000);

    // A straggler completes hours later; save_frontier offers the stale min over servers
    // through the same monotonic writer. The persisted frontier must not move backwards.
    repo.advance_sync_frontier(1_600_000_000_000).await.unwrap();
    assert_eq!(
        repo.get_sync_frontier().await.unwrap(),
        1_700_000_000_000,
        "a lagging floor offer must never lower the persisted frontier"
    );

    // A genuinely newer floor still raises it.
    repo.advance_sync_frontier(1_800_000_000_000).await.unwrap();
    assert_eq!(repo.get_sync_frontier().await.unwrap(), 1_800_000_000_000);

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn advance_from_scratch_installs_the_first_value() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let repo = LiveDeploymentRepository::new(pool.clone());
    assert_eq!(repo.get_sync_frontier().await.unwrap(), 0);
    repo.advance_sync_frontier(123_456).await.unwrap();
    assert_eq!(repo.get_sync_frontier().await.unwrap(), 123_456);
    teardown(&pool, &schema).await;
}
