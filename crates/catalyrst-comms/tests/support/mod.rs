//! Shared test-support for the catalyrst-comms optimization perf tests.
//!
//! SQL statement counting for these tests is `catalyrst_testgate::sql_capture`.
//! It still requires every counting test to use a plain `#[tokio::test]`
//! (current-thread flavor -- never `flavor = "multi_thread"`), because routing
//! is keyed by the OS thread that called `sql_capture()`.
#![allow(dead_code)]

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use sqlx::PgPool;

/// An [`AppState`] over the given pools. Mirrors the literal in
/// `tests/submit_commit_epoch_author.rs`, adding the three knobs the batching
/// tests vary: `places_pool`, `dapps_pool`, and `dapps_schema`.
pub fn test_state(
    pool: PgPool,
    places_pool: Option<PgPool>,
    dapps_pool: Option<PgPool>,
    dapps_schema: impl Into<String>,
) -> AppState {
    let dapps_schema = dapps_schema.into();
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(dapps_pool.clone(), dapps_schema.clone()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool,
        dapps_pool,
        dapps_schema,
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_api_url: "https://livekit.local".into(),
        livekit_ws_url: "wss://livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: "test-peer".into(),
    })
}

/// Signed-fetch headers for `method`+`path` (metadata `{}`), copied from
/// `tests/submit_commit_epoch_author.rs`.
pub fn signed_headers(wallet: &Wallet, method: &str, path: &str) -> HeaderMap {
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
