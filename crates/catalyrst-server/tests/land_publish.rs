//! Env-gated Postgres integration tests for the LAND-publish provenance +
//! unpublish machinery (local_entities, tombstone repointing) and the squid
//! parcel-access rule with the operator-resolver leg.
//!
//! Requires a reachable Postgres; set CATALYRST_SERVER_TEST_PG (or have the
//! default postgres://postgres:postgres@127.0.0.1:5432/postgres running).
//! Each test creates a unique schema, applies the content schema migrations,
//! and drops the schema afterwards. Tests silently skip when no DB is up.

use std::time::Duration;

use async_trait::async_trait;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_server::land_publish::{
    local_provenance, record_local_provenance, tombstone_and_repoint, UnpublishError,
};
use catalyrst_server::state::Deployer;
use catalyrst_server::write_deployer::WriteDeployer;
use catalyrst_validator::squid_checker::{
    check_parcel_access, LandOperatorResolver, LandOperators,
};

fn pg_url() -> String {
    std::env::var("CATALYRST_SERVER_TEST_PG")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/postgres".into())
}

fn unique_schema() -> String {
    format!("test_land_publish_{}", uuid::Uuid::new_v4().simple())
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

    let sql = include_str!("../migrations/0001_content_schema.sql").replace("public.", "");
    apply_sql(&pool, &sql).await;
    apply_sql(
        &pool,
        include_str!("../migrations/0003_local_provenance.sql"),
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
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {} CASCADE",
        schema
    )))
    .execute(pool)
    .await
    .ok();
}

async fn insert_deployment(pool: &PgPool, entity_id: &str, ts_ms: i64, pointers: &[&str]) -> i32 {
    let ptrs: Vec<String> = pointers.iter().map(|p| p.to_string()).collect();
    sqlx::query_scalar(
        r#"
        INSERT INTO deployments
            (deployer_address, version, entity_type, entity_id, entity_metadata,
             entity_timestamp, entity_pointers, local_timestamp, auth_chain)
        VALUES ('0xdeployer', 'v3', 'scene', $1, NULL,
                to_timestamp($2 / 1000.0), $3, now(), '[]'::json)
        RETURNING id
        "#,
    )
    .bind(entity_id)
    .bind(ts_ms as f64)
    .bind(&ptrs)
    .fetch_one(pool)
    .await
    .expect("deployment insert")
}

async fn set_active_pointer(pool: &PgPool, pointer: &str, entity_id: &str) {
    sqlx::query(
        "INSERT INTO active_pointers (pointer, entity_id) VALUES ($1, $2) \
         ON CONFLICT (pointer) DO UPDATE SET entity_id = EXCLUDED.entity_id",
    )
    .bind(pointer)
    .bind(entity_id)
    .execute(pool)
    .await
    .expect("active_pointers upsert");
}

async fn set_deleter(pool: &PgPool, entity_id: &str, deleter: i32) {
    sqlx::query("UPDATE deployments SET deleter_deployment = $2 WHERE entity_id = $1")
        .bind(entity_id)
        .bind(deleter)
        .execute(pool)
        .await
        .expect("deleter update");
}

async fn active_pointer(pool: &PgPool, pointer: &str) -> Option<String> {
    sqlx::query_scalar("SELECT entity_id FROM active_pointers WHERE pointer = $1")
        .bind(pointer)
        .fetch_optional(pool)
        .await
        .expect("active_pointers select")
}

async fn record_provenance(pool: &PgPool, entity_id: &str, signer: &str) {
    let mut conn = pool.acquire().await.expect("acquire");
    record_local_provenance(&mut conn, entity_id, signer)
        .await
        .expect("provenance record");
}

