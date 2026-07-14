use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::Query;
use axum::routing::get;
use axum::{Json, Router};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_events::mirror::{parse_page, run_cycle, sweep_removed, upsert_event, SweepOutcome};
use catalyrst_events::ports::events::{EventListFilters, EventListType, EventsComponent};
use serde_json::Value;
use sqlx::PgPool;

const ACTIVE_PAGE: &str = include_str!("fixtures/upstream_events_active_page0.json");

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_EVENTS_TEST_PG", "cg_events_mirror").await?;
    scratch
        .apply_sql(include_str!("../migrations/0002_event_catalog.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0003_local_overlays.sql"))
        .await;
    Some(scratch)
}

fn fixture_events() -> Vec<Value> {
    let body: Value = serde_json::from_str(ACTIVE_PAGE).unwrap();
    parse_page(&body).unwrap()
}

async fn event_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn upsert_real_page_is_idempotent_and_serves() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    assert_eq!(events.len(), 100);

    for e in &events {
        assert!(upsert_event(&pool, e).await.unwrap());
    }
    assert_eq!(event_count(&pool).await, 100);

    for e in &events {
        assert!(upsert_event(&pool, e).await.unwrap());
    }
    assert_eq!(event_count(&pool).await, 100, "upsert is idempotent");

    let component = EventsComponent::new(pool.clone(), None);
    let sample_id = events[0]["id"].as_str().unwrap();
    let rec = component
        .get(sample_id)
        .await
        .unwrap()
        .expect("mirrored event serves through the read port");
    assert_eq!(rec.id, sample_id);
    assert!(rec.approved);
    assert_eq!(rec.name, events[0]["name"].as_str().unwrap());
    assert_eq!(rec.x as i64, events[0]["coordinates"][0].as_i64().unwrap());

    let (all, total) = component
        .query(
            &EventListFilters {
                limit: 500,
                list: EventListType::All,
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
    assert_eq!(total, 100);
    assert_eq!(all.len(), 100);

    scratch.drop().await;
}

#[tokio::test]
async fn sweep_deletes_unseen_rows_but_spares_local_writes() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    for e in &events[..3] {
        upsert_event(&pool, e).await.unwrap();
    }
    let gone_id = events[0]["id"].as_str().unwrap();
    let local_id = events[1]["id"].as_str().unwrap();
    let fresh_id = events[2]["id"].as_str().unwrap();

    sqlx::query("UPDATE event SET fetched_at = now() - interval '3 hours' WHERE id = ANY($1)")
        .bind(vec![gone_id.to_string(), local_id.to_string()])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO events_local (id, signer, signed_payload) VALUES ($1, $2, '{}')")
        .bind(local_id)
        .bind("0x00")
        .execute(&pool)
        .await
        .unwrap();

    let cutoff = chrono::Utc::now() - chrono::Duration::hours(2);
    let swept = sweep_removed(&pool, cutoff).await.unwrap();
    assert_eq!(swept, SweepOutcome::Swept(1));

    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM event ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!ids.contains(&gone_id.to_string()), "unseen row deleted");
    assert!(ids.contains(&local_id.to_string()), "local write spared");
    assert!(ids.contains(&fresh_id.to_string()), "fresh row spared");

    scratch.drop().await;
}

