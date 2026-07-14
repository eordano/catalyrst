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

const WORLD_ID: &str = "world:flagtag.dcl.eth";
const WORLD_NAME: &str = "flagtag.dcl.eth";
const WORLD_TITLE: &str = "Flag Tag";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "cg_places_worldname").await
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

async fn seed_world(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO place_world_local \
         (id, base_position, title, deployed_at, raw, world, world_name) \
         VALUES ($1, '0,0', $2, now(), $3, true, $4)",
    )
    .bind(WORLD_ID)
    .bind(WORLD_TITLE)
    .bind(serde_json::json!({ "world": true, "world_name": WORLD_NAME }))
    .bind(WORLD_NAME)
    .execute(pool)
    .await
    .expect("seed world");
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
async fn destination_picker_search_still_finds_a_world_by_name() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;
    seed_world(&scratch.pool).await;

    let body = get_json(&scratch.pool, "/api/worlds?limit=24&search=flagtag").await;
    let rows = body["data"].as_array().expect("data array");
    assert_eq!(
        rows.len(),
        1,
        "the picker's ?search=flagtag must reach the world: {body}"
    );
    assert_eq!(rows[0]["id"], Value::String(WORLD_ID.into()));
    assert_eq!(rows[0]["world_name"], Value::String(WORLD_NAME.into()));
    assert_eq!(rows[0]["world"], Value::Bool(true));

    let named = get_json(&scratch.pool, "/api/worlds?limit=24&names=FlagTag.DCL.eth").await;
    let named_rows = named["data"].as_array().expect("data array");
    assert_eq!(
        named_rows.len(),
        1,
        "names= must still match case-insensitively on the world_name column: {named}"
    );

    let listed = get_json(&scratch.pool, "/api/world_names").await;
    assert_eq!(
        listed["data"],
        serde_json::json!([WORLD_NAME]),
        "world_names must read the promoted column"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_world_without_a_name_cannot_be_stored() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;
    seed_world(&scratch.pool).await;

    let null_name = sqlx::query(
        "INSERT INTO place_world_local (id, base_position, raw, world, world_name) \
         VALUES ('world:nameless', '0,0', '{\"world\":true}'::jsonb, true, NULL)",
    )
    .execute(&scratch.pool)
    .await;
    assert!(
        null_name.is_err(),
        "world=true with a NULL world_name must be rejected"
    );

    let blank_name = sqlx::query(
        "INSERT INTO place_world_local (id, base_position, raw, world, world_name) \
         VALUES ('world:blank', '0,0', '{\"world\":true}'::jsonb, true, '   ')",
    )
    .execute(&scratch.pool)
    .await;
    assert!(
        blank_name.is_err(),
        "world=true with a blank world_name must be rejected"
    );

    let unname = sqlx::query("UPDATE place_world_local SET world_name = NULL WHERE id = $1")
        .bind(WORLD_ID)
        .execute(&scratch.pool)
        .await;
    assert!(
        unname.is_err(),
        "an existing world must not be able to lose its name"
    );

    sqlx::query("INSERT INTO place (id, base_position, raw) VALUES ($1, '1,1', $2)")
        .bind("mirror:nameless")
        .bind(serde_json::json!({ "world": true }))
        .execute(&scratch.pool)
        .await
        .expect("the mirror must accept whatever upstream sends");

    let nameless: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM place_indexed WHERE world AND (world_name IS NULL OR btrim(world_name) = '')",
    )
    .fetch_one(&scratch.pool)
    .await
    .expect("count nameless worlds");
    assert_eq!(
        nameless, 0,
        "place_indexed must never surface a world without a name"
    );

    let mirrored_is_a_place: bool =
        sqlx::query_scalar("SELECT world IS FALSE FROM place_indexed WHERE id = 'mirror:nameless'")
            .fetch_one(&scratch.pool)
            .await
            .expect("mirror row");
    assert!(
        mirrored_is_a_place,
        "an unnamed upstream row reads as a place, never as a nameless world"
    );

    scratch.drop().await;
}
