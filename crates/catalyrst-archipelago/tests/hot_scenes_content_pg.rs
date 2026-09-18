//! The per-tile scene cache and the single joined scene query behind /hot-scenes.
//! Gated on CATALYRST_ARCHIPELAGO_TEST_PG / CATALYRST_TEST_PG (a throwaway server).

use std::str::FromStr;

use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

use catalyrst_archipelago::content::ContentResolver;

async fn scratch() -> Option<(PgPool, PgPool, String)> {
    let url = std::env::var("CATALYRST_ARCHIPELAGO_TEST_PG")
        .or_else(|_| std::env::var("CATALYRST_TEST_PG"))
        .ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .ok()?;
    let schema = format!("cg_arch_hot_{}", std::process::id());
    sqlx::query(AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&admin)
    .await
    .ok()?;
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .ok()?;
    let opts = PgConnectOptions::from_str(&url)
        .ok()?
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .ok()?;
    for ddl in [
        "CREATE TABLE deployments (id bigserial PRIMARY KEY, entity_id text NOT NULL, \
         entity_type text NOT NULL, entity_metadata jsonb, deleter_deployment bigint)",
        "CREATE TABLE active_pointers (entity_id text NOT NULL, pointer text NOT NULL)",
        "CREATE TABLE content_files (deployment bigint NOT NULL, key text NOT NULL, \
         content_hash text NOT NULL)",
    ] {
        sqlx::query(ddl).execute(&pool).await.unwrap();
    }
    Some((admin, pool, schema))
}

async fn deploy(
    pool: &PgPool,
    entity_id: &str,
    entity_type: &str,
    metadata: serde_json::Value,
    pointers: &[&str],
    deleted: bool,
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO deployments (entity_id, entity_type, entity_metadata, deleter_deployment) \
         VALUES ($1, $2, $3, CASE WHEN $4 THEN 1 ELSE NULL END) RETURNING id",
    )
    .bind(entity_id)
    .bind(entity_type)
    .bind(metadata)
    .bind(deleted)
    .fetch_one(pool)
    .await
    .unwrap();
    for p in pointers {
        sqlx::query("INSERT INTO active_pointers (entity_id, pointer) VALUES ($1, $2)")
            .bind(entity_id)
            .bind(p)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

fn scene_meta(base: &str, parcels: &[&str], thumbnail: &str) -> serde_json::Value {
    json!({
        "display": { "title": format!("scene at {base}"), "navmapThumbnail": thumbnail },
        "scene": { "base": base, "parcels": parcels },
        "contact": { "name": "creator" },
    })
}

#[tokio::test]
async fn hot_scenes_resolve_thumbnails_in_the_scene_query_and_cache_per_tile() {
    let Some((admin, pool, schema)) = scratch().await else {
        return;
    };

    let s1 = deploy(
        &pool,
        "bafy-s1",
        "scene",
        scene_meta("0,0", &["0,0", "0,1"], "thumb.png"),
        &["0,0", "0,1"],
        false,
    )
    .await;
    sqlx::query("INSERT INTO content_files (deployment, key, content_hash) VALUES ($1, 'thumb.png', 'bafy-thumb')")
        .bind(s1)
        .execute(&pool)
        .await
        .unwrap();
    deploy(
        &pool,
        "bafy-s2",
        "scene",
        json!({ "v": scene_meta("5,5", &["5,5"], "https://cdn/t.png") }),
        &["5,5"],
        false,
    )
    .await;
    deploy(
        &pool,
        "bafy-s3",
        "scene",
        scene_meta("9,9", &["9,9"], "gone.png"),
        &["9,9"],
        true,
    )
    .await;
    deploy(&pool, "bafy-p1", "profile", json!({}), &["0,0"], false).await;

    let resolver = ContentResolver::new(Some(pool.clone()), "https://content/".into(), 60);
    let tiles: Vec<String> = ["0,0", "5,5", "9,9", "7,7"]
        .iter()
        .map(|t| t.to_string())
        .collect();
    let scenes = resolver.fetch_scenes(&tiles).await.expect("scenes");
    let ids: Vec<&str> = scenes.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["bafy-s1", "bafy-s2"],
        "deleted and non-scene entities are skipped"
    );
    assert_eq!(
        scenes[0].thumbnail.as_deref(),
        Some("https://content/contents/bafy-thumb"),
        "a file-key thumbnail resolves to its content hash in the same query"
    );
    assert_eq!(scenes[0].parcels, vec!["0,0", "0,1"]);
    assert_eq!(scenes[0].creator.as_deref(), Some("creator"));
    assert_eq!(
        scenes[1].thumbnail.as_deref(),
        Some("https://cdn/t.png"),
        "an http thumbnail is kept verbatim; v-wrapped metadata is unwrapped"
    );
    assert_eq!(scenes[1].base, [5, 5]);

    deploy(
        &pool,
        "bafy-s4",
        "scene",
        scene_meta("7,7", &["7,7"], "x.png"),
        &["7,7"],
        false,
    )
    .await;
    let again = resolver
        .fetch_scenes(&["0,1".to_string(), "7,7".to_string()])
        .await
        .expect("scenes");
    let ids: Vec<&str> = again.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["bafy-s1"],
        "an unseen tile is queried, a tile seen inside the TTL is served from its entry"
    );

    pool.close().await;
    sqlx::query(AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
}
