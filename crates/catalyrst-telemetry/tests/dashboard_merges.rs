use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::{json, Value};

use catalyrst_telemetry::handlers::dashboard;
use catalyrst_telemetry::{build_state, AppState, Config};

struct Scratch {
    state: AppState,
    db: ScratchDb,
}

const PG_VAR: &str = "CATALYRST_TELEMETRY_TEST_PG";

async fn setup() -> Option<Scratch> {
    let url = catalyrst_testgate::require_pg(PG_VAR)?;
    let db = ScratchDb::builder(PG_VAR, "cg_telem_dash")
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

async fn teardown(s: Scratch) {
    let Scratch { state, db } = s;
    drop(state);
    db.drop().await;
}

async fn seed(st: &AppState, source: &str, kind: &str, secs_ago: i64, body: Value) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO telemetry.telemetry_events (source, project, event_kind, received_at, body) \
         VALUES ($1, 'p', $2, now() - make_interval(secs => $3::int), $4) RETURNING id",
    )
    .bind(source)
    .bind(kind)
    .bind(secs_ago)
    .bind(body)
    .fetch_one(&st.pool)
    .await
    .expect("seed")
}

fn pairs(v: &Value) -> Vec<(String, i64)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_i64().unwrap()))
        .collect()
}

fn q<T: serde::de::DeserializeOwned>(v: Value) -> Query<T> {
    Query(serde_json::from_value(v).unwrap())
}

