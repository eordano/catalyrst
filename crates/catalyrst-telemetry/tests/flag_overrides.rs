use axum::extract::State;
use axum::Json;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::json;
use sqlx::Row;

use catalyrst_telemetry::handlers::dashboard;
use catalyrst_telemetry::{build_state, AppState, Config};

struct Scratch {
    state: AppState,
    db: ScratchDb,
}

const PG_VAR: &str = "CATALYRST_TELEMETRY_TEST_PG";

async fn setup() -> Option<Scratch> {
    let url = catalyrst_testgate::require_pg(PG_VAR)?;
    let db = ScratchDb::builder(PG_VAR, "cg_telem")
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

fn body(v: serde_json::Value) -> Json<dashboard::FlagSetBody> {
    Json(serde_json::from_value(v).expect("flag body"))
}

#[tokio::test]
async fn flag_override_roundtrip_merge_clear_and_audit() {
    let Some(s) = setup().await else {
        return;
    };
    let st = s.state.clone();

    let _ = dashboard::flag_set(
        State(st.clone()),
        body(json!({ "flag": "gv_banner", "state": "forced", "variant": "guided" })),
    )
    .await
    .expect("flag_set");

    let row =
        sqlx::query("SELECT state, forced_variant FROM telemetry.flag_overrides WHERE flag = $1")
            .bind("gv_banner")
            .fetch_one(&st.pool)
            .await
            .expect("override row");
    assert_eq!(row.get::<String, _>("state"), "forced");
    assert_eq!(
        row.get::<Option<String>, _>("forced_variant").as_deref(),
        Some("guided")
    );

    let flags = dashboard::flags(
        State(st.clone()),
        axum::extract::Query(dashboard::FlagsQuery { user: None }),
    )
    .await
    .expect("flags")
    .0;
    let entry = &flags["flags"]["gv_banner"];
    assert_eq!(entry["value"], json!(true));
    assert_eq!(entry["variant"], json!("guided"));
    assert_eq!(entry["overridden"], json!(true));
    assert_eq!(entry["override_state"], json!("forced"));
    assert_eq!(flags["overrides"]["gv_banner"]["state"], json!("forced"));

    let (sets,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM admin_audit WHERE action = 'flag.set' AND detail->>'flag' = 'gv_banner'",
    )
    .fetch_one(&st.pool)
    .await
    .expect("set audit count");
    assert_eq!(sets, 1);

    let _ = dashboard::flag_set(
        State(st.clone()),
        body(json!({ "flag": "gv_banner", "clear": true })),
    )
    .await
    .expect("flag clear");

    let (remaining,): (i64,) = sqlx::query_as("SELECT count(*) FROM telemetry.flag_overrides")
        .fetch_one(&st.pool)
        .await
        .expect("remaining count");
    assert_eq!(remaining, 0);

    let flags2 = dashboard::flags(
        State(st.clone()),
        axum::extract::Query(dashboard::FlagsQuery { user: None }),
    )
    .await
    .expect("flags2")
    .0;
    assert!(flags2["flags"].get("gv_banner").is_none());

    let (clears,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM admin_audit WHERE action = 'flag.clear' AND detail->>'flag' = 'gv_banner'",
    )
    .fetch_one(&st.pool)
    .await
    .expect("clear audit count");
    assert_eq!(clears, 1);

    drop(st);
    teardown(s).await;
}
