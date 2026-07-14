use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::deferred::{STREAM_ACCESS_NOT_FOUND_MSG, STREAM_RESET_FAILED_MSG};
use catalyrst_comms::handlers::responses::SceneStreamAccessResponse;
use catalyrst_comms::handlers::scene_adapter::{
    adapter_room_name, get_server_scene_adapter, realm_name_from, scene_id_from,
    SceneAdapterRequest,
};
use catalyrst_comms::handlers::scene_participants::{list_participants, ParticipantsQuery};
use catalyrst_comms::http::{conflict, not_found_labeled, unauthorized, ApiError};
use catalyrst_comms::livekit::{scene_room_name, world_room_name, world_scene_room_name};
use catalyrst_comms::ports::extra_addresses::has_world_access_permission;
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
    lazy_state_with_world(authoritative_server_address, "http://127.0.0.1:1")
}

fn lazy_state_with_world(
    authoritative_server_address: Option<String>,
    world_content_url: &str,
) -> AppState {
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
        world_content_url: world_content_url.into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_api_url: "http://127.0.0.1:1".into(),
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

fn adapter_claims(adapter: &str) -> Value {
    let token = adapter.split("access_token=").nth(1).unwrap();
    let claims = token.split('.').nth(1).unwrap();
    let decoded = URL_SAFE_NO_PAD.decode(claims).unwrap();
    serde_json::from_slice(&decoded).unwrap()
}

fn adapter_room(adapter: &str) -> String {
    adapter_claims(adapter)["video"]["room"]
        .as_str()
        .expect("the minted token carries a room grant")
        .to_string()
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

    let payload = adapter_claims(&resp.adapter);
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

#[test]
fn preview_realms_derive_a_plain_scene_room() {
    assert_eq!(
        adapter_room_name("LocalPreview", "b64-preview-scene"),
        scene_room_name("LocalPreview", "b64-preview-scene")
    );
    assert_eq!(
        adapter_room_name("preview", "b64-preview-scene"),
        "scene:preview:b64-preview-scene"
    );
    assert_eq!(
        adapter_room_name("foo.eth", "bafkreix"),
        world_scene_room_name("foo.eth", "bafkreix")
    );
}

#[tokio::test]
async fn server_scene_adapter_mints_the_client_room_for_a_preview_realm() {
    let wallet =
        Wallet::from_hex("0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d")
            .unwrap();

    for realm in ["LocalPreview", "preview"] {
        let state = lazy_state(Some(wallet.address().to_lowercase()));
        let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
        let body: SceneAdapterRequest = serde_json::from_value(json!({
            "sceneId": "b64-preview-scene",
            "realmName": realm,
        }))
        .unwrap();

        let Json(resp) = get_server_scene_adapter(State(state), headers, Json(body))
            .await
            .expect("a preview realm still mints an authoritative adapter");

        let room = adapter_room(&resp.adapter);
        assert_eq!(
            room,
            scene_room_name(realm, "b64-preview-scene"),
            "the authoritative server must land in the room its own clients join"
        );
        assert_eq!(room, adapter_room_name(realm, "b64-preview-scene"));
        assert!(
            !room.starts_with("preview-"),
            "preview-<sceneId> splits the authoritative server off from its clients, got {room}"
        );
    }
}

#[tokio::test]
async fn server_scene_adapter_requires_a_scene_id_on_every_realm() {
    let wallet =
        Wallet::from_hex("0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d")
            .unwrap();

    for realm in ["LocalPreview", "preview", "main", "foo.eth"] {
        let state = lazy_state(Some(wallet.address().to_lowercase()));
        let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
        let body: SceneAdapterRequest = serde_json::from_value(json!({
            "realmName": realm,
        }))
        .unwrap();

        let err = get_server_scene_adapter(State(state), headers, Json(body))
            .await
            .expect_err("a request naming no scene can only mint a room named after nothing");

        assert_eq!(err.code, 400, "realm {realm}");
        assert_eq!(
            body_value(err.into_response()).await,
            json!({ "error": "Access denied, invalid signed-fetch request, no sceneId" }),
            "realm {realm}"
        );
    }
}

type StubRoutes = Arc<HashMap<String, (StatusCode, Value)>>;

async fn stub_world_content(routes: &[(&str, StatusCode, Value)]) -> String {
    let map: StubRoutes = Arc::new(
        routes
            .iter()
            .map(|(path, status, body)| ((*path).to_string(), (*status, body.clone())))
            .collect(),
    );
    let app = Router::new()
        .fallback(|State(routes): State<StubRoutes>, uri: Uri| async move {
            match routes.get(uri.path()) {
                Some((status, body)) => (*status, Json(body.clone())).into_response(),
                None => {
                    (StatusCode::NOT_FOUND, Json(json!({ "error": "Not Found" }))).into_response()
                }
            }
        })
        .with_state(map);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn authoritative_wallet() -> Wallet {
    Wallet::from_hex("0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d").unwrap()
}

#[tokio::test]
async fn server_scene_adapter_resolves_a_world_name_sent_as_the_scene_id() {
    let wallet = authoritative_wallet();
    let about = json!({
        "configurations": {
            "scenesUrn": ["urn:decentraland:entity:bafkreiworldentity?baseUrl=https://x/contents/"]
        }
    });
    let world_content =
        stub_world_content(&[("/world/foo.eth/about", StatusCode::OK, about)]).await;

    let state = lazy_state_with_world(Some(wallet.address().to_lowercase()), &world_content);
    let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "foo.eth",
        "realmName": "foo.eth",
    }))
    .unwrap();

    let Json(resp) = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect("the world name resolves to a scene entity id");

    let room = adapter_room(&resp.adapter);
    assert_eq!(
        room,
        world_scene_room_name("foo.eth", "bafkreiworldentity"),
        "the authoritative server must join the room its clients derive from the entity id"
    );
    assert_ne!(room, world_scene_room_name("foo.eth", "foo.eth"));
}

