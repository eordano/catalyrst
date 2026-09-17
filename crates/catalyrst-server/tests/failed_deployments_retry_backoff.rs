//! Retry-backoff state on failed_deployments (migration 0006): the columns only ever move
//! forward, and the give-up DELETE only takes rows that are still at the cap. Both properties
//! live in SQL, so they are asserted against a real postgres.
//!
//! Same gating as sync_flush_overwrites.rs: CATALYRST_SERVER_TEST_PG, with
//! ALLOW_SKIPPED_INTEGRATION=1 downgrading an unreachable DB to a skip.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_server::sync::{AuthChain, AuthLink, AuthLinkType};
use catalyrst_server::sync::{FailedDeployment, FailureReason, LiveFailedDeploymentsStore};

const PG_VAR: &str = "CATALYRST_SERVER_TEST_PG";

fn pg_url() -> String {
    catalyrst_testgate::require_pg_or(
        PG_VAR,
        "postgres://postgres:postgres@127.0.0.1:5432/postgres",
    )
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
    let schema = format!("test_retry_backoff_{}", uuid::Uuid::new_v4().simple());
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

    for migration in [
        include_str!("../migrations/0001_content_schema.sql"),
        include_str!("../migrations/0006_failed_deployments_retry_backoff.sql"),
    ] {
        apply_sql(&pool, &migration.replace("public.", "")).await;
    }

    Some((pool, schema))
}

async fn apply_sql(pool: &PgPool, sql: &str) {
    let mut buf = String::new();
    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("statement failed: {e}\n{buf}"));
            buf.clear();
        }
    }
    assert!(buf.trim().is_empty(), "trailing sql without ';': {buf}");
}

async fn teardown(pool: &PgPool, schema: &str) {
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA {} CASCADE",
        schema
    )))
    .execute(pool)
    .await;
}

fn auth_chain() -> AuthChain {
    vec![AuthLink {
        link_type: AuthLinkType::SIGNER,
        payload: "0x0000000000000000000000000000000000000001".to_string(),
        signature: None,
    }]
}

const FIRST_FAILURE_MS: i64 = 1_700_000_000_000;

fn failure(entity_id: &str, retry_count: u32, next_retry_at: i64) -> FailedDeployment {
    FailedDeployment {
        entity_type: "scene".to_string(),
        entity_id: entity_id.to_string(),
        reason: FailureReason::DeploymentError,
        auth_chain: auth_chain(),
        error_description: "boom".to_string(),
        failure_timestamp: FIRST_FAILURE_MS,
        snapshot_hash: None,
        retry_count,
        next_retry_at,
    }
}

async fn stored(store: &LiveFailedDeploymentsStore, entity_id: &str) -> FailedDeployment {
    store
        .get_all_failed()
        .await
        .unwrap()
        .into_iter()
        .find(|f| f.entity_id == entity_id)
        .unwrap_or_else(|| panic!("{entity_id} is not in failed_deployments"))
}

#[tokio::test]
async fn a_fresh_failure_is_due_immediately() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());

    store
        .report_failure(failure("bafyfresh", 0, 0))
        .await
        .unwrap();

    let row = stored(&store, "bafyfresh").await;
    assert_eq!(row.retry_count, 0);
    assert_eq!(row.next_retry_at, 0);
    assert_eq!(row.failure_timestamp, FIRST_FAILURE_MS);

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_plain_re_report_cannot_rewind_the_backoff() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());
    let deadline = 1_800_000_000_000i64;

    store
        .report_failure(failure("bafyback", 0, 0))
        .await
        .unwrap();
    store
        .report_failure(failure("bafyback", 3, deadline))
        .await
        .unwrap();

    let advanced = stored(&store, "bafyback").await;
    assert_eq!(advanced.retry_count, 3);
    assert_eq!(advanced.next_retry_at, deadline);
    assert_eq!(advanced.failure_timestamp, FIRST_FAILURE_MS);

    store
        .report_failure(failure("bafyback", 0, 0))
        .await
        .unwrap();

    let after_sync_report = stored(&store, "bafyback").await;
    assert_eq!(after_sync_report.retry_count, 3);
    assert_eq!(after_sync_report.next_retry_at, deadline);

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_retry_re_report_keeps_the_original_failure_time() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());
    let deadline = 1_800_000_000_000i64;

    store
        .report_failure(failure("bafyfirst", 0, 0))
        .await
        .unwrap();

    let loaded = stored(&store, "bafyfirst").await;
    let deferred = FailedDeployment {
        error_description: "still broken".to_string(),
        retry_count: loaded.retry_count + 1,
        next_retry_at: deadline,
        ..loaded
    };
    store.report_failure(deferred).await.unwrap();

    let after = stored(&store, "bafyfirst").await;
    assert_eq!(after.failure_timestamp, FIRST_FAILURE_MS);
    assert_eq!(after.retry_count, 1);
    assert_eq!(after.next_retry_at, deadline);
    assert_eq!(after.error_description, "still broken");

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn eviction_only_takes_rows_still_at_the_cap() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());

    store
        .report_failure(failure("bafyexhausted", 10, 1))
        .await
        .unwrap();
    store
        .report_failure(failure("bafyrefailed", 1, 1))
        .await
        .unwrap();

    let removed = store
        .remove_exhausted(
            &[
                "bafyexhausted".to_string(),
                "bafyrefailed".to_string(),
                "bafyabsent".to_string(),
            ],
            10,
        )
        .await
        .unwrap();

    assert_eq!(removed, vec!["bafyexhausted".to_string()]);
    let left: Vec<String> = store
        .get_all_failed()
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.entity_id)
        .collect();
    assert_eq!(left, vec!["bafyrefailed".to_string()]);

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn eviction_of_nothing_touches_nothing() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());

    store
        .report_failure(failure("bafykeep", 10, 1))
        .await
        .unwrap();

    assert!(store.remove_exhausted(&[], 10).await.unwrap().is_empty());
    assert_eq!(store.get_all_failed().await.unwrap().len(), 1);

    teardown(&pool, &schema).await;
}

