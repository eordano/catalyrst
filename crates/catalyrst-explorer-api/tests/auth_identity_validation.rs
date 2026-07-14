mod support;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::modules::auth_api::{IdentityRecord, IdentityStatus};
use catalyrst_explorer_api::{api_router, build_state, AppState};

use support::{
    identity_post, mint_identity, signed_fetch_headers, signed_identity_post, EPHEMERAL_KEY,
    IDENTITY_PATH, OTHER_KEY, SIGNER_KEY,
};

const IP: &str = "203.0.113.9";

async fn state() -> AppState {
    let cfg = Config::from_env().expect("config from env defaults");
    build_state(&cfg).await.expect("build state")
}

fn app(state: &AppState) -> Router {
    api_router().with_state(state.clone())
}

async fn body_json(res: Response) -> Value {
    let bytes = to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn get_identity(app: &Router, id: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/auth/identities/{id}"))
        .header("x-real-ip", IP)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    (status, body_json(res).await)
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[tokio::test]
async fn unsigned_post_is_refused_as_unauthenticated() {
    let app = app(&state().await);
    let identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let res = app
        .oneshot(identity_post(&identity, vec![], IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert!(body["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert_eq!(
        body["message"],
        "This endpoint requires a signed fetch request. See ADR-44."
    );
}

#[tokio::test]
async fn signed_identity_is_created_and_served_back_verbatim() {
    let app = app(&state().await);
    let (identity, request) = signed_identity_post(IP);
    let res = app.clone().oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = body_json(res).await;
    let id = created["identityId"].as_str().unwrap();
    assert_eq!(id.len(), 36);
    assert!(created["expiration"].as_str().is_some());

    let (status, body) = get_identity(&app, id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["identity"], identity);

    let (status, body) = get_identity(&app, id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Identity was already consumed");
}

#[tokio::test]
async fn ephemeral_address_must_match_the_chain_final_authority() {
    let app = app(&state().await);
    let mut identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&identity, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    identity["ephemeralIdentity"]["address"] = json!(support::wallet(OTHER_KEY).address());
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(res).await["error"],
        "Ephemeral wallet address does not match auth chain final authority"
    );
}

#[tokio::test]
async fn request_signer_must_own_the_identity() {
    let app = app(&state().await);
    let identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let stranger = mint_identity(OTHER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&stranger, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(res).await["error"],
        "Request sender does not match identity owner"
    );
}

#[tokio::test]
async fn ephemeral_private_key_must_derive_the_ephemeral_address() {
    let app = app(&state().await);
    let mut identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&identity, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    identity["ephemeralIdentity"]["privateKey"] = json!(OTHER_KEY);
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(res).await["error"],
        "Ephemeral private key does not match the provided address"
    );
}

#[tokio::test]
async fn tampered_delegation_signature_is_a_bad_request() {
    let app = app(&state().await);
    let mut identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&identity, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    let forged = mint_identity(OTHER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    identity["authChain"][1]["signature"] = forged["authChain"][1]["signature"].clone();
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(body_json(res).await["error"].as_str().is_some());
}

#[tokio::test]
async fn identity_chain_must_end_in_an_ephemeral_delegation() {
    let app = app(&state().await);
    let mut identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&identity, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    identity["authChain"] = json!([identity["authChain"][0].clone()]);
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(res).await["error"],
        "Could not get final authority from auth chain"
    );
}

#[tokio::test]
async fn identity_body_must_carry_the_persisted_shape() {
    let app = app(&state().await);
    let mut identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(&identity, EPHEMERAL_KEY, "post", IDENTITY_PATH, now_ms());
    identity
        .as_object_mut()
        .unwrap()
        .remove("ephemeralIdentity");
    let res = app
        .oneshot(identity_post(&identity, headers, IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(body_json(res).await["error"]
        .as_str()
        .is_some_and(|e| e.starts_with("Invalid AuthIdentity")));
}

// The apex and gateway vhosts mount the crate under /auth-api and forward the
// public pathname the browser signed; the fanout host (auth-api.example.com) strips
// to the unprefixed route, which is the fallback.
#[tokio::test]
async fn x_original_path_rebinds_the_signed_path_to_the_public_prefix() {
    let app = app(&state().await);
    let identity = mint_identity(SIGNER_KEY, EPHEMERAL_KEY, Utc::now() + Duration::days(1));
    let headers = signed_fetch_headers(
        &identity,
        EPHEMERAL_KEY,
        "post",
        "/auth-api/identities",
        now_ms(),
    );

    let res = app
        .clone()
        .oneshot(identity_post(&identity, headers.clone(), IP))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    let mut request = identity_post(&identity, headers, IP);
    request.headers_mut().insert(
        "x-original-path",
        "/auth-api/identities?flow=deeplink".parse().unwrap(),
    );
    let res = app.oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

fn stale_record(id: &str, expiration: chrono::DateTime<Utc>) -> IdentityRecord {
    IdentityRecord {
        identity_id: id.to_string(),
        identity: json!({ "authChain": [] }),
        ip_address: IP.to_string(),
        is_mobile: false,
        created_at: expiration - Duration::seconds(3600),
        expiration,
    }
}

fn status_at(created_at: chrono::DateTime<Utc>) -> IdentityStatus {
    IdentityStatus {
        expiration: created_at + Duration::seconds(3600),
        created_at,
        consumed: false,
        signer: String::new(),
        deletion_reason: None,
    }
}

#[tokio::test]
async fn expired_identities_are_swept_by_the_next_post() {
    let state = state().await;
    let app = app(&state);
    let expired_id = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
    let live_id = "8c545b52-c08e-4c39-a854-46e2b5b03c48";
    let now = Utc::now();
    state.auth_api.identities.insert(
        expired_id.into(),
        stale_record(expired_id, now - Duration::seconds(1)),
    );
    state
        .auth_api
        .identity_status
        .insert(expired_id.into(), status_at(now - Duration::seconds(3601)));
    state.auth_api.identities.insert(
        live_id.into(),
        stale_record(live_id, now + Duration::seconds(600)),
    );

    let (_, request) = signed_identity_post(IP);
    let res = app.clone().oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    assert!(!state.auth_api.identities.contains_key(expired_id));
    assert!(state.auth_api.identities.contains_key(live_id));
    let (status, body) = get_identity(&app, expired_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Identity was evicted");
}

#[tokio::test]
async fn tombstones_outlive_their_identity_for_two_weeks_only() {
    let state = state().await;
    let app = app(&state);
    let fresh = "8c545b52-c08e-4c39-a854-46e2b5b03c48";
    let ancient = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
    let now = Utc::now();
    state
        .auth_api
        .identity_status
        .insert(fresh.into(), status_at(now - Duration::days(13)));
    state
        .auth_api
        .identity_status
        .insert(ancient.into(), status_at(now - Duration::days(15)));

    let (_, request) = signed_identity_post(IP);
    let res = app.clone().oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    assert_eq!(
        get_identity(&app, fresh).await.1["error"],
        "Identity was evicted"
    );
    assert_eq!(
        get_identity(&app, ancient).await.1["error"],
        "Identity not found"
    );
}

#[tokio::test]
async fn pending_identities_are_capped() {
    let state = state().await;
    let app = app(&state);
    let now = Utc::now();
    for i in 0..10_000u32 {
        let id = format!("00000000-0000-4000-8000-{i:012x}");
        state
            .auth_api
            .identities
            .insert(id.clone(), stale_record(&id, now + Duration::seconds(600)));
    }

    let (_, request) = signed_identity_post(IP);
    let res = app.clone().oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(state.auth_api.identities.len(), 10_000);

    state.auth_api.identities.clear();
    let (_, request) = signed_identity_post(IP);
    let res = app.oneshot(request).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}
