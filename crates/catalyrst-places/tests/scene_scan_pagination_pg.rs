use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::catalog::ensure_schema;
use catalyrst_places::catalog::sync::{fetch_scene_page, run_once, BEFORE_FIRST_SCENE};
use sqlx::{PgPool, Row};

const PAGE: i64 = 5;
const SCENES: i32 = 12;

const OFFSET_SELECT: &str = "SELECT d.id FROM deployments d \
     WHERE d.entity_type = 'scene' AND d.deleter_deployment IS NULL \
     ORDER BY d.id LIMIT $1 OFFSET $2";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_scene_scan",
    )
    .await
}

async fn create_content_tables(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        CREATE TABLE deployments (
            id                 serial PRIMARY KEY,
            deployer_address   text NOT NULL,
            entity_type        text NOT NULL,
            entity_id          text NOT NULL,
            entity_metadata    json,
            entity_timestamp   timestamp without time zone NOT NULL,
            entity_pointers    text[] NOT NULL,
            deleter_deployment integer
        );
        CREATE TABLE content_files (
            deployment   integer NOT NULL,
            content_hash text NOT NULL,
            key          text NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await
    .expect("create content tables");
}

async fn seed_scenes(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deployments \
         (deployer_address, entity_type, entity_id, entity_metadata, entity_timestamp, entity_pointers) \
         SELECT '0x0000000000000000000000000000000000000abc', 'scene', 'entity-' || g, \
                json_build_object('v', json_build_object( \
                    'scene', json_build_object('base', g || ',0'), \
                    'display', json_build_object('title', 'Harbour ' || g))), \
                now(), ARRAY[g || ',0'] \
         FROM generate_series(1, $1) g",
    )
    .bind(SCENES)
    .execute(pool)
    .await
    .expect("seed scene deployments");
}

async fn survivors(pool: &PgPool) -> Vec<i32> {
    sqlx::query("SELECT id FROM deployments WHERE deleter_deployment IS NULL ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("survivors")
        .iter()
        .map(|r| r.get::<i32, _>("id"))
        .collect()
}

/// Undeploying a scene is exactly what removes a row from the filtered set the
/// rebuild pages over, and PRUNE deletes whatever the pass did not visit.
#[tokio::test]
async fn a_scene_undeployed_mid_scan_never_hides_a_survivor_from_the_page_loop() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_content_tables(&pool).await;
    seed_scenes(&pool).await;

    let mut seen: Vec<i32> = Vec::new();
    let mut last_id = BEFORE_FIRST_SCENE;
    let first = fetch_scene_page(&pool, PAGE, last_id)
        .await
        .expect("first page");
    for row in &first {
        last_id = row.get("id");
        seen.push(last_id);
    }

    let undeployed = seen[1];
    sqlx::query("UPDATE deployments SET deleter_deployment = $1 WHERE id = $2")
        .bind(SCENES + 1)
        .bind(undeployed)
        .execute(&pool)
        .await
        .expect("undeploy a scene already visited");

    loop {
        let rows = fetch_scene_page(&pool, PAGE, last_id)
            .await
            .expect("next page");
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            last_id = row.get("id");
            seen.push(last_id);
        }
    }

    for id in survivors(&pool).await {
        assert!(
            seen.contains(&id),
            "scene {id} was never visited, so PRUNE would delete a still-served place: {seen:?}"
        );
    }

    sqlx::query("UPDATE deployments SET deleter_deployment = NULL")
        .execute(&pool)
        .await
        .expect("restore the scene set");

    let mut offset_seen: Vec<i32> = Vec::new();
    let mut offset = 0i64;
    loop {
        let rows = sqlx::query(OFFSET_SELECT)
            .bind(PAGE)
            .bind(offset)
            .fetch_all(&pool)
            .await
            .expect("offset page");
        let fetched = rows.len() as i64;
        for row in &rows {
            offset_seen.push(row.get("id"));
        }
        if offset == 0 {
            sqlx::query("UPDATE deployments SET deleter_deployment = $1 WHERE id = $2")
                .bind(SCENES + 1)
                .bind(undeployed)
                .execute(&pool)
                .await
                .expect("undeploy the same scene mid-scan");
        }
        if fetched < PAGE {
            break;
        }
        offset += fetched;
    }
    let missed: Vec<i32> = survivors(&pool)
        .await
        .into_iter()
        .filter(|id| !offset_seen.contains(id))
        .collect();
    assert!(
        !missed.is_empty(),
        "the OFFSET walk this replaced must be the one that skips a survivor: {offset_seen:?}"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_complete_scan_refreshes_every_place_and_prunes_none() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    create_content_tables(&pool).await;
    seed_scenes(&pool).await;

    let (derived, pruned) = run_once(&pool, &pool, "/content")
        .await
        .expect("first catalog sync");
    assert_eq!(derived as i32, SCENES);
    assert_eq!(pruned, 0);

    let (derived, pruned) = run_once(&pool, &pool, "/content")
        .await
        .expect("second catalog sync");
    assert_eq!(derived as i32, SCENES);
    assert_eq!(
        pruned, 0,
        "a pass that visited every scene must leave every place behind it"
    );

    let stale: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM place WHERE fetched_at < now() - interval '1 minute'",
    )
    .fetch_one(&pool)
    .await
    .expect("count stale places");
    assert_eq!(stale, 0, "every visited place carries a fresh fetched_at");

    scratch.drop().await;
}

/// Curation inheritance inside one page keeps its row-by-row meaning: a fresh
/// scene that just inherited counts as curated for the fresh scenes after it.
#[tokio::test]
async fn a_page_inherits_curation_in_scene_order() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    create_content_tables(&pool).await;
    sqlx::query(
        "INSERT INTO place (id, title, base_position, creator_address, highlighted, raw) \
         VALUES ('curated-x', 'X', '9,9', '0x0000000000000000000000000000000000000abc', true, \
                 '{\"positions\": [\"1,0\", \"2,0\", \"3,0\"]}'::jsonb)",
    )
    .execute(&pool)
    .await
    .expect("seed curated place");
    for (base, pointers) in [
        ("1,0", vec!["1,0"]),
        ("2,0", vec!["1,0", "2,0"]),
        ("3,0", vec!["3,0"]),
    ] {
        sqlx::query(
            "INSERT INTO deployments \
             (deployer_address, entity_type, entity_id, entity_metadata, entity_timestamp, entity_pointers) \
             VALUES ('0x0000000000000000000000000000000000000abc', 'scene', 'entity-' || $1, \
                     json_build_object('scene', json_build_object('base', $1::text, 'parcels', to_json($2::text[])), \
                                       'display', json_build_object('title', 'Harbour ' || $1)), \
                     now(), $2)",
        )
        .bind(base)
        .bind(pointers)
        .execute(&pool)
        .await
        .expect("seed scene");
    }

    let (derived, _) = run_once(&pool, &pool, "/content")
        .await
        .expect("catalog sync");
    assert_eq!(derived, 3);
    let highlighted = |base: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT highlighted FROM place WHERE base_position = $1 AND id <> 'curated-x'",
            )
            .bind(base)
            .fetch_one(&pool)
            .await
            .expect("derived place")
        }
    };
    assert!(highlighted("1,0").await, "one curated overlap is inherited");
    assert!(
        !highlighted("2,0").await,
        "two curated overlaps (the seed and the row before) are ambiguous"
    );
    assert!(highlighted("3,0").await);

    scratch.drop().await;
}
