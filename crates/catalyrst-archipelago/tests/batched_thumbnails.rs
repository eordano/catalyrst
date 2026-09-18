use catalyrst_archipelago::content::ContentResolver;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn thumbnails_keep_entity_scope_external_urls_and_optional_failure() {
    let Some(db) = ScratchDb::create("CATALYRST_ARCHIPELAGO_TEST_PG", "thumbnail_batch").await
    else {
        return;
    };
    sqlx::raw_sql("CREATE TABLE deployments (id serial PRIMARY KEY, entity_id text UNIQUE, entity_type text, entity_metadata jsonb, deleter_deployment int); CREATE TABLE active_pointers (pointer text, entity_id text); CREATE TABLE content_files (deployment int, key text, content_hash text);")
        .execute(&db.pool).await.unwrap();
    for (id, thumb, wrapped) in [
        ("a", Some("preview.png"), true),
        ("b", Some("preview.png"), false),
        ("c", Some("https://example.org/image.png"), true),
        ("d", Some("missing.png"), true),
        ("e", None, true),
    ] {
        let metadata = json!({"display":{"title":id,"navmapThumbnail":thumb},"scene":{"base":"0,0","parcels":["0,0"]}});
        let metadata = if wrapped {
            json!({"v":metadata})
        } else {
            metadata
        };
        let deployment: i32 = sqlx::query_scalar("INSERT INTO deployments (entity_id,entity_type,entity_metadata) VALUES ($1,'scene',$2) RETURNING id").bind(id).bind(metadata).fetch_one(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO active_pointers VALUES ($1,$1),($1,$1)")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO content_files VALUES ($1,'preview.png',$2)")
            .bind(deployment)
            .bind(format!("hash-{id}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let resolver = ContentResolver::new(Some(db.pool.clone()), "http://content".into(), 30);
    let pointers: Vec<_> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    let scenes = resolver.fetch_scenes(&pointers).await.unwrap();
    assert_eq!(
        capture.count(),
        1,
        "scenes and thumbnail hashes come from one statement"
    );
    assert_eq!(scenes.len(), 5);
    let thumbs: HashMap<_, _> = scenes.into_iter().map(|s| (s.id, s.thumbnail)).collect();
    assert_eq!(
        thumbs["a"].as_deref(),
        Some("http://content/contents/hash-a")
    );
    assert_eq!(
        thumbs["b"].as_deref(),
        Some("http://content/contents/hash-b")
    );
    assert_eq!(
        thumbs["c"].as_deref(),
        Some("https://example.org/image.png")
    );
    assert_eq!(thumbs["d"], None);
    assert_eq!(thumbs["e"], None);
    sqlx::query("DROP TABLE content_files")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(resolver.fetch_scenes(&pointers).await.unwrap().len(), 5);
    capture.reset();
    let uncached = ContentResolver::new(Some(db.pool.clone()), "http://content".into(), 30);
    let scenes = uncached.fetch_scenes(&pointers).await.unwrap();
    assert_eq!(
        capture.count_containing("NULL::text"),
        1,
        "a failed joined statement is retried without thumbnail hashes"
    );
    assert_eq!(
        scenes.len(),
        5,
        "optional thumbnail failures must preserve scenes"
    );
    assert!(scenes
        .iter()
        .filter(|s| s.id != "c")
        .all(|s| s.thumbnail.is_none()));
    db.drop().await;
}
