use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{signed_fetch_headers, test_wallet, Wallet};
use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router, AppState, AppStateInner};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use tower::ServiceExt;

const VICTIM_EVIDENCE: &str = "victim evidence: griefing at 10,20";

async fn state_for(pool: PgPool) -> AppState {
    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    places.ensure_local_schema().await.expect("local schema");
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool),
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

async fn scratch() -> Option<ScratchSchema> {
    std::env::set_var("PLACES_REPORT_LOCAL_FALLBACK", "true");
    ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "places_report_authz").await
}

fn signed_request(wallet: &Wallet, method: &str, path: &str, body: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method.to_uppercase().as_str())
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in signed_fetch_headers(wallet, method, path) {
        builder = builder.header(name, value);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn anonymous_request(method: &str, path: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method.to_uppercase().as_str())
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn file_a_report(app: &Router, reporter: &Wallet) -> Value {
    let req = signed_request(
        reporter,
        "post",
        "/api/report",
        &json!({ "entity_id": "place-1", "reason": VICTIM_EVIDENCE }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "victim report must be filed");
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn stored_report(pool: &PgPool) -> (String, String, Value) {
    let row = sqlx::query("SELECT filename, reporter, payload FROM place_reports_local")
        .fetch_one(pool)
        .await
        .expect("report row");
    (
        row.get::<String, _>("filename"),
        row.get::<String, _>("reporter"),
        row.get::<Value, _>("payload"),
    )
}

#[tokio::test]
async fn anonymous_upload_cannot_overwrite_another_users_report() {
    let Some(scratch) = scratch().await else {
        return;
    };
    let app: Router = api_router().with_state(state_for(scratch.pool.clone()).await);

    let victim = test_wallet(3);
    file_a_report(&app, &victim).await;
    let (filename, reporter, before) = stored_report(&scratch.pool).await;
    assert_eq!(reporter, victim.address().to_lowercase());

    let resp = app
        .clone()
        .oneshot(anonymous_request(
            "put",
            &format!("/api/report/upload/{filename}"),
            &json!({ "reason": "" }),
        ))
        .await
        .unwrap();
    let status = resp.status();

    let (_, _, after) = stored_report(&scratch.pool).await;
    scratch.drop().await;

    assert_eq!(
        after, before,
        "anonymous PUT wiped the victim's report evidence"
    );
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unauthenticated report upload must be rejected"
    );
}

#[tokio::test]
async fn signed_stranger_cannot_overwrite_another_users_report() {
    let Some(scratch) = scratch().await else {
        return;
    };
    let app: Router = api_router().with_state(state_for(scratch.pool.clone()).await);

    let victim = test_wallet(3);
    let attacker = test_wallet(9);
    assert_ne!(victim.address(), attacker.address());
    file_a_report(&app, &victim).await;
    let (filename, _, before) = stored_report(&scratch.pool).await;

    let path = format!("/api/report/upload/{filename}");
    let resp = app
        .clone()
        .oneshot(signed_request(
            &attacker,
            "put",
            &path,
            &json!({ "reason": "" }),
        ))
        .await
        .unwrap();
    let status = resp.status();

    let (_, reporter_after, after) = stored_report(&scratch.pool).await;
    scratch.drop().await;

    assert_eq!(
        reporter_after,
        victim.address().to_lowercase(),
        "the row still attributes the report to the victim"
    );
    assert_eq!(
        after, before,
        "a signed stranger overwrote the victim's report payload"
    );
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a report that does not belong to the caller must not be writable"
    );
}

#[tokio::test]
async fn reporter_uploads_evidence_to_own_report() {
    let Some(scratch) = scratch().await else {
        return;
    };
    let app: Router = api_router().with_state(state_for(scratch.pool.clone()).await);

    let reporter = test_wallet(3);
    file_a_report(&app, &reporter).await;
    let (filename, _, _) = stored_report(&scratch.pool).await;

    let path = format!("/api/report/upload/{filename}");
    let resp = app
        .clone()
        .oneshot(signed_request(
            &reporter,
            "put",
            &path,
            &json!({ "detail": "screenshot" }),
        ))
        .await
        .unwrap();
    let status = resp.status();

    let (_, _, after) = stored_report(&scratch.pool).await;
    scratch.drop().await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(after, json!({ "detail": "screenshot" }));
}
