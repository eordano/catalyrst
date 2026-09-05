use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Query;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::catalog::worlds_mirror::{
    parse_page, run_cycle, sweep_removed, upsert_world, SweepOutcome,
};
use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router_with_spec, AppState, AppStateInner};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

/// Real captured response from places.decentraland.org/api/worlds.
const WORLDS_PAGE: &str = include_str!("fixtures/worlds_page.json");

fn fixture_worlds() -> Vec<Value> {
    let body: Value = serde_json::from_str(WORLDS_PAGE).unwrap();
    parse_page(&body).unwrap()
}

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "cg_places_worldsmirror").await
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
            raw            jsonb   NOT NULL DEFAULT '{}'::jsonb,
            fetched_at     timestamptz NOT NULL DEFAULT now()
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

fn as_ts(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

// Upstream serializes whole-number floats as JSON integers (ranking: 0,
// like_rate: 1) while the served rows are typed f64; the values are equal
// even though the JSON number variants differ.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

#[tokio::test]
async fn mirrored_worlds_serve_the_upstream_shape() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    let rows = fixture_worlds();
    for row in &rows {
        assert!(
            upsert_world(&scratch.pool, row).await.expect("upsert"),
            "every real upstream row must mirror: {row}"
        );
    }

    let body = get_json(&scratch.pool, "/api/worlds?limit=100&offset=0").await;
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["total"].as_i64(), Some(rows.len() as i64));
    let served = body["data"].as_array().expect("data array");
    assert_eq!(served.len(), rows.len());

    // Field-for-field parity with the captured upstream page, for every field
    // upstream serves on a world row and the explorer's PlaceInfo DTO reads.
    for upstream in &rows {
        let id = upstream["id"].as_str().unwrap();
        let world = served
            .iter()
            .find(|w| w["id"] == upstream["id"])
            .unwrap_or_else(|| panic!("{id} must be served"));
        for key in [
            "id",
            "title",
            "description",
            "image",
            "owner",
            "world_name",
            "content_rating",
            "categories",
            "likes",
            "dislikes",
            "favorites",
            "like_rate",
            "like_score",
            "disabled",
            "disabled_at",
            "base_position",
            "contact_name",
            "highlighted",
            "highlighted_image",
            "ranking",
            "is_private",
            "show_in_places",
            "single_player",
            "skybox_time",
            "user_visits",
            "user_count",
            "user_favorite",
            "user_like",
            "user_dislike",
        ] {
            assert!(
                json_eq(&world[key], &upstream[key]),
                "{id}: served {key} must equal upstream: {} != {}",
                world[key],
                upstream[key]
            );
        }
        for key in ["created_at", "updated_at", "deployed_at"] {
            assert_eq!(
                as_ts(&world[key]),
                as_ts(&upstream[key]),
                "{id}: served {key} must equal upstream"
            );
        }
        assert_eq!(
            world["world"],
            Value::Bool(true),
            "{id} must serve as a world"
        );
    }

    let named = get_json(&scratch.pool, "/api/worlds?names=metadynelabs.dcl.eth").await;
    assert_eq!(named["data"].as_array().map(Vec::len), Some(1));

    scratch.drop().await;
}

#[tokio::test]
async fn sweep_deletes_unseen_worlds_but_spares_locals_and_places() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    let rows = fixture_worlds();
    for row in &rows {
        upsert_world(&scratch.pool, row).await.expect("upsert");
    }
    // A genesis place and a locally served world, both outside the mirror's
    // ownership, made stale enough that only ownership can spare them.
    sqlx::query(
        "INSERT INTO place (id, base_position, raw, fetched_at) \
         VALUES ('genesis', '1,1', '{}', now() - interval '3 days')",
    )
    .execute(&scratch.pool)
    .await
    .expect("seed genesis place");
    sqlx::query(
        "INSERT INTO place_world_local \
         (id, base_position, deployed_at, raw, world, world_name, fetched_at) \
         VALUES ('world:local.dcl.eth', '0,0', now(), \
                 '{\"world\": true, \"world_name\": \"local.dcl.eth\"}', true, 'local.dcl.eth', \
                 now() - interval '3 days')",
    )
    .execute(&scratch.pool)
    .await
    .expect("seed local world");

    let dropped = rows[0]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE place SET fetched_at = now() - interval '3 hours' WHERE id = $1")
        .bind(&dropped)
        .execute(&scratch.pool)
        .await
        .expect("age the dropped world");

    let cutoff = Utc::now() - chrono::Duration::hours(2);
    let swept = sweep_removed(&scratch.pool, cutoff).await.expect("sweep");
    assert_eq!(
        swept,
        SweepOutcome::Swept(1),
        "exactly the dropped world ages out"
    );

    let body = get_json(&scratch.pool, "/api/worlds?limit=100&offset=0").await;
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&dropped.as_str()), "{dropped} must be gone");
    for row in &rows[1..] {
        assert!(
            ids.contains(&row["id"].as_str().unwrap()),
            "refreshed worlds must survive the sweep"
        );
    }
    assert!(
        ids.contains(&"world:local.dcl.eth"),
        "place_world_local is outside the mirror's sweep"
    );

    let genesis: i64 = sqlx::query_scalar("SELECT count(*) FROM place WHERE id = 'genesis'")
        .fetch_one(&scratch.pool)
        .await
        .expect("count genesis");
    assert_eq!(genesis, 1, "world=false rows are outside the sweep");

    scratch.drop().await;
}