#[tokio::test]
async fn provenance_recorded_and_surfaced_in_states() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };

    let dep = insert_deployment(&pool, "bafylocal1", 2_000, &["1,1"]).await;
    set_active_pointer(&pool, "1,1", "bafylocal1").await;
    record_provenance(&pool, "bafylocal1", "0xOwner").await;

    let p = local_provenance(&pool, "bafylocal1")
        .await
        .expect("provenance query")
        .expect("provenance row");
    assert_eq!(p["signer"], "0xowner");
    assert_eq!(p["origin"], "land-publish");
    assert_eq!(p["status"], "active");
    assert_eq!(p["superseded"], false);

    let newer = insert_deployment(&pool, "bafyupstream2", 3_000, &["1,1"]).await;
    assert!(newer > dep);
    set_deleter(&pool, "bafylocal1", newer).await;
    let p = local_provenance(&pool, "bafylocal1")
        .await
        .expect("provenance query")
        .expect("provenance row");
    assert_eq!(p["status"], "superseded");
    assert_eq!(p["superseded"], true);

    assert!(local_provenance(&pool, "bafyupstream2")
        .await
        .expect("provenance query")
        .is_none());

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn unpublish_repoints_to_last_synced_upstream_row() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };

    let _r = insert_deployment(&pool, "bafyupstream", 1_000, &["2,2", "2,3"]).await;
    let t = insert_deployment(&pool, "bafylocal", 2_000, &["2,2", "2,3"]).await;
    set_deleter(&pool, "bafyupstream", t).await;
    set_active_pointer(&pool, "2,2", "bafylocal").await;
    set_active_pointer(&pool, "2,3", "bafylocal").await;
    record_provenance(&pool, "bafylocal", "0xowner").await;

    let outcome = tombstone_and_repoint(&pool, "2,2")
        .await
        .expect("unpublish");
    assert_eq!(outcome.entity_id, "bafylocal");
    assert_eq!(outcome.repointed.len(), 2);
    for (_, replacement) in &outcome.repointed {
        assert_eq!(replacement.as_deref(), Some("bafyupstream"));
    }

    assert_eq!(
        active_pointer(&pool, "2,2").await.as_deref(),
        Some("bafyupstream")
    );
    assert_eq!(
        active_pointer(&pool, "2,3").await.as_deref(),
        Some("bafyupstream")
    );

    let restored: Option<i32> = sqlx::query_scalar(
        "SELECT deleter_deployment FROM deployments WHERE entity_id = 'bafyupstream'",
    )
    .fetch_one(&pool)
    .await
    .expect("deleter select");
    assert_eq!(restored, None);

    let p = local_provenance(&pool, "bafylocal")
        .await
        .expect("provenance query")
        .expect("provenance row");
    assert_eq!(p["status"], "unpublished");

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn unpublish_deletes_pointer_when_nothing_underneath() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };

    insert_deployment(&pool, "bafyonly", 1_000, &["3,3"]).await;
    set_active_pointer(&pool, "3,3", "bafyonly").await;
    record_provenance(&pool, "bafyonly", "0xowner").await;

    let outcome = tombstone_and_repoint(&pool, "3,3")
        .await
        .expect("unpublish");
    assert_eq!(outcome.repointed, vec![("3,3".to_string(), None)]);
    assert_eq!(active_pointer(&pool, "3,3").await, None);

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn unpublish_refuses_synced_and_already_tombstoned_entities() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };

    insert_deployment(&pool, "bafysynced", 1_000, &["4,4"]).await;
    set_active_pointer(&pool, "4,4", "bafysynced").await;
    let err = tombstone_and_repoint(&pool, "4,4").await.unwrap_err();
    assert!(matches!(err, UnpublishError::NotLocal(_)));

    insert_deployment(&pool, "bafylocal4", 2_000, &["4,5"]).await;
    set_active_pointer(&pool, "4,5", "bafylocal4").await;
    record_provenance(&pool, "bafylocal4", "0xowner").await;
    tombstone_and_repoint(&pool, "4,5")
        .await
        .expect("first unpublish");
    let err = tombstone_and_repoint(&pool, "4,5").await.unwrap_err();
    assert!(matches!(err, UnpublishError::NotLocal(_)));

    assert_eq!(
        active_pointer(&pool, "4,4").await.as_deref(),
        Some("bafysynced")
    );

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn unpublish_skips_tombstoned_locals_when_repointing() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };

    insert_deployment(&pool, "bafyup5", 1_000, &["5,5"]).await;
    insert_deployment(&pool, "bafyold5", 2_000, &["5,5"]).await;
    record_provenance(&pool, "bafyold5", "0xowner").await;
    sqlx::query("UPDATE local_entities SET tombstoned_at = now() WHERE entity_id = 'bafyold5'")
        .execute(&pool)
        .await
        .expect("pre-tombstone");
    let t2 = insert_deployment(&pool, "bafynew5", 3_000, &["5,5"]).await;
    set_deleter(&pool, "bafyup5", t2).await;
    set_deleter(&pool, "bafyold5", t2).await;
    set_active_pointer(&pool, "5,5", "bafynew5").await;
    record_provenance(&pool, "bafynew5", "0xowner").await;

    let outcome = tombstone_and_repoint(&pool, "5,5")
        .await
        .expect("unpublish");
    assert_eq!(outcome.repointed[0].1.as_deref(), Some("bafyup5"));
    assert_eq!(
        active_pointer(&pool, "5,5").await.as_deref(),
        Some("bafyup5")
    );

    drop_schema(&pool, &schema).await;
}

