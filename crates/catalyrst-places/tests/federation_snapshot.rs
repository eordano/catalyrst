use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;
use tower::ServiceExt;

use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::handlers::fed_sync::{changes_view, snapshot_view};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router, AppStateInner};

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_fedsnapshot",
    )
    .await
}

fn component(pool: PgPool) -> PlacesComponent {
    PlacesComponent::new(pool.clone()).with_writer(pool)
}

fn state(places: PlacesComponent, pool: PgPool) -> Arc<AppStateInner> {
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool),
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
    let Some(scratch) = setup().await else {
        return;
    };
    let raw = scratch.pool.clone();
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

    scratch.drop().await;
}

#[tokio::test]
async fn snapshot_shape_is_deterministic() {
    let Some(scratch) = setup().await else {
        return;
    };
    let raw = scratch.pool.clone();
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

    scratch.drop().await;
}

#[tokio::test]
async fn endpoints_reachable_without_auth() {
    let Some(scratch) = setup().await else {
        return;
    };
    let raw = scratch.pool.clone();
    let places = component(raw.clone());
    places.ensure_local_schema().await.expect("schema");
    let app = api_router().with_state(state(places, raw));

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

    scratch.drop().await;
}
