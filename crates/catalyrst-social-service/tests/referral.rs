//! Route tests for /v1/referral-progress: the zero-activity stats shape, the
//! unauthenticated gate, and the POST/PATCH attribution flow.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{signed_fetch_headers, test_wallet, Wallet};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::api_router_with_spec;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::ports::bans::BansComponent;
use catalyrst_social_service::rest::ports::communities::CommunitiesComponent;
use catalyrst_social_service::rest::ports::invites::InvitesComponent;
use catalyrst_social_service::rest::ports::members::MembersComponent;
use catalyrst_social_service::rest::ports::moderation::ModerationComponent;
use catalyrst_social_service::rest::ports::peers_stats::PeersStatsClient;
use catalyrst_social_service::rest::ports::places::PlacesComponent;
use catalyrst_social_service::rest::ports::places_api::PlacesApiClient;
use catalyrst_social_service::rest::ports::posts::PostsComponent;
use catalyrst_social_service::rest::ports::profiles::ProfilesComponent;
use catalyrst_social_service::rest::ports::requests::RequestsComponent;
use catalyrst_social_service::rest::ports::voice::VoiceComponent;
use catalyrst_social_service::rest::{AppState, AppStateInner};

async fn build_state(pool: PgPool, content_dir: PathBuf) -> AppState {
    let content_store = Arc::new(ContentStore::new(content_dir, MAX_BODY_BYTES));
    content_store.init().await.unwrap();
    Arc::new(AppStateInner {
        admin_token: None,
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        peers_stats: PeersStatsClient::new("http://127.0.0.1:1".to_string()),
        places: PlacesComponent::new(pool.clone()),
        places_api: PlacesApiClient::new(None),
        posts: PostsComponent::new(pool.clone()),
        profiles: Arc::new(ProfilesComponent::new(None, "http://127.0.0.1:9".into())),
        requests: RequestsComponent::new(pool.clone()),
        voice: VoiceComponent::new(pool.clone()),
        pool: pool.clone(),
        mutes_pool: None,
        replay: Replay::new(pool.clone()).await.unwrap(),
        limiter: Arc::new(RateLimiter::new(10_000, Duration::from_secs(60))),
        gossip: Arc::new(NoopPublisher),
        domain: domains::communities(),
        content_store,
        cdn_url: "http://cdn.test".into(),
        global_moderators: vec![],
        restricted_names: vec![],
        gatekeeper: Gatekeeper::with_token("http://127.0.0.1:1".to_string(), None),
    })
}

async fn setup(prefix: &str) -> Option<(ScratchDb, Router, PathBuf)> {
    let scratch = ScratchDb::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", prefix).await?;
    sqlx::migrate!("./migrations")
        .run(&scratch.pool)
        .await
        .unwrap();
    let content_dir = std::env::temp_dir().join(format!("referral-{}", scratch.database));
    let state = build_state(scratch.pool.clone(), content_dir.clone()).await;
    let app: Router = api_router_with_spec().0.with_state(state);
    Some((scratch, app, content_dir))
}

async fn hit(
    app: &Router,
    method: &str,
    wallet: Option<&Wallet>,
    body: Option<Value>,
) -> (u16, Value) {
    let path = "/v1/referral-progress";
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(wallet) = wallet {
        for (name, value) in signed_fetch_headers(wallet, method, path) {
            builder = builder.header(name, value);
        }
    }
    let request = match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&v).unwrap()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

fn addr(w: &Wallet) -> String {
    w.address().to_lowercase()
}

#[tokio::test]
async fn get_reports_zero_activity_shape() {
    let Some((scratch, app, dir)) = setup("cg_referral_zero").await else {
        return;
    };
    let user = test_wallet(31);

    let expected = json!({
        "invitedUsersAccepted": 0,
        "invitedUsersAcceptedViewed": 0,
        "rewardImages": []
    });
    let (status, body) = hit(&app, "GET", Some(&user), None).await;
    assert_eq!(status, 200);
    assert_eq!(body, expected);

    // The viewed write-back stores the accepted count (0), so a second read
    // reports the same zero-activity shape.
    let (status, body) = hit(&app, "GET", Some(&user), None).await;
    assert_eq!(status, 200);
    assert_eq!(body, expected);

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unauthenticated_requests_answer_401_unified_envelope() {
    let Some((scratch, app, dir)) = setup("cg_referral_unauth").await else {
        return;
    };

    for (method, body) in [
        ("GET", None),
        (
            "POST",
            Some(json!({ "referrer": "0x0000000000000000000000000000000000000001" })),
        ),
        ("PATCH", None),
    ] {
        let (status, v) = hit(&app, method, None, body).await;
        assert_eq!(status, 401, "{method} unauthenticated");
        assert_eq!(v["ok"], Value::Bool(false), "{method}: {v}");
        assert_eq!(v["error"], "Invalid Auth Chain", "{method}: {v}");
        assert!(
            v["message"].as_str().unwrap().contains("ADR-44"),
            "{method}: {v}"
        );
    }

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn create_and_signed_up_flow() {
    let Some((scratch, app, dir)) = setup("cg_referral_flow").await else {
        return;
    };
    let referrer = test_wallet(37);
    let invited = test_wallet(41);
    let other = test_wallet(43);

    let refer = json!({ "referrer": addr(&referrer) });

    // Create, then a same-referrer duplicate: both 204 (retry-safe attribution).
    let (status, _) = hit(&app, "POST", Some(&invited), Some(refer.clone())).await;
    assert_eq!(status, 204);
    let (status, _) = hit(&app, "POST", Some(&invited), Some(refer.clone())).await;
    assert_eq!(status, 204);

    // A different referrer for the same invited user: first-wins conflict.
    let (status, v) = hit(
        &app,
        "POST",
        Some(&invited),
        Some(json!({ "referrer": addr(&other) })),
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        v["message"].as_str().unwrap().contains("already exists"),
        "{v}"
    );

    let (status, v) = hit(&app, "POST", Some(&referrer), Some(refer.clone())).await;
    assert_eq!(status, 400);
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("cannot refer themselves"),
        "{v}"
    );

    let (status, _) = hit(
        &app,
        "POST",
        Some(&other),
        Some(json!({ "referrer": "not-an-address" })),
    )
    .await;
    assert_eq!(status, 400);

    // PATCH walks pending -> signed_up once; the repeat reports the invalid status.
    let (status, _) = hit(&app, "PATCH", Some(&invited), None).await;
    assert_eq!(status, 204);
    let (status, v) = hit(&app, "PATCH", Some(&invited), None).await;
    assert_eq!(status, 400);
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("Invalid referral status"),
        "{v}"
    );

    let (status, v) = hit(&app, "PATCH", Some(&other), None).await;
    assert_eq!(status, 404);
    assert!(v["message"].as_str().unwrap().contains("not found"), "{v}");

    // signed_up is not accepted: the referrer's stats stay at zero.
    let (status, v) = hit(&app, "GET", Some(&referrer), None).await;
    assert_eq!(status, 200);
    assert_eq!(v["invitedUsersAccepted"], 0);

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