/// The deployer is asynchronous: a retry that reaches the batch buffer returns Ok, so the retry
/// worker clears the row while the entity is still only queued. When the flush later fails, the
/// re-report is the only record left -- it has to carry the attempt the retry already spent, or
/// the entity comes back at retry_count 0 and never accrues backoff.
#[tokio::test]
async fn a_flush_failure_after_a_retry_keeps_the_spent_attempts() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());
    let deadline = 1_800_000_000_000i64;

    store
        .report_failure(failure("bafyqueued", 3, 1))
        .await
        .unwrap();

    let loaded = stored(&store, "bafyqueued").await;
    let stamped = (loaded.retry_count + 1, deadline);

    store
        .remove("bafyqueued", loaded.retry_count)
        .await
        .unwrap();

    store
        .report_failure(FailedDeployment {
            error_description: "batch flush failed: boom".to_string(),
            retry_count: stamped.0,
            next_retry_at: stamped.1,
            ..failure("bafyqueued", 0, 0)
        })
        .await
        .unwrap();

    let after = stored(&store, "bafyqueued").await;
    assert_eq!(after.retry_count, 4);
    assert_eq!(after.next_retry_at, deadline);

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn a_no_entity_reason_survives_a_retry_re_report() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());

    store
        .report_failure(FailedDeployment {
            reason: FailureReason::NoEntity,
            ..failure("bafynoentity", 0, 0)
        })
        .await
        .unwrap();

    let loaded = stored(&store, "bafynoentity").await;
    assert_eq!(loaded.reason, FailureReason::NoEntity);

    store
        .report_failure(FailedDeployment {
            error_description: "still missing".to_string(),
            retry_count: loaded.retry_count + 1,
            ..loaded
        })
        .await
        .unwrap();

    let (column,): (String,) =
        sqlx::query_as("SELECT reason FROM failed_deployments WHERE entity_id = $1")
            .bind("bafynoentity")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(column, "No entity");
    assert_eq!(
        stored(&store, "bafynoentity").await.reason,
        FailureReason::NoEntity
    );

    teardown(&pool, &schema).await;
}

/// The worker clears the row of an entity it just deployed, but the deployer is asynchronous: the
/// flush carrying that same entity can fail and commit its report first. The clear is pinned to
/// the attempt count the worker read, so a fresher record survives it and the entity keeps both
/// its evidence and its progress towards the cap.
#[tokio::test]
async fn a_clear_pinned_to_an_older_attempt_spares_a_fresher_failure() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    let store = LiveFailedDeploymentsStore::new(pool.clone());

    store
        .report_failure(failure("bafyraced", 4, 1_800_000_000_000))
        .await
        .unwrap();
    store.remove("bafyraced", 3).await.unwrap();

    let survivor = stored(&store, "bafyraced").await;
    assert_eq!(survivor.retry_count, 4);
    assert_eq!(survivor.next_retry_at, 1_800_000_000_000);

    store
        .report_failure(failure("bafycleared", 3, 1))
        .await
        .unwrap();
    store.remove("bafycleared", 3).await.unwrap();

    assert!(store
        .get_all_failed()
        .await
        .unwrap()
        .iter()
        .all(|f| f.entity_id != "bafycleared"));

    teardown(&pool, &schema).await;
}
