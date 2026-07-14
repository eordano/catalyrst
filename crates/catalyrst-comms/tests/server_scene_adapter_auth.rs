use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::Json;
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::scene_adapter::{get_server_scene_adapter, SceneAdapterRequest};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use serde_json::json;
use sqlx::PgPool;

const AUTHORITATIVE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const OUTSIDER_KEY: &str = "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba";

fn test_state(authoritative_server_address: Option<String>) -> AppState {
    let pool = PgPool::connect_lazy("postgres://postgres@127.0.0.1:1/postgres")
        .expect("lazy pool never connects in this test");
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
        livekit_host: "livekit.local".into(),
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

fn attacker_body(identity: &str) -> SceneAdapterRequest {
    serde_json::from_value(json!({
        "sceneId": "bafkreiattackerchosenscene",
        "identity": identity,
        "realmName": "someworld.dcl.eth",
    }))
    .unwrap()
}

#[tokio::test]
async fn body_identity_without_auth_chain_cannot_mint_authoritative_token() {
    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let authoritative_address = authoritative.address().to_lowercase();
    let state = test_state(Some(authoritative_address.clone()));

    let err = get_server_scene_adapter(
        State(state),
        HeaderMap::new(),
        Json(attacker_body(&authoritative_address)),
    )
    .await
    .err()
    .unwrap_or_else(|| {
        panic!(
            "an unauthenticated caller who merely knows the public authoritative address {authoritative_address} received a can_publish LiveKit token"
        )
    });

    assert_eq!(
        err.code, 401,
        "unsigned request must be rejected, got {}: {}",
        err.code, err.message
    );
}

#[tokio::test]
async fn body_identity_cannot_override_a_non_authoritative_signature() {
    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let outsider = Wallet::from_hex(OUTSIDER_KEY).unwrap();
    let state = test_state(Some(authoritative.address().to_lowercase()));

    let headers = signed_headers(&outsider, "post", "/get-server-scene-adapter");
    let err = get_server_scene_adapter(
        State(state),
        headers,
        Json(attacker_body(&authoritative.address().to_lowercase())),
    )
    .await
    .expect_err("a signature from a non-authoritative wallet must be rejected");

    assert_eq!(err.code, 401);
}

#[tokio::test]
async fn authoritative_server_signature_still_mints_a_token() {
    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let state = test_state(Some(authoritative.address().to_lowercase()));

    let headers = signed_headers(&authoritative, "post", "/get-server-scene-adapter");
    let body: SceneAdapterRequest = serde_json::from_value(json!({
        "sceneId": "bafkreiservertoken",
        "realmName": "main",
    }))
    .unwrap();

    let Json(value) = get_server_scene_adapter(State(state), headers, Json(body))
        .await
        .expect("the signed authoritative server must still get an adapter");
    assert!(value
        .adapter
        .starts_with("livekit:wss://livekit.local?access_token="));
}