struct StubResolver(Result<Option<LandOperators>, String>);

#[async_trait]
impl LandOperatorResolver for StubResolver {
    async fn operators(&self, _x: i32, _y: i32) -> Result<Option<LandOperators>, String> {
        self.0.clone()
    }
}

async fn seed_squid(pool: &PgPool) {
    apply_sql(
        pool,
        "CREATE SCHEMA IF NOT EXISTS squid_marketplace;\n\
         CREATE TABLE IF NOT EXISTS squid_marketplace.parcel (\n\
             id text PRIMARY KEY, x int NOT NULL, y int NOT NULL,\n\
             owner_id text, estate_id text);\n\
         CREATE TABLE IF NOT EXISTS squid_marketplace.estate (\n\
             id text PRIMARY KEY, owner_id text);",
    )
    .await;
}

fn unique_coord() -> i32 {
    1_000_000 + (uuid::Uuid::new_v4().as_u128() % 1_000_000) as i32
}

#[tokio::test]
async fn parcel_access_owner_estate_and_operator_legs() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    seed_squid(&pool).await;

    let owner = "0x1111111111111111111111111111111111111111";
    let estate_owner = "0x2222222222222222222222222222222222222222";
    let operator = "0x3333333333333333333333333333333333333333";
    let stranger = "0x4444444444444444444444444444444444444444";

    let x_owned = unique_coord();
    let x_estate = unique_coord();
    sqlx::query(
        "INSERT INTO squid_marketplace.parcel (id, x, y, owner_id, estate_id) VALUES \
         ($1, $2, -1, $3, NULL), ($4, $5, -1, '0x9999-ETHEREUM', $6)",
    )
    .bind(format!("parcel-{x_owned}"))
    .bind(x_owned)
    .bind(format!("{owner}-ETHEREUM"))
    .bind(format!("parcel-{x_estate}"))
    .bind(x_estate)
    .bind(format!("estate-{x_estate}"))
    .execute(&pool)
    .await
    .expect("seed parcels");
    sqlx::query("INSERT INTO squid_marketplace.estate (id, owner_id) VALUES ($1, $2)")
        .bind(format!("estate-{x_estate}"))
        .bind(format!("{estate_owner}-ETHEREUM"))
        .execute(&pool)
        .await
        .expect("seed estate");

    assert!(check_parcel_access(&pool, None, owner, x_owned, -1)
        .await
        .unwrap());
    assert!(!check_parcel_access(&pool, None, stranger, x_owned, -1)
        .await
        .unwrap());

    assert!(check_parcel_access(&pool, None, estate_owner, x_estate, -1)
        .await
        .unwrap());
    assert!(!check_parcel_access(&pool, None, stranger, x_estate, -1)
        .await
        .unwrap());

    let granted = StubResolver(Ok(Some(LandOperators {
        update_operator: Some(operator.to_string()),
        ..Default::default()
    })));
    assert!(
        check_parcel_access(&pool, Some(&granted), operator, x_owned, -1)
            .await
            .unwrap()
    );

    let revoked = StubResolver(Ok(Some(LandOperators::default())));
    assert!(
        !check_parcel_access(&pool, Some(&revoked), operator, x_owned, -1)
            .await
            .unwrap()
    );

    let broken = StubResolver(Err("subgraph down".to_string()));
    assert!(
        !check_parcel_access(&pool, Some(&broken), operator, x_owned, -1)
            .await
            .unwrap(),
        "resolver outage must deny operators (fail-closed)"
    );
    assert!(
        check_parcel_access(&pool, Some(&broken), owner, x_owned, -1)
            .await
            .unwrap(),
        "resolver outage must never lock out owners"
    );

    sqlx::query("DELETE FROM squid_marketplace.parcel WHERE x = $1 OR x = $2")
        .bind(x_owned)
        .bind(x_estate)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM squid_marketplace.estate WHERE id = $1")
        .bind(format!("estate-{x_estate}"))
        .execute(&pool)
        .await
        .ok();

    drop_schema(&pool, &schema).await;
}

