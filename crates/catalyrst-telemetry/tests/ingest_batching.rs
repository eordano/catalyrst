use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::{json, Value};

use catalyrst_telemetry::handlers::{segment, sentry};
use catalyrst_telemetry::{build_state, AppState, Config};

struct Scratch {
    state: AppState,
    db: ScratchDb,
}

const PG_VAR: &str = "CATALYRST_TELEMETRY_TEST_PG";

async fn setup() -> Option<Scratch> {
    let url = catalyrst_testgate::require_pg(PG_VAR)?;
    let db = ScratchDb::builder(PG_VAR, "cg_telem_ingest")
        .schemas(["telemetry"])
        .build()
        .await?;
    let (prefix, _) = url
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{PG_VAR} is not a postgres URL: {url}"));
    let bare = format!("{prefix}/{}", db.database);
    let cfg = Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        database_url: format!("{bare}?options=-c%20search_path%3Dtelemetry"),
        admin_token: None,
    };
    let state = build_state(&cfg).await.expect("build telemetry state");
    Some(Scratch { state, db })
}

async fn rows(st: &AppState) -> Vec<(String, String, String, Value)> {
    sqlx::query_as(
        "SELECT source, project, event_kind, body FROM telemetry.telemetry_events ORDER BY id",
    )
    .fetch_all(&st.pool)
    .await
    .expect("rows")
}

fn bytes(v: &Value) -> Bytes {
    Bytes::from(serde_json::to_vec(v).unwrap())
}

#[tokio::test]
async fn batch_single_envelope_and_store_land_in_order() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();

    let batch = json!({ "writeKey": "wk", "batch": [
        { "type": "track", "event": "a", "properties": { "n": 1 } },
        { "type": "identify", "userId": "u1" },
        { "type": "track", "event": "b" },
    ]});
    let _ = segment::batch(State(st.clone()), HeaderMap::new(), bytes(&batch)).await;
    let _ = segment::single(
        State(st.clone()),
        HeaderMap::new(),
        bytes(&json!({ "writeKey": "wk", "type": "page", "name": "home" })),
    )
    .await;

    let envelope = concat!(
        "{\"event_id\":\"e1\"}\n",
        "{\"type\":\"event\"}\n",
        "{\"message\":\"boom\"}\n",
        "{\"type\":\"session\"}\n",
        "{\"status\":\"ok\"}\n",
    );
    let out = sentry::envelope(
        State(st.clone()),
        Path("proj".into()),
        HeaderMap::new(),
        Bytes::from_static(envelope.as_bytes()),
    )
    .await;
    assert_eq!(out.0["id"], json!("e1"));
    let _ = sentry::envelope(
        State(st.clone()),
        Path("proj".into()),
        HeaderMap::new(),
        Bytes::from_static(b"{\"event_id\":\"e2\"}\n"),
    )
    .await;
    let _ = sentry::store(
        State(st.clone()),
        Path("proj".into()),
        HeaderMap::new(),
        bytes(&json!({ "event_id": "e3", "message": "stored" })),
    )
    .await;

    st.writer.flush().await;
    let got = rows(&st).await;
    let shape: Vec<(&str, &str, &str)> = got
        .iter()
        .map(|(s, p, k, _)| (s.as_str(), p.as_str(), k.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("segment", "wk", "track"),
            ("segment", "wk", "identify"),
            ("segment", "wk", "track"),
            ("segment", "wk", "page"),
            ("sentry", "proj", "event"),
            ("sentry", "proj", "session"),
            ("sentry", "proj", "envelope"),
            ("sentry", "proj", "event"),
        ]
    );
    assert_eq!(got[0].3["properties"]["n"], json!(1));
    assert_eq!(got[4].3["message"], json!("boom"));
    assert!(got[6].3["raw"].as_str().unwrap().contains("e2"));
    assert_eq!(got[7].3["event_id"], json!("e3"));

    drop(st);
    let Scratch { state, db } = s;
    drop(state);
    db.drop().await;
}

#[tokio::test]
async fn a_bad_row_only_loses_itself() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();

    let batch = json!({ "writeKey": "wk", "batch": [
        { "type": "track", "event": "ok1" },
        { "type": "track", "event": "nul\u{0}byte" },
        { "type": "track", "event": "ok2" },
    ]});
    let _ = segment::batch(State(st.clone()), HeaderMap::new(), bytes(&batch)).await;
    let _ = segment::single(
        State(st.clone()),
        HeaderMap::new(),
        bytes(&json!({ "writeKey": "wk", "type": "track", "event": "bad\u{0}" })),
    )
    .await;
    let _ = segment::single(
        State(st.clone()),
        HeaderMap::new(),
        bytes(&json!({ "writeKey": "wk", "type": "track", "event": "ok3" })),
    )
    .await;
    st.writer.flush().await;

    let events: Vec<String> = rows(&st)
        .await
        .into_iter()
        .map(|(_, _, _, b)| b["event"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(events, vec!["ok1", "ok2", "ok3"]);

    drop(st);
    let Scratch { state, db } = s;
    drop(state);
    db.drop().await;
}
