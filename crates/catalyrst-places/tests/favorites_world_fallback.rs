use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

use catalyrst_places::handlers::federation::lookup_entity;
use catalyrst_places::ports::places::PlacesComponent;

const PLACE_UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
const MISSING_UUID: &str = "00000000-0000-0000-0000-000000000000";
const WORLD_NAME: &str = "my-world.dcl.eth";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_favfallback",
    )
    .await
}

async fn create_place_table(pool: &PgPool) {
    sqlx::query(
        r#"
        CREATE TABLE place (
            id             text PRIMARY KEY,
            title          text,
            description    text,
            creator_address text,
            base_position  text NOT NULL,
            content_rating text,
            disabled       boolean NOT NULL DEFAULT false,
            favorites      integer NOT NULL DEFAULT 0,
            likes          integer NOT NULL DEFAULT 0,
            dislikes       integer NOT NULL DEFAULT 0,
            categories     text[]  NOT NULL DEFAULT '{}',
            highlighted    boolean NOT NULL DEFAULT false,
            deployed_at    timestamptz,
            raw            jsonb   NOT NULL DEFAULT '{}'::jsonb
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create place table");

    sqlx::raw_sql(include_str!("../migrations/0002_place_indexed.sql"))
        .execute(pool)
        .await
        .expect("create place_indexed");

    sqlx::raw_sql(include_str!("../migrations/0003_place_world_name.sql"))
        .execute(pool)
        .await
        .expect("promote world_name");
}

async fn seed(pool: &PgPool, id: &str, raw: serde_json::Value) {
    sqlx::query("INSERT INTO place (id, base_position, raw) VALUES ($1, $2, $3)")
        .bind(id)
        .bind("0,0")
        .bind(raw)
        .execute(pool)
        .await
        .expect("seed place");
}

#[tokio::test]
async fn places_route_falls_back_to_world_for_legacy_world_name() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    seed(&pool, PLACE_UUID, serde_json::json!({})).await;
    seed(
        &pool,
        "world-entity-1",
        serde_json::json!({ "world": true, "world_name": WORLD_NAME }),
    )
    .await;

    let places = PlacesComponent::new(pool.clone());

    let hit = lookup_entity(&places, WORLD_NAME, false)
        .await
        .expect("lookup ok");
    assert_eq!(
        hit.as_ref().map(|p| p.id.as_str()),
        Some("world-entity-1"),
        "/places route must fall back to world lookup for a legacy world name"
    );

    let hit_upper = lookup_entity(&places, &WORLD_NAME.to_ascii_uppercase(), false)
        .await
        .expect("lookup ok");
    assert_eq!(
        hit_upper.as_ref().map(|p| p.id.as_str()),
        Some("world-entity-1"),
        "world-name fallback must be case-insensitive"
    );

    let place = lookup_entity(&places, PLACE_UUID, false)
        .await
        .expect("lookup ok");
    assert_eq!(place.as_ref().map(|p| p.id.as_str()), Some(PLACE_UUID));

    let missing = lookup_entity(&places, MISSING_UUID, false)
        .await
        .expect("lookup ok");
    assert!(
        missing.is_none(),
        "an absent place UUID must not trigger a world fallback"
    );

    let nothing = lookup_entity(&places, "no-such-world.dcl.eth", false)
        .await
        .expect("lookup ok");
    assert!(nothing.is_none());

    let world = lookup_entity(&places, WORLD_NAME, true)
        .await
        .expect("lookup ok");
    assert_eq!(
        world.as_ref().map(|p| p.id.as_str()),
        Some("world-entity-1")
    );

    scratch.drop().await;
}
