//! Env-gated Postgres integration tests for the sync-flush overwrite
//! bookkeeping (deployments.deleter_deployment + active_pointers maintenance).
//!
//! Requires a reachable Postgres; set CATALYRST_SERVER_TEST_PG (or have the
//! default postgres://postgres:postgres@127.0.0.1:5432/postgres running).
//! Each test creates a unique schema, applies the content schema migration,
//! and drops the schema afterwards. Tests silently skip when no DB is up.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_server::sync_backends::LiveSyncDeployer;
use catalyrst_sync::{AuthChain, AuthLink, AuthLinkType, Deployer, DeploymentContext};

fn pg_url() -> String {
    std::env::var("CATALYRST_SERVER_TEST_PG")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/postgres".into())
}

fn unique_schema() -> String {
    format!("test_sync_flush_{}", uuid::Uuid::new_v4().simple())
}

async fn setup_db() -> Option<(PgPool, String)> {
    let url = pg_url();
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    // The migration schema-qualifies everything with `public.`; strip that so
    // the tables land in this test's unique schema instead.
    let sql = include_str!("../migrations/0001_content_schema.sql").replace("public.", "");
    apply_sql(&pool, &sql).await;

    // The live content DB (originally created by the upstream TS
    // content-server) has an entity_type column on active_pointers that the
    // sync flush path writes; the fresh-replica migration does not include it.
    apply_sql(
        &pool,
        "ALTER TABLE active_pointers ADD COLUMN IF NOT EXISTS entity_type text;",
    )
    .await;

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

fn entity_json(
    entity_type: &str,
    pointers: &[&str],
    timestamp: f64,
    content: &[(&str, &str)],
) -> Vec<u8> {
    let content: Vec<serde_json::Value> = content
        .iter()
        .map(|(file, hash)| serde_json::json!({"file": file, "hash": hash}))
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "version": "v3",
        "type": entity_type,
        "pointers": pointers,
        "timestamp": timestamp,
        "content": content,
        "metadata": {"name": "test"},
    }))
    .unwrap()
}

async fn deploy(
    deployer: &LiveSyncDeployer,
    entity_id: &str,
    entity_type: &str,
    pointers: &[&str],
    timestamp: f64,
    content: &[(&str, &str)],
) {
    deployer
        .deploy_entity(
            &entity_json(entity_type, pointers, timestamp, content),
            entity_id,
            &auth_chain(),
            DeploymentContext::Synced,
        )
        .await
        .unwrap();
}

async fn deleter_of(pool: &PgPool, entity_id: &str) -> Option<i32> {
    let (deleter,): (Option<i32>,) =
        sqlx::query_as("SELECT deleter_deployment FROM deployments WHERE entity_id = $1")
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .unwrap();
    deleter
}

async fn deployment_id(pool: &PgPool, entity_id: &str) -> i32 {
    let (id,): (i32,) = sqlx::query_as("SELECT id FROM deployments WHERE entity_id = $1")
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .unwrap();
    id
}

