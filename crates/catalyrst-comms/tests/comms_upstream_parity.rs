use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::deferred::{STREAM_ACCESS_NOT_FOUND_MSG, STREAM_RESET_FAILED_MSG};
use catalyrst_comms::handlers::responses::SceneStreamAccessResponse;
use catalyrst_comms::handlers::scene_adapter::{get_server_scene_adapter, SceneAdapterRequest};
use catalyrst_comms::handlers::scene_participants::{list_participants, ParticipantsQuery};
use catalyrst_comms::http::{conflict, not_found_labeled, unauthorized, ApiError};
use catalyrst_comms::livekit::{scene_room_name, world_room_name, world_scene_room_name};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{api_router, AppState, AppStateInner};
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use sqlx::PgPool;

fn lazy_state(authoritative_server_address: Option<String>) -> AppState {
    let pool = PgPool::connect_lazy("postgres://postgres@127.0.0.1:1/postgres")
        .expect("lazy pool never connects in these tests");
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool: None,
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_host: "127.0.0.1:1".into(),
        livekit_ws_url: "wss://livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: "test-peer".into(),
    })
}

fn signed_headers(wallet: &Wallet, method: &str, path: &str) -> HeaderMap {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let payload = build_payload(method, path, &timestamp, "{}");
    let chain = create_simple_auth_chain(wallet, &payload).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp).unwrap(),
    );
    headers.insert(AUTH_METADATA_HEADER, HeaderValue::from_static("{}"));
    for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
        headers.insert(
            HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes()).unwrap(),
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
    }
    headers
}

async fn body_value(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn error_envelope_matches_live_upstream() {
    assert_eq!(
        body_value(unauthorized("Access denied, invalid signed-fetch request").into_response())
            .await,
        json!({ "error": "Access denied, invalid signed-fetch request" })
    );
    assert_eq!(
        body_value(
            ApiError::bad_request("Either pointer or realm_name must be provided").into_response()
        )
        .await,
        json!({ "error": "Either pointer or realm_name must be provided" })
    );
    assert_eq!(
        body_value(ApiError::internal("boom").into_response()).await,
        json!({ "error": "Internal Server Error" })
    );
    assert_eq!(
        body_value(conflict("scene already banned").into_response()).await,
        json!({ "error": "Conflict", "message": "scene already banned" })
    );
    assert_eq!(
        body_value(not_found_labeled("ban not found").into_response()).await,
        json!({ "error": "Not Found", "message": "ban not found" })
    );
}

#[test]
fn scene_room_name_is_realm_scoped() {
    assert_eq!(scene_room_name("main", "bafkreix"), "scene:main:bafkreix");
    assert_ne!(
        scene_room_name("realm-a", "bafkreix"),
        scene_room_name("realm-b", "bafkreix")
    );
    assert_eq!(world_scene_room_name("foo.eth", "xyz"), "world-foo.eth-xyz");
    assert_eq!(world_room_name("foo.eth"), "world-foo.eth");
}

#[tokio::test]
async fn scene_adapter_body_and_ttl() {
    let wallet =
        Wallet::from_hex("0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d")
            .unwrap();
    let state = lazy_state(Some(wallet.address().to_lowercase()));
    let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "bafkreiservertoken",
        "realmName": "main",
    }))
    .unwrap();

    let Json(resp) = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect("authoritative signer mints an adapter");

    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 1);
    assert!(v.get("adapter").is_some());
    assert!(resp.adapter.starts_with("livekit:wss://"));

    let token = resp.adapter.split("access_token=").nth(1).unwrap();
    let claims = token.split('.').nth(1).unwrap();
    let decoded = URL_SAFE_NO_PAD.decode(claims).unwrap();
    let payload: Value = serde_json::from_slice(&decoded).unwrap();
    let exp = payload["exp"].as_i64().unwrap();
    let nbf = payload["nbf"].as_i64().unwrap();
    assert_eq!(exp - nbf, 300);
}

#[tokio::test]
async fn scene_participants_envelope() {
    let state = lazy_state(None);
    let Json(resp) = list_participants(
        State(state),
        Query(ParticipantsQuery {
            pointer: None,
            realm_name: Some("foo.eth".into()),
            room: None,
        }),
    )
    .await
    .expect("world roster degrades to empty");
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        json!({ "ok": true, "data": { "addresses": [] } })
    );
}

#[test]
fn stream_access_response_shape() {
    let v = serde_json::to_value(SceneStreamAccessResponse {
        streaming_url: "rtmp://ingest/x".into(),
        streaming_key: "sk_fresh".into(),
        created_at: 1_700_000_000_000,
        ends_at: 1_700_345_600_000,
    })
    .unwrap();
    assert_eq!(
        v,
        json!({
            "streaming_url": "rtmp://ingest/x",
            "streaming_key": "sk_fresh",
            "created_at": 1_700_000_000_000i64,
            "ends_at": 1_700_345_600_000i64,
        })
    );
}

#[tokio::test]
async fn stream_access_route_surface() {
    for method in ["GET", "POST", "PUT", "DELETE"] {
        let state = lazy_state(None);
        let app = api_router(state.clone()).with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/scene-stream-access")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::NOT_FOUND, "{method}");
        assert_ne!(resp.status(), StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert!(resp.status().is_client_error(), "{method}");
        let v = body_value(resp).await;
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 1, "{method}");
        assert!(obj.contains_key("error"), "{method}");
    }
}

#[tokio::test]
async fn stream_access_messages_match_upstream() {
    assert_eq!(
        STREAM_ACCESS_NOT_FOUND_MSG,
        "No active streaming access found for place"
    );
    assert_eq!(
        STREAM_RESET_FAILED_MSG,
        "Failed to reset scene stream access"
    );
    assert_eq!(
        body_value(ApiError::not_found(STREAM_ACCESS_NOT_FOUND_MSG).into_response()).await,
        json!({ "error": "No active streaming access found for place" })
    );
}
