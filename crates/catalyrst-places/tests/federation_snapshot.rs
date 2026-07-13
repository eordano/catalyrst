use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rand::RngExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;

use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::handlers::fed_sync::{changes_view, snapshot_view};
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router, AppStateInner};

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_PLACES_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/places".into()))
}

fn unique_schema() -> String {
    let b: [u8; 8] = rand::rng().random();
    format!("test_fed_places_{}", hex::encode(b))
}

async fn setup() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;
    Some((pool, schema, url))
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn component(pool: PgPool) -> PlacesComponent {
    PlacesComponent::new(pool.clone()).with_writer(pool)
}

fn state(places: PlacesComponent) -> Arc<AppStateInner> {
    Arc::new(AppStateInner {
        places,
        admin_addresses: vec![],
        data_team_auth_token: None,
        admin_auth_token: None,
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:0".into()),
        events: Events::new("http://127.0.0.1:0".into()),
        presence: Presence::new("http://127.0.0.1:0".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

async fn seed_action(pool: &PlacesComponent, sig: &str, place: &str, action: &str, signer: &str) {
    pool.record_signed_action(
        sig,
        signer,
        place,
        action,
        &serde_json::json!({ "place_id": place, "action": action }),
        1_700_000_000,
        None,
    )
    .await
    .expect("record signed action");
}

#[tokio::test]
async fn changes_pages_by_seq_and_clamps_limit() {
    let Some((raw, schema, admin_url)) = setup().await else {
        eprintln!("skipping changes_pages_by_seq_and_clamps_limit: no postgres reachable");
        return;
    };
    let places = component(raw);
    places.ensure_local_schema().await.expect("schema");
    let pool = places.writer_pool();

    for i in 0..5 {
        seed_action(
            &places,
            &format!("{:064x}", i),
            &format!("place-{}", i),
            if i % 2 == 0 { "favorite" } else { "vote_up" },
            "0xsigner",
        )
        .await;
    }

    let page = changes_view(pool, 0, 2).await.unwrap();
    let actions = page["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 2);
    let s0 = actions[0]["seq"].as_i64().unwrap();
    let s1 = actions[1]["seq"].as_i64().unwrap();
    assert!(s0 < s1, "ascending by seq");
    assert_eq!(page["latest_seq"].as_i64().unwrap(), s1);

    let page2 = changes_view(pool, s1, 100).await.unwrap();
    let rest = page2["actions"].as_array().unwrap();
    assert_eq!(rest.len(), 3);
    assert!(rest.iter().all(|a| a["seq"].as_i64().unwrap() > s1));

    let empty = changes_view(pool, s1 + 100, 100).await.unwrap();
    assert!(empty["actions"].as_array().unwrap().is_empty());

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn snapshot_shape_is_deterministic() {
    let Some((raw, schema, admin_url)) = setup().await else {
        eprintln!("skipping snapshot_shape_is_deterministic: no postgres reachable");
        return;
    };
    let places = component(raw);
    places.ensure_local_schema().await.expect("schema");
    let pool = places.writer_pool();

    seed_action(
        &places,
        &format!("{:064x}", 1),
        "place-a",
        "favorite",
        "0xa",
    )
    .await;
    seed_action(&places, &format!("{:064x}", 2), "place-b", "vote_up", "0xb").await;
    seed_action(&places, &format!("{:064x}", 3), "place-a", "report", "0xc").await;

    let snap = snapshot_view(pool).await.unwrap();
    assert_eq!(snap["scope"], "places");
    assert_eq!(snap["latest_seq"].as_i64().unwrap(), 3);
    assert_eq!(snap["action_count"].as_i64().unwrap(), 3);
    assert_eq!(snap["actions_by_type"]["favorite"].as_i64().unwrap(), 1);
    assert_eq!(snap["actions_by_type"]["report"].as_i64().unwrap(), 1);
    assert!(snap["log_hash"].is_string());

    let snap2 = snapshot_view(pool).await.unwrap();
    assert_eq!(snap["log_hash"], snap2["log_hash"]);

    seed_action(
        &places,
        &format!("{:064x}", 4),
        "place-c",
        "vote_down",
        "0xd",
    )
    .await;
    let snap3 = snapshot_view(pool).await.unwrap();
    assert_ne!(snap["log_hash"], snap3["log_hash"]);
    assert_eq!(snap3["latest_seq"].as_i64().unwrap(), 4);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn endpoints_reachable_without_auth() {
    let Some((raw, schema, admin_url)) = setup().await else {
        eprintln!("skipping endpoints_reachable_without_auth: no postgres reachable");
        return;
    };
    let places = component(raw);
    places.ensure_local_schema().await.expect("schema");
    let app = api_router().with_state(state(places));

    for path in [
        "/federation/places/snapshot",
        "/federation/places/changes?since=0&limit=10",
    ] {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{path} should 200 without auth"
        );
    }

    cleanup(&admin_url, &schema).await;
}
