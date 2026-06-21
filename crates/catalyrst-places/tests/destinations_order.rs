use std::time::Duration;

use rand::RngExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use catalyrst_places::ports::places::{PlaceListFilters, PlacesComponent};

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_PLACES_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/places".into()))
}

fn unique_schema() -> String {
    let b: [u8; 8] = rand::rng().random();
    format!("test_dest_order_{}", hex::encode(b))
}

async fn setup() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
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
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!(
            "skipping destinations_float_highlighted_then_ranking_above_order_by: no postgres reachable"
        );
        return;
    };
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

    cleanup(&admin_url, &schema).await;
}