async fn make_deployer(pool: &PgPool, storage_root: &str) -> WriteDeployer {
    let storage = catalyrst_storage::ContentStorage::new(storage_root)
        .await
        .expect("content storage");
    WriteDeployer::new(
        pool.clone(),
        std::sync::Arc::new(storage),
        pool.clone(),
        "https://rpc.decentraland.org/mainnet".to_string(),
        false,
        None,
        None,
        None,
        false,
        None,
    )
}

fn signed_scene_entity(
    wallet: &catalyrst_crypto::sign::Wallet,
    pointer: &str,
    ts_ms: i64,
) -> (Vec<u8>, String, serde_json::Value) {
    let entity = serde_json::json!({
        "version": "v3",
        "type": "scene",
        "pointers": [pointer],
        "timestamp": ts_ms,
        "content": [],
        "metadata": { "display": { "title": "landpub e2e" } },
    });
    let bytes = serde_json::to_vec(&entity).expect("entity json");
    let entity_id = catalyrst_hashing::hash_bytes_v1(&bytes);
    let chain = catalyrst_crypto::create_simple_auth_chain(wallet, &entity_id).expect("chain");
    (bytes, entity_id, chain)
}

#[tokio::test]
async fn publish_roundtrip_owner_deploys_stranger_denied_then_unpublish() {
    let Some((pool, schema)) = setup_db().await else {
        return;
    };
    seed_squid(&pool).await;

    let owner_wallet = catalyrst_crypto::sign::Wallet::from_hex(
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    )
    .expect("owner wallet");
    let stranger_wallet = catalyrst_crypto::sign::Wallet::from_hex(
        "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
    )
    .expect("stranger wallet");

    let x = unique_coord();
    let pointer = format!("{x},-7");
    sqlx::query(
        "INSERT INTO squid_marketplace.parcel (id, x, y, owner_id, estate_id) \
         VALUES ($1, $2, -7, $3, NULL)",
    )
    .bind(format!("parcel-{x}"))
    .bind(x)
    .bind(format!("{}-ETHEREUM", owner_wallet.address()))
    .execute(&pool)
    .await
    .expect("seed parcel");

    let storage_root = format!("{}/landpub-e2e-{}", std::env::temp_dir().display(), schema);
    let now_ms = chrono::Utc::now().timestamp_millis();

    let (bytes, entity_id, chain) = signed_scene_entity(&owner_wallet, &pointer, now_ms);
    let deployer = make_deployer(&pool, &storage_root).await;
    deployer
        .deploy_entity(vec![bytes.into()], &entity_id, chain, "LOCAL")
        .await
        .expect("owner deploy must succeed");

    assert_eq!(
        active_pointer(&pool, &pointer).await.as_deref(),
        Some(entity_id.as_str())
    );
    let p = local_provenance(&pool, &entity_id)
        .await
        .expect("provenance query")
        .expect("provenance recorded via LOCAL context");
    assert_eq!(p["status"], "active");
    assert_eq!(p["signer"], owner_wallet.address().to_lowercase());

    let (bytes2, entity_id2, chain2) =
        signed_scene_entity(&stranger_wallet, &pointer, now_ms + 1_000);
    let deployer2 = make_deployer(&pool, &storage_root).await;
    let errors = deployer2
        .deploy_entity(vec![bytes2.into()], &entity_id2, chain2, "LOCAL")
        .await
        .expect_err("stranger deploy must be rejected");
    assert!(
        errors.iter().any(|e| e.contains("does not have access")),
        "expected parcel-access denial, got: {errors:?}"
    );

    let outcome = tombstone_and_repoint(&pool, &pointer)
        .await
        .expect("unpublish");
    assert_eq!(outcome.entity_id, entity_id);
    assert_eq!(active_pointer(&pool, &pointer).await, None);

    sqlx::query("DELETE FROM squid_marketplace.parcel WHERE x = $1")
        .bind(x)
        .execute(&pool)
        .await
        .ok();
    tokio::fs::remove_dir_all(&storage_root).await.ok();
    drop_schema(&pool, &schema).await;
}
