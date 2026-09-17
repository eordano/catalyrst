use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::catalog::ensure_schema;
use catalyrst_places::catalog::sync::run_once;
use catalyrst_places::ports::places::PlacesComponent;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

const FIRST_DEPLOYMENT: &str = "2026-01-01 00:00:00";
const SECOND_DEPLOYMENT: &str = "2026-02-01 00:00:00";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_rebuild_state",
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

async fn seed_scene(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deployments \
         (deployer_address, entity_type, entity_id, entity_metadata, entity_timestamp, entity_pointers) \
         VALUES ('0x0000000000000000000000000000000000000abc', 'scene', 'entity-1', \
                 json_build_object('v', json_build_object( \
                     'scene', json_build_object('base', '7,7'), \
                     'display', json_build_object('title', 'Harbour'))), \
                 $1::timestamp, ARRAY['7,7'])",
    )
    .bind(FIRST_DEPLOYMENT)
    .execute(pool)
    .await
    .expect("seed scene deployment");
}

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("{}Z", value.replace(' ', "T")))
        .expect("parse instant")
        .with_timezone(&Utc)
}

async fn only_place_id(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT id FROM place")
        .fetch_one(pool)
        .await
        .expect("the rebuild derived exactly one place")
}

#[tokio::test]
async fn a_content_place_reports_the_deployment_time_for_both_timestamps() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    create_content_tables(&pool).await;
    seed_scene(&pool).await;

    run_once(&pool, &pool, "/content")
        .await
        .expect("catalog sync");

    let id = only_place_id(&pool).await;
    let places = PlacesComponent::new(pool.clone());
    let row = places
        .find_by_id(&id)
        .await
        .expect("read the place")
        .expect("the place exists");

    assert_eq!(
        row.created_at,
        Some(instant(FIRST_DEPLOYMENT)),
        "upstream carries created_at NOT NULL, so a content-derived place must not report null"
    );
    assert_eq!(
        row.updated_at,
        Some(instant(FIRST_DEPLOYMENT)),
        "upstream carries updated_at NOT NULL, so a content-derived place must not report null"
    );

    scratch.drop().await;
}

/// An operator disable is the only writer of these keys: nothing rebuilds them,
/// so the hourly content pass has to carry them across or the audit trail is gone
/// while the place stays disabled.
#[tokio::test]
async fn a_rebuild_keeps_the_disable_audit_and_the_first_creation_time() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    ensure_schema(&pool).await.expect("place schema");
    create_content_tables(&pool).await;
    seed_scene(&pool).await;

    run_once(&pool, &pool, "/content")
        .await
        .expect("first catalog sync");
    let id = only_place_id(&pool).await;

    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    assert!(
        places
            .set_disabled(&id, true, Some("opt_out"))
            .await
            .expect("disable the place"),
        "the disable must land on the row the rebuild derived"
    );

    sqlx::query("UPDATE deployments SET entity_timestamp = $1::timestamp")
        .bind(SECOND_DEPLOYMENT)
        .execute(&pool)
        .await
        .expect("redeploy the scene");

    let (derived, pruned) = run_once(&pool, &pool, "/content")
        .await
        .expect("second catalog sync");
    assert_eq!(derived, 1);
    assert_eq!(pruned, 0);

    let row = places
        .find_by_id(&id)
        .await
        .expect("read the place")
        .expect("the place survives the rebuild");

    assert!(
        row.disabled,
        "the rebuild must not re-enable a disabled place"
    );
    assert!(
        row.disabled_at.is_some(),
        "a rebuild that drops disabled_at reports a disabled place with no disable time"
    );
    assert_eq!(
        row.disabled_reason.as_deref(),
        Some("opt_out"),
        "a rebuild that drops disabled_reason loses why the place was disabled"
    );
    assert_eq!(
        row.created_at,
        Some(instant(FIRST_DEPLOYMENT)),
        "upstream never updates created_at from a deployment"
    );
    assert_eq!(
        row.updated_at,
        Some(instant(SECOND_DEPLOYMENT)),
        "a redeployment is exactly what moves updated_at"
    );
    assert_eq!(row.deployed_at, Some(instant(SECOND_DEPLOYMENT)));

    scratch.drop().await;
}