#[tokio::test]
async fn merged_dashboards_keep_their_shapes() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();

    let ev = |user: &str, start: &str, level: &str, msg: &str| {
        json!({ "message": msg, "level": level, "environment": "prod", "release": "r1",
                "user": { "id": user }, "contexts": { "app": { "app_start_time": start } } })
    };
    let a1 = seed(&st, "sentry", "event", 30, ev("u1", "A", "error", "e1")).await;
    seed(&st, "sentry", "event", 20, ev("u1", "A", "info", "i1")).await;
    seed(&st, "sentry", "event", 10, ev("u1", "A", "fatal", "f1")).await;
    seed(&st, "sentry", "event", 25, ev("u1", "B", "info", "i2")).await;
    let u2 = seed(&st, "sentry", "event", 5, ev("u2", "A", "warning", "w1")).await;
    let sess = |status: &str, did: &str, rel: &str| json!({ "status": status, "did": did, "attrs": { "release": rel } });
    let s1 = seed(&st, "sentry", "session", 40, sess("ok", "d1", "r1")).await;
    seed(&st, "sentry", "session", 39, sess("crashed", "d1", "r1")).await;
    seed(&st, "sentry", "session", 38, sess("exited", "d2", "r1")).await;
    seed(&st, "sentry", "session", 37, sess("", "d2", "r2")).await;

    let x1 = seed(
        &st,
        "segment",
        "track",
        60,
        json!({ "event": "a", "userId": "x", "properties": {} }),
    )
    .await;
    seed(
        &st,
        "segment",
        "track",
        50,
        json!({ "event": "b", "userId": "x", "context": { "campaign": { "name": "old" } } }),
    )
    .await;
    seed(
        &st,
        "segment",
        "track",
        40,
        json!({ "event": "a", "userId": "y", "properties": { "campaign": { "name": "new" } } }),
    )
    .await;
    seed(&st, "segment", "identify", 30, json!({ "userId": "x" })).await;
    let orphan = seed(&st, "segment", "page", 20, json!({ "name": "home" })).await;
    seed(
        &st,
        "segment",
        "track",
        3600 * 30,
        json!({ "event": "a", "userId": "x" }),
    )
    .await;

    let stats = dashboard::stats(State(st.clone()), q(json!({ "hours": 24 })))
        .await
        .expect("stats")
        .0;
    assert_eq!(stats["total"], json!(14));
    assert_eq!(stats["bucket"], json!("hour"));
    assert_eq!(
        pairs(&stats["by_source"]),
        vec![("sentry".into(), 9), ("segment".into(), 5)]
    );
    assert_eq!(
        pairs(&stats["by_kind"])[0],
        ("event".to_string(), 5),
        "{}",
        stats["by_kind"]
    );
    assert_eq!(pairs(&stats["by_env"]), vec![("prod".into(), 5)]);
    assert_eq!(pairs(&stats["by_release"]), vec![("r1".into(), 5)]);
    let levels = pairs(&stats["by_level"]);
    assert_eq!(levels[0], ("(none)".to_string(), 9));
    assert_eq!(levels.iter().map(|(_, c)| c).sum::<i64>(), 14);
    assert_eq!(
        pairs(&stats["series"]).iter().map(|(_, c)| c).sum::<i64>(),
        14
    );
    let scoped = dashboard::stats(
        State(st.clone()),
        q(json!({ "hours": 24, "source": "segment" })),
    )
    .await
    .expect("stats scoped")
    .0;
    assert_eq!(scoped["total"], json!(5));
    assert_eq!(scoped["by_env"], json!([]));

    let health = dashboard::health(State(st.clone()), q(json!({ "hours": 24 })))
        .await
        .expect("health")
        .0;
    assert_eq!(health["total"], json!(4));
    assert_eq!(health["crashed"], json!(1));
    assert_eq!(health["unhealthy"], json!(1));
    assert_eq!(health["total_users"], json!(2));
    assert_eq!(health["crashed_users"], json!(1));
    assert_eq!(health["crash_free_rate"], json!(75.0));
    assert_eq!(health["crash_free_users_rate"], json!(50.0));
    assert_eq!(pairs(&health["by_status"])[0], ("ok".to_string(), 2));
    assert_eq!(health["by_release"][0]["release"], json!("r1"));
    assert_eq!(health["by_release"][0]["sessions"], json!(3));
    assert_eq!(health["by_release"][1]["release"], json!("r2"));
    assert_eq!(health["by_release"][1]["crash_free"], json!(100.0));
    assert_eq!(
        pairs(&health["series"]).iter().map(|(_, c)| c).sum::<i64>(),
        4
    );
    let filtered = dashboard::health(
        State(st.clone()),
        q(json!({ "hours": 24, "release": "r2" })),
    )
    .await
    .expect("health r2")
    .0;
    assert_eq!(filtered["total"], json!(1));
    assert_eq!(filtered["total_users"], json!(1));

    let metrics = dashboard::metrics(State(st.clone()), q(json!({ "hours": 24 })))
        .await
        .expect("metrics")
        .0;
    assert_eq!(metrics["total"], json!(5));
    assert_eq!(metrics["users"], json!(2));
    assert_eq!(
        pairs(&metrics["by_event"]),
        vec![("a".into(), 2), ("b".into(), 1)]
    );
    assert_eq!(pairs(&metrics["by_type"])[0], ("track".to_string(), 3));
    assert_eq!(
        pairs(&metrics["series"])
            .iter()
            .map(|(_, c)| c)
            .sum::<i64>(),
        5
    );

    let sess_a = dashboard::session(State(st.clone()), Path(a1))
        .await
        .expect("session")
        .0;
    assert_eq!(sess_a["user"], json!("u1"));
    assert_eq!(sess_a["app_start"], json!("A"));
    assert_eq!(sess_a["anchor"], json!(a1));
    assert_eq!(sess_a["total"], json!(3));
    assert_eq!(sess_a["errors"], json!(2));
    assert!(sess_a["first"].as_str().unwrap() <= sess_a["last"].as_str().unwrap());
    let titles: Vec<&str> = sess_a["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["e1", "i1", "f1"]);
    assert_eq!(sess_a["events"][0]["id"], json!(a1));
    assert_eq!(sess_a["events"][0]["kind"], json!("event"));
    assert_eq!(pairs(&sess_a["by_level"]).len(), 3);
    let sess_u2 = dashboard::session(State(st.clone()), Path(u2))
        .await
        .expect("session u2")
        .0;
    assert_eq!(sess_u2["total"], json!(1));
    let sess_none = dashboard::session(State(st.clone()), Path(s1))
        .await
        .expect("session without user")
        .0;
    assert_eq!(sess_none, json!({ "user": null, "events": [] }));
    let sess_missing = dashboard::session(State(st.clone()), Path(-1))
        .await
        .expect("session missing")
        .0;
    assert_eq!(sess_missing, json!({ "user": null, "events": [] }));

    let story = dashboard::story(State(st.clone()), Path(x1))
        .await
        .expect("story")
        .0;
    assert_eq!(story["user"], json!("x"));
    assert_eq!(story["count"], json!(3));
    assert_eq!(story["utm"], json!({ "name": "old" }));
    let events = story["events"].as_array().unwrap();
    assert_eq!(events[0]["id"], json!(x1));
    assert_eq!(events[0]["current"], json!(true));
    assert_eq!(events[1]["current"], json!(false));
    assert_eq!(events[1]["kind"], json!("track"));
    assert_eq!(events[2]["kind"], json!("identify"));
    assert_eq!(events[0]["source"], json!("segment"));
    let story_orphan = dashboard::story(State(st.clone()), Path(orphan))
        .await
        .expect("story orphan")
        .0;
    assert_eq!(
        story_orphan,
        json!({ "user": null, "utm": null, "events": [] })
    );
    let missing = dashboard::story(State(st.clone()), Path(-1)).await;
    assert_eq!(missing.err().map(|e| e.0), Some(StatusCode::NOT_FOUND));

    drop(st);
    teardown(s).await;
}
