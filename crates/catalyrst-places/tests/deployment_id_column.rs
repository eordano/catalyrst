// Upstream places #856 (fix/scene-base-integrity) added a nullable public
// `deployment_id` field to every Place API response, sourced from a new
// `places.deployment_id` column their ingest stamps. We read the archive, not
// the ingest path, so the value rides through in the `raw` JSON payload and is
// surfaced by PLACE_COLUMNS' `raw->>'deployment_id'`. This proves the read path
// carries the value when present and reports null (never omits) when absent --
// matching upstream's "Null only for legacy rows awaiting reconciliation".

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router_with_spec, AppState, AppStateInner};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

const DEPLOYMENT_ID: &str = "bafkreideploymentidexamplehash";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "cg_places_deployid").await
}

async fn migrate(pool: &PgPool) {
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

async fn seed_place(pool: &PgPool, id: &str, raw: Value) {
    sqlx::query(
        "INSERT INTO place (id, base_position, title, deployed_at, raw) \
         VALUES ($1, '10,20', 'a place', now(), $2)",
    )
    .bind(id)
    .bind(raw)
    .execute(pool)
    .await
    .expect("seed place");
}

async fn build_state(pool: PgPool) -> AppState {
    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    places.ensure_local_schema().await.unwrap();
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool.clone()),
        admin_addresses: vec![],
        data_team_auth_token: None,
        admin_auth_token: None,
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new("http://127.0.0.1:9".into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

async fn get_json(pool: &PgPool, uri: &str) -> Value {
    let (router, _spec) = api_router_with_spec();
    let app = router.with_state(build_state(pool.clone()).await);
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("route response");
    assert_eq!(res.status(), StatusCode::OK, "{uri} must answer 200");
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn place_api_surfaces_deployment_id_when_present_and_null_when_absent() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    seed_place(
        &scratch.pool,
        "place:with-deploy",
        serde_json::json!({ "deployment_id": DEPLOYMENT_ID }),
    )
    .await;
    seed_place(&scratch.pool, "place:legacy", serde_json::json!({})).await;

    let with = get_json(&scratch.pool, "/api/places/place:with-deploy").await;
    assert_eq!(
        with["data"]["deployment_id"],
        Value::String(DEPLOYMENT_ID.into()),
        "deployment_id in raw must ride through to the API: {with}"
    );

    let legacy = get_json(&scratch.pool, "/api/places/place:legacy").await;
    assert!(
        legacy["data"]
            .as_object()
            .unwrap()
            .contains_key("deployment_id"),
        "deployment_id must always be present, never omitted: {legacy}"
    );
    assert!(
        legacy["data"]["deployment_id"].is_null(),
        "a legacy row awaiting reconciliation must read null: {legacy}"
    );

    scratch.drop().await;
}
