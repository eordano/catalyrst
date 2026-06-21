// DB-gated integration tests for the storage read cache and the value::text
// passthrough. Set CATALYRST_WORLD_STORAGE_TEST_PG to a postgres URL to run;
// each test works in a throwaway schema and drops it on the way out.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use catalyrst_world_storage::config::{NamespaceLimits, StorageCacheConfig};
use catalyrst_world_storage::handlers::common::raw_paginated_response;
use catalyrst_world_storage::storage::Storage;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const WORLD: &str = "test.dcl.eth";
const PLACE: &str = "11111111-1111-1111-1111-111111111111";
const PLAYER_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PLAYER_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const LIMITS: NamespaceLimits = NamespaceLimits {
    max_value_size_bytes: 1_048_576,
    max_total_size_bytes: 10_485_760,
};

fn cache_cfg(enabled: bool) -> StorageCacheConfig {
    StorageCacheConfig {
        enabled,
        ttl_seconds: 300,
        max_entries: 1000,
        max_value_bytes: 32_768,
    }
}

fn unique_schema() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test_wstorage_{}_{}_{}", std::process::id(), nanos, n)
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = std::env::var("CATALYRST_WORLD_STORAGE_TEST_PG").ok()?;
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

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations apply");

    Some((pool, schema, url))
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn ser(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

async fn raw_world_value_text(pool: &PgPool, key: &str) -> Option<String> {
    sqlx::query(
        "SELECT value::text AS value FROM world_storage
         WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
    )
    .bind(WORLD)
    .bind(PLACE)
    .bind(key)
    .fetch_optional(pool)
    .await
    .unwrap()
    .map(|r| r.get("value"))
}

// A direct SQL write that bypasses Storage: a subsequent cached read must NOT see it,
// a subsequent uncached read must.
async fn sneaky_world_update(pool: &PgPool, key: &str, value: &Value) {
    sqlx::query(
        "UPDATE world_storage SET value = $4::jsonb
         WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
    )
    .bind(WORLD)
    .bind(PLACE)
    .bind(key)
    .bind(ser(value))
    .execute(pool)
    .await
    .unwrap();
}

async fn sneaky_player_update(pool: &PgPool, player: &str, key: &str, value: &Value) {
    sqlx::query(
        "UPDATE player_storage SET value = $5::jsonb
         WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3 AND key = $4",
    )
    .bind(WORLD)
    .bind(PLACE)
    .bind(player)
    .bind(key)
    .bind(ser(value))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn world_single_key_cache_hit_miss_and_write_invalidation() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping world_single_key_cache_hit_miss_and_write_invalidation: set CATALYRST_WORLD_STORAGE_TEST_PG to run");
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(true));

    assert!(storage
        .world_get(WORLD, PLACE, "k")
        .await
        .unwrap()
        .is_none());

    storage
        .world_upsert_with_quota(WORLD, PLACE, "k", &ser(&json!({"a": 1})), LIMITS)
        .await
        .unwrap();
    let first = storage.world_get(WORLD, PLACE, "k").await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&first).unwrap(),
        json!({"a": 1})
    );

    sneaky_world_update(&pool, "k", &json!({"sneaky": true})).await;
    let cached = storage.world_get(WORLD, PLACE, "k").await.unwrap().unwrap();
    assert_eq!(&*cached, &*first, "second read must be served by the cache");

    storage
        .world_upsert_with_quota(WORLD, PLACE, "k", &ser(&json!({"a": 2})), LIMITS)
        .await
        .unwrap();
    let after_write = storage.world_get(WORLD, PLACE, "k").await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&after_write).unwrap(),
        json!({"a": 2}),
        "a write must invalidate the cached value"
    );

    storage.world_delete(WORLD, PLACE, "k").await.unwrap();
    assert!(
        storage
            .world_get(WORLD, PLACE, "k")
            .await
            .unwrap()
            .is_none(),
        "a delete must invalidate the cached value"
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn player_single_key_cache_hit_miss_and_write_invalidation() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping player_single_key_cache_hit_miss_and_write_invalidation: set CATALYRST_WORLD_STORAGE_TEST_PG to run");
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(true));

    storage
        .player_upsert_with_quota(WORLD, PLACE, PLAYER_A, "k", &ser(&json!("v1")), LIMITS)
        .await
        .unwrap();
    let first = storage
        .player_get(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&first).unwrap(), json!("v1"));

    sneaky_player_update(&pool, PLAYER_A, "k", &json!("sneaky")).await;
    let cached = storage
        .player_get(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&*cached, &*first, "second read must be served by the cache");

    storage
        .player_upsert_with_quota(WORLD, PLACE, PLAYER_A, "k", &ser(&json!("v2")), LIMITS)
        .await
        .unwrap();
    let after_write = storage
        .player_get(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&after_write).unwrap(),
        json!("v2")
    );

    storage
        .player_delete(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap();
    assert!(storage
        .player_get(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap()
        .is_none());

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn delete_all_paths_invalidate_their_scope() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping delete_all_paths_invalidate_their_scope: set CATALYRST_WORLD_STORAGE_TEST_PG to run"
        );
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(true));

    for key in ["k1", "k2"] {
        storage
            .world_upsert_with_quota(WORLD, PLACE, key, "1", LIMITS)
            .await
            .unwrap();
        storage.world_get(WORLD, PLACE, key).await.unwrap();
    }
    storage.world_delete_all(WORLD, PLACE).await.unwrap();
    for key in ["k1", "k2"] {
        assert!(
            storage
                .world_get(WORLD, PLACE, key)
                .await
                .unwrap()
                .is_none(),
            "world delete_all must clear cached values for the scene"
        );
    }

    for player in [PLAYER_A, PLAYER_B] {
        storage
            .player_upsert_with_quota(WORLD, PLACE, player, "k", &ser(&json!(player)), LIMITS)
            .await
            .unwrap();
        storage.player_get(WORLD, PLACE, player, "k").await.unwrap();
    }
    storage
        .player_delete_all_for_player(WORLD, PLACE, PLAYER_A)
        .await
        .unwrap();
    assert!(storage
        .player_get(WORLD, PLACE, PLAYER_A, "k")
        .await
        .unwrap()
        .is_none());
    // The other player's cached entry survives a per-player clear: remove their row
    // behind the cache's back and the read must still be served.
    sqlx::query(
        "DELETE FROM player_storage
         WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3",
    )
    .bind(WORLD)
    .bind(PLACE)
    .bind(PLAYER_B)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        storage
            .player_get(WORLD, PLACE, PLAYER_B, "k")
            .await
            .unwrap()
            .is_some(),
        "per-player clear must not touch other players' cached values"
    );

    storage.player_delete_all(WORLD, PLACE).await.unwrap();
    assert!(
        storage
            .player_get(WORLD, PLACE, PLAYER_B, "k")
            .await
            .unwrap()
            .is_none(),
        "scene-wide player clear must drop every cached player value"
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn disabled_cache_reads_always_hit_the_database() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping disabled_cache_reads_always_hit_the_database: set CATALYRST_WORLD_STORAGE_TEST_PG to run"
        );
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(false));

    storage
        .world_upsert_with_quota(WORLD, PLACE, "k", "1", LIMITS)
        .await
        .unwrap();
    storage.world_get(WORLD, PLACE, "k").await.unwrap();
    sneaky_world_update(&pool, "k", &json!(2)).await;
    let read = storage.world_get(WORLD, PLACE, "k").await.unwrap().unwrap();
    assert_eq!(&*read, "2", "with the cache disabled every read is fresh");

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn values_pass_through_byte_identical_to_postgres_text() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping values_pass_through_byte_identical_to_postgres_text: set CATALYRST_WORLD_STORAGE_TEST_PG to run"
        );
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(true));

    let values = [
        json!(0),
        json!(false),
        json!(""),
        json!(null),
        json!({"quotes":"a\"b","backslash":"c\\d","nl":"e\nf","unicode":"héllo 🌍"}),
        json!([1, 2.5, {"nested": [true, null, "\\u0000 literal"]}]),
    ];
    for (i, v) in values.iter().enumerate() {
        let key = format!("k{}", i);
        storage
            .world_upsert_with_quota(WORLD, PLACE, &key, &ser(v), LIMITS)
            .await
            .unwrap();
        let got = storage.world_get(WORLD, PLACE, &key).await.unwrap();
        let got = got.expect("stored falsy values must read back as Some");
        let pg_text = raw_world_value_text(&pool, &key).await.unwrap();
        assert_eq!(&*got, &*pg_text, "get must return the jsonb text verbatim");
        assert_eq!(&serde_json::from_str::<Value>(&got).unwrap(), v);
    }

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn list_page_splices_row_text_verbatim() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping list_page_splices_row_text_verbatim: set CATALYRST_WORLD_STORAGE_TEST_PG to run"
        );
        return;
    };
    let storage = Storage::new(pool.clone(), cache_cfg(true));

    let rows = [
        ("plain", json!({"a": 1})),
        ("with\"quote", json!("line1\nline2")),
        ("uni🔑", json!(["héllo", 2.5, null])),
    ];
    for (key, v) in &rows {
        storage
            .world_upsert_with_quota(WORLD, PLACE, key, &ser(v), LIMITS)
            .await
            .unwrap();
        storage
            .player_upsert_with_quota(WORLD, PLACE, PLAYER_A, key, &ser(v), LIMITS)
            .await
            .unwrap();
    }

    let entries = storage
        .world_list(WORLD, PLACE, 100, 0, None)
        .await
        .unwrap();
    assert_eq!(entries.len(), rows.len());
    for e in &entries {
        let pg_text = raw_world_value_text(&pool, &e.key).await.unwrap();
        assert_eq!(
            e.value, pg_text,
            "list rows must carry the jsonb text verbatim"
        );
    }

    let body: Value = serde_json::from_str(&raw_paginated_response(&entries, 100, 0, 3).0).unwrap();
    let mut expected: Vec<(&str, &Value)> = rows.iter().map(|(k, v)| (*k, v)).collect();
    expected.sort_by_key(|(k, _)| k.to_string());
    let expected_data: Vec<Value> = expected
        .iter()
        .map(|(k, v)| json!({"key": k, "value": v}))
        .collect();
    assert_eq!(
        body,
        json!({"data": expected_data, "pagination": {"limit": 100, "offset": 0, "total": 3}})
    );

    let player_entries = storage
        .player_list(WORLD, PLACE, PLAYER_A, 100, 0, None)
        .await
        .unwrap();
    let player_body: Value =
        serde_json::from_str(&raw_paginated_response(&player_entries, 100, 0, 3).0).unwrap();
    assert_eq!(player_body["data"], json!(expected_data));

    cleanup(&admin_url, &schema).await;
}
