use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

use catalyrst_places::ports::places::{PlaceListFilters, PlaceOrderBy, PlacesComponent};

async fn setup(schema: &str) -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        schema,
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
    let name = format!("{id}.dcl.eth");
    let mut raw = raw;
    raw["world"] = serde_json::json!(true);
    raw["world_name"] = serde_json::json!(name);
    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, deployed_at, raw, world, world_name) \
         VALUES ($1, '0,0', now(), $2, true, $3)",
    )
    .bind(id)
    .bind(raw)
    .bind(&name)
    .execute(pool)
    .await
    .expect("seed place");
}

#[tokio::test]
async fn a_malformed_raw_value_nulls_its_own_column_instead_of_aborting_the_listing() {
    let Some(scratch) = setup("cg_places_raw_cast_guard").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    seed(
        &pool,
        "good",
        serde_json::json!({
                "ranking": 4.5,
                "like_score": 0.85771024,
                "like_rate": 0.5,
                "skybox_time": "1e-300",
                "user_count": 2147483647,
                "user_visits": 2000000000,
                "created_at": "2023-10-17T16:31:57.766Z",
                "updated_at": "2024-02-29T00:00:00+00:00",
            "disabled_at": "2000-02-29"
        }),
    )
    .await;

    seed(
        &pool,
        "good_edges",
        serde_json::json!({
                "ranking": "1e306",
                "skybox_time": "1.7e308",
                "created_at": "2024-01-01T00:00:00+14:00",
                "updated_at": "2024-01-01T00:00:00-15:59",
            "disabled_at": "0001-01-01"
        }),
    )
    .await;

    seed(
        &pool,
        "garbage",
        serde_json::json!({
                "ranking": "N/A",
                "like_score": true,
                "like_rate": "",
                "skybox_time": "1e999",
                "user_count": "99999999999999999999",
                "user_visits": "twelve",
                "created_at": "2023-02-29T00:00:00Z",
                "updated_at": "1900-02-29",
            "disabled_at": "2023-13-01T00:00:00Z"
        }),
    )
    .await;

    seed(
        &pool,
        "garbage_edges",
        serde_json::json!({
                "ranking": "1e309",
                "skybox_time": "1e-400",
                "created_at": "0000-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00+16:30",
            "disabled_at": "2024-01-01T00:00:00-16:00"
        }),
    )
    .await;

    sqlx::query(
        "UPDATE place_world_local SET raw = jsonb_set(raw, '{ranking}', \
         to_jsonb(0.1::float8 + 0.2::float8)) WHERE id = 'good'",
    )
    .execute(&pool)
    .await
    .expect("seed a full-precision ranking");

    let places = PlacesComponent::new(pool.clone());

    for order_by in [
        PlaceOrderBy::LikeScore,
        PlaceOrderBy::UpdatedAt,
        PlaceOrderBy::CreatedAt,
        PlaceOrderBy::UserVisits,
        PlaceOrderBy::MostActive,
    ] {
        for destinations_mode in [false, true] {
            let rows = places
                .find_list(&PlaceListFilters {
                    limit: 100,
                    order_by,
                    order_desc: true,
                    destinations_mode,
                    ..Default::default()
                })
                .await
                .unwrap_or_else(|e| {
                    panic!("one malformed raw value aborted the whole listing ({order_by:?}, destinations={destinations_mode}): {e}")
                });
            assert_eq!(
                rows.len(),
                4,
                "every row must survive a malformed neighbour ({order_by:?})"
            );
        }
    }

    let rows = places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            ..Default::default()
        })
        .await
        .expect("listing");
    let good = rows.iter().find(|r| r.id == "good").expect("good row");
    assert_eq!(
        good.ranking,
        Some(0.1f64 + 0.2f64),
        "the guard must bound magnitude, not precision: a full-precision double \
         is exactly what our own PUT /api/destinations/ranking writes"
    );
    assert_eq!(good.like_score, Some(0.85771024));
    assert_eq!(good.like_rate, Some(0.5));
    assert_eq!(
        good.skybox_time,
        Some(1e-300),
        "a raw STRING in exponent form still reads back as the double it spells"
    );
    assert_eq!(
        good.user_count,
        Some(2147483647),
        "the whole int4 range must survive the guard"
    );
    assert_eq!(good.user_visits, 2000000000);
    assert!(good.created_at.is_some(), "an ISO timestamp still parses");
    assert!(good.updated_at.is_some(), "a leap day still parses");
    assert!(good.disabled_at.is_some(), "a leap century still parses");

    let good_edges = rows
        .iter()
        .find(|r| r.id == "good_edges")
        .expect("good_edges row");
    assert_eq!(
        good_edges.ranking,
        Some(1e306),
        "an exponent form above 1e305 is still a double the cast reads"
    );
    assert_eq!(
        good_edges.skybox_time,
        Some(1.7e308),
        "the top of the float8 range must survive the guard"
    );
    assert!(
        good_edges.created_at.is_some(),
        "a displacement Postgres accepts must still reach the cast"
    );
    assert!(
        good_edges.updated_at.is_some(),
        "the whole accepted displacement range must survive the guard"
    );
    assert!(
        good_edges.disabled_at.is_some(),
        "the first year Postgres can read must still reach the cast"
    );

    let bad = rows
        .iter()
        .find(|r| r.id == "garbage")
        .expect("garbage row");
    assert_eq!(bad.ranking, None);
    assert_eq!(bad.like_score, None);
    assert_eq!(bad.like_rate, None);
    assert_eq!(bad.skybox_time, None);
    assert_eq!(bad.user_count, None);
    assert_eq!(bad.user_visits, 0);
    assert_eq!(
        bad.created_at, None,
        "29 February of a common year must not reach the cast"
    );
    assert_eq!(
        bad.updated_at, None,
        "a century that is not a leap year must not reach the cast"
    );
    assert_eq!(
        bad.disabled_at, None,
        "an impossible month must not be read"
    );

    let bad_edges = rows
        .iter()
        .find(|r| r.id == "garbage_edges")
        .expect("garbage_edges row");
    assert_eq!(
        bad_edges.ranking, None,
        "a magnitude past float8 must not reach the cast"
    );
    assert_eq!(
        bad_edges.skybox_time, None,
        "a magnitude under the smallest subnormal must not reach the cast"
    );
    assert_eq!(
        bad_edges.created_at, None,
        "year zero is not a timestamptz and must not reach the cast"
    );
    assert_eq!(
        bad_edges.updated_at, None,
        "a timezone displacement past the Postgres cap must not reach the cast"
    );
    assert_eq!(
        bad_edges.disabled_at, None,
        "a negative displacement past the cap must not reach the cast either"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn the_like_score_expression_index_still_rejects_a_non_numeric_upstream_row() {
    let Some(scratch) = setup("cg_places_like_score_index").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    let err = sqlx::query(
        "INSERT INTO place (id, base_position, raw) \
         VALUES ('bad-like-score', '0,0', '{\"like_score\": true}'::jsonb)",
    )
    .execute(&pool)
    .await
    .expect_err(
        "0003's like_score expression index casts on write, so this INSERT must still fail; \
         when the deployment's bootstrap-places.sh pairing lands, migration 0006 must rebuild the \
         index on the guarded expression and this expectation flips",
    )
    .to_string();
    assert!(
        err.contains("double precision"),
        "the write must fail at the like_score cast, not somewhere else: {err}"
    );

    scratch.drop().await;
}
