use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

use catalyrst_telemetry::handlers::dashboard;
use catalyrst_telemetry::{build_state, AppState, Config};

// Scratch database on :5434, created + dropped per run. Mirrors production:
// schema `telemetry`, search_path=telemetry (the handlers reference
// telemetry.* explicitly).
struct Scratch {
    state: AppState,
    admin: PgPool,
    db: String,
}

const PG_VAR: &str = "CATALYRST_TELEMETRY_TEST_PG";

async fn setup() -> Option<Scratch> {
    let base = catalyrst_testgate::require_pg(PG_VAR)?;
    let admin = match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&base)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return catalyrst_testgate::pg_unusable(
                PG_VAR,
                &format!("connect to {base} failed: {e}"),
            )
        }
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let db = format!("cg_telem_{}_{}", std::process::id(), nanos);
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db}")))
        .execute(&admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE DATABASE {db} failed: {e}"));
    let (prefix, _) = base
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{PG_VAR} is not a postgres URL: {base}"));
    let bare = format!("{prefix}/{db}");

    let seed = PgPoolOptions::new()
        .max_connections(1)
        .connect(&bare)
        .await
        .unwrap_or_else(|e| panic!("connect to scratch database {db} failed: {e}"));
    sqlx::query("CREATE SCHEMA IF NOT EXISTS telemetry")
        .execute(&seed)
        .await
        .expect("create telemetry schema");
    seed.close().await;

    // FLAGS_URL unreachable => upstream config is null, so the merge reflects
    // only operator overrides (hermetic, no dependency on the flag service).
    std::env::set_var("FLAGS_URL", "http://127.0.0.1:1/explorer.json");

    let cfg = Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        database_url: format!("{bare}?options=-c%20search_path%3Dtelemetry"),
        admin_token: None,
    };
    let state = build_state(&cfg).await.expect("build telemetry state");
    Some(Scratch { state, admin, db })
}

async fn teardown(s: Scratch) {
    let Scratch { state, admin, db } = s;
    drop(state);
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {db} WITH (FORCE)"
    )))
    .execute(&admin)
    .await;
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

    // merged into /flags: the forced flag reflects in current values, marked.
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
