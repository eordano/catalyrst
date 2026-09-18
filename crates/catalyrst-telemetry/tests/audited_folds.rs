use axum::extract::{FromRequestParts, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::{json, Value};

use catalyrst_telemetry::handlers::admin::{self, TelemetryAdmin};
use catalyrst_telemetry::handlers::{dashboard, groups};
use catalyrst_telemetry::{build_state, AppState, Config};

struct Scratch {
    state: AppState,
    db: ScratchDb,
}

const PG_VAR: &str = "CATALYRST_TELEMETRY_TEST_PG";

async fn setup() -> Option<Scratch> {
    let url = catalyrst_testgate::require_pg(PG_VAR)?;
    let db = ScratchDb::builder(PG_VAR, "cg_telem_folds")
        .schemas(["telemetry"])
        .build()
        .await?;
    let (prefix, _) = url
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{PG_VAR} is not a postgres URL: {url}"));
    let bare = format!("{prefix}/{}", db.database);
    std::env::set_var("FLAGS_URL", "http://127.0.0.1:1/explorer.json");
    let cfg = Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        database_url: format!("{bare}?options=-c%20search_path%3Dtelemetry"),
        admin_token: Some("tok".into()),
    };
    let state = build_state(&cfg).await.expect("build telemetry state");
    Some(Scratch { state, db })
}

async fn teardown(s: Scratch) {
    let Scratch { state, db } = s;
    drop(state);
    db.drop().await;
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Json<T> {
    Json(serde_json::from_value(v).expect("body"))
}

fn q<T: serde::de::DeserializeOwned>(v: Value) -> Query<T> {
    Query(serde_json::from_value(v).expect("query"))
}

async fn audits(st: &AppState, action: &str) -> Vec<Value> {
    sqlx::query_scalar("SELECT detail FROM admin_audit WHERE action = $1 ORDER BY id")
        .bind(action)
        .fetch_all(&st.pool)
        .await
        .expect("audit rows")
}

async fn seed(st: &AppState, source: &str, days_ago: i64, body: Value) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO telemetry.telemetry_events (source, project, event_kind, received_at, body) \
         VALUES ($1, 'p', 'event', now() - make_interval(days => $2::int), $3) RETURNING id",
    )
    .bind(source)
    .bind(days_ago)
    .bind(body)
    .fetch_one(&st.pool)
    .await
    .expect("seed")
}

