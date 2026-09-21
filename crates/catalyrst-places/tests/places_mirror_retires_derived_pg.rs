use std::net::SocketAddr;

use axum::routing::get;
use axum::{Json, Router};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::catalog::ensure_schema;
use catalyrst_places::catalog::mirror::run_once;
use serde_json::{json, Value};
use sqlx::PgPool;

const UPSTREAM_PLAZA: &str = "3f0d2c1e-0000-4000-8000-000000000001";
const DERIVED_PLAZA: &str = "derived-0-0";
const WORLD: &str = "harbour.dcl.eth";

async fn setup(schema: &str) -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        schema,
    )
    .await
}

async fn upstream(places: Vec<Value>) -> String {
    let total = places.len();
    let app = Router::new().route(
        "/api/places",
        get(move || {
            let places = places.clone();
            async move { Json(json!({ "ok": true, "total": total, "data": places })) }
        }),
    );
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind the stub upstream");
    let addr = listener.local_addr().expect("stub upstream address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

async fn seed(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO place (id, base_position, title, raw) VALUES \
         ($1, '0,0', 'Genesis Plaza', \
          jsonb_build_object('source', 'content', 'world', false, 'positions', jsonb_build_array('0,0'))), \
         ($2, '0,0', 'Harbour', \
          jsonb_build_object('world', true, 'world_name', $2::text))",
    )
    .bind(DERIVED_PLAZA)
    .bind(WORLD)
    .execute(pool)
    .await
    .expect("seed a derived place and a world");
}

async fn place_ids(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM place ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("read place ids")
}

#[tokio::test]
async fn a_complete_mirror_cycle_replaces_the_derived_rows_and_keeps_worlds() {
    let Some(scratch) = setup("cg_places_mirror_retires").await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    seed(&pool).await;
    let base = upstream(vec![json!({
        "id": UPSTREAM_PLAZA,
        "title": "Genesis Plaza",
        "base_position": "0,0",
        "positions": ["0,0"],
        "categories": ["poi"],
        "likes": 40,
        "highlighted": true,
        "world": false,
    })])
    .await;

    let (mirrored, retired) = run_once(&pool, &reqwest::Client::new(), &base)
        .await
        .expect("mirror cycle");

    assert_eq!((mirrored, retired), (1, 1));
    assert_eq!(place_ids(&pool).await, vec![UPSTREAM_PLAZA, WORLD]);
}

#[tokio::test]
async fn an_empty_upstream_answer_leaves_the_derived_rows_in_place() {
    let Some(scratch) = setup("cg_places_mirror_empty").await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    seed(&pool).await;
    let base = upstream(vec![]).await;

    let (mirrored, retired) = run_once(&pool, &reqwest::Client::new(), &base)
        .await
        .expect("mirror cycle");

    assert_eq!((mirrored, retired), (0, 0));
    assert_eq!(place_ids(&pool).await, vec![DERIVED_PLAZA, WORLD]);
}