async fn active_pointer(pool: &PgPool, pointer: &str) -> Option<String> {
    sqlx::query_scalar("SELECT entity_id FROM active_pointers WHERE pointer = $1")
        .bind(pointer)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// Same query the /contents/{hash}/active-entities endpoint uses.
async fn active_by_hash(pool: &PgPool, hash: &str) -> Vec<String> {
    catalyrst_db::deployments_repository::get_active_deployments_by_content_hash(pool, hash)
        .await
        .unwrap()
}

#[tokio::test]
async fn newer_first_then_older_marks_late_arrival_overwritten() {
    let Some((pool, schema)) = setup_db().await else {
        eprintln!("skipping: no Postgres available");
        return;
    };

    let deployer = LiveSyncDeployer::new(pool.clone());
    deploy(
        &deployer,
        "bafynewer1",
        "wearable",
        &["urn:pointer:a"],
        2_000_000.0,
        &[("thumbnail.png", "bafyhashnew1")],
    )
    .await;
    deployer.flush().await.unwrap();
    deploy(
        &deployer,
        "bafyolder1",
        "wearable",
        &["urn:pointer:a"],
        1_000_000.0,
        &[("thumbnail.png", "bafyhashold1")],
    )
    .await;
    deployer.flush().await.unwrap();

    let newer_id = deployment_id(&pool, "bafynewer1").await;
    assert_eq!(deleter_of(&pool, "bafyolder1").await, Some(newer_id));
    assert_eq!(deleter_of(&pool, "bafynewer1").await, None);
    assert_eq!(
        active_pointer(&pool, "urn:pointer:a").await.as_deref(),
        Some("bafynewer1")
    );
    // Active-by-hash must not surface the superseded late arrival.
    assert!(active_by_hash(&pool, "bafyhashold1").await.is_empty());
    assert_eq!(
        active_by_hash(&pool, "bafyhashnew1").await,
        vec!["bafynewer1".to_string()]
    );

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn older_first_then_newer_marks_older_overwritten() {
    let Some((pool, schema)) = setup_db().await else {
        eprintln!("skipping: no Postgres available");
        return;
    };

    let deployer = LiveSyncDeployer::new(pool.clone());
    deploy(
        &deployer,
        "bafyolder2",
        "scene",
        &["10,10"],
        1_000_000.0,
        &[("scene.json", "bafyhashold2")],
    )
    .await;
    deployer.flush().await.unwrap();
    deploy(
        &deployer,
        "bafynewer2",
        "scene",
        &["10,10"],
        2_000_000.0,
        &[("scene.json", "bafyhashnew2")],
    )
    .await;
    deployer.flush().await.unwrap();

    let newer_id = deployment_id(&pool, "bafynewer2").await;
    assert_eq!(deleter_of(&pool, "bafyolder2").await, Some(newer_id));
    assert_eq!(deleter_of(&pool, "bafynewer2").await, None);
    assert_eq!(
        active_pointer(&pool, "10,10").await.as_deref(),
        Some("bafynewer2")
    );
    assert!(active_by_hash(&pool, "bafyhashold2").await.is_empty());

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn intra_batch_overwrite_leaves_exactly_one_survivor() {
    let Some((pool, schema)) = setup_db().await else {
        eprintln!("skipping: no Postgres available");
        return;
    };

    let deployer = LiveSyncDeployer::new(pool.clone());
    deploy(
        &deployer,
        "bafyolder3",
        "scene",
        &["20,20"],
        1_000_000.0,
        &[("scene.json", "bafyhashold3")],
    )
    .await;
    deploy(
        &deployer,
        "bafynewer3",
        "scene",
        &["20,20"],
        2_000_000.0,
        &[("scene.json", "bafyhashnew3")],
    )
    .await;
    deployer.flush().await.unwrap();

    let newer_id = deployment_id(&pool, "bafynewer3").await;
    assert_eq!(deleter_of(&pool, "bafyolder3").await, Some(newer_id));
    assert_eq!(deleter_of(&pool, "bafynewer3").await, None);
    assert_eq!(
        active_pointer(&pool, "20,20").await.as_deref(),
        Some("bafynewer3")
    );

    teardown(&pool, &schema).await;
}

#[tokio::test]
async fn shrunk_pointer_set_clears_uncovered_active_pointer() {
    let Some((pool, schema)) = setup_db().await else {
        eprintln!("skipping: no Postgres available");
        return;
    };

    let deployer = LiveSyncDeployer::new(pool.clone());
    deploy(
        &deployer,
        "bafyolder4",
        "scene",
        &["30,30", "30,31"],
        1_000_000.0,
        &[("scene.json", "bafyhashold4")],
    )
    .await;
    deployer.flush().await.unwrap();
    assert_eq!(
        active_pointer(&pool, "30,30").await.as_deref(),
        Some("bafyolder4")
    );
    assert_eq!(
        active_pointer(&pool, "30,31").await.as_deref(),
        Some("bafyolder4")
    );

    deploy(
        &deployer,
        "bafynewer4",
        "scene",
        &["30,30"],
        2_000_000.0,
        &[("scene.json", "bafyhashnew4")],
    )
    .await;
    deployer.flush().await.unwrap();

    let newer_id = deployment_id(&pool, "bafynewer4").await;
    assert_eq!(deleter_of(&pool, "bafyolder4").await, Some(newer_id));
    assert_eq!(
        active_pointer(&pool, "30,30").await.as_deref(),
        Some("bafynewer4")
    );
    // The pointer the new entity does not cover must become inactive.
    assert_eq!(active_pointer(&pool, "30,31").await, None);

    teardown(&pool, &schema).await;
}