#[tokio::test]
async fn groups_flags_and_experiments_fold_with_audit() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();

    for g in [
        json!({ "name": "beta", "members": ["u1"], "priority": 10 }),
        json!({ "name": "alpha", "members": ["u1", "u2"], "priority": 5, "description": "d" }),
    ] {
        let _ = groups::set(State(st.clone()), body(g)).await.expect("set");
    }
    for t in [
        json!({ "group": "beta", "flag": "f1", "state": "forced", "variant": "vb" }),
        json!({ "group": "alpha", "flag": "f1", "state": "off" }),
        json!({ "group": "alpha", "flag": "f2" }),
        json!({ "group": "alpha", "exp_key": "e1", "variant": "va" }),
        json!({ "group": "beta", "exp_key": "e1", "killed": true }),
    ] {
        let _ = groups::set_target(State(st.clone()), body(t))
            .await
            .expect("set_target");
    }
    let _ = groups::set_area(
        State(st.clone()),
        body(json!({ "kind": "flag", "name": "f1", "area": "core" })),
    )
    .await
    .expect("set_area");
    let both = groups::set_target(
        State(st.clone()),
        body(json!({ "group": "beta", "flag": "f1", "exp_key": "e1" })),
    )
    .await;
    assert_eq!(both.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    let list = groups::list(State(st.clone())).await.expect("list").0;
    let names: Vec<&str> = list["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["beta", "alpha"]);
    let alpha = &list["groups"][1];
    assert_eq!(alpha["member_count"], json!(2));
    assert_eq!(alpha["members"], json!(["u1", "u2"]));
    assert_eq!(alpha["description"], json!("d"));
    assert_eq!(
        alpha["flag_targets"],
        json!([
            { "flag": "f1", "state": "off", "variant": null },
            { "flag": "f2", "state": "on", "variant": null },
        ])
    );
    assert_eq!(
        alpha["experiment_targets"],
        json!([{ "exp_key": "e1", "killed": false, "variant": "va" }])
    );
    assert_eq!(
        list["groups"][0]["flag_targets"],
        json!([{ "flag": "f1", "state": "forced", "variant": "vb" }])
    );
    assert_eq!(
        list["areas"],
        json!([{ "kind": "flag", "name": "f1", "area": "core" }])
    );
    assert_eq!(audits(&st, "group.set").await.len(), 2);
    assert_eq!(audits(&st, "group.target.set").await.len(), 5);
    assert_eq!(audits(&st, "area.set").await[0]["area"], json!("core"));

    let f = |user: Option<&str>| {
        dashboard::flags(
            State(st.clone()),
            Query(dashboard::FlagsQuery {
                user: user.map(String::from),
            }),
        )
    };
    let u1 = f(Some("u1")).await.expect("flags u1").0;
    assert_eq!(u1["config"], Value::Null);
    assert_eq!(u1["user"], json!("u1"));
    assert_eq!(u1["groups"], json!(["beta", "alpha"]));
    assert_eq!(u1["overrides"]["f1"]["state"], json!("forced"));
    assert_eq!(u1["overrides"]["f1"]["variant"], json!("vb"));
    assert_eq!(u1["overrides"]["f2"]["state"], json!("on"));
    let mut targeted: Vec<&str> = u1["group_targeted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    targeted.sort();
    assert_eq!(targeted, vec!["f1", "f2"]);
    assert_eq!(u1["areas"], json!({ "f1": "core" }));
    assert_eq!(u1["observed"], json!([]));
    assert_eq!(u1["source_url"], json!("http://127.0.0.1:1/explorer.json"));
    let u2 = f(Some("u2")).await.expect("flags u2").0;
    assert_eq!(u2["groups"], json!(["alpha"]));
    assert_eq!(u2["overrides"]["f1"]["state"], json!("off"));
    let anon = f(None).await.expect("flags anon").0;
    assert_eq!(anon["groups"], json!([]));
    assert_eq!(anon["group_targeted"], json!([]));
    assert_eq!(anon["overrides"], json!({}));

    let get = |key: &str, user: Option<&str>| {
        dashboard::experiments_get(State(st.clone()), q(json!({ "key": key, "user": user })))
    };
    assert_eq!(
        get("e1", Some("u1")).await.expect("e1 u1").0,
        json!({ "killed": true, "variant": null, "flags": {} })
    );
    assert_eq!(
        get("e1", Some("u2")).await.expect("e1 u2").0,
        json!({ "killed": false, "variant": "va", "flags": {} })
    );
    assert_eq!(get("e1", None).await.expect("e1 anon").0, json!({}));
    let _ = dashboard::experiment_set(
        State(st.clone()),
        body(json!({ "exp_key": "e1", "killed": true, "variant": "x", "flags": { "a": 1 } })),
    )
    .await
    .expect("experiment_set");
    let override_row = json!({ "killed": true, "variant": "x", "flags": { "a": 1 } });
    assert_eq!(get("e1", None).await.expect("e1 override").0, override_row);
    assert_eq!(get("e1", Some("u3")).await.expect("e1 u3").0, override_row);
    assert_eq!(
        get("e1", Some("u2")).await.expect("e1 u2 again").0,
        json!({ "killed": false, "variant": "va", "flags": {} })
    );
    let _ = dashboard::experiment_set(
        State(st.clone()),
        body(json!({ "exp_key": "e1", "clear": true })),
    )
    .await
    .expect("experiment clear");
    assert_eq!(get("e1", None).await.expect("e1 cleared").0, json!({}));
    assert_eq!(
        audits(&st, "experiment.set").await[0]["flags"],
        json!({ "a": 1 })
    );
    assert_eq!(audits(&st, "experiment.clear").await.len(), 1);

    let _ = groups::set_target(
        State(st.clone()),
        body(json!({ "group": "alpha", "flag": "f2", "clear": true })),
    )
    .await
    .expect("target clear");
    let _ = groups::set_area(
        State(st.clone()),
        body(json!({ "kind": "flag", "name": "f1", "clear": true })),
    )
    .await
    .expect("area clear");
    let _ = groups::set(
        State(st.clone()),
        body(json!({ "name": "beta", "clear": true })),
    )
    .await
    .expect("group clear");
    let list = groups::list(State(st.clone())).await.expect("list2").0;
    assert_eq!(list["groups"].as_array().unwrap().len(), 1);
    assert_eq!(list["groups"][0]["name"], json!("alpha"));
    assert_eq!(
        list["groups"][0]["flag_targets"].as_array().unwrap().len(),
        1
    );
    assert_eq!(list["areas"], json!([]));
    assert_eq!(audits(&st, "group.clear").await.len(), 1);
    assert_eq!(audits(&st, "group.target.clear").await.len(), 1);
    assert_eq!(audits(&st, "area.clear").await.len(), 1);
    let u1 = f(Some("u1")).await.expect("flags u1 after").0;
    assert_eq!(u1["groups"], json!(["alpha"]));
    assert_eq!(u1["overrides"]["f1"]["state"], json!("off"));

    let sql = dashboard::sql_query(
        State(st.clone()),
        body(json!({ "sql": "SELECT count(*) AS n FROM telemetry.flag_groups;" })),
    )
    .await
    .expect("sql")
    .0;
    assert_eq!(sql["rows"][0]["n"], json!(1));
    let bad = dashboard::sql_query(
        State(st.clone()),
        body(json!({ "sql": "SELECT * FROM telemetry.nope" })),
    )
    .await;
    assert!(bad.is_err());
    let again = dashboard::sql_query(
        State(st.clone()),
        body(json!({ "sql": "SELECT 1 AS one, current_setting('statement_timeout') AS timeout, current_setting('transaction_read_only') AS ro" })),
    )
    .await
    .expect("sql after error")
    .0;
    assert_eq!(again["rows"][0]["one"], json!(1), "{again}");
    assert_eq!(again["rows"][0]["timeout"], json!("15s"));
    assert_eq!(again["rows"][0]["ro"], json!("on"));
    let write = dashboard::sql_query(
        State(st.clone()),
        body(json!({ "sql": "WITH x AS (INSERT INTO telemetry.flag_groups (name) VALUES ('zz') RETURNING 1) SELECT * FROM x" })),
    )
    .await;
    assert_eq!(write.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    drop(st);
    teardown(s).await;
}

async fn admin_of(st: &AppState) -> TelemetryAdmin {
    let (mut parts, ()) = axum::http::Request::builder()
        .header("authorization", "Bearer tok")
        .body(())
        .unwrap()
        .into_parts();
    TelemetryAdmin::from_request_parts(&mut parts, st)
        .await
        .expect("admin")
}

fn actor(name: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-catalyrst-admin", name.parse().unwrap());
    h
}

#[tokio::test]
async fn admin_writes_audit_in_one_statement() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();
    let aq = || q::<admin::ActorQuery>(json!({}));

    seed(&st, "segment", 10, json!({ "e": 1 })).await;
    seed(&st, "segment", 9, json!({ "e": 2 })).await;
    seed(&st, "segment", 1, json!({ "e": 3 })).await;
    let s1 = seed(&st, "sentry", 3, json!({ "level": "error" })).await;
    let s2 = seed(&st, "sentry", 2, json!({ "level": "info" })).await;
    let s3 = seed(&st, "sentry", 1, json!({ "level": "error" })).await;

    let purged = admin::purge(
        admin_of(&st).await,
        State(st.clone()),
        actor("alice"),
        aq(),
        body(json!({ "older_than_days": 5, "source": "segment" })),
    )
    .await
    .expect("purge")
    .0;
    assert_eq!(purged, json!({ "ok": true, "deleted": 2 }));
    let purge_audit = audits(&st, "purge").await;
    assert_eq!(purge_audit[0]["deleted"], json!(2));
    assert_eq!(purge_audit[0]["source"], json!("segment"));
    let (actor_name,): (String,) =
        sqlx::query_as("SELECT actor FROM admin_audit WHERE action = 'purge'")
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(actor_name, "alice");

    let exported = admin::export(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "source": "sentry", "limit": 2 })),
    )
    .await
    .expect("export")
    .0;
    assert_eq!(exported["count"], json!(2));
    assert_eq!(exported["truncated"], json!(true));
    let ids: Vec<i64> = exported["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![s3, s2]);
    let mut keys: Vec<&str> = exported["events"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "body",
            "event_kind",
            "fingerprint",
            "id",
            "project",
            "received_at",
            "source"
        ]
    );
    assert!(exported["events"][0]["received_at"]
        .as_str()
        .unwrap()
        .ends_with('Z'));
    let export_audit = audits(&st, "export").await;
    assert_eq!(export_audit[0]["count"], json!(2));
    assert_eq!(export_audit[0]["level"], Value::Null);

    let deleted = admin::bulk_delete(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "level": "error" })),
    )
    .await
    .expect("bulk_delete")
    .0;
    assert_eq!(deleted["deleted"], json!(2));
    assert_eq!(audits(&st, "bulk_delete").await[0]["deleted"], json!(2));
    let remaining: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM telemetry.telemetry_events WHERE source = 'sentry' ORDER BY id",
    )
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(remaining, vec![s2]);
    assert!(s1 != s2);
    let unfiltered = admin::bulk_delete(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({})),
    )
    .await;
    assert_eq!(unfiltered.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    let _ = admin::ingest_toggle(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "enabled": false })),
    )
    .await
    .expect("toggle off");
    assert!(!st.ingest.enabled.load(std::sync::atomic::Ordering::Relaxed));
    let (v,): (String,) =
        sqlx::query_as("SELECT value FROM admin_settings WHERE key = 'ingest_enabled'")
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(v, "false");
    let _ = admin::ingest_toggle(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "enabled": true })),
    )
    .await
    .expect("toggle on");
    assert!(st.ingest.enabled.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(audits(&st, "ingest_toggle").await.len(), 2);

    let _ = admin::quota(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "project": "p", "daily_limit": 5 })),
    )
    .await
    .expect("quota set");
    assert_eq!(st.ingest.quotas.read().unwrap().get("p"), Some(&5));
    let (limit,): (i64,) =
        sqlx::query_as("SELECT daily_limit FROM project_quota WHERE project = 'p'")
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(limit, 5);
    let _ = admin::quota(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "project": "p" })),
    )
    .await
    .expect("quota clear");
    assert!(st.ingest.quotas.read().unwrap().get("p").is_none());
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM project_quota")
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let quota_audit = audits(&st, "quota").await;
    assert_eq!(quota_audit.len(), 2);
    assert_eq!(quota_audit[1]["daily_limit"], Value::Null);

    let regrouped = admin::regroup(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "canonical": "c", "sources": ["a", " b ", "a", "c", ""] })),
    )
    .await
    .expect("regroup")
    .0;
    assert_eq!(
        regrouped,
        json!({ "ok": true, "canonical": "c", "merged": 3 })
    );
    let merges: Vec<(String, String)> = sqlx::query_as(
        "SELECT source_fingerprint, canonical_fingerprint FROM issue_merge ORDER BY 1",
    )
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(
        merges,
        vec![("a".into(), "c".into()), ("b".into(), "c".into())]
    );
    let _ = admin::regroup(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        aq(),
        body(json!({ "canonical": "d", "sources": ["a"] })),
    )
    .await
    .expect("regroup again");
    let (canon,): (String,) = sqlx::query_as(
        "SELECT canonical_fingerprint FROM issue_merge WHERE source_fingerprint = 'a'",
    )
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(canon, "d");
    let regroup_audit = audits(&st, "regroup").await;
    assert_eq!(regroup_audit[0]["sources"], json!(["a", "b", "a"]));
    assert_eq!(regroup_audit[0]["merged"], json!(3));
    assert_eq!(regroup_audit[0]["fingerprint"], json!("c"));
    let listed = admin::audit_list(
        admin_of(&st).await,
        State(st.clone()),
        q(json!({ "fingerprint": "c" })),
    )
    .await
    .expect("audit_list")
    .0;
    assert_eq!(listed["count"], json!(1));
    assert_eq!(listed["items"][0]["action"], json!("regroup"));

    let released = admin::release(
        admin_of(&st).await,
        State(st.clone()),
        HeaderMap::new(),
        q(json!({ "actor": "bob" })),
        body(json!({ "release": "r1", "state": "broken", "note": "" })),
    )
    .await
    .expect("release")
    .0;
    assert_eq!(
        released,
        json!({ "ok": true, "release": "r1", "state": "broken" })
    );
    let (state_v, note): (String, Option<String>) =
        sqlx::query_as("SELECT state, note FROM release_state WHERE release = 'r1'")
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!((state_v.as_str(), note), ("broken", None));
    let (release_actor,): (String,) =
        sqlx::query_as("SELECT actor FROM admin_audit WHERE action = 'release'")
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(release_actor, "bob");

    drop(st);
    teardown(s).await;
}