#[tokio::test]
async fn server_scene_adapter_rejects_a_world_whose_scene_id_cannot_be_resolved() {
    let wallet = authoritative_wallet();
    let world_content = stub_world_content(&[(
        "/world/foo.eth/about",
        StatusCode::SERVICE_UNAVAILABLE,
        json!({ "error": "down" }),
    )])
    .await;

    let state = lazy_state_with_world(Some(wallet.address().to_lowercase()), &world_content);
    let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "foo.eth",
        "realmName": "foo.eth",
    }))
    .unwrap();

    let err = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect_err("an unresolvable world must not mint a room named after the world name");

    assert_eq!(err.code, 400);
    assert_eq!(
        body_value(err.into_response()).await,
        json!({ "error": "Failed to resolve scene ID for world foo.eth" })
    );
}

#[tokio::test]
async fn server_scene_adapter_requires_a_realm_name() {
    let wallet = authoritative_wallet();
    let state = lazy_state(Some(wallet.address().to_lowercase()));
    let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "bafkreix",
    }))
    .unwrap();

    let err = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect_err("a request naming no realm must not be pooled into a default realm");

    assert_eq!(err.code, 401);
    assert_eq!(
        body_value(err.into_response()).await,
        json!({ "error": "Access denied, invalid signed-fetch request, no realm" })
    );
}

#[tokio::test]
async fn server_scene_adapter_reports_the_missing_realm_before_the_missing_scene_id() {
    let wallet = authoritative_wallet();
    let state = lazy_state(Some(wallet.address().to_lowercase()));
    let headers = signed_headers(&wallet, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({})).unwrap();

    let err = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect_err("a request naming neither realm nor scene cannot mint a room");

    assert_eq!(
        err.code, 401,
        "upstream reaches the realm inside validate(), ahead of the handler's sceneId check"
    );
    assert_eq!(
        body_value(err.into_response()).await,
        json!({ "error": "Access denied, invalid signed-fetch request, no realm" })
    );
}

