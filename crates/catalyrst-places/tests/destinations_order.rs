use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

use catalyrst_places::ports::places::{PlaceListFilters, PlacesComponent};

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

async fn seed(pool: &PgPool, id: &str, highlighted: bool, ranking: Option<f64>, like_score: f64) {
    let mut raw = serde_json::json!({ "like_score": like_score });
    if let Some(r) = ranking {
        raw["ranking"] = serde_json::json!(r);
    }
    sqlx::query("INSERT INTO place (id, base_position, highlighted, raw) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("0,0")
        .bind(highlighted)
        .bind(raw)
        .execute(pool)
        .await
        .expect("seed place");
}

#[tokio::test]
async fn destinations_float_highlighted_then_ranking_above_order_by() {
    let Some(scratch) = setup("cg_places_destorder").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    seed(&pool, "A", false, None, 0.9).await;
    seed(&pool, "B", true, None, 0.5).await;
    seed(&pool, "C", false, Some(5.0), 0.3).await;
    seed(&pool, "D", false, Some(1.0), 0.1).await;

    let places = PlacesComponent::new(pool.clone());

    let dest = places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            destinations_mode: true,
            ..Default::default()
        })
        .await
        .expect("destinations list");
    let dest_ids: Vec<&str> = dest.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        dest_ids,
        vec!["B", "C", "D", "A"],
        "destinations must sort highlighted then ranking above the order_by column"
    );

    let plc = places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            destinations_mode: false,
            ..Default::default()
        })
        .await
        .expect("places list");
    let plc_ids: Vec<&str> = plc.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        plc_ids,
        vec!["A", "B", "C", "D"],
        "/api/places must NOT apply the highlighted+ranking prefix"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_page_carries_the_same_rows_whatever_the_limit_is() {
    let Some(scratch) = setup("cg_places_destorder_total").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    for i in 0..12 {
        seed(&pool, &format!("tied-{i:02}"), false, None, 0.0).await;
    }

    let places = PlacesComponent::new(pool.clone());
    let page = |limit: i64| {
        let places = &places;
        async move {
            places
                .find_list(&PlaceListFilters {
                    limit,
                    order_desc: true,
                    destinations_mode: true,
                    ..Default::default()
                })
                .await
                .expect("destinations list")
                .into_iter()
                .map(|r| r.id)
                .collect::<Vec<_>>()
        }
    };

    let short = page(5).await;
    let long = page(13).await;
    assert_eq!(short.len(), 5);
    assert_eq!(long.len(), 12);
    assert_eq!(
        short,
        long[..5].to_vec(),
        "the first page must not depend on how many rows were asked for"
    );
    assert_eq!(
        long,
        (0..12).map(|i| format!("tied-{i:02}")).collect::<Vec<_>>(),
        "rows tied on every other column fall back to the primary key"
    );

    scratch.drop().await;
}

async fn seed_content_place(pool: &PgPool, id: &str, deployed_at: &str, updated_at: Option<&str>) {
    let mut raw = serde_json::json!({
        "id": id,
        "base_position": "0,0",
        "positions": ["0,0"],
        "categories": [],
        "disabled": false,
        "world": false,
        "deployed_at": deployed_at,
        "source": "content",
    });
    if let Some(u) = updated_at {
        raw["updated_at"] = serde_json::json!(u);
    }
    sqlx::query(
        "INSERT INTO place (id, base_position, deployed_at, raw) \
         VALUES ($1, '0,0', $2::timestamptz, $3)",
    )
    .bind(id)
    .bind(deployed_at)
    .bind(raw)
    .execute(pool)
    .await
    .expect("seed content place");
}

#[tokio::test]
async fn a_content_derived_feed_still_ends_newest_deployed_first() {
    let Some(scratch) = setup("cg_places_destorder_tail").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    seed_content_place(
        &pool,
        "aaaaaaaa-0000-5000-8000-000000000001",
        "2026-01-01T00:00:00Z",
        None,
    )
    .await;
    seed_content_place(
        &pool,
        "bbbbbbbb-0000-5000-8000-000000000002",
        "2026-02-01T00:00:00Z",
        None,
    )
    .await;
    seed_content_place(
        &pool,
        "cccccccc-0000-5000-8000-000000000003",
        "2026-03-01T00:00:00Z",
        None,
    )
    .await;
    seed_content_place(
        &pool,
        "dddddddd-0000-5000-8000-000000000004",
        "2025-01-01T00:00:00Z",
        Some("2026-04-01T00:00:00Z"),
    )
    .await;

    let places = PlacesComponent::new(pool.clone());
    let rows = places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            destinations_mode: true,
            ..Default::default()
        })
        .await
        .expect("destinations list");
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "dddddddd-0000-5000-8000-000000000004",
            "cccccccc-0000-5000-8000-000000000003",
            "bbbbbbbb-0000-5000-8000-000000000002",
            "aaaaaaaa-0000-5000-8000-000000000001",
        ],
        "a raw with no updated_at must fall back to the deployment time, not to the identifier"
    );

    scratch.drop().await;
}