/// Serves the real fixture page at offset 0 and an empty page beyond it, on
/// an ephemeral port.
async fn serve_upstream(fail_first_page: bool) -> SocketAddr {
    #[derive(serde::Deserialize)]
    struct Q {
        offset: Option<i64>,
    }
    let app = Router::new().route(
        "/api/worlds",
        get(move |Query(q): Query<Q>| async move {
            let offset = q.offset.unwrap_or(0);
            if offset == 0 {
                if fail_first_page {
                    return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                }
                let body: Value = serde_json::from_str(WORLDS_PAGE).unwrap();
                Ok(Json(body))
            } else {
                Ok(Json(
                    serde_json::json!({"total": 5, "ok": true, "data": []}),
                ))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn run_cycle_pages_upserts_and_reports_complete() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;
    let addr = serve_upstream(false).await;
    let client = reqwest::Client::new();

    let out = run_cycle(
        &scratch.pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();
    assert_eq!(out.upserted, 5);
    assert_eq!(out.skipped_rows, 0);
    assert_eq!(out.failed_rows, 0);
    assert_eq!(out.failed_pages, 0);
    assert!(out.complete);
    assert_eq!(out.swept, 0);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM place WHERE world")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(count, 5);

    scratch.drop().await;
}

#[tokio::test]
async fn a_world_missed_once_survives_and_missed_twice_is_swept() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    // With interval = 1h the cutoff sits 2h before the pass: a world last
    // seen 90 minutes ago (missed by exactly one pass) survives, while one
    // last seen 3 hours ago (missed by two consecutive passes) is deleted.
    sqlx::query(
        "INSERT INTO place (id, base_position, raw, fetched_at) VALUES \
         ('missed-once.dcl.eth', '0,0', \
          '{\"world\": true, \"world_name\": \"missed-once.dcl.eth\"}', \
          now() - interval '90 minutes'), \
         ('missed-twice.dcl.eth', '0,0', \
          '{\"world\": true, \"world_name\": \"missed-twice.dcl.eth\"}', \
          now() - interval '3 hours')",
    )
    .execute(&scratch.pool)
    .await
    .unwrap();

    let addr = serve_upstream(false).await;
    let client = reqwest::Client::new();
    let out = run_cycle(
        &scratch.pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();

    assert!(out.complete);
    assert_eq!(out.swept, 1);
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM place WHERE id IN ('missed-once.dcl.eth', 'missed-twice.dcl.eth')",
    )
    .fetch_all(&scratch.pool)
    .await
    .unwrap();
    assert_eq!(
        ids,
        vec!["missed-once.dcl.eth"],
        "one missed pass never deletes"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn sweep_fuse_refuses_mass_deletion_but_allows_small_sweeps() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    for row in &fixture_worlds() {
        upsert_world(&scratch.pool, row).await.expect("upsert");
    }
    let synthetic: Vec<String> = (0..95).map(|i| format!("w{i:03}.dcl.eth")).collect();
    for id in &synthetic {
        upsert_world(
            &scratch.pool,
            &serde_json::json!({"id": id, "world_name": id}),
        )
        .await
        .expect("upsert synthetic world");
    }
    let pool = scratch.pool.clone();
    let world_count = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM place WHERE world")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    assert_eq!(world_count().await, 100);

    // 60 of 100 mirrored rows go stale: candidates 60 > max(50, 20% of 100),
    // so the fuse trips and nothing is deleted.
    sqlx::query("UPDATE place SET fetched_at = now() - interval '3 hours' WHERE id = ANY($1)")
        .bind(&synthetic[..60])
        .execute(&scratch.pool)
        .await
        .unwrap();
    let cutoff = Utc::now() - chrono::Duration::hours(2);
    assert_eq!(
        sweep_removed(&scratch.pool, cutoff).await.expect("sweep"),
        SweepOutcome::Refused {
            candidates: 60,
            mirrored: 100
        }
    );
    assert_eq!(world_count().await, 100, "a refused sweep deletes zero");

    // Refreshing 50 of the stale rows drops the candidate set under the
    // threshold and the sweep proceeds.
    sqlx::query("UPDATE place SET fetched_at = now() WHERE id = ANY($1)")
        .bind(&synthetic[..50])
        .execute(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(
        sweep_removed(&scratch.pool, cutoff).await.expect("sweep"),
        SweepOutcome::Swept(10)
    );
    assert_eq!(world_count().await, 90);

    scratch.drop().await;
}

#[tokio::test]
async fn run_cycle_fails_open_and_never_sweeps_on_a_bad_page() {
    let Some(scratch) = setup().await else {
        return;
    };
    migrate(&scratch.pool).await;

    sqlx::query(
        "INSERT INTO place (id, base_position, raw, fetched_at) \
         VALUES ('stale.dcl.eth', '0,0', \
                 '{\"world\": true, \"world_name\": \"stale.dcl.eth\"}', now() - interval '3 days')",
    )
    .execute(&scratch.pool)
    .await
    .unwrap();

    let addr = serve_upstream(true).await;
    let client = reqwest::Client::new();
    let out = run_cycle(
        &scratch.pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();

    assert!(out.failed_pages >= 1);
    assert!(!out.complete);
    assert_eq!(out.swept, 0);
    let stale: i64 = sqlx::query_scalar("SELECT count(*) FROM place WHERE id = 'stale.dcl.eth'")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(stale, 1, "an incomplete pass never deletes");

    scratch.drop().await;
}