#[tokio::test]
async fn server_scene_adapter_checks_the_server_key_before_the_request_shape() {
    let authoritative = authoritative_wallet();
    let outsider =
        Wallet::from_hex("0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba")
            .unwrap();
    let state = lazy_state(Some(authoritative.address().to_lowercase()));
    let headers = signed_headers(&outsider, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "realmName": "main",
    }))
    .unwrap();

    let err = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect_err("a non-authoritative signer is rejected whatever its request shape");

    assert_eq!(err.code, 401);
    assert_eq!(
        body_value(err.into_response()).await,
        json!({ "error": "Access denied, invalid server public key" })
    );
}

#[test]
fn scene_and_realm_fall_back_to_the_unsigned_body() {
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "bafkreibody",
        "realmName": "body.eth",
    }))
    .unwrap();

    let signed = json!({ "sceneId": "bafkreimeta", "realmName": "meta.eth" });
    assert_eq!(
        scene_id_from(&signed, &body).as_deref(),
        Some("bafkreimeta")
    );
    assert_eq!(realm_name_from(&signed, &body).as_deref(), Some("meta.eth"));

    let realm_only = json!({ "realm": { "serverName": "nested.eth" } });
    assert_eq!(
        realm_name_from(&realm_only, &body).as_deref(),
        Some("nested.eth")
    );
    assert_eq!(
        scene_id_from(&realm_only, &body).as_deref(),
        Some("bafkreibody")
    );

    let empty = json!({});
    let scene_id = scene_id_from(&empty, &body).unwrap();
    let realm_name = realm_name_from(&empty, &body).unwrap();
    assert_eq!(scene_id, "bafkreibody");
    assert_eq!(realm_name, "body.eth");
    assert_eq!(
        adapter_room_name(&realm_name, &scene_id),
        world_scene_room_name("body.eth", "bafkreibody"),
        "the room is derived from the same identifier the ban check is keyed on"
    );

    let no_scene: SceneAdapterRequest = serde_json::from_value(json!({})).unwrap();
    assert_eq!(scene_id_from(&empty, &no_scene), None);
    assert_eq!(realm_name_from(&empty, &no_scene), None);
}

#[tokio::test]
async fn world_access_permission_is_fail_closed() {
    let identity = "0xabc0000000000000000000000000000000000001";

    let unreachable = lazy_state(None);
    assert!(!has_world_access_permission(&unreachable, identity, "foo.eth").await);

    let non_2xx = stub_world_content(&[(
        "/world/foo.eth/permissions",
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "boom" }),
    )])
    .await;
    let state = lazy_state_with_world(None, &non_2xx);
    assert!(!has_world_access_permission(&state, identity, "foo.eth").await);

    let unparseable = stub_world_content(&[(
        "/world/foo.eth/permissions",
        StatusCode::OK,
        json!(["not", "an", "object"]),
    )])
    .await;
    let state = lazy_state_with_world(None, &unparseable);
    assert!(!has_world_access_permission(&state, identity, "foo.eth").await);
}

#[tokio::test]
async fn world_access_permission_honors_owner_unrestricted_and_allow_list() {
    let identity = "0xAbC0000000000000000000000000000000000001";

    let owner = stub_world_content(&[(
        "/world/foo.eth/permissions",
        StatusCode::OK,
        json!({
            "owner": identity.to_lowercase(),
            "permissions": { "access": { "type": "allow-list", "wallets": [] } }
        }),
    )])
    .await;
    let state = lazy_state_with_world(None, &owner);
    assert!(has_world_access_permission(&state, identity, "foo.eth").await);

    let unrestricted = stub_world_content(&[(
        "/world/foo.eth/permissions",
        StatusCode::OK,
        json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": { "access": { "type": "unrestricted" } }
        }),
    )])
    .await;
    let state = lazy_state_with_world(None, &unrestricted);
    assert!(has_world_access_permission(&state, identity, "foo.eth").await);

    let allow_list = stub_world_content(&[(
        "/world/foo.eth/permissions",
        StatusCode::OK,
        json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": {
                "access": {
                    "type": "allow-list",
                    "wallets": ["0x1111111111111111111111111111111111111111"]
                }
            }
        }),
    )])
    .await;
    let state = lazy_state_with_world(None, &allow_list);
    assert!(!has_world_access_permission(&state, identity, "foo.eth").await);
}