#[tokio::test]
async fn local_overlay_wins_over_mirror_upsert() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    let event = &events[0];
    let id = event["id"].as_str().unwrap();

    upsert_event(&pool, event).await.unwrap();
    sqlx::query("INSERT INTO events_local (id, signer, signed_payload) VALUES ($1, $2, '{}')")
        .bind(id)
        .bind("0x00")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE event SET raw = raw || '{\"deleted_by_admin\": true}', name = 'locally deleted' \
         WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(upsert_event(&pool, event).await.unwrap());
    let (name, raw): (String, Value) = sqlx::query_as("SELECT name, raw FROM event WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        name, "locally deleted",
        "mirror does not clobber local writes"
    );
    assert_eq!(raw["deleted_by_admin"], Value::Bool(true));

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
        "/api/events",
        get(move |Query(q): Query<Q>| async move {
            let offset = q.offset.unwrap_or(0);
            if offset == 0 {
                if fail_first_page {
                    return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                }
                let body: Value = serde_json::from_str(ACTIVE_PAGE).unwrap();
                Ok(Json(body))
            } else {
                Ok(Json(serde_json::json!({"ok": true, "data": []})))
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
    let pool = scratch.pool.clone();
    let addr = serve_upstream(false).await;
    let client = reqwest::Client::new();

    let out = run_cycle(
        &pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();
    assert_eq!(out.upserted, 100);
    assert_eq!(out.failed_pages, 0);
    assert_eq!(out.failed_rows, 0);
    assert!(out.complete);
    assert_eq!(out.swept, 0);
    assert_eq!(event_count(&pool).await, 100);

    scratch.drop().await;
}

#[tokio::test]
async fn a_row_missed_once_survives_and_missed_twice_is_swept() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();

    // With interval = 1h the cutoff sits 2h before the pass: a row last seen
    // 90 minutes ago (missed by exactly one pass) survives, while a row last
    // seen 3 hours ago (missed by two consecutive passes) is deleted.
    sqlx::query(
        "INSERT INTO event (id, name, raw, fetched_at) VALUES \
         ('missed-once', 'missed-once', '{}', now() - interval '90 minutes'), \
         ('missed-twice', 'missed-twice', '{}', now() - interval '3 hours')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let addr = serve_upstream(false).await;
    let client = reqwest::Client::new();
    let out = run_cycle(
        &pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();

    assert!(out.complete);
    assert_eq!(out.swept, 1);
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM event WHERE id IN ('missed-once', 'missed-twice')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ids, vec!["missed-once"], "one missed pass never deletes");

    scratch.drop().await;
}

#[tokio::test]
async fn sweep_fuse_refuses_mass_deletion_but_allows_small_sweeps() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let events = fixture_events();
    for e in &events {
        upsert_event(&pool, e).await.unwrap();
    }
    let ids: Vec<String> = events
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();

    // 60 of 100 mirrored rows go stale: candidates 60 > max(50, 20% of 100),
    // so the fuse trips and nothing is deleted.
    sqlx::query("UPDATE event SET fetched_at = now() - interval '3 hours' WHERE id = ANY($1)")
        .bind(&ids[..60])
        .execute(&pool)
        .await
        .unwrap();
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(2);
    assert_eq!(
        sweep_removed(&pool, cutoff).await.unwrap(),
        SweepOutcome::Refused {
            candidates: 60,
            mirrored: 100
        }
    );
    assert_eq!(
        event_count(&pool).await,
        100,
        "a refused sweep deletes zero"
    );

    // Refreshing 50 of the stale rows drops the candidate set under the
    // threshold and the sweep proceeds.
    sqlx::query("UPDATE event SET fetched_at = now() WHERE id = ANY($1)")
        .bind(&ids[..50])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sweep_removed(&pool, cutoff).await.unwrap(),
        SweepOutcome::Swept(10)
    );
    assert_eq!(event_count(&pool).await, 90);

    scratch.drop().await;
}

#[tokio::test]
async fn run_cycle_fails_open_and_never_sweeps_on_a_bad_page() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();

    sqlx::query(
        "INSERT INTO event (id, name, raw, fetched_at) \
         VALUES ('stale', 'stale', '{}', now() - interval '3 days')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let addr = serve_upstream(true).await;
    let client = reqwest::Client::new();
    let out = run_cycle(
        &pool,
        &client,
        &format!("http://{addr}"),
        Duration::from_secs(3600),
    )
    .await
    .unwrap();

    assert!(out.failed_pages >= 1);
    assert!(!out.complete);
    assert_eq!(out.swept, 0);
    let stale: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE id = 'stale'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stale, 1, "incomplete pass never deletes");

    scratch.drop().await;
}
