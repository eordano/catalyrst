//! Pins the writer semantics the sync path relies on (upstream snapshots-fetcher 53e9c07
//! parity round, plus the per-server cursor work): the persisted global sync frontier is
//! GREATEST-monotonic through `advance_sync_frontier`, so a stale offer can never rewind the
//! durable frontier or the freshness gauge derived from it -- and each server's OWN cursor
//! (`server_sync_cursors`, migration 0004) has the same guarantee through
//! `advance_server_sync_cursor`, while bootstrap resumes every server from its own cursor
//! rather than the max-over-servers frontier. Requires a test postgres; the upserts'
//! GREATEST shapes are also pinned without a database by the SQL-shape tests in
//! sync/backends.rs.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_server::sync::sync_orchestrator::resume_point;
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

async fn setup_db_with(cursor_table: bool) -> Option<(PgPool, String)> {
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

    if cursor_table {
        // Mirrors migrations/0004_server_sync_cursors.sql.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS server_sync_cursors (
                server_url text NOT NULL,
                cursor_ms  bigint NOT NULL,
                updated_at timestamptz NOT NULL DEFAULT now(),
                CONSTRAINT server_sync_cursors_pkey PRIMARY KEY (server_url)
            )",
        )
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("create server_sync_cursors failed: {e}"));
    }

    Some((pool, schema))
}

async fn setup_db() -> Option<(PgPool, String)> {
    setup_db_with(true).await
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

// The structural fix the per-server cursors exist for: server A stalls in bootstrap with only
// early confirmed progress while server B's steady stream carries its own cursor -- and the
// global frontier -- far ahead. After a restart, each server resumes from ITS OWN cursor: A
// from its stall point (so its undeployed entities are re-pulled, not skipped until the
// re-snapshot pass), B from its confirmed progress. The global frontier keeps its
// max-over-servers meaning untouched and serves only the fallback for a never-seen server.
#[tokio::test]
async fn each_server_resumes_from_its_own_cursor_after_restart() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let repo = LiveDeploymentRepository::new(pool.clone());
    let server_a = "https://stalled.example.com/content";
    let server_b = "https://healthy.example.com/content";
    let server_c = "https://never-seen.example.com/content";

    // A confirmed one early poll boundary, then stalled in bootstrap.
    repo.advance_server_sync_cursor(server_a, 1_000_000)
        .await
        .unwrap();
    repo.advance_sync_frontier(1_000_000).await.unwrap();

    // B's steady stream kept confirming boundaries; every one also fed the global frontier.
    repo.advance_server_sync_cursor(server_b, 9_000_000)
        .await
        .unwrap();
    repo.advance_sync_frontier(9_000_000).await.unwrap();

    // "Restart": bootstrap reads the per-server cursors and the global frontier.
    let frontier = repo.get_sync_frontier().await.unwrap();
    let cursor_a = repo.get_server_sync_cursor(server_a).await.unwrap();
    let cursor_b = repo.get_server_sync_cursor(server_b).await.unwrap();
    let cursor_c = repo.get_server_sync_cursor(server_c).await.unwrap();

    assert_eq!(
        resume_point(cursor_a, frontier),
        1_000_000,
        "the stalled server must resume from its own cursor, not ride the global frontier \
         past its undeployed entities"
    );
    assert_eq!(resume_point(cursor_b, frontier), 9_000_000);
    assert_eq!(
        (cursor_c, resume_point(cursor_c, frontier)),
        (None, 9_000_000),
        "a server never seen before falls back to the global frontier"
    );
    assert_eq!(
        frontier, 9_000_000,
        "the global frontier stays max-over-servers reporting state"
    );

    // Removing a server from sync leaves its cursor row: re-adding it later resumes
    // correctly instead of starting as never-seen.
    assert_eq!(
        repo.get_server_sync_cursor(server_a).await.unwrap(),
        Some(1_000_000)
    );

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_stale_offer_cannot_rewind_a_server_cursor() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let repo = LiveDeploymentRepository::new(pool.clone());
    let server_a = "https://a.example.com/content";
    let server_b = "https://b.example.com/content";

    repo.advance_server_sync_cursor(server_a, 2_000)
        .await
        .unwrap();
    repo.advance_server_sync_cursor(server_b, 5_000)
        .await
        .unwrap();

    // The bootstrap pointer-changes shift (cursor - 20 min) re-offers older boundaries; the
    // per-server cursor must be as rewind-proof as the global frontier.
    repo.advance_server_sync_cursor(server_a, 1_500)
        .await
        .unwrap();
    assert_eq!(
        repo.get_server_sync_cursor(server_a).await.unwrap(),
        Some(2_000),
        "a stale offer must never lower a server's persisted cursor"
    );

    // A genuinely newer boundary still advances it -- and only for that server.
    repo.advance_server_sync_cursor(server_a, 2_500)
        .await
        .unwrap();
    assert_eq!(
        repo.get_server_sync_cursor(server_a).await.unwrap(),
        Some(2_500)
    );
    assert_eq!(
        repo.get_server_sync_cursor(server_b).await.unwrap(),
        Some(5_000),
        "one server's writes must never touch another server's cursor"
    );

    teardown(&pool, &schema).await;
}

// The upgrade path: migration 0004 not applied yet. Reads degrade to "no cursor" (so resume
// falls back to the global frontier, exactly the pre-cursor behavior) and writes are
// swallowed instead of failing the stream that offered them.
#[tokio::test]
async fn missing_cursor_table_degrades_to_the_global_frontier_resume() {
    let Some((pool, schema)) = setup_db_with(false).await else {
        return;
    };
    let repo = LiveDeploymentRepository::new(pool.clone());
    let server = "https://a.example.com/content";

    assert_eq!(repo.get_server_sync_cursor(server).await.unwrap(), None);
    repo.advance_server_sync_cursor(server, 1_234)
        .await
        .unwrap();
    assert_eq!(repo.get_server_sync_cursor(server).await.unwrap(), None);

    repo.advance_sync_frontier(7_777).await.unwrap();
    let frontier = repo.get_sync_frontier().await.unwrap();
    assert_eq!(
        resume_point(repo.get_server_sync_cursor(server).await.unwrap(), frontier),
        7_777
    );

    teardown(&pool, &schema).await;
}
