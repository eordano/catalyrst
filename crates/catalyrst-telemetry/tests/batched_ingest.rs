use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
};
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_telemetry::{
    build_state,
    handlers::{segment, sentry},
    Config,
};
use serde_json::{json, Value};

#[tokio::test]
async fn batches_preserve_quotas_contract_flags_and_valid_neighbors() {
    let Some(db) = ScratchDb::create("CATALYRST_TELEMETRY_TEST_PG", "ingest_batch").await else {
        return;
    };
    let url = catalyrst_testgate::require_pg("CATALYRST_TELEMETRY_TEST_PG").unwrap();
    let base = url.rsplit_once('/').unwrap().0;
    let mut state = build_state(&Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        database_url: format!("{base}/{}", db.database),
        admin_token: None,
    })
    .await
    .unwrap();
    std::sync::Arc::get_mut(&mut state).unwrap().contract = Some(std::sync::Arc::new(
        serde_json::from_value(json!({"events":{"tour":{"props":{"parcel":{"kind":"string"}}}}}))
            .unwrap(),
    ));
    sqlx::raw_sql("CREATE TABLE insert_calls (n int); CREATE FUNCTION count_ingest() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO insert_calls VALUES (1); RETURN NULL; END $$; CREATE TRIGGER count_ingest AFTER INSERT ON telemetry_events FOR EACH STATEMENT EXECUTE FUNCTION count_ingest();")
        .execute(&db.pool).await.unwrap();
    let batch: Vec<Value> = (0..600).map(|i| json!({"event":"tour", "properties":{"parcel": if i == 1 {json!(false)} else {json!("0,0")}}, "i":i})).collect();
    state
        .ingest
        .quotas
        .write()
        .unwrap()
        .insert("segment-test".into(), 513);
    let response = segment::batch(
        State(state.clone()),
        HeaderMap::new(),
        Bytes::from(json!({"writeKey":"segment-test","batch":batch}).to_string()),
    )
    .await;
    assert_eq!(response.0, json!({"success":true}));
    let (count, invalid, last): (i64, i64, i32) = sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE invalid_reason IS NOT NULL), max((body->>'i')::int) FROM telemetry_events").fetch_one(&db.pool).await.unwrap();
    assert_eq!((count, invalid, last), (513, 1, 512));
    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM insert_calls")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        calls, 3,
        "513 rows must take three bounded insert statements"
    );
    let _ = segment::batch(
        State(state.clone()),
        HeaderMap::new(),
        Bytes::from(json!({"writeKey":"segment-test","batch":[{"i":999}]}).to_string()),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM telemetry_events")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 513);

    sqlx::query("ALTER TABLE telemetry_events ADD CONSTRAINT reject_bad_event CHECK (body->>'reject' IS DISTINCT FROM 'yes')").execute(&db.pool).await.unwrap();
    let _ = segment::batch(
        State(state.clone()),
        HeaderMap::new(),
        Bytes::from(
            json!({"writeKey":"fallback", "batch":[{"ok":1},{"reject":"yes"},{"ok":2}]})
                .to_string(),
        ),
    )
    .await;
    let bodies: Vec<Value> = sqlx::query_scalar(
        "SELECT body FROM telemetry_events WHERE project='fallback' ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(bodies, vec![json!({"ok":1}), json!({"ok":2})]);

    state
        .ingest
        .quotas
        .write()
        .unwrap()
        .insert("sentry-test".into(), 2);
    let envelope = "{\"event_id\":\"abc\"}\n{\"type\":\"event\"}\n{\"message\":\"one\"}\n{\"type\":\"session\"}\n{\"status\":\"ok\"}\n{\"type\":\"event\"}\n{\"message\":\"excluded\"}\n";
    let response = sentry::envelope(
        State(state.clone()),
        Path("sentry-test".into()),
        HeaderMap::new(),
        Bytes::from(envelope),
    )
    .await;
    assert_eq!(response.0, json!({"id":"abc"}));
    state.writer.flush().await;
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM telemetry_events WHERE project='sentry-test' ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(kinds, vec!["event", "session"]);
    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM insert_calls")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        calls, 6,
        "three batch inserts, two fallback successes, one Sentry batch"
    );
    drop(state);
    db.drop().await;
}
